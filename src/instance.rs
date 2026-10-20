//! Per-workspace launch coordination and local requests to reveal an existing window.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Result of claiming the local instance for one normalized workspace.
pub(crate) enum LaunchOutcome {
    Primary(InstanceGuard),
    Existing,
}

/// Keeps a workspace claim alive and receives requests to show its window.
pub(crate) struct InstanceGuard {
    workspace: PathBuf,
    lock: File,
    endpoint: PathBuf,
    address: SocketAddr,
    stopping: Arc<AtomicBool>,
    show_requested: Arc<AtomicBool>,
    repaint: Arc<Mutex<Option<eframe::egui::Context>>>,
    worker: Option<JoinHandle<()>>,
}

impl InstanceGuard {
    /// Tests whether a path names the workspace protected by this guard.
    ///
    /// # Arguments
    ///
    /// - `workspace` - Workspace path, including an equivalent canonical alias.
    ///
    /// # Returns
    ///
    /// Whether this guard already owns that workspace.
    pub(crate) fn owns_workspace(&self, workspace: &Path) -> bool {
        let workspace = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_owned());
        self.workspace == workspace
    }

    /// Connects reveal notifications to the initialized native event loop.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context whose native event loop should wake for reveal requests.
    pub(crate) fn set_repaint_context(&self, ctx: eframe::egui::Context) {
        if let Ok(mut repaint) = self.repaint.lock() {
            *repaint = Some(ctx);
        }
    }

    /// Preserves native wakeups when replacing the claim during a workspace switch.
    ///
    /// # Arguments
    ///
    /// - `previous` - Existing claim attached to this application's native event loop.
    pub(crate) fn inherit_repaint_context(&self, previous: &Self) {
        let ctx = previous.repaint.lock().ok().and_then(|ctx| ctx.clone());
        if let Some(ctx) = ctx {
            self.set_repaint_context(ctx);
        }
    }

    /// Takes all pending reveal requests, coalescing repeated launches.
    ///
    /// # Returns
    ///
    /// Whether at least one ordinary duplicate launch requested the window.
    pub(crate) fn take_show_request(&self) -> bool {
        self.show_requested.swap(false, Ordering::AcqRel)
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // A stop flag cannot interrupt blocking accept. Join only after waking
        // it successfully; otherwise detach instead of hanging application exit.
        let woke = TcpStream::connect_timeout(&self.address, Duration::from_millis(150)).is_ok();
        if let Some(worker) = self.worker.take()
            && woke
        {
            let _ = worker.join();
        }
        // Keep the claim until its endpoint has been removed; a subsequent
        // primary must never have its newly written endpoint removed by us.
        let _ = fs::remove_file(&self.endpoint);
        let _ = self.lock.unlock();
    }
}

/// Claims a workspace, or asks its existing local instance to reveal the window.
///
/// A duplicate startup launch stays quiet. Stale endpoint files are replaced
/// only after the operating system grants the workspace's exclusive file lock.
///
/// # Arguments
///
/// - `workspace` - Workspace whose lists this launch will display.
/// - `local_state_dir` - Local application settings directory for coordination files.
/// - `start_hidden` - Whether this is a quiet startup launch.
///
/// # Returns
///
/// A guard for the primary application, or confirmation that an existing owner handled it.
///
/// # Errors
///
/// Returns a visible startup error if locking, listener setup, or revealing an owner fails.
pub(crate) fn claim(
    workspace: &Path,
    local_state_dir: &Path,
    start_hidden: bool,
) -> Result<LaunchOutcome, String> {
    let workspace = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_owned());
    // A fixed FNV-1a hash keeps the filename stable across Rust/toolchain updates.
    let key = workspace
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let directory = local_state_dir.join("instances");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create instance directory: {error}"))?;
    let stem = format!("{key:016x}");
    let lock_path = directory.join(format!("{stem}.lock"));
    let endpoint = directory.join(format!("{stem}.endpoint"));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("could not open instance lock: {error}"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            if start_hidden || notify_existing(&endpoint, &lock)? {
                return Ok(LaunchOutcome::Existing);
            }
        }
        Err(TryLockError::Error(error)) => {
            return Err(format!("could not claim workspace instance: {error}"));
        }
    }

    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("could not listen for window requests: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read window listener address: {error}"))?;
    let token = Uuid::new_v4().to_string();
    fs::write(&endpoint, format!("{address}\n{token}\n"))
        .map_err(|error| format!("could not publish window listener: {error}"))?;
    let stopping = Arc::new(AtomicBool::new(false));
    let show_requested = Arc::new(AtomicBool::new(false));
    let repaint: Arc<Mutex<Option<eframe::egui::Context>>> = Arc::new(Mutex::new(None));
    let thread_stop = stopping.clone();
    let thread_show = show_requested.clone();
    let thread_repaint = repaint.clone();
    let worker = thread::Builder::new()
        .name("window-requests".into())
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if thread_stop.load(Ordering::Acquire) {
                            break;
                        }
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));
                        let expected = format!("SHOW {token}\n");
                        let mut request = vec![0; expected.len()];
                        if stream.read_exact(&mut request).is_ok() && request == expected.as_bytes()
                        {
                            thread_show.store(true, Ordering::Release);
                            if let Ok(repaint) = thread_repaint.lock()
                                && let Some(ctx) = repaint.as_ref()
                            {
                                ctx.request_repaint();
                            }
                            let _ = stream.write_all(b"OK\n");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => {
                        // Resource exhaustion and transient socket failures must
                        // not permanently disable repeated-launch window reveal.
                        thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        })
        .map_err(|error| {
            let _ = fs::remove_file(&endpoint);
            format!("could not start window listener: {error}")
        })?;
    Ok(LaunchOutcome::Primary(InstanceGuard {
        workspace,
        lock,
        endpoint,
        address,
        stopping,
        show_requested,
        repaint,
        worker: Some(worker),
    }))
}

fn notify_existing(endpoint: &Path, lock: &File) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last_error = "window listener is not ready".to_owned();
    while Instant::now() < deadline {
        // The old owner may be quitting between our initial lock attempt and
        // notification. Acquiring its released claim lets this launch continue.
        match lock.try_lock() {
            Ok(()) => return Ok(false),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(format!("could not claim workspace instance: {error}"));
            }
        }
        let attempt = || -> Result<(), String> {
            let contents = fs::read_to_string(endpoint).map_err(|error| error.to_string())?;
            let mut lines = contents.lines();
            let address: SocketAddr = lines
                .next()
                .unwrap_or_default()
                .parse()
                .map_err(|_| "invalid window listener address")?;
            if !address.ip().is_loopback() {
                return Err("window listener address must be local".into());
            }
            let token = lines
                .next()
                .filter(|token| Uuid::parse_str(token).is_ok())
                .ok_or("invalid window listener token")?;
            let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(150))
                .map_err(|error| error.to_string())?;
            stream
                .set_read_timeout(Some(Duration::from_millis(250)))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_millis(150)))
                .map_err(|error| error.to_string())?;
            stream
                .write_all(format!("SHOW {token}\n").as_bytes())
                .map_err(|error| error.to_string())?;
            let mut ack = [0; 3];
            stream
                .read_exact(&mut ack)
                .map_err(|error| error.to_string())?;
            if ack != *b"OK\n" {
                return Err("window listener rejected the request".into());
            }
            Ok(())
        };
        match attempt() {
            Ok(()) => return Ok(true),
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(30));
    }
    Err(format!(
        "another instance owns this workspace, but its window could not be shown: {last_error}"
    ))
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;
