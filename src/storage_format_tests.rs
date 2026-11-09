//! Shared-file format rejection and destructive-operation compatibility checks.

use crate::model::{Accent, FORMAT_VERSION};
use crate::storage::{
    backups_dir, create_list, delete_list, load_workspace, rename_list_file, save_list,
};
use std::{env, fs, path::PathBuf};
use uuid::Uuid;

fn root() -> PathBuf {
    env::temp_dir().join(format!("minimalist-format-{}", Uuid::new_v4()))
}

fn unsupported_bytes(id: Uuid, version: u32) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "format_version": version,
        "id": id,
        "title": {"future_title": "A differently shaped title"},
        "tasks": "A differently shaped task collection",
        "future_payload": [null, {"preserve": true}]
    }))
    .unwrap()
}

#[test]
fn future_format_is_rejected_before_decoding_changed_known_fields() {
    let root = root();
    let supported = create_list(&root, "Supported", Accent::Mint).unwrap();
    let future = create_list(&root, "Future", Accent::Sky).unwrap();
    let bytes = unsupported_bytes(future.key, 999);
    fs::write(&future.path, &bytes).unwrap();

    let loaded = load_workspace(&root).unwrap();
    assert_eq!(loaded.lists.len(), 1);
    assert_eq!(loaded.lists[0].key, supported.key);
    assert_eq!(loaded.lists[0].data.format_version, FORMAT_VERSION);
    assert_eq!(
        loaded.failed_paths.as_slice(),
        std::slice::from_ref(&future.path)
    );
    assert!(
        loaded.warnings.iter().any(|warning| {
            warning.contains("unsupported list format 999")
                && warning.to_lowercase().contains("update")
        }),
        "{:?}",
        loaded.warnings
    );
    assert_eq!(fs::read(&future.path).unwrap(), bytes);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zero_format_is_rejected_without_initializing_a_replacement_list() {
    let root = root();
    let list = create_list(&root, "Unsupported", Accent::Mint).unwrap();
    let bytes = unsupported_bytes(list.key, 0);
    fs::write(&list.path, &bytes).unwrap();

    let loaded = load_workspace(&root).unwrap();
    assert!(loaded.lists.is_empty());
    assert_eq!(
        loaded.failed_paths.as_slice(),
        std::slice::from_ref(&list.path)
    );
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported list format 0"))
    );
    assert_eq!(fs::read(&list.path).unwrap(), bytes);
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_in_memory_candidates_cannot_replace_a_supported_file() {
    for version in [0, 999] {
        let root = root();
        let mut list = create_list(&root, "Supported", Accent::Mint).unwrap();
        let original = fs::read(&list.path).unwrap();
        list.data.format_version = version;
        list.data.title = "Unsaved candidate".into();

        let error = save_list(&list).unwrap_err();
        assert!(error.contains(&format!("unsupported list format {version}")));
        assert_eq!(fs::read(&list.path).unwrap(), original);
        assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn cached_list_cannot_rename_or_delete_an_unsupported_replacement() {
    let root = root();
    let mut cached = create_list(&root, "Original", Accent::Mint).unwrap();
    let original = cached.path.clone();
    let destination = original.with_file_name(format!("renamed--{}.json", cached.key));
    let backups = backups_dir(&root);
    assert!(!backups.exists());
    let bytes = unsupported_bytes(cached.key, 999);
    fs::write(&original, &bytes).unwrap();
    let loaded = load_workspace(&root).unwrap();
    assert_eq!(
        loaded.failed_paths.as_slice(),
        std::slice::from_ref(&original)
    );
    assert!(loaded.lists.is_empty());

    cached.data.title = "Renamed".into();
    let error = rename_list_file(&mut cached).unwrap_err();
    assert!(error.contains("unsupported list format 999"));
    assert_eq!(cached.path, original);
    assert!(!destination.exists());
    assert_eq!(fs::read(&original).unwrap(), bytes);

    let error = delete_list(&root, &cached).unwrap_err();
    assert!(error.contains("unsupported list format 999"));
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(!backups.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_list_cannot_rename_or_delete_a_different_list_at_the_same_path() {
    let root = root();
    let original = create_list(&root, "Original", Accent::Mint).unwrap();
    let path = original.path.with_file_name("imported.json");
    fs::rename(&original.path, &path).unwrap();
    let mut cached = load_workspace(&root).unwrap().lists.remove(0);
    let replacement = create_list(&root, "Replacement", Accent::Sky).unwrap();
    let bytes = fs::read(&replacement.path).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::remove_file(&replacement.path).unwrap();

    cached.data.title = "Renamed".into();
    let error = rename_list_file(&mut cached).unwrap_err();
    assert!(error.contains("belongs to a different list"));
    let error = delete_list(&root, &cached).unwrap_err();
    assert!(error.contains("belongs to a different list"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(!backups_dir(&root).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn future_duplicates_prevent_deletion_using_filename_or_embedded_identity() {
    for filename_identity in [true, false] {
        let root = root();
        let list = create_list(&root, "Supported", Accent::Mint).unwrap();
        let original = fs::read(&list.path).unwrap();
        let duplicate = if filename_identity {
            root.join("lists")
                .join(format!("future-copy--{}.json", list.key))
        } else {
            root.join("lists").join("Dropbox conflicted copy.json")
        };
        let embedded_id = if filename_identity {
            Uuid::new_v4()
        } else {
            list.key
        };
        let bytes = unsupported_bytes(embedded_id, 999);
        fs::write(&duplicate, &bytes).unwrap();

        let error = delete_list(&root, &list).unwrap_err();
        assert!(error.contains("duplicate list identity"));
        assert_eq!(fs::read(&list.path).unwrap(), original);
        assert_eq!(fs::read(&duplicate).unwrap(), bytes);
        assert!(!backups_dir(&root).exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_future_files_filename_identity_takes_precedence_over_its_embedded_id() {
    let root = root();
    let list = create_list(&root, "Supported", Accent::Mint).unwrap();
    let unrelated = root
        .join("lists")
        .join(format!("future-other--{}.json", Uuid::new_v4()));
    let bytes = unsupported_bytes(list.key, 999);
    fs::write(&unrelated, &bytes).unwrap();

    delete_list(&root, &list).unwrap();
    assert!(!list.path.exists());
    assert_eq!(fs::read(&unrelated).unwrap(), bytes);
    fs::remove_dir_all(backups_dir(&root)).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_one_defaults_and_unknown_fields_survive_saved_edits() {
    let root = root();
    fs::create_dir_all(root.join("lists")).unwrap();
    let path = root.join("lists/11111111-1111-4111-8111-111111111111.json");
    // An older writer can omit optional fields; a compatible newer one can add fields.
    let original = serde_json::json!({
        "format_version": 1,
        "id": "11111111-1111-4111-8111-111111111111",
        "title": "Shared list",
        "created_at_unix": 100,
        "accent": "Mint",
        "future_list": {"nested": ["large integer", "precise decimal", "large exponent"]},
        "tasks": [{
            "id": "22222222-2222-4222-8222-222222222222",
            "text": "Old task without notes or children",
            "completed": false,
            "created_at_unix": 101,
            "future_task": [1, {"keep": "value"}]
        }],
        "archive": [{
            "archived_at_unix": 200,
            "future_archive": null,
            "task": {
                "id": "33333333-3333-4333-8333-333333333333",
                "text": "Archived task",
                "completed": true,
                "created_at_unix": 102,
                "completed_at_unix": null,
                "future_archived_task": {"keep": 42},
                "subtasks": [{
                    "id": "44444444-4444-4444-8444-444444444444",
                    "text": "Child",
                    "completed": true,
                    "future_child": "keep child data"
                }]
            }
        }]
    });
    let original = original
        .to_string()
        .replace("\"large integer\"", "18446744073709551617")
        .replace("\"precise decimal\"", "0.1234567890123456789")
        .replace("\"large exponent\"", "1e+400");
    fs::write(&path, &original).unwrap();
    let snapshot = load_workspace(&root).unwrap();
    assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    let mut list = snapshot.lists.into_iter().next().unwrap();
    assert!(list.data.tasks[0].context.is_empty());
    assert!(list.data.tasks[0].subtasks.is_empty());
    assert_eq!(list.data.tasks[0].completed_at_unix, None);
    list.data.title = "Edited list".into();
    list.data.tasks[0].toggle();
    save_list(&list).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    for number in ["18446744073709551617", "0.1234567890123456789", "1e+400"] {
        assert!(saved.contains(number), "missing exact number {number}");
    }
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let original: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(saved["format_version"], 1);
    assert_eq!(saved["title"], "Edited list");
    assert_eq!(saved["tasks"][0]["completed"], true);
    assert_eq!(saved["future_list"], original["future_list"]);
    assert_eq!(
        saved["tasks"][0]["future_task"],
        original["tasks"][0]["future_task"]
    );
    assert!(
        saved["archive"][0]
            .as_object()
            .unwrap()
            .contains_key("future_archive")
    );
    assert_eq!(
        saved["archive"][0]["future_archive"],
        serde_json::Value::Null
    );
    assert_eq!(saved["archive"][0]["task"], original["archive"][0]["task"]);
    assert!(saved.get("extra").is_none());
    let empty = create_list(&root, "No unknown fields", Accent::Sky).unwrap();
    let empty: serde_json::Value = serde_json::from_slice(&fs::read(empty.path).unwrap()).unwrap();
    assert_eq!(empty.as_object().unwrap().len(), 7);
    fs::remove_dir_all(root).unwrap();
}
