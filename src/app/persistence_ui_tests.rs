//! Deferred preference writes, external-edit preservation, and recovery layout.

use super::*;
use crate::{
    app::tests::{run_frame, test_app},
    model::Task,
};
use std::fs;

#[test]
fn appearance_writes_wait_for_a_pause_and_shutdown_flushes_the_final_value() {
    let (ctx, mut app) = test_app();
    app.save_settings();
    let path = storage::settings_path();
    let original = fs::read(&path).unwrap();
    app.settings.font_size = 23.0;
    app.schedule_settings_save(&ctx);
    app.flush_deferred_settings(&ctx);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!app.persistence.settings_dirty);
    app.settings.font_size = 25.0;
    app.schedule_settings_save(&ctx);
    app.persistence.settings_save_after = Some(Instant::now());
    app.flush_deferred_settings(&ctx);
    assert_eq!(storage::load_settings().unwrap().font_size, 25.0);
    assert!(app.persistence.settings_save_after.is_none());
    app.settings.background_opacity = 0.4;
    app.schedule_settings_save(&ctx);
    assert!(app.finish_pending_changes());
    assert_eq!(storage::load_settings().unwrap().background_opacity, 0.4);
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn deferred_reload_during_inline_edit_cannot_overwrite_synced_task_changes() {
    let (_, mut app) = test_app();
    let task = Task::new("Original task");
    let task_id = task.id;
    let list_id = app.lists[0].key;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    app.editing = Some((list_id, task_id));
    app.edit_text = "Local edited title".into();
    let mut remote = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks[0].text = "Synced task title with another device edit".into();
    storage::save_list(&remote).unwrap();
    let remote_bytes = fs::read(&remote.path).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert_eq!(app.lists[0].data.tasks[0].text, "Original task");
    app.finish_inline_edit();
    assert_eq!(app.lists[0].data.tasks[0].text, "Local edited title");
    assert!(app.persistence.dirty_lists.contains(&list_id));
    assert!(app.status.as_ref().unwrap().contains("changed on disk"));
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    assert!(!app.retry_dirty_lists());
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    assert!(app.keep_local_copy_and_reload(list_id));
    assert!(app.persistence.dirty_lists.is_empty());
    assert!(app.finish_pending_changes());
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    let local_copy = saved
        .lists
        .iter()
        .find(|stored| stored.key != list_id)
        .unwrap();
    assert_eq!(local_copy.data.tasks[0].text, "Local edited title");
    assert_ne!(local_copy.data.tasks[0].id, task_id);
    let original = saved
        .lists
        .iter()
        .find(|stored| stored.key == list_id)
        .unwrap();
    assert_eq!(
        original.data.tasks[0].text,
        "Synced task title with another device edit"
    );
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn narrow_workspace_warning_keeps_the_entire_instruction_and_actions_visible() {
    let (ctx, mut app) = test_app();
    app.persistence.workspace_failure =
        Some("/Volumes/Cloud/Shared task lists/Personal and work/tasks".into());
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(260.0, 400.0));
    let palette = app.palette();
    let mut output = None;
    for _ in 0..3 {
        output = Some(run_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |ui| {
                ui.set_width(228.0);
                app.status_ui(ui, palette);
            },
        ));
    }
    let output = output.unwrap();
    for label in [
        "Retry or choose another folder.",
        "Retry workspace",
        "Choose workspace…",
    ] {
        let (clip, text) = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && text.galley.text().contains(label)
                {
                    Some((shape.clip_rect, text))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("missing {label}"));
        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
        assert!(
            clip.contains_rect(rect),
            "clipped {label}: {rect:?} outside {clip:?}"
        );
        assert!(viewport.contains_rect(rect), "offscreen {label}");
    }
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn conflict_copy_preserves_latest_context_draft_without_blocking_shutdown() {
    let (_, mut app) = test_app();
    let task = Task::new("Task with context");
    let task_id = task.id;
    let list = app.lists[0].key;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    app.open_task_context(list, task_id, false);
    app.task_context.as_mut().unwrap().text = "Accepted local notes".into();
    let mut remote = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks[0].context = "Remote notes".into();
    storage::save_list(&remote).unwrap();
    let remote_bytes = fs::read(&remote.path).unwrap();
    assert!(!app.save_task_context());
    app.task_context.as_mut().unwrap().text = "Latest local notes after failed save".into();
    assert!(app.keep_local_copy_and_reload(list));
    assert!(app.task_context.is_none());
    assert!(app.finish_pending_changes());
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    let copy = saved
        .lists
        .iter()
        .find(|stored| stored.key != list)
        .unwrap();
    assert_eq!(
        copy.data.tasks[0].context,
        "Latest local notes after failed save"
    );
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn conflict_copy_preserves_latest_failed_quick_capture_without_duplicate_tasks() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.open_quick_capture(Some(list));
    app.quick_capture.as_mut().unwrap().text = "Accepted captured task".into();
    let mut remote = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks.push(Task::new("Remote addition"));
    storage::save_list(&remote).unwrap();
    let remote_bytes = fs::read(&remote.path).unwrap();
    run_frame(&ctx, Default::default(), |_| {
        app.overlay_ui(&ctx, app.palette())
    });
    run_frame(
        &ctx,
        egui::RawInput {
            events: crate::app::tests::chord(egui::Key::Enter, egui::Modifiers::NONE),
            ..Default::default()
        },
        |_| app.overlay_ui(&ctx, app.palette()),
    );
    assert!(app.quick_capture.as_ref().unwrap().failure.is_some());
    app.quick_capture.as_mut().unwrap().text = "Latest captured task".into();
    assert!(app.keep_local_copy_and_reload(list));
    assert!(app.quick_capture.is_none());
    assert!(app.finish_pending_changes());
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    let copy = saved
        .lists
        .iter()
        .find(|stored| stored.key != list)
        .unwrap();
    assert_eq!(copy.data.tasks.len(), 1);
    assert_eq!(copy.data.tasks[0].text, "Latest captured task");
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn failed_capture_recovery_preserves_underlying_unsubmitted_context_and_rename() {
    let (ctx, mut app) = test_app();
    let task = Task::new("Task with unsaved notes");
    let task_id = task.id;
    let list = app.lists[0].key;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    app.open_settings(Some(list));
    app.rename_input = "Unsaved local list title".into();
    app.open_task_context(list, task_id, false);
    app.task_context.as_mut().unwrap().text = "Unsubmitted notes underneath capture".into();
    app.open_quick_capture(Some(list));
    app.quick_capture.as_mut().unwrap().text = "Accepted captured task".into();
    let mut remote = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks[0].text = "Remotely updated original".into();
    storage::save_list(&remote).unwrap();
    let remote_bytes = fs::read(&remote.path).unwrap();
    run_frame(&ctx, Default::default(), |_| {
        app.overlay_ui(&ctx, app.palette())
    });
    run_frame(
        &ctx,
        egui::RawInput {
            events: crate::app::tests::chord(egui::Key::Enter, egui::Modifiers::NONE),
            ..Default::default()
        },
        |_| app.overlay_ui(&ctx, app.palette()),
    );
    assert!(app.quick_capture.as_ref().unwrap().failure.is_some());
    assert!(app.task_context.as_ref().unwrap().failure.is_none());
    assert!(app.keep_local_copy_and_reload(list));
    let saved = storage::load_workspace(&app.workspace).unwrap();
    let copy = saved
        .lists
        .iter()
        .find(|stored| stored.key != list)
        .unwrap();
    assert_eq!(copy.data.title, "Unsaved local list title (local copy)");
    assert_eq!(copy.data.tasks.len(), 2);
    assert!(
        copy.data
            .tasks
            .iter()
            .any(|task| task.text == "Accepted captured task")
    );
    assert!(
        copy.data
            .tasks
            .iter()
            .any(|task| task.context == "Unsubmitted notes underneath capture")
    );
    assert!(app.task_context.is_none());
    assert!(app.quick_capture.is_none());
    assert!(app.finish_pending_changes());
    assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
    fs::remove_dir_all(app.workspace).unwrap();
}
