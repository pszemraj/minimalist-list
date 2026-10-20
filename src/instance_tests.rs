//! Isolated launch, recovery, and reveal coordination checks.

use super::*;

fn fixture() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("minimalist-instance-{}", Uuid::new_v4()));
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    (root, workspace)
}

fn primary(workspace: &Path, state: &Path) -> InstanceGuard {
    match claim(workspace, state, false).unwrap() {
        LaunchOutcome::Primary(guard) => guard,
        LaunchOutcome::Existing => panic!("expected a new primary instance"),
    }
}

#[test]
fn ordinary_duplicate_requests_show_and_hidden_duplicate_stays_quiet() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    guard.set_repaint_context(eframe::egui::Context::default());
    assert!(!guard.take_show_request());
    assert!(matches!(
        claim(&workspace, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(guard.take_show_request());
    assert!(!guard.take_show_request());
    assert!(matches!(
        claim(&workspace, &root, true).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(!guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn normal_exit_releases_claim_and_stale_endpoint_is_replaced() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    let endpoint = guard.endpoint.clone();
    let stale = fs::read_to_string(&endpoint).unwrap();
    let address = guard.address;
    drop(guard);
    assert!(
        TcpStream::connect(address).is_err(),
        "listener survived normal shutdown"
    );
    assert!(!endpoint.exists());
    // A killed process leaves this file behind while its OS file lock is released.
    fs::write(&endpoint, &stale).unwrap();
    let guard = primary(&workspace, &root);
    assert_ne!(fs::read_to_string(&endpoint).unwrap(), stale);
    assert!(matches!(
        claim(&workspace, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn different_workspaces_and_aliases_have_the_expected_owner() {
    let (root, workspace) = fixture();
    let other = root.join("other-workspace");
    fs::create_dir_all(&other).unwrap();
    let first = primary(&workspace, &root);
    let second = primary(&other, &root);
    assert_ne!(first.endpoint, second.endpoint);
    let alias = workspace.join("..").join("workspace");
    assert!(first.owns_workspace(&workspace));
    assert!(first.owns_workspace(&alias));
    assert!(!first.owns_workspace(&other));
    assert!(matches!(
        claim(&alias, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(first.take_show_request());
    assert!(!second.take_show_request());
    assert!(matches!(
        claim(&other, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(second.take_show_request());
    drop(first);
    drop(second);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn simultaneous_launches_elect_one_owner_and_deliver_the_reveal_request() {
    let (root, workspace) = fixture();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let outcomes = thread::scope(|scope| {
        let launches: Vec<_> = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let workspace = &workspace;
                let root = &root;
                scope.spawn(move || {
                    barrier.wait();
                    claim(workspace, root, false).unwrap()
                })
            })
            .collect();
        launches
            .into_iter()
            .map(|launch| launch.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut owner = None;
    let mut existing = 0;
    for outcome in outcomes {
        match outcome {
            LaunchOutcome::Primary(guard) => {
                assert!(owner.is_none());
                owner = Some(guard);
            }
            LaunchOutcome::Existing => existing += 1,
        }
    }
    assert_eq!(existing, 1);
    assert!(owner.as_ref().unwrap().take_show_request());
    drop(owner);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_listener_publication_releases_the_workspace_claim() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    let endpoint = guard.endpoint.clone();
    drop(guard);
    fs::create_dir(&endpoint).unwrap();
    assert!(claim(&workspace, &root, false).is_err());
    fs::remove_dir(&endpoint).unwrap();
    let guard = primary(&workspace, &root);
    assert!(guard.owns_workspace(&workspace));
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn listener_rejects_requests_without_the_matching_endpoint_token() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    let endpoint = fs::read_to_string(&guard.endpoint).unwrap();
    let address: SocketAddr = endpoint.lines().next().unwrap().parse().unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    stream
        .write_all(format!("SHOW {}\n", Uuid::new_v4()).as_bytes())
        .unwrap();
    let mut ack = [0; 3];
    assert!(stream.read_exact(&mut ack).is_err());
    assert!(!guard.take_show_request());
    // A rejected request does not disable the listener for the next launch.
    assert!(matches!(
        claim(&workspace, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_listener_never_starts_a_second_owner() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    let endpoint = guard.endpoint.clone();
    let original = fs::read_to_string(&endpoint).unwrap();
    // Simulate an owner which still holds its claim but has a broken endpoint.
    fs::write(
        &endpoint,
        "127.0.0.1:0\n00000000-0000-0000-0000-000000000000\n",
    )
    .unwrap();
    let result = claim(&workspace, &root, false);
    assert!(result.is_err());
    assert!(matches!(
        claim(&workspace, &root, true).unwrap(),
        LaunchOutcome::Existing
    ));
    fs::write(&endpoint, original).unwrap();
    assert!(matches!(
        claim(&workspace, &root, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

fn child_launch(workspace: &Path, state: &Path, mode: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "instance::tests::claim_from_child_process",
            "--nocapture",
        ])
        .env("MINIMALIST_INSTANCE_TEST_WORKSPACE", workspace)
        .env("MINIMALIST_INSTANCE_TEST_STATE", state)
        .env("MINIMALIST_INSTANCE_TEST_MODE", mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn claim_from_child_process() {
    let Ok(mode) = std::env::var("MINIMALIST_INSTANCE_TEST_MODE") else {
        return;
    };
    let workspace = PathBuf::from(std::env::var_os("MINIMALIST_INSTANCE_TEST_WORKSPACE").unwrap());
    let state = PathBuf::from(std::env::var_os("MINIMALIST_INSTANCE_TEST_STATE").unwrap());
    if mode == "exit-owner" {
        let _guard = primary(&workspace, &state);
        // Exit without Rust destructors, as a terminated app does: the OS must
        // release its lock even though its published endpoint remains on disk.
        std::process::exit(0);
    }
    assert!(matches!(
        claim(&workspace, &state, mode == "hidden").unwrap(),
        LaunchOutcome::Existing
    ));
}

#[test]
fn separate_process_reopen_reveals_and_quiet_startup_does_not() {
    let (root, workspace) = fixture();
    let guard = primary(&workspace, &root);
    child_launch(&workspace, &root, "show");
    assert!(guard.take_show_request());
    child_launch(&workspace, &root, "hidden");
    assert!(!guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn process_exit_without_destructors_releases_the_lock_and_stale_endpoint() {
    let (root, workspace) = fixture();
    child_launch(&workspace, &root, "exit-owner");
    let endpoints: Vec<_> = fs::read_dir(root.join("instances"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "endpoint")
        })
        .collect();
    assert_eq!(endpoints.len(), 1);
    let stale = fs::read_to_string(endpoints[0].path()).unwrap();
    let guard = primary(&workspace, &root);
    assert_ne!(fs::read_to_string(&guard.endpoint).unwrap(), stale);
    child_launch(&workspace, &root, "show");
    assert!(guard.take_show_request());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopening_during_owner_shutdown_acquires_the_released_claim() {
    let (root, workspace) = fixture();
    let mut guard = primary(&workspace, &root);
    // Drop stops the listener before removing the endpoint and unlocking. Hold
    // that real shutdown stage long enough for a concurrent launch to see it.
    guard.stopping.store(true, Ordering::Release);
    let _ = TcpStream::connect_timeout(&guard.address, Duration::from_millis(150));
    guard.worker.take().unwrap().join().unwrap();
    let shutdown = thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        drop(guard);
    });
    let outcome = claim(&workspace, &root, false);
    shutdown.join().unwrap();
    let LaunchOutcome::Primary(replacement) = outcome.unwrap() else {
        panic!("a launch concurrent with shutdown must replace the exited owner");
    };
    drop(replacement);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_shutdown_wake_releases_endpoint_and_claim_without_waiting_for_accept() {
    let (root, workspace) = fixture();
    let endpoint = root.join("shutdown.endpoint");
    let lock_path = root.join("shutdown.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock().unwrap();
    fs::write(&endpoint, "published endpoint").unwrap();
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let actual_address = listener.local_addr().unwrap();
    let (finished, worker_finished) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        // Unlike the production loop's stop check, this enters accept even if
        // Drop runs first, making a missing wake deterministically block join.
        listener.accept().unwrap();
        finished.send(()).unwrap();
    });
    let guard = InstanceGuard {
        workspace,
        lock,
        endpoint: endpoint.clone(),
        // Port zero cannot name a TCP listener, so this wake always fails.
        address: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)),
        stopping: Arc::new(AtomicBool::new(false)),
        show_requested: Arc::new(AtomicBool::new(false)),
        repaint: Arc::new(Mutex::new(None)),
        worker: Some(worker),
    };
    let (dropped, drop_finished) = std::sync::mpsc::channel();
    let dropping = thread::spawn(move || {
        drop(guard);
        dropped.send(()).unwrap();
    });
    let returned_without_wake = drop_finished.recv_timeout(Duration::from_secs(1)).is_ok();
    // Always unblock the real listener before asserting, including when the
    // regression fails, so neither worker nor Drop can be left blocked.
    TcpStream::connect_timeout(&actual_address, Duration::from_secs(1)).unwrap();
    worker_finished
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    dropping.join().unwrap();
    assert!(
        returned_without_wake,
        "Drop waited for an accept it could not wake"
    );
    assert!(!endpoint.exists());
    let next_owner = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    assert!(
        next_owner.try_lock().is_ok(),
        "failed wake retained the workspace claim"
    );
    drop(next_owner);
    fs::remove_dir_all(root).unwrap();
}
