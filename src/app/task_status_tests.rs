//! In-progress status inference, persistence, and recovery regressions.

use super::*;
use crate::{
    app::tests::test_app,
    model::{Task, TaskStatus, TodoList},
    storage,
};
use std::fs;

fn saved(app: &MinimalistApp, index: usize) -> TodoList {
    serde_json::from_slice(&fs::read(&app.lists[index].path).unwrap()).unwrap()
}

#[test]
fn task_status_infers_starts_from_checked_children_and_survives_reopening() {
    let mut task = Task::new("Plan the trip");
    assert_eq!(task.status(), TaskStatus::NotStarted);
    task.toggle_started();
    assert_eq!(
        (task.status(), task.started),
        (TaskStatus::InProgress, Some(true))
    );
    task.toggle_started();
    assert_eq!(
        (task.status(), task.started),
        (TaskStatus::NotStarted, Some(false))
    );

    // A file from before the flag existed infers progress from its checklist.
    let mut legacy: Task = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "text": "Legacy",
        "completed": false,
        "created_at_unix": 1,
        "completed_at_unix": null,
        "subtasks": [
            {"id": Uuid::new_v4(), "text": "One", "completed": true},
            {"id": Uuid::new_v4(), "text": "Two", "completed": false}
        ]
    }))
    .unwrap();
    assert_eq!(
        (legacy.status(), legacy.started),
        (TaskStatus::InProgress, None)
    );
    let first = legacy.subtasks[0].id;
    assert!(legacy.toggle_subtask(first));
    assert_eq!(legacy.status(), TaskStatus::InProgress);

    // Marking it not started sticks despite checked children until another check.
    assert!(legacy.toggle_subtask(first));
    legacy.toggle_started();
    assert_eq!(legacy.status(), TaskStatus::NotStarted);
    let second = legacy.subtasks[1].id;
    assert!(legacy.toggle_subtask(second));
    assert_eq!(legacy.status(), TaskStatus::InProgress);

    legacy.toggle();
    assert_eq!(legacy.status(), TaskStatus::Done);
    legacy.toggle();
    assert_eq!(legacy.status(), TaskStatus::InProgress);
    legacy.toggle();
    legacy.toggle_started();
    assert_eq!(legacy.status(), TaskStatus::InProgress);
    assert_eq!(legacy.completed_at_unix, None);

    let untouched = serde_json::to_value(Task::new("Unstarted")).unwrap();
    assert!(untouched.get("started").is_none());
}

#[test]
fn started_saved_as_an_unknown_field_by_0_4_0_loads_once_as_in_progress() {
    let (_ctx, app) = test_app();
    let path = app.lists[0].path.clone();
    let mut file: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    // 0.4.0 writes fields it does not recognize after the known task fields.
    file["tasks"] = serde_json::json!([{
        "id": Uuid::new_v4(),
        "text": "Marked before the upgrade",
        "completed": false,
        "created_at_unix": 1,
        "completed_at_unix": null,
        "subtasks": [],
        "started": true,
        "future_task": {"keep": true}
    }]);
    fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();

    let mut list = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    let task = &list.data.tasks[0];
    assert_eq!(task.status(), TaskStatus::InProgress);
    assert!(!task.extra.contains_key("started"));
    list.data.title = "Edited after the upgrade".into();
    storage::save_list(&list).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("\"started\"").count(), 1);
    let reloaded: TodoList = serde_json::from_str(&text).unwrap();
    assert_eq!(reloaded.tasks[0].started, Some(true));
    assert_eq!(
        reloaded.tasks[0].extra["future_task"],
        serde_json::json!({"keep": true})
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn restoring_history_keeps_the_previous_open_status() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Started before archiving");
    task.toggle_started();
    task.toggle();
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.archive_completed(list);
    app.restore_archived(list, id);
    let restored = &saved(&app, 0).tasks[0];
    assert_eq!(restored.status(), TaskStatus::InProgress);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn deleting_and_undoing_the_last_checked_child_preserves_the_parents_open_status() {
    for started in [None, Some(false), Some(true)] {
        for done in [false, true] {
            let (_ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let mut task = Task::new("Legacy parent");
            let id = task.id;
            let mut child = Subtask::new("Checked child");
            let child_id = child.id;
            child.completed = true;
            task.subtasks.push(child);
            task.started = started;
            if done {
                task.toggle();
            }
            let status = task.status();
            let completed_at = task.completed_at_unix;
            app.lists[0].data.tasks.push(task);
            app.save_list_index(0);

            app.delete_subtask(list, id, child_id);
            let deleted = saved(&app, 0).tasks.remove(0);
            assert!(deleted.subtasks.is_empty());
            assert_eq!(deleted.status(), status);
            assert_eq!(deleted.started, started.or(Some(true)));
            assert_eq!(deleted.completed_at_unix, completed_at);

            app.undo_last();
            let restored = saved(&app, 0).tasks.remove(0);
            assert_eq!(restored.status(), status);
            assert_eq!(restored.started, deleted.started);
            assert_eq!(restored.completed_at_unix, completed_at);
            assert!(restored.subtasks[0].completed);
            assert_eq!(restored.subtasks[0].id, child_id);
            fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}
