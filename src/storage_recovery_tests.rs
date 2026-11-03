//! Preservation of external edits and interrupted or malformed persistence files.

use super::*;
use crate::model::Task;
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
fn repairing_partial_settings_merges_local_edits_with_valid_newer_preferences() {
    let root = root();
    let path = root.join("settings.json");
    let baseline = Settings::default();
    write_json_atomic(&path, &baseline).unwrap();
    let malformed = br#"{"font_size":26,"row_padding":"invalid"}"#;
    fs::write(&path, malformed).unwrap();
    let desired = Settings {
        bold_text: true,
        ..baseline.clone()
    };

    let merged = save_settings_changes_at(&path, &desired, &baseline).unwrap();
    assert_eq!(merged.font_size, 26.0);
    assert_eq!(merged.row_padding, baseline.row_padding);
    assert!(merged.bold_text);
    let (reloaded, warning) = load_settings_from(&path);
    assert!(warning.is_none());
    assert_eq!(reloaded.font_size, 26.0);
    assert!(reloaded.bold_text);
    let backup = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.invalid-")
        })
        .unwrap();
    assert_eq!(fs::read(backup.path()).unwrap(), malformed);
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

#[test]
fn metadata_only_changes_keep_the_loaded_byte_baseline_through_local_edits() {
    let root = root();
    let list = create_list(&root, "Original", Accent::Mint).unwrap();
    // Keep a valid, non-pretty file so comparing against serialization cannot pass.
    let original = serde_json::to_vec(&list.data).unwrap();
    fs::write(&list.path, &original).unwrap();
    let mut local = load_workspace(&root).unwrap().lists.remove(0);
    assert_eq!(
        local.source_bytes.borrow().as_deref(),
        Some(original.as_slice())
    );
    for (second, title) in [(1_000, "First local edit"), (2_000, "Second local edit")] {
        let baseline = fs::read(&local.path).unwrap();
        File::options()
            .write(true)
            .open(&local.path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(second))
            .unwrap();
        local.data.title = title.into();
        assert!(!list_changed_on_disk(&local).unwrap());
        assert_eq!(fs::read(&local.path).unwrap(), baseline);
        save_list(&local).unwrap();
        let saved = fs::read(&local.path).unwrap();
        assert_eq!(
            local.source_bytes.borrow().as_deref(),
            Some(saved.as_slice())
        );
        assert_eq!(load_workspace(&root).unwrap().lists[0].data.title, title);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn matching_metadata_still_compares_contents_before_saving() {
    let root = root();
    let mut local = create_list(&root, "Local", Accent::Mint).unwrap();
    let observed = fs::metadata(&local.path).unwrap().modified().unwrap();
    let mut remote = local.data.clone();
    remote.title = "Other".into();
    let bytes = serde_json::to_vec_pretty(&remote).unwrap();
    fs::write(&local.path, &bytes).unwrap();
    // Coarse timestamps or a sync client that preserves them leave the metadata unchanged.
    File::options()
        .write(true)
        .open(&local.path)
        .unwrap()
        .set_modified(observed)
        .unwrap();
    assert_eq!(
        Some(stamp_from_metadata(&fs::metadata(&local.path).unwrap())),
        local.stamp.get()
    );
    local.data.title = "Local edit".into();
    assert!(list_changed_on_disk(&local).unwrap());
    assert!(save_list(&local).unwrap_err().contains("changed on disk"));
    assert_eq!(fs::read(&local.path).unwrap(), bytes);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn metadata_mismatch_compares_original_bytes_and_rejects_changed_content_of_any_length() {
    for replacement in ["Other", "Remote title with a different length"] {
        let root = root();
        let mut local = create_list(&root, "Local", Accent::Mint).unwrap();
        let baseline = fs::read(&local.path).unwrap();
        let mut remote = local.data.clone();
        remote.title = replacement.into();
        let bytes = serde_json::to_vec_pretty(&remote).unwrap();
        if replacement == "Other" {
            assert_eq!(bytes.len(), baseline.len());
        } else {
            assert_ne!(bytes.len(), baseline.len());
        }
        fs::write(&local.path, &bytes).unwrap();
        File::options()
            .write(true)
            .open(&local.path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_000))
            .unwrap();
        local.data.title = replacement.into();
        // Matching the current local edit is insufficient: the loaded version differed.
        assert!(list_changed_on_disk(&local).unwrap());
        assert!(save_list(&local).unwrap_err().contains("changed on disk"));
        assert_eq!(fs::read(&local.path).unwrap(), bytes);
        assert_eq!(
            local.source_bytes.borrow().as_deref(),
            Some(baseline.as_slice())
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn metadata_only_first_check_does_not_bypass_the_final_content_check() {
    let root = root();
    let mut local = create_list(&root, "Local", Accent::Mint).unwrap();
    File::options()
        .write(true)
        .open(&local.path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_000))
        .unwrap();
    let mut remote = local.data.clone();
    remote.title = "Other".into();
    let bytes = serde_json::to_vec_pretty(&remote).unwrap();
    local.data.title = "Unsaved local changes".into();
    let checks = Cell::new(0);
    let result = write_json_atomic_checked(&local.path, &local.data, || {
        if checks.replace(checks.get() + 1) == 1 {
            fs::write(&local.path, &bytes).unwrap();
            File::options()
                .write(true)
                .open(&local.path)
                .unwrap()
                .set_modified(UNIX_EPOCH + Duration::from_secs(2_000))
                .unwrap();
        }
        if list_changed_on_disk(&local)? {
            Err("external content changed".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(checks.get(), 2);
    assert!(result.unwrap_err().contains("external content changed"));
    assert_eq!(fs::read(&local.path).unwrap(), bytes);
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

fn numeric_snapshots(directory: &Path) -> Vec<u64> {
    let mut times = fs::read_dir(directory)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_suffix(".json")?
                .parse()
                .ok()
        })
        .collect::<Vec<u64>>();
    times.sort_unstable();
    times
}

#[test]
fn workspace_keys_stay_byte_identical_across_releases() {
    assert_eq!(
        workspace_key(Path::new("/Users/test/workspace")),
        "81ba70295b23e18d"
    );
    assert_eq!(
        workspace_key(Path::new("/tmp/Minimalist List/lists ü")),
        "42a036088b230b2c"
    );
}

#[test]
fn snapshots_are_written_at_most_hourly_even_when_the_clock_moves_back() {
    let directory = root();
    let base = 1_700_000_000;
    backup::snapshot_into(&directory, b"first", base, false).unwrap();
    backup::snapshot_into(&directory, b"too soon", base + 3_599, false).unwrap();
    assert_eq!(numeric_snapshots(&directory), [base]);
    assert_eq!(
        fs::read(directory.join(format!("{base}.json"))).unwrap(),
        b"first"
    );
    backup::snapshot_into(&directory, b"second", base + 3_600, false).unwrap();
    assert_eq!(numeric_snapshots(&directory), [base, base + 3_600]);
    // A newest snapshot dated after the current time cannot suppress backups.
    backup::snapshot_into(&directory, b"earlier clock", base + 10, false).unwrap();
    assert_eq!(
        numeric_snapshots(&directory),
        [base, base + 10, base + 3_600]
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn forced_snapshots_bypass_the_hourly_limit_without_replacing_earlier_ones() {
    let directory = root();
    let base = 1_700_000_000;
    backup::snapshot_into(&directory, b"first", base, false).unwrap();
    backup::snapshot_into(&directory, b"forced", base + 5, true).unwrap();
    // A second forced snapshot in the same second takes the next free name.
    backup::snapshot_into(&directory, b"same second", base + 5, true).unwrap();
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 3);
    for (name, bytes) in [
        (format!("{base}.json"), "first"),
        (format!("{}.json", base + 5), "forced"),
        (format!("{}-1.json", base + 5), "same second"),
    ] {
        assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), bytes);
    }
    for index in 0..105 {
        backup::snapshot_into(&directory, b"{}", base + 10 + index, true).unwrap();
    }
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 100);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_same_second_history_snapshot_survives_the_next_ordinary_saves() {
    let directory = root();
    fs::create_dir_all(&directory).unwrap();
    let now = 1_700_000_000;
    for stamp in now - 99..=now {
        fs::write(directory.join(format!("{stamp}.json")), b"Older version").unwrap();
    }

    backup::snapshot_into(&directory, b"History before clearing", now, true).unwrap();
    backup::snapshot_into(&directory, b"History before clearing", now, false).unwrap();
    backup::snapshot_into(&directory, b"Cleared History", now + 1, false).unwrap();

    assert_eq!(fs::read_dir(&directory).unwrap().count(), 100);
    assert_eq!(
        fs::read(directory.join(format!("{now}-1.json"))).unwrap(),
        b"History before clearing"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn snapshot_list_now_copies_the_observed_bytes_right_after_another_snapshot() {
    let root = root();
    let list = create_list(&root, "Tasks", Accent::Mint).unwrap();
    let directory = backups_dir(&root)
        .join("snapshots")
        .join(list.key.to_string());
    snapshot_list(&root, &list).unwrap();
    snapshot_list(&root, &list).unwrap();
    assert_eq!(numeric_snapshots(&directory).len(), 1);
    snapshot_list_now(&root, &list).unwrap();
    let kept = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(kept.len(), 2);
    let observed = list.source_bytes.borrow().clone().unwrap();
    for path in kept {
        assert_eq!(fs::read(path).unwrap(), &*observed);
    }
    fs::remove_dir_all(backups_dir(&root)).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshots_are_capped_at_one_hundred_and_other_files_are_untouched() {
    let directory = root();
    fs::create_dir_all(directory.join("7.json")).unwrap();
    let foreign = [
        "notes.txt",
        "backup.json",
        "12ab.json",
        "5.json.bak",
        ".5.json.tmp",
    ];
    for name in foreign {
        fs::write(directory.join(name), name).unwrap();
    }
    let base = 1_700_000_000;
    for index in 0..105 {
        backup::snapshot_into(&directory, b"{}", base + index * 3_600, false).unwrap();
    }
    let kept = numeric_snapshots(&directory);
    assert_eq!(kept.len(), 100);
    assert_eq!(kept[0], base + 5 * 3_600);
    assert_eq!(kept[99], base + 104 * 3_600);
    for name in foreign {
        assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), name);
    }
    assert!(directory.join("7.json").is_dir());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_deleted_list_is_kept_in_backups_and_restores_by_copying_it_back() {
    let root = root();
    create_list(&root, "Keep", Accent::Mint).unwrap();
    let doomed = create_list(&root, "Errands", Accent::Sky).unwrap();
    let backups = backups_dir(&root);
    let contents = fs::read(&doomed.path).unwrap();
    delete_list(&root, &doomed).unwrap();
    assert!(!doomed.path.exists());
    assert_eq!(load_workspace(&root).unwrap().lists.len(), 1);
    let deleted = backups.join("deleted");
    let kept = fs::read_dir(&deleted)
        .unwrap()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(kept.len(), 1);
    let name = kept[0].file_name().into_string().unwrap();
    let (stamp, original_name) = name.split_once('-').unwrap();
    assert!(stamp.parse::<u64>().is_ok());
    assert_eq!(original_name, doomed.path.file_name().unwrap());
    assert_eq!(fs::read(kept[0].path()).unwrap(), contents);
    // The copy loads with its original identity, with or without its timestamp prefix.
    let lists = root.join("lists");
    fs::copy(kept[0].path(), lists.join(&name)).unwrap();
    let loaded = load_workspace(&root).unwrap();
    let restored = loaded.lists.iter().find(|item| item.key == doomed.key);
    assert_eq!(restored.unwrap().data.title, "Errands");
    fs::remove_file(lists.join(&name)).unwrap();
    fs::copy(kept[0].path(), &doomed.path).unwrap();
    let loaded = load_workspace(&root).unwrap();
    let restored = loaded
        .lists
        .iter()
        .find(|item| item.key == doomed.key)
        .unwrap();
    assert_eq!(restored.path, doomed.path);
    // Deleting it again never replaces the earlier copy.
    delete_list(&root, restored).unwrap();
    assert_eq!(fs::read_dir(&deleted).unwrap().count(), 2);
    assert_eq!(fs::read(kept[0].path()).unwrap(), contents);
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_list_leaves_a_replacement_directory_in_the_workspace() {
    let root = root();
    let list = create_list(&root, "Tasks", Accent::Mint).unwrap();
    fs::remove_file(&list.path).unwrap();
    fs::create_dir(&list.path).unwrap();
    let sentinel = list.path.join("other-data");
    fs::write(&sentinel, "keep this").unwrap();

    let result = delete_list(&root, &list);
    let contents = fs::read_to_string(sentinel);
    let backups = backups_dir(&root);
    if backups.exists() {
        fs::remove_dir_all(backups).unwrap();
    }
    fs::remove_dir_all(root).unwrap();

    assert!(result.is_err(), "deletion accepted a directory");
    assert_eq!(contents.unwrap(), "keep this");
}

#[test]
fn deleted_long_filename_restores_its_identity_and_bytes() {
    for length in [157, 207] {
        let root = root();
        let list = create_list(&root, "Long filename", Accent::Mint).unwrap();
        let id = Uuid::new_v4();
        let path = list
            .path
            .with_file_name(format!("{}--{id}.json", "x".repeat(length)));
        fs::rename(&list.path, &path).unwrap();
        let contents = fs::read(&path).unwrap();
        let loaded = load_workspace(&root).unwrap().lists.remove(0);
        assert_eq!(loaded.key, id);

        delete_list(&root, &loaded).unwrap();
        assert!(!path.exists());
        let backups = backups_dir(&root);
        let backup = fs::read_dir(backups.join("deleted"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(backup.path()).unwrap(), contents);
        fs::copy(backup.path(), root.join("lists").join(backup.file_name())).unwrap();
        let mut restored = load_workspace(&root).unwrap().lists.remove(0);
        assert_eq!(restored.key, id);
        assert_eq!(restored.data.title, list.data.title);
        restored.data.tasks.push(Task::new("Edited after restore"));
        save_list(&restored).unwrap();
        let saved = load_workspace(&root).unwrap().lists.remove(0);
        assert_eq!(saved.data.tasks[0].text, "Edited after restore");
        fs::remove_dir_all(backups).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_list_restored_under_an_older_long_backup_name_is_writable() {
    let root = root();
    let list = create_list(&root, "Restored", Accent::Mint).unwrap();
    let restored =
        list.path
            .with_file_name(format!("1700000000-{}--{}.json", "x".repeat(157), list.key));
    fs::rename(&list.path, &restored).unwrap();
    let mut loaded = load_workspace(&root).unwrap().lists.remove(0);
    loaded.data.tasks.push(Task::new("Edited after restore"));
    save_list(&loaded).unwrap();
    let saved = load_workspace(&root).unwrap().lists.remove(0);
    assert_eq!(saved.key, list.key);
    assert_eq!(saved.data.tasks[0].text, "Edited after restore");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleted_backup_preserves_the_loaded_identity_of_an_external_name() {
    let root = root();
    let keep = create_list(&root, "Keep", Accent::Mint).unwrap();
    let remove = create_list(&root, "Remove and restore", Accent::Sky).unwrap();
    let path = remove.path.with_file_name(format!("--{}.json", keep.key));
    fs::rename(&remove.path, &path).unwrap();
    let (loaded, _) = read_list(&path).unwrap();
    assert_eq!(loaded.key, remove.key);

    delete_list(&root, &loaded).unwrap();
    let backups = backups_dir(&root);
    let backup = fs::read_dir(backups.join("deleted"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    fs::copy(backup.path(), root.join("lists").join(backup.file_name())).unwrap();
    let restored = load_workspace(&root).unwrap();
    assert!(restored.warnings.is_empty());
    assert!(restored.lists.iter().any(|list| list.key == remove.key));
    assert!(restored.lists.iter().any(|list| list.key == keep.key));
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleted_uuid_only_list_restores_with_its_filename_identity() {
    let root = root();
    let keep = create_list(&root, "Keep", Accent::Mint).unwrap();
    let mut list = create_list(&root, "Legacy", Accent::Sky).unwrap();
    let legacy_path = list.path.with_file_name(format!("{}.json", list.key));
    fs::rename(&list.path, &legacy_path).unwrap();
    list.path = legacy_path;
    list.data.id = keep.key;
    save_list(&list).unwrap();
    let contents = fs::read(&list.path).unwrap();
    let (loaded, _) = read_list(&list.path).unwrap();
    assert_eq!(loaded.key, list.key);
    delete_list(&root, &loaded).unwrap();

    let backups = backups_dir(&root);
    let backup = fs::read_dir(backups.join("deleted"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let backup_name = backup.file_name().into_string().unwrap();
    let (_, descriptive_name) = backup_name.split_once('-').unwrap();
    assert_eq!(fs::read(backup.path()).unwrap(), contents);
    assert!(!list.path.exists());
    for name in [backup_name.as_str(), descriptive_name] {
        let restored_path = root.join("lists").join(name);
        fs::copy(backup.path(), &restored_path).unwrap();
        let snapshot = load_workspace(&root).unwrap();
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
        let restored = snapshot
            .lists
            .iter()
            .find(|entry| entry.path == restored_path)
            .unwrap();
        assert_eq!(restored.key, list.key);
        assert_eq!(restored.data.title, "Legacy");
        fs::remove_file(restored_path).unwrap();
    }
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn deleted_relative_symlink_leaves_a_readable_backup_and_preserves_the_target() {
    let root = root();
    create_list(&root, "Keep", Accent::Mint).unwrap();
    let list = create_list(&root, "Linked", Accent::Sky).unwrap();
    let target = root.join("linked.json");
    fs::rename(&list.path, &target).unwrap();
    std::os::unix::fs::symlink("../linked.json", &list.path).unwrap();
    let contents = fs::read(&target).unwrap();
    let (loaded, _) = read_list(&list.path).unwrap();
    delete_list(&root, &loaded).unwrap();

    let backups = backups_dir(&root);
    let backup = fs::read_dir(backups.join("deleted"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert!(backup.file_type().unwrap().is_file());
    assert_eq!(fs::read(backup.path()).unwrap(), contents);
    assert!(fs::symlink_metadata(&list.path).is_err());
    assert_eq!(fs::read(&target).unwrap(), contents);
    fs::copy(backup.path(), root.join("lists").join(backup.file_name())).unwrap();
    let restored = load_workspace(&root)
        .unwrap()
        .lists
        .into_iter()
        .find(|entry| entry.key == list.key)
        .unwrap();
    assert_eq!(restored.data.title, "Linked");
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_malformed_setting_keeps_every_other_preference() {
    let root = root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let stored = r#"{"workspace_path":"/kept/workspace","font_size":"huge","bold_text":true,"always_on_top":true,"global_shortcut":"","extra":1}"#;
    fs::write(&path, stored).unwrap();
    let (settings, warning) = load_settings_from(&path);
    assert_eq!(settings.workspace_path, "/kept/workspace");
    assert_eq!(settings.font_size, Settings::default().font_size);
    assert!(settings.bold_text && settings.always_on_top);
    assert!(settings.global_shortcut.is_empty());
    let warning = warning.unwrap();
    assert!(warning.contains("could not parse") && warning.contains("font_size"));
    // The next preference save still preserves the original file before replacing it.
    let mut desired = settings.clone();
    desired.row_padding = 9.0;
    save_settings_changes_at(&path, &desired, &settings).unwrap();
    let preserved = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.invalid-")
        })
        .collect::<Vec<_>>();
    assert_eq!(preserved.len(), 1);
    assert_eq!(fs::read_to_string(preserved[0].path()).unwrap(), stored);
    let (saved, warning) = load_settings_from(&path);
    assert!(warning.is_none());
    assert_eq!(saved.workspace_path, "/kept/workspace");
    assert_eq!(saved.row_padding, 9.0);
    assert!(saved.bold_text);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unparseable_settings_load_defaults_with_a_warning() {
    let root = root();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let defaults = serde_json::to_value(Settings::default()).unwrap();
    for stored in ["{ broken", "[1, 2]", "null", ""] {
        fs::write(&path, stored).unwrap();
        let (settings, warning) = load_settings_from(&path);
        assert_eq!(serde_json::to_value(settings).unwrap(), defaults);
        assert!(warning.unwrap().contains("could not parse"));
    }
    fs::remove_dir_all(root).unwrap();
}
