//! Preservation of external edits and interrupted or malformed persistence files.

use super::*;
use std::time::Duration;

fn root() -> PathBuf {
    env::temp_dir().join(format!("minimalist-recovery-{}", Uuid::new_v4()))
}

#[test]
fn a_stale_list_save_keeps_external_contents_even_when_length_is_unchanged() {
    let root = root();
    let mut local = create_list(&root, "Local", Accent::Mint).unwrap();
    let mut external = load_workspace(&root).unwrap().lists.remove(0);
    external.data.title = "Other".into();
    save_list(&external).unwrap();
    let contents = fs::read(&external.path).unwrap();
    let timestamp = UNIX_EPOCH + Duration::from_secs(1_000);
    File::options()
        .write(true)
        .open(&external.path)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    local.data.title = "Edits".into();
    let error = save_list(&local).unwrap_err();
    assert!(error.contains("changed on disk"));
    assert_eq!(fs::read(&external.path).unwrap(), contents);
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    let mut fresh = load_workspace(&root).unwrap().lists.remove(0);
    fresh.data.title = "Saved after reload".into();
    save_list(&fresh).unwrap();
    fresh.data.title = "Next local edit".into();
    save_list(&fresh).unwrap();
    assert_eq!(
        load_workspace(&root).unwrap().lists[0].data.title,
        "Next local edit"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleting_a_list_externally_does_not_allow_a_stale_save_to_recreate_it() {
    let root = root();
    let list = create_list(&root, "Tasks", Accent::Mint).unwrap();
    fs::remove_file(&list.path).unwrap();
    assert!(save_list(&list).unwrap_err().contains("changed on disk"));
    assert!(!list.path.exists());
    fs::remove_dir(root.join("lists")).unwrap();
    assert!(save_list(&list).unwrap_err().contains("changed on disk"));
    assert!(!root.join("lists").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_write_rechecks_before_replacing_and_removes_failed_temporary_file() {
    let root = root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("list.json");
    fs::write(&path, "original").unwrap();
    let checked = Cell::new(false);
    let result = write_json_atomic_checked(&path, &"local edit", || {
        if checked.replace(true) {
            fs::write(&path, "newer external edit").unwrap();
            return Err("changed while preparing the write".into());
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "newer external edit");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unchanged_settings_keep_the_same_file_and_merge_other_instance_values() {
    let root = root();
    let path = root.join("settings.json");
    let baseline = Settings::default();
    write_json_atomic(&path, &baseline).unwrap();
    let mut newer = baseline.clone();
    newer.font_size = 26.0;
    write_json_atomic(&path, &newer).unwrap();
    let file = File::options().write(true).open(&path).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs(1_000))
        .unwrap();
    let before = file.metadata().unwrap().modified().unwrap();
    let bytes = fs::read(&path).unwrap();
    let merged = save_settings_changes_at(&path, &baseline, &baseline).unwrap();
    assert_eq!(merged.font_size, 26.0);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_settings_are_preserved_before_recovery_and_old_manual_ids_are_removed() {
    let root = root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let malformed = b"{ personal preferences needing repair";
    fs::write(&path, malformed).unwrap();
    let settings = Settings::default();
    save_settings_changes_at(&path, &settings, &settings).unwrap();
    let backups = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.invalid-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(backups[0].path()).unwrap(), malformed);
    let mut legacy = settings.clone();
    legacy
        .manual_task_order
        .insert("old workspace::list".into(), vec![Uuid::new_v4()]);
    write_json_atomic(&path, &legacy).unwrap();
    let merged = save_settings_changes_at(&path, &legacy, &legacy).unwrap();
    assert!(merged.manual_task_order.is_empty());
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains("manual_task_order")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_loading_cleans_only_stale_unlocked_owned_temporary_files() {
    let root = root();
    let list = create_list(&root, "Tasks", Accent::Mint).unwrap();
    let directory = root.join("lists");
    let name = list.path.file_name().unwrap().to_string_lossy();
    let temporary = || directory.join(format!(".{name}.tmp-123-{}", Uuid::new_v4()));
    let stale = temporary();
    let fresh = temporary();
    let locked = temporary();
    let unrelated = directory.join(".notes.json.tmp-123-not-an-app-uuid");
    for path in [&stale, &fresh, &locked, &unrelated] {
        fs::write(path, "interrupted write").unwrap();
    }
    for path in [&stale, &locked, &unrelated] {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_000))
            .unwrap();
    }
    let active_write = File::options().write(true).open(&locked).unwrap();
    active_write.lock().unwrap();
    let snapshot = load_workspace(&root).unwrap();
    assert_eq!(snapshot.lists.len(), 1);
    assert!(!stale.exists());
    assert!(fresh.exists() && locked.exists() && unrelated.exists());
    drop(active_write);
    cleanup_stale_temps(&directory);
    assert!(!locked.exists());
    fs::remove_dir_all(root).unwrap();
}
