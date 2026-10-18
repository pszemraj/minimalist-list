//! Explicit filename renames remain retryable through failed writes and collisions.

use crate::{
    app::{MinimalistApp, tests::test_app},
    model::{Task, TaskSort},
    storage,
};
use std::{fs, path::PathBuf};
use uuid::Uuid;

fn fixture() -> (MinimalistApp, Uuid, PathBuf, Vec<Uuid>) {
    let (_, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["Zulu", "Alpha"].map(Task::new).to_vec();
    app.lists[0].data.tasks[0].context = "Keep this hidden context".into();
    app.save_list_index(0);
    let original = app.lists[0].path.clone();
    let ids = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
    app.open_settings(Some(list));
    (app, list, original, ids)
}

#[test]
fn failed_initial_rename_write_retries_content_filename_and_fingerprint() {
    let (mut app, list, original, manual) = fixture();
    let root = app.workspace.clone();
    fs::rename(root.join("lists"), root.join("lists-offline")).unwrap();
    fs::write(root.join("lists"), "temporarily blocked").unwrap();
    app.rename_input = "Recovered title".into();
    app.rename_list(list);
    assert!(app.persistence.pending_filenames.contains(&list));
    assert!(app.persistence.dirty_lists.contains(&list));
    assert_eq!(app.lists[0].path, original);
    fs::remove_file(root.join("lists")).unwrap();
    fs::rename(root.join("lists-offline"), root.join("lists")).unwrap();
    assert!(app.retry_dirty_lists());
    assert!(!original.exists());
    assert_eq!(
        app.lists[0].path.file_name().unwrap().to_string_lossy(),
        format!("recovered-title--{list}.json")
    );
    let saved = storage::load_workspace(&root).unwrap();
    assert_eq!(saved.lists.len(), 1);
    assert_eq!(saved.lists[0].data.title, "Recovered title");
    assert_eq!(
        saved.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        manual
    );
    assert_eq!(
        saved.lists[0].data.tasks[0].context,
        "Keep this hidden context"
    );
    assert!(app.persistence.pending_filenames.is_empty());
    assert!(app.persistence.dirty_lists.is_empty());
    assert_eq!(
        app.fingerprint,
        storage::workspace_fingerprint(&root).unwrap()
    );
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn filename_collision_retries_without_duplicates_or_losing_sorted_manual_order() {
    let (mut app, list, original, manual) = fixture();
    let root = app.workspace.clone();
    app.sort_tasks(list, TaskSort::Title);
    let destination = original.with_file_name(format!("renamed--{list}.json"));
    fs::write(&destination, "unrelated existing file").unwrap();
    app.rename_input = "Renamed".into();
    app.rename_list(list);
    assert_eq!(app.lists[0].path, original);
    assert!(app.persistence.pending_filenames.contains(&list));
    assert!(!app.retry_dirty_lists());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "unrelated existing file"
    );
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(saved.title, "Renamed");
    assert_eq!(
        saved.tasks.iter().map(|task| task.id).collect::<Vec<_>>(),
        manual
    );
    app.switch_workspace(&root.join("other").to_string_lossy());
    assert_eq!(app.workspace, root);
    fs::remove_file(&destination).unwrap();
    assert!(app.retry_dirty_lists());
    assert_eq!(app.lists[0].path, destination);
    assert!(!original.exists());
    assert_eq!(app.lists[0].data.tasks[0].text, "Alpha");
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(
        app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        manual
    );
    assert_eq!(
        app.fingerprint,
        storage::workspace_fingerprint(&root).unwrap()
    );
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_legacy_saves_keep_their_path_until_an_explicit_rename() {
    let (mut app, list, descriptive, _) = fixture();
    let root = app.workspace.clone();
    let legacy = descriptive.with_file_name(format!("{list}.json"));
    fs::rename(&descriptive, &legacy).unwrap();
    app.lists[0].path = legacy.clone();
    app.fingerprint = storage::workspace_fingerprint(&root).unwrap();
    app.lists[0].data.tasks[0].text = "Changed task".into();
    assert!(app.save_list_index(0));
    assert_eq!(app.lists[0].path, legacy);
    assert!(legacy.exists());
    assert!(app.persistence.pending_filenames.is_empty());
    app.rename_input = "Explicit new name".into();
    app.rename_list(list);
    assert!(!legacy.exists());
    assert!(
        app.lists[0]
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("explicit-new-name--")
    );
    assert_eq!(fs::read_dir(root.join("lists")).unwrap().count(), 1);
    assert_eq!(
        app.fingerprint,
        storage::workspace_fingerprint(&root).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn context_candidate_save_keeps_a_pending_filename_retry_and_latest_content() {
    let (mut app, list, original, manual) = fixture();
    let root = app.workspace.clone();
    let destination = original.with_file_name(format!("renamed--{list}.json"));
    fs::write(&destination, "unrelated content").unwrap();
    app.rename_input = "Renamed".into();
    app.rename_list(list);
    app.open_task_context(list, manual[0], false);
    app.task_context.as_mut().unwrap().text = "Latest context\nfrom a candidate write".into();
    assert!(app.save_task_context());
    assert!(app.persistence.pending_filenames.contains(&list));
    assert!(app.persistence.dirty_lists.contains(&list));
    assert_eq!(app.lists[0].path, original);
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "unrelated content"
    );
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(
        saved.tasks[0].context,
        "Latest context\nfrom a candidate write"
    );
    fs::remove_file(destination).unwrap();
    assert!(app.retry_dirty_lists());
    let saved = storage::load_workspace(&root).unwrap();
    assert_eq!(saved.lists.len(), 1);
    assert_eq!(saved.lists[0].data.title, "Renamed");
    assert_eq!(
        saved.lists[0].data.tasks[0].context,
        "Latest context\nfrom a candidate write"
    );
    assert!(!original.exists());
    assert_eq!(
        app.fingerprint,
        storage::workspace_fingerprint(&root).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn renaming_a_list_reapplies_title_view_without_changing_manual_tile_order() {
    let (mut app, list, _, _) = fixture();
    let root = app.workspace.clone();
    let other = storage::create_list(&root, "Zulu", crate::model::Accent::Mint).unwrap();
    let other_id = other.key;
    app.lists.push(other);
    app.set_list_sort(crate::model::ListSort::Title);
    app.rename_input = "Zzz renamed".into();
    app.rename_list(list);
    assert_eq!(
        app.lists.iter().map(|item| item.key).collect::<Vec<_>>(),
        [other_id, list]
    );
    app.set_list_sort(crate::model::ListSort::Manual);
    assert_eq!(
        app.lists.iter().map(|item| item.key).collect::<Vec<_>>(),
        [list, other_id]
    );
    let saved = storage::load_workspace(&root).unwrap();
    assert_eq!(
        saved
            .lists
            .iter()
            .find(|item| item.key == list)
            .unwrap()
            .data
            .title,
        "Zzz renamed"
    );
    fs::remove_dir_all(root).unwrap();
}
