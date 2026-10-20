//! Persistence and measured-layout regressions for task operations.

use super::*;
use crate::{
    app::tests::test_app,
    model::{Accent, Task, TodoList},
    storage,
};
use std::{
    fs::{self, File, FileTimes},
    time::{Duration, UNIX_EPOCH},
};

fn saved(app: &MinimalistApp, index: usize) -> TodoList {
    serde_json::from_slice(&fs::read(&app.lists[index].path).unwrap()).unwrap()
}

#[test]
fn archive_commits_completed_editor_and_retires_its_checklist() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Before");
    task.context = "Keep archived context".into();
    task.toggle();
    task.subtasks.push(Subtask::new("Checklist"));
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.editing = Some((list, id));
    app.edit_text = "After".into();
    app.subtasks_for = Some((list, id));
    app.begin_clear_completed(list);
    app.archive_completed(list);
    assert!(app.editing.is_none());
    assert!(app.subtasks_for.is_none());
    assert_eq!(saved(&app, 0).archive[0].task.text, "After");
    assert_eq!(
        saved(&app, 0).archive[0].task.context,
        "Keep archived context"
    );
    assert_eq!(saved(&app, 0).archive[0].task.subtasks[0].text, "Checklist");
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn task_and_child_undo_restore_identity_content_completion_and_position() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut middle = Task::new("Keep all content");
    middle.context = "Preserve hidden context\nand links".into();
    middle.toggle();
    middle.subtasks = ["First", "Middle", "Last"].map(Subtask::new).to_vec();
    middle.subtasks[1].completed = true;
    let original = serde_json::to_value(&middle).unwrap();
    let id = middle.id;
    let child = middle.subtasks[1].id;
    app.lists[0].data.tasks = vec![Task::new("Before"), middle, Task::new("After")];
    app.remove_task(list, id);
    assert_eq!(saved(&app, 0).tasks.len(), 2);
    app.undo_delete();
    assert_eq!(
        serde_json::to_value(&saved(&app, 0).tasks[1]).unwrap(),
        original
    );
    app.delete_subtask(list, id, child);
    assert_eq!(saved(&app, 0).tasks[1].subtasks.len(), 2);
    app.undo_delete();
    assert_eq!(
        serde_json::to_value(&saved(&app, 0).tasks[1]).unwrap(),
        original
    );
    app.undo_delete();
    assert_eq!(saved(&app, 0).tasks.len(), 3);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn undo_keeps_a_deleted_task_whose_identity_reappears_elsewhere() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Deleted here, stale elsewhere");
    let id = task.id;
    app.lists[0].data.tasks.push(task.clone());
    app.remove_task(list, id);
    let other = storage::create_list(&app.workspace, "Other", Accent::Sky).unwrap();
    app.lists.push(other);
    // A sync reload can bring back an older copy in another list.
    app.lists[1].data.tasks.push(task);
    app.undo_delete();
    assert_eq!(
        app.status.as_deref(),
        Some("The deleted task already exists in the workspace.")
    );
    assert!(app.details.deleted.is_some(), "Undo must remain available");
    app.lists[1].data.tasks.clear();
    app.undo_delete();
    assert!(app.details.deleted.is_none());
    assert_eq!(
        saved(&app, 0)
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        [id]
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn undo_keeps_a_deleted_subtask_whose_identity_reappears() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks = vec![Subtask::new("Deleted version")];
    let (parent, child) = (task.id, task.subtasks[0].id);
    let mut stale = task.subtasks[0].clone();
    stale.text = "Stale version".into();
    app.lists[0].data.tasks.push(task);
    app.delete_subtask(list, parent, child);
    // A sync reload can bring back an older copy of the same child.
    app.lists[0].data.tasks[0].subtasks.push(stale);
    app.undo_delete();
    assert_eq!(
        app.status.as_deref(),
        Some("The deleted subtask already exists in its task.")
    );
    assert!(app.details.deleted.is_some(), "Undo must remain available");
    assert_eq!(app.lists[0].data.tasks[0].subtasks[0].text, "Stale version");
    app.lists[0].data.tasks[0].subtasks.clear();
    app.undo_delete();
    assert!(app.details.deleted.is_none());
    assert_eq!(saved(&app, 0).tasks[0].subtasks[0].text, "Deleted version");
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn editing_completed_middle_child_preserves_identity_state_and_neighbors() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks = ["First", "Typo", "Last"].map(Subtask::new).to_vec();
    task.subtasks[1].completed = true;
    let id = task.id;
    let child = task.subtasks[1].id;
    let neighbors = (task.subtasks[0].id, task.subtasks[2].id);
    app.lists[0].data.tasks.push(task);
    app.begin_subtask_edit(list, id, child);
    app.details.editor.as_mut().unwrap().text = "Corrected\nwith details".into();
    app.finish_subtask_edit();
    let task = &saved(&app, 0).tasks[0];
    assert_eq!((task.subtasks[0].id, task.subtasks[2].id), neighbors);
    assert_eq!(task.subtasks[1].id, child);
    assert!(task.subtasks[1].completed);
    assert_eq!(task.subtasks[1].text, "Corrected\nwith details");
    app.begin_subtask_edit(list, id, child);
    app.details.editor.as_mut().unwrap().text = "Discarded".into();
    app.cancel_subtask_edit();
    assert_eq!(
        saved(&app, 0).tasks[0].subtasks[1].text,
        "Corrected\nwith details"
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn move_keeps_one_copy_and_preserves_entire_task() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let destination = storage::create_list(&app.workspace, "Destination", Accent::Mint).unwrap();
    let dest_id = destination.key;
    app.lists.push(destination);
    let mut task = Task::new("Move me");
    task.context = "Move supporting details intact".into();
    task.toggle();
    task.subtasks.push(Subtask::new("Child"));
    task.subtasks[0].completed = true;
    let original = serde_json::to_value(&task).unwrap();
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    assert!(app.move_task_to_list(source, id, dest_id));
    assert!(saved(&app, 0).tasks.is_empty());
    assert_eq!(
        serde_json::to_value(&saved(&app, 1).tasks[0]).unwrap(),
        original
    );
    assert!(!app.move_task_to_list(source, id, dest_id));
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn unavailable_move_destination_keeps_source_and_retry_does_not_duplicate() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let destination = storage::create_list(&app.workspace, "Destination", Accent::Mint).unwrap();
    let destination_id = destination.key;
    app.lists.push(destination);
    let task = Task::new("Keep recoverable");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    let original_path = app.lists[1].path.clone();
    let backup_path = app.workspace.join("destination-backup.json");
    // A directory where a file belongs fails deterministically without OS permission assumptions.
    fs::rename(&original_path, &backup_path).unwrap();
    fs::create_dir(&original_path).unwrap();
    assert!(!app.move_task_to_list(source, id, destination_id));
    assert_eq!(saved(&app, 0).tasks[0].id, id);
    assert!(app.lists[1].data.tasks.is_empty());
    fs::remove_dir(&original_path).unwrap();
    fs::rename(&backup_path, &original_path).unwrap();
    assert!(app.move_task_to_list(source, id, destination_id));
    assert!(saved(&app, 0).tasks.is_empty());
    assert_eq!(saved(&app, 1).tasks.len(), 1);
    assert_eq!(saved(&app, 1).tasks[0].id, id);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn history_and_checklists_measure_multiline_content_and_reserve_actions() {
    for width in [320.0, 520.0] {
        let (ctx, mut app) = test_app();
        app.settings.font_size = 30.0;
        app.settings.font = crate::model::FontChoice::Mono;
        let list = app.lists[0].key;
        let mut task = Task::new("Parent");
        task.subtasks.push(Subtask::new(
            "Long child with multiple lines and a long word ".repeat(15)
                + "\nOne more line\nAnd another",
        ));
        let archived: Vec<_> = (0..30)
            .map(|_| {
                ArchivedTask::new(Task::new(
                    "Long archived title ".repeat(15) + "\nExtra line",
                ))
            })
            .collect();
        let last_id = archived.last().unwrap().task.id;
        app.lists[0].data.archive = archived;
        let palette = app.palette();
        let mut panel_rect = egui::Rect::NOTHING;
        let mut history_rect = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 40000.0),
                )),
                ..Default::default()
            },
            |ui| {
                panel_rect = ui
                    .scope(|ui| app.subtasks_panel(ui, list, &task, palette, 1.0))
                    .response
                    .rect;
                history_rect = ui
                    .scope(|ui| app.history_ui(ui, list, palette, 1.0))
                    .response
                    .rect;
            },
        );
        output.textures_delta.clear();
        let entry = ctx
            .read_response(Id::new(("new-subtask", list, task.id)))
            .unwrap();
        assert!(panel_rect.contains_rect(entry.rect));
        assert!(
            entry.rect.right() <= width,
            "entry escaped narrow panel: {:?}",
            entry.rect
        );
        let restore = ctx
            .read_response(FocusItem::Archived(list, last_id).id())
            .unwrap();
        assert!(history_rect.contains_rect(restore.rect));
        assert!(
            restore.rect.right() <= width,
            "Restore escaped narrow panel: {:?}",
            restore.rect
        );
        assert!(restore.rect.bottom() <= ctx.content_rect().bottom());
        fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn inspecting_archived_children_does_not_restore_or_rewrite_state() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Archived parent");
    task.toggle();
    task.subtasks.push(Subtask::new("Distinctive child"));
    let id = task.id;
    app.lists[0].data.archive.push(ArchivedTask::new(task));
    app.save_list_index(0);
    let before = fs::read(&app.lists[0].path).unwrap();
    app.expand_archived(list, id);
    let palette = app.palette();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        app.history_ui(ui, list, palette, 1.0)
    });
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("Distinctive child"))));
    output.textures_delta.clear();
    assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
    assert!(app.lists[0].data.tasks.is_empty());
    assert!(app.lists[0].data.archive[0].task.completed);
    assert!(!app.lists[0].data.archive[0].task.subtasks[0].completed);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn failed_source_save_rolls_back_the_already_written_destination() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let destination = storage::create_list(&app.workspace, "Destination", Accent::Mint).unwrap();
    let destination_id = destination.key;
    app.lists.push(destination);
    let task = Task::new("Survive partial move");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    let source_path = app.lists[0].path.clone();
    let backup_path = app.workspace.join("source-backup.json");
    fs::rename(&source_path, &backup_path).unwrap();
    fs::create_dir(&source_path).unwrap();
    assert!(!app.move_task_to_list(source, id, destination_id));
    assert!(
        saved(&app, 1).tasks.is_empty(),
        "destination copy must roll back after the source write fails"
    );
    assert_eq!(app.lists[0].data.tasks[0].id, id);
    fs::remove_dir(&source_path).unwrap();
    fs::rename(&backup_path, &source_path).unwrap();
    assert!(app.retry_dirty_lists());
    assert_eq!(saved(&app, 0).tasks[0].id, id);
    assert!(saved(&app, 1).tasks.is_empty());
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clearing_completed_only_writes_when_tasks_are_archived() {
    let (_ctx, mut app) = test_app();
    let list_id = app.lists[0].key;
    let active = Task::new("Keep active");
    let active_id = active.id;
    app.lists[0].data.tasks.push(active);
    app.save_list_index(0);
    let path = app.lists[0].path.clone();
    let original = fs::read(&path).unwrap();
    // A fixed old timestamp detects atomic rewrites without relying on clock resolution.
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)))
        .unwrap();
    // Observe the fixture timestamp as the current disk baseline before editing.
    app.lists = storage::load_workspace(&app.workspace).unwrap().lists;
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    for _ in 0..2 {
        app.archive_completed(list_id);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert!(app.lists[0].data.archive.is_empty());
        assert_eq!(app.lists[0].data.tasks[0].id, active_id);
    }

    let mut completed = Task::new("Move to History");
    completed.toggle();
    completed.subtasks.push(Subtask::new("Keep checklist"));
    let completed_id = completed.id;
    app.lists[0].data.tasks.push(completed);
    app.archive_completed(list_id);

    let saved: TodoList = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved.tasks.len(), 1);
    assert_eq!(saved.tasks[0].id, active_id);
    assert_eq!(saved.archive.len(), 1);
    assert_eq!(saved.archive[0].task.id, completed_id);
    assert!(saved.archive[0].task.completed);
    assert_eq!(saved.archive[0].task.subtasks[0].text, "Keep checklist");
    assert_ne!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn moving_a_task_carries_its_unfinished_checklist_draft_to_the_destination() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let destination = storage::create_list(&app.workspace, "Destination", Accent::Mint).unwrap();
    let destination_id = destination.key;
    app.lists.push(destination);
    let task = Task::new("Move with draft");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.switch_subtask_panel(Some((source, id)));
    app.new_subtask = "Unfinished child in original list".into();
    assert!(app.move_task_to_list(source, id, destination_id));
    app.switch_subtask_panel(Some((destination_id, id)));
    assert_eq!(app.new_subtask, "Unfinished child in original list");
    app.add_subtask(destination_id, id);
    assert!(saved(&app, 0).tasks.is_empty());
    assert_eq!(
        saved(&app, 1).tasks[0].subtasks[0].text,
        "Unfinished child in original list"
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn undo_from_another_list_preserves_its_open_checklist_and_draft() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let mut parent = Task::new("Original parent");
    parent.subtasks.push(Subtask::new("Restore me"));
    let original_parent = parent.id;
    let child = parent.subtasks[0].id;
    app.lists[0].data.tasks.push(parent);
    app.delete_subtask(source, original_parent, child);
    let mut destination = storage::create_list(&app.workspace, "Other", Accent::Mint).unwrap();
    let current_parent = Task::new("Current parent");
    let current_parent_id = current_parent.id;
    destination.data.tasks.push(current_parent);
    let current = destination.key;
    app.lists.push(destination);
    app.open_list(current);
    app.switch_subtask_panel(Some((current, current_parent_id)));
    app.new_subtask = "Current checklist draft".into();
    app.undo_delete();
    assert_eq!(app.subtasks_for, Some((current, current_parent_id)));
    assert_eq!(app.new_subtask, "Current checklist draft");
    assert_eq!(saved(&app, 0).tasks[0].subtasks[0].id, child);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn startup_list_directory_failure_keeps_selected_workspace_and_exposes_retry() {
    let (ctx, app) = test_app();
    let selected = app.workspace.join("unavailable-lists");
    fs::create_dir_all(&selected).unwrap();
    fs::write(selected.join("lists"), b"not a directory").unwrap();
    let selected_text = selected.to_string_lossy().into_owned();
    let settings = storage::Settings {
        workspace_path: selected_text.clone(),
        ..Default::default()
    };
    let recovered = MinimalistApp::new(&ctx, settings, None, None);
    assert_eq!(recovered.persisted_workspace_path, selected_text);
    assert!(
        recovered.persistence.workspace_failure.is_some(),
        "a failed list-directory read needs the same Retry recovery as a failed workspace-directory open"
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn move_picker_keeps_exclusive_input_ownership_across_capture_search_shortcuts() {
    for key in [egui::Key::N, egui::Key::F] {
        let (ctx, mut app) = test_app();
        let source = app.lists[0].key;
        let destination = storage::create_list(&app.workspace, "Other", Accent::Mint).unwrap();
        app.lists.push(destination);
        let task = Task::new("Moving");
        let id = task.id;
        app.lists[0].data.tasks.push(task);
        app.begin_move_task(source, id);
        let input = || egui::RawInput {
            events: crate::app::tests::chord(key, egui::Modifiers::COMMAND),
            ..Default::default()
        };
        crate::app::tests::run_frame(&ctx, input(), |ui| app.handle_app_shortcuts(ui.ctx()));
        assert!(app.details.move_task.is_some());
        assert!(
            app.quick_capture.is_none() && app.find.is_none(),
            "a move picker must not leave another modal active underneath it"
        );
        // Positive control: the same delivered event opens its overlay without
        // the move picker. This would fail if the harness discarded key input.
        app.details.move_task = None;
        crate::app::tests::run_frame(&ctx, input(), |ui| app.handle_app_shortcuts(ui.ctx()));
        assert!(if key == egui::Key::N {
            app.quick_capture.is_some()
        } else {
            app.find.is_some()
        });
        fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn child_undo_after_parent_archival_restores_content_without_restoring_parent() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Completed parent");
    task.toggle();
    task.subtasks = ["Before", "Deleted", "After"].map(Subtask::new).to_vec();
    task.subtasks[1].completed = true;
    let original = serde_json::to_value(&task).unwrap();
    let task_id = task.id;
    let child = task.subtasks[1].id;
    app.lists[0].data.tasks.push(task);
    app.delete_subtask(list, task_id, child);
    app.archive_completed(list);
    app.undo_delete();
    let saved = saved(&app, 0);
    assert!(saved.tasks.is_empty());
    assert_eq!(
        serde_json::to_value(&saved.archive[0].task).unwrap(),
        original
    );
    assert!(app.history_open);
    assert!(app.details.expanded_history.contains(&(list, task_id)));
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn child_undo_follows_its_parent_after_move_to_another_list() {
    let (_ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let mut task = Task::new("Move parent after deleting child");
    task.subtasks.push(Subtask::new("Recover at destination"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    app.delete_subtask(source, parent, child);
    let destination = storage::create_list(&app.workspace, "Destination", Accent::Mint).unwrap();
    let destination_id = destination.key;
    app.lists.push(destination);
    assert!(app.move_task_to_list(source, parent, destination_id));
    app.undo_delete();
    assert!(saved(&app, 0).tasks.is_empty());
    assert_eq!(saved(&app, 1).tasks[0].subtasks[0].id, child);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn deletion_undo_and_history_restore_keep_manual_positions_while_view_is_sorted() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["Zulu", "Alpha", "Middle"].map(Task::new).to_vec();
    app.lists[0].data.tasks[1].context = "Hidden details must survive sorted Undo".into();
    let original: Vec<_> = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
    app.save_list_index(0);
    app.sort_tasks(list, crate::model::TaskSort::Title);
    app.remove_task(list, original[1]);
    app.undo_delete();
    assert_eq!(
        saved(&app, 0)
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        original
    );
    assert_eq!(
        saved(&app, 0).tasks[1].context,
        "Hidden details must survive sorted Undo"
    );
    let archived = Task::new("Restored last in Manual");
    let restored_id = archived.id;
    app.lists[0].data.archive.push(ArchivedTask::new(archived));
    app.restore_archived(list, restored_id);
    let mut expected = original;
    expected.push(restored_id);
    assert_eq!(
        saved(&app, 0)
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        expected
    );
    app.sort_tasks(list, crate::model::TaskSort::Manual);
    assert_eq!(
        app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        expected
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}
