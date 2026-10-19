//! Descriptive filenames, legacy identity, and preference compatibility checks.

use super::*;
use crate::model::{Subtask, Task};

fn root() -> PathBuf {
    env::temp_dir().join(format!("minimalist-filenames-{}", Uuid::new_v4()))
}

#[test]
fn descriptive_names_are_clean_portable_and_keep_original_display_titles() {
    let root = root();
    let title = " CON / Project: α 🗂 <2026> . ";
    let list = create_list(&root, title, Accent::Mint).unwrap();
    assert_eq!(
        list.path.file_name().unwrap().to_string_lossy(),
        format!("con-project-α-2026--{}.json", list.key)
    );
    assert_eq!(list.data.title, title);
    assert_eq!(load_workspace(&root).unwrap().lists[0].data.title, title);
    let same_slug = create_list(&root, "CON Project α 2026", Accent::Sky).unwrap();
    assert_ne!(list.path, same_slug.path);
    let punctuation = create_list(&root, "../ : <> ? * \\", Accent::Sand).unwrap();
    assert_eq!(
        punctuation.path.file_name().unwrap().to_string_lossy(),
        format!("list--{}.json", punctuation.key)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn long_unicode_names_fit_common_filename_limits_without_cutting_characters() {
    let root = root();
    let title = "研究 / 📝 ".repeat(200);
    let list = create_list(&root, &title, Accent::Mint).unwrap();
    let name = list.path.file_name().unwrap().to_str().unwrap();
    assert!(name.len() <= 139);
    assert!(name.ends_with(&format!("--{}.json", list.key)));
    assert_eq!(load_workspace(&root).unwrap().lists[0].data.title, title);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn uuid_only_legacy_files_load_and_save_in_place_with_filename_identity() {
    let root = root();
    let mut list = create_list(&root, "Legacy", Accent::Mint).unwrap();
    let legacy = root.join("lists").join(format!("{}.json", list.key));
    fs::rename(&list.path, &legacy).unwrap();
    list.path = legacy.clone();
    list.data.id = Uuid::new_v4();
    save_list(&list).unwrap();
    let mut snapshot = load_workspace(&root).unwrap();
    assert_eq!(snapshot.lists[0].key, list.key);
    assert_eq!(snapshot.lists[0].data.id, list.key);
    snapshot.lists[0].data.title = "Changed legacy display title".into();
    save_list(&snapshot.lists[0]).unwrap();
    assert_eq!(snapshot.lists[0].path, legacy);
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn descriptive_filename_identity_wins_over_json_and_sync_conflict_copies() {
    let root = root();
    let mut list = create_list(&root, "Current title", Accent::Mint).unwrap();
    list.data.id = Uuid::new_v4();
    save_list(&list).unwrap();
    let mut copy = list.clone();
    copy.data.id = list.key;
    copy.data.title = "Old legacy title".into();
    copy.path = root.join("lists").join(format!("{}.json", list.key));
    save_list(&copy).unwrap();
    let legacy = copy.path.clone();
    copy.data.title = "Conflicted title".into();
    copy.path = list.path.with_file_name(format!(
        "current-title--{} (conflicted copy).json",
        list.key
    ));
    save_list(&copy).unwrap();
    let loaded = load_workspace(&root).unwrap();
    assert_eq!(loaded.lists.len(), 1);
    assert_eq!(loaded.lists[0].key, list.key);
    assert_eq!(loaded.lists[0].data.id, list.key);
    assert_eq!(loaded.lists[0].data.title, "Current title");
    assert_eq!(loaded.warnings.len(), 2);
    assert!(copy.path.exists() && legacy.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_rename_preserves_whole_task_data_and_refreshes_fingerprint() {
    let root = root();
    let mut list = create_list(&root, "Original", Accent::Mint).unwrap();
    let mut task = Task::new("Parent");
    task.toggle();
    task.subtasks.push(Subtask::new("Child"));
    list.data.tasks.push(task);
    save_list(&list).unwrap();
    let old = list.path.clone();
    let mut fingerprint = workspace_fingerprint(&root).unwrap();
    list.data.title = "New / descriptive title".into();
    save_list(&list).unwrap();
    let expected = serde_json::to_value(&list.data).unwrap();
    rename_list_file(&mut list).unwrap();
    assert!(!old.exists());
    assert_eq!(
        list.path.file_name().unwrap().to_string_lossy(),
        format!("new-descriptive-title--{}.json", list.key)
    );
    remove_file_from_fingerprint(&mut fingerprint, &old);
    update_fingerprint_for_file(&mut fingerprint, &list.path).unwrap();
    assert_eq!(fingerprint, workspace_fingerprint(&root).unwrap());
    let loaded = load_workspace(&root).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.lists[0].data).unwrap(),
        expected
    );
    rename_list_file(&mut list).unwrap();
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn renaming_refuses_an_existing_destination_without_overwriting_either_file() {
    let root = root();
    let mut list = create_list(&root, "Original", Accent::Mint).unwrap();
    let original = list.path.clone();
    list.data.title = "Renamed".into();
    save_list(&list).unwrap();
    let destination = original.with_file_name(list_file_name(&list.data.title, list.key));
    fs::write(&destination, "unrelated existing content").unwrap();
    assert!(rename_list_file(&mut list).is_err());
    assert_eq!(list.path, original);
    assert_eq!(
        fs::read_to_string(destination).unwrap(),
        "unrelated existing content"
    );
    assert_eq!(read_list(&original).unwrap().0.data.title, "Renamed");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_rename_keeps_the_original_associated_path() {
    let root = root();
    let mut list = create_list(&root, "Original", Accent::Mint).unwrap();
    let old = list.path.clone();
    fs::remove_file(&old).unwrap();
    list.data.title = "Renamed".into();
    assert!(rename_list_file(&mut list).is_err());
    assert_eq!(list.path, old);
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn rename_preserves_an_existing_dangling_symlink() {
    let root = root();
    let mut list = create_list(&root, "Original", Accent::Mint).unwrap();
    let original = list.path.clone();
    list.data.title = "Renamed".into();
    save_list(&list).unwrap();
    let destination = original.with_file_name(list_file_name(&list.data.title, list.key));
    std::os::unix::fs::symlink(root.join("missing-target"), &destination).unwrap();
    assert!(rename_list_file(&mut list).is_err());
    assert_eq!(list.path, original);
    assert!(
        fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(original.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn omitted_shortcut_uses_default_while_explicit_empty_remains_disabled() {
    let legacy: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.global_shortcut, "CommandOrControl+Alt+Shift+KeyL");
    let disabled: Settings = serde_json::from_str(r#"{"global_shortcut":""}"#).unwrap();
    assert!(disabled.global_shortcut.is_empty());
    save_settings(&disabled).unwrap();
    assert!(load_settings().unwrap().global_shortcut.is_empty());
}

#[test]
fn show_subtasks_defaults_off_and_merges_as_a_global_preference() {
    let baseline: Settings = serde_json::from_str("{}").unwrap();
    assert!(!baseline.show_subtasks);
    save_settings(&baseline).unwrap();
    let mut first = baseline.clone();
    first.show_subtasks = true;
    save_settings_changes(&first, &baseline).unwrap();
    let mut second = baseline.clone();
    second.font_size = 24.0;
    let merged = save_settings_changes(&second, &baseline).unwrap();
    assert!(merged.show_subtasks);
    assert_eq!(merged.font_size, 24.0);
    assert!(load_settings().unwrap().show_subtasks);
    let disabled = Settings {
        show_subtasks: false,
        ..merged.clone()
    };
    assert!(
        !save_settings_changes(&disabled, &merged)
            .unwrap()
            .show_subtasks
    );
}

#[test]
fn invalid_settings_json_recovers_on_preference_save() {
    for invalid in ["{", r#"{"font_size":"large"}"#] {
        let root = root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, invalid).unwrap();
        assert!(
            load_settings_from(&path)
                .unwrap_err()
                .contains("could not parse")
        );
        let baseline = Settings::default();
        let mut desired = baseline.clone();
        desired.font_size = 28.0;
        desired.global_shortcut.clear();
        desired.workspace_path = root.join("selected").to_string_lossy().into_owned();
        let merged = save_settings_changes_at(&path, &desired, &baseline).unwrap();
        let saved = load_settings_from(&path).unwrap();
        for recovered in [merged, saved] {
            assert_eq!(
                serde_json::to_value(recovered).unwrap(),
                serde_json::to_value(&desired).unwrap()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn settings_read_io_errors_are_not_treated_as_invalid_json() {
    let root = root();
    let path = root.join("settings.json");
    fs::create_dir_all(&path).unwrap();
    let baseline = Settings::default();
    let mut desired = baseline.clone();
    desired.font_size = 28.0;
    let error = save_settings_changes_at(&path, &desired, &baseline).unwrap_err();
    assert!(error.contains("could not read"), "{error}");
    assert!(path.is_dir());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_merge_waits_for_lock_before_reading_newer_preferences() {
    use std::sync::mpsc;
    use std::time::Duration;

    let root = root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let baseline = Settings::default();
    write_json_atomic(&path, &baseline).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut desired = baseline.clone();
    desired.font_size = 28.0;
    let worker_path = path.clone();
    let worker_baseline = baseline.clone();
    let (started, ready) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        finished
            .send(save_settings_changes_at(
                &worker_path,
                &desired,
                &worker_baseline,
            ))
            .unwrap();
    });
    ready.recv().unwrap();
    let early = result.recv_timeout(Duration::from_millis(200));
    let waited = matches!(early, Err(mpsc::RecvTimeoutError::Timeout));
    let mut newer = baseline;
    newer.show_subtasks = true;
    let manual = vec![Uuid::new_v4(), Uuid::new_v4()];
    newer
        .list_order
        .insert("other workspace".into(), manual.clone());
    write_json_atomic(&path, &newer).unwrap();
    drop(lock);
    let merged = early.unwrap_or_else(|_| result.recv().unwrap()).unwrap();
    worker.join().unwrap();
    let saved = load_settings_from(&path).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(waited, "settings merge ignored another writer's lock");
    for settings in [merged, saved] {
        assert_eq!(settings.font_size, 28.0);
        assert!(settings.show_subtasks);
        assert_eq!(settings.list_order["other workspace"], manual);
    }
}

#[test]
fn settings_merge_preserves_other_workspaces_list_order() {
    let baseline = Settings::default();
    save_settings(&baseline).unwrap();
    let mut first = baseline.clone();
    let first_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    first
        .list_order
        .insert("/workspace/first".into(), first_ids.clone());
    first
        .list_sort
        .insert("/workspace/first".into(), ListSort::Title);
    first
        .task_sort
        .insert("/workspace/first::task-list".into(), TaskSort::Title);
    first
        .manual_task_order
        .insert("/workspace/first::task-list".into(), first_ids.clone());
    save_settings_changes(&first, &baseline).unwrap();
    let mut second = baseline.clone();
    let second_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    second
        .list_order
        .insert("/workspace/second".into(), second_ids.clone());
    second
        .list_sort
        .insert("/workspace/second".into(), ListSort::Newest);
    second.task_sort.insert(
        "/workspace/second::task-list".into(),
        TaskSort::IncompleteFirst,
    );
    second
        .manual_task_order
        .insert("/workspace/second::task-list".into(), second_ids.clone());
    let merged = save_settings_changes(&second, &baseline).unwrap();
    assert_eq!(merged.list_order["/workspace/first"], first_ids);
    assert_eq!(merged.list_order["/workspace/second"], second_ids);
    assert_eq!(merged.list_sort["/workspace/first"], ListSort::Title);
    assert_eq!(merged.list_sort["/workspace/second"], ListSort::Newest);
    assert_eq!(
        merged.task_sort["/workspace/first::task-list"],
        TaskSort::Title
    );
    assert_eq!(
        merged.task_sort["/workspace/second::task-list"],
        TaskSort::IncompleteFirst
    );
    assert_eq!(
        merged.manual_task_order["/workspace/first::task-list"],
        first_ids
    );
    assert_eq!(
        merged.manual_task_order["/workspace/second::task-list"],
        second_ids
    );
    let mut removed = merged.clone();
    removed.list_order.remove("/workspace/first");
    let merged = save_settings_changes(&removed, &merged).unwrap();
    assert!(!merged.list_order.contains_key("/workspace/first"));
    assert_eq!(merged.list_order["/workspace/second"], second_ids);
}
