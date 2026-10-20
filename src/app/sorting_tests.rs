//! Saved-data and restart checks for temporary task sort views.

use super::*;
use crate::{
    app::{keyboard::FocusItem, tests::test_app},
    model::Subtask,
    storage,
};
use std::collections::BTreeMap;

fn fixture() -> (egui::Context, MinimalistApp, Uuid, Vec<Uuid>) {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["Bravo", "alpha", "Zulu", "ALPHA"]
        .into_iter()
        .enumerate()
        .map(|(i, title)| {
            let mut task = Task::new(title);
            task.created_at_unix = [30, 10, 40, 20][i];
            task.context = format!("Preserved hidden context {i}\nsecond line");
            if i == 0 || i == 3 {
                task.toggle();
            }
            task.subtasks = vec![Subtask::new(format!("Child {i}"))];
            task.subtasks[0].completed = i == 1;
            task
        })
        .collect();
    app.save_list_index(0);
    let ids = ids(&app);
    (ctx, app, list, ids)
}

fn ids(app: &MinimalistApp) -> Vec<Uuid> {
    app.lists[0].data.tasks.iter().map(|task| task.id).collect()
}

fn disk_ids(app: &MinimalistApp) -> Vec<Uuid> {
    storage::load_workspace(&app.workspace).unwrap().lists[0]
        .data
        .tasks
        .iter()
        .map(|task| task.id)
        .collect()
}

#[test]
fn every_view_preserves_manual_files_and_restores_complete_tasks() {
    for (mode, expected) in [
        (TaskSort::Newest, [2, 0, 3, 1]),
        (TaskSort::Oldest, [1, 3, 0, 2]),
        (TaskSort::Title, [1, 3, 0, 2]),
        (TaskSort::IncompleteFirst, [1, 2, 0, 3]),
    ] {
        let (_, mut app, list, manual) = fixture();
        let bytes = std::fs::read(&app.lists[0].path).unwrap();
        let before: BTreeMap<_, _> = app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| (task.id, serde_json::to_value(task).unwrap()))
            .collect();
        app.sort_tasks(list, mode);
        assert_eq!(ids(&app), expected.map(|i| manual[i]));
        assert_eq!(std::fs::read(&app.lists[0].path).unwrap(), bytes);
        let after: BTreeMap<_, _> = app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| (task.id, serde_json::to_value(task).unwrap()))
            .collect();
        assert_eq!(after, before);
        app.sort_tasks(list, TaskSort::Manual);
        assert_eq!(ids(&app), manual);
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn view_mode_and_manual_order_survive_restart() {
    let (_, mut app, list, manual) = fixture();
    app.sort_tasks(list, TaskSort::Title);
    let sorted = ids(&app);
    let settings = storage::load_settings().unwrap();
    let ctx = egui::Context::default();
    let mut restarted = MinimalistApp::new(&ctx, settings, None, None);
    assert_eq!(restarted.task_sort_mode(list), TaskSort::Title);
    assert_eq!(ids(&restarted), sorted);
    restarted.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&restarted), manual);
    assert_eq!(disk_ids(&restarted), manual);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn edited_titles_and_completion_resort_the_view_without_rewriting_manual_order() {
    let (_, mut app, list, manual) = fixture();
    app.sort_tasks(list, TaskSort::Title);
    let index = app.lists[0]
        .data
        .tasks
        .iter()
        .position(|task| task.id == manual[1])
        .unwrap();
    app.lists[0].data.tasks[index].text = "zzzz edited".into();
    app.save_list_index(0);
    assert_eq!(ids(&app).last(), Some(&manual[1]));
    assert_eq!(disk_ids(&app), manual);
    app.sort_tasks(list, TaskSort::IncompleteFirst);
    let index = app.lists[0]
        .data
        .tasks
        .iter()
        .position(|task| task.id == manual[2])
        .unwrap();
    app.lists[0].data.tasks[index].toggle();
    app.save_list_index(0);
    assert_eq!(ids(&app), [manual[1], manual[0], manual[2], manual[3]]);
    assert_eq!(disk_ids(&app), manual);
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&app), manual);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn candidate_capture_and_context_edits_preserve_manual_order_and_whole_tasks() {
    let (_, mut app, list, manual) = fixture();
    app.sort_tasks(list, TaskSort::Title);
    let mut candidate = app.lists[0].clone();
    candidate.data.tasks[0].context = "Updated candidate context".into();
    let changed = candidate.data.tasks[0].id;
    let captured = Task::new("New capture");
    let new_id = captured.id;
    candidate.data.tasks.insert(0, captured);
    app.prepare_list_for_save(&mut candidate);
    let expected = [vec![new_id], manual].concat();
    assert_eq!(
        candidate
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        expected
    );
    storage::save_list(&candidate).unwrap();
    app.lists[0] = candidate;
    app.apply_task_sorts();
    assert_eq!(disk_ids(&app), expected);
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&app), expected);
    assert_eq!(
        app.lists[0]
            .data
            .tasks
            .iter()
            .find(|task| task.id == changed)
            .unwrap()
            .context,
        "Updated candidate context"
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn external_manual_json_reorder_is_retained_during_a_sorted_view() {
    let (_, mut app, list, manual) = fixture();
    app.sort_tasks(list, TaskSort::Title);
    let mut external = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    external.data.tasks.rotate_right(1);
    storage::save_list(&external).unwrap();
    let expected = [manual[3], manual[0], manual[1], manual[2]];
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    assert_eq!(app.task_sort_mode(list), TaskSort::Title);
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&app), expected);
    assert_eq!(disk_ids(&app), expected);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn deleting_and_undoing_in_sorted_view_restores_original_manual_position() {
    let (_, mut app, list, manual) = fixture();
    let before = serde_json::to_value(&app.lists[0].data.tasks).unwrap();
    app.sort_tasks(list, TaskSort::Title);
    app.remove_task(list, manual[0]);
    assert!(!disk_ids(&app).contains(&manual[0]));
    app.undo_delete();
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&app), manual);
    assert_eq!(
        serde_json::to_value(&app.lists[0].data.tasks).unwrap(),
        before
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn equal_keys_use_manual_ties_even_after_a_different_sort_mode() {
    let (_, mut app, list, manual) = fixture();
    app.sort_tasks(list, TaskSort::Newest);
    app.sort_tasks(list, TaskSort::Title);
    assert_eq!(&ids(&app)[..2], &[manual[1], manual[3]]);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn manual_reorder_is_saved_and_survives_a_temporary_sorted_view() {
    let (_, mut app, list, manual) = fixture();
    app.apply_row_action(crate::app::list::RowAction::Reorder(list, manual[0], 2));
    let expected = [manual[1], manual[2], manual[0], manual[3]];
    assert_eq!(disk_ids(&app), expected);
    app.sort_tasks(list, TaskSort::Title);
    app.sort_tasks(list, TaskSort::Manual);
    assert_eq!(ids(&app), expected);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn sorting_settings_return_to_the_same_task_identity() {
    let (ctx, mut app, list, manual) = fixture();
    let focus = FocusItem::Task(list, manual[2]).id();
    app.request_task_focus = false;
    app.keyboard.pending = Some(focus);
    let render = |app: &mut MinimalistApp| {
        app.view_zoom = 1.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest()),
        );
        output.textures_delta.clear();
    };
    for _ in 0..3 {
        render(&mut app);
    }
    app.open_settings(Some(list));
    for _ in 0..3 {
        render(&mut app);
    }
    app.sort_tasks(list, TaskSort::Title);
    app.close_settings();
    for _ in 0..3 {
        render(&mut app);
    }
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(focus));
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn retaining_an_unreadable_sorted_list_does_not_replace_its_manual_snapshot() {
    let (_, mut app, list, manual) = fixture();
    let other = storage::create_list(&app.workspace, "Other", crate::model::Accent::Mint).unwrap();
    app.lists.push(other);
    app.sort_tasks(list, TaskSort::Title);
    let path = app.lists[app.list_index(list).unwrap()].path.clone();
    std::fs::write(&path, "temporarily incomplete JSON").unwrap();
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    app.sort_tasks(list, TaskSort::Manual);
    let index = app.list_index(list).unwrap();
    assert_eq!(
        app.lists[index]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        manual
    );
    assert!(!app.save_list_index(index));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "temporarily incomplete JSON"
    );
    assert!(app.persistence.dirty_lists.contains(&list));
    assert_eq!(app.manual_task_order[&list], manual);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn task_saves_and_reopening_selected_list_do_not_write_settings() {
    for mode in [TaskSort::Manual, TaskSort::Title] {
        let (_, mut app, list, manual) = fixture();
        app.sort_tasks(list, mode);
        app.open_list(list);
        app.save_settings();
        let path = storage::settings_path();
        let bytes = std::fs::read(&path).unwrap();
        let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
        let task = Task::new("New task");
        let added = task.id;
        app.lists[0].data.tasks.insert(0, task);
        assert!(app.save_list_index(0));
        if mode == TaskSort::Manual {
            app.apply_row_action(crate::app::list::RowAction::Reorder(list, added, 2));
        }
        app.go_to_overview();
        app.open_list(list);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), stamp);
        assert!(app.settings.manual_task_order.is_empty());
        let expected = if mode == TaskSort::Manual {
            vec![manual[0], manual[1], added, manual[2], manual[3]]
        } else {
            [vec![added], manual].concat()
        };
        assert_eq!(disk_ids(&app), expected);
        app.sort_tasks(list, TaskSort::Manual);
        assert_eq!(ids(&app), expected);
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn pruning_list_preferences_keeps_other_workspaces_and_live_lists() {
    let (_, mut app, list, _) = fixture();
    let removed = Uuid::new_v4();
    let workspace = app.workspace.to_string_lossy().into_owned();
    let elsewhere = format!("{workspace}-other");
    app.settings
        .task_sort
        .insert(app.task_order_key(list), TaskSort::Title);
    app.settings
        .task_sort
        .insert(app.task_order_key(removed), TaskSort::Newest);
    let other_key = format!("{elsewhere}::{removed}");
    app.settings
        .task_sort
        .insert(other_key.clone(), TaskSort::Oldest);
    app.settings
        .list_order
        .insert(workspace.clone(), vec![removed, list]);
    app.settings
        .list_order
        .insert(elsewhere.clone(), vec![removed]);
    app.capture_loaded_task_orders();
    assert_eq!(app.settings.task_sort.len(), 2);
    assert_eq!(
        app.settings.task_sort[&app.task_order_key(list)],
        TaskSort::Title
    );
    assert_eq!(app.settings.task_sort[&other_key], TaskSort::Oldest);
    assert_eq!(app.settings.list_order[&workspace], vec![list]);
    assert_eq!(app.settings.list_order[&elsewhere], vec![removed]);
    std::fs::remove_dir_all(app.workspace).unwrap();
}
