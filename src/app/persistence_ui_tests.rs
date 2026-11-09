//! Deferred preference writes, external-edit preservation, and recovery layout.

use super::*;
use crate::{
    app::tests::{run_frame, test_app},
    model::{Task, TodoList},
};
use std::fs;

/// Reads the one discarded list saved by a test.
fn discarded_list(app: &MinimalistApp, filename: &std::ffi::OsStr) -> TodoList {
    let backups = storage::backups_dir(&app.workspace);
    let mut entries = fs::read_dir(backups.join("conflicted")).unwrap();
    let path = entries.next().unwrap().unwrap().path().join(filename);
    assert!(entries.next().is_none());
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

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
    assert_eq!(storage::load_settings().0.font_size, 25.0);
    assert!(app.persistence.settings_save_after.is_none());
    app.settings.background_opacity = 0.4;
    app.schedule_settings_save(&ctx);
    assert!(app.finish_pending_changes());
    assert_eq!(storage::load_settings().0.background_opacity, 0.4);
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
    app.status = Some("Unrelated sync warning".into());
    assert!(app.keep_local_copy_and_reload(list_id));
    assert_eq!(app.status.as_deref(), Some("Unrelated sync warning"));
    assert!(app.persistence.notice.is_some());
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
fn unresolved_list_keeps_local_edits_while_other_lists_reload_and_can_be_discarded() {
    let (_, mut app) = test_app();
    let original = app.lists[0].key;
    let independent =
        storage::create_list(&app.workspace, "Independent", crate::model::Accent::Mint).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    let original_index = app.list_index(original).unwrap();
    let mut synced = app.lists[original_index].clone();
    synced.data.tasks.push(Task::new("Synced original"));
    storage::save_list(&synced).unwrap();
    app.lists[original_index]
        .data
        .tasks
        .push(Task::new("Rejected local edit"));
    assert!(!app.save_list_index(original_index));

    let mut independent = independent;
    independent
        .data
        .tasks
        .push(Task::new("Updated independently"));
    storage::save_list(&independent).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert!(app.has_save_conflict(original));
    assert!(app.persistence.dirty_lists.contains(&original));
    assert_eq!(
        app.lists[app.list_index(original).unwrap()].data.tasks[0].text,
        "Rejected local edit"
    );
    assert_eq!(
        app.lists[app.list_index(independent.key).unwrap()]
            .data
            .tasks[0]
            .text,
        "Updated independently"
    );
    let remote_bytes = fs::read(&synced.path).unwrap();
    assert!(app.discard_local_change_and_reload(original));
    assert!(!app.has_save_conflict(original));
    assert!(app.retry_dirty_lists());
    assert_eq!(
        app.lists[app.list_index(original).unwrap()].data.tasks[0].text,
        "Synced original"
    );
    assert_eq!(fs::read(&synced.path).unwrap(), remote_bytes);
    let backup = discarded_list(&app, synced.path.file_name().unwrap());
    assert_eq!(backup.id, original);
    assert_eq!(backup.tasks[0].text, "Rejected local edit");
    assert_eq!(
        fs::read_dir(app.workspace.join("lists")).unwrap().count(),
        2
    );
    fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn a_rejected_write_can_be_discarded_while_the_disk_copy_is_unchanged() {
    use crate::app::tests::label_center;

    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let path = app.lists[0].path.clone();
    let disk = fs::read(&path).unwrap();
    // A failed atomic write can leave the loaded bytes readable and unchanged.
    app.lists[0].data.tasks.push(Task::new("Rejected write"));
    app.lists[0].data.tasks.push(Task::new("A later edit"));
    app.persistence.dirty_lists.insert(list);
    assert!(!app.has_save_conflict(list));
    let task = app.lists[0].data.tasks[0].id;
    app.editing = Some((list, task));
    app.edit_text = "Still editing this title".into();
    let palette = app.palette();
    let output = run_frame(&ctx, Default::default(), |ui| app.status_ui(ui, palette));
    label_center(&output, "Retry save");
    let pos = label_center(&output, "Discard all unsaved changes");
    for pressed in [true, false] {
        let input = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        run_frame(&ctx, input, |ui| app.status_ui(ui, palette));
    }
    assert!(!app.persistence.dirty_lists.contains(&list));
    assert!(app.editing.is_none());
    assert!(app.lists[0].data.tasks.is_empty());
    assert_eq!(fs::read(&path).unwrap(), disk);
    let backup = discarded_list(&app, path.file_name().unwrap());
    assert_eq!(backup.tasks[0].id, task);
    assert_eq!(backup.tasks[0].text, "Still editing this title");
    assert_eq!(backup.tasks[1].text, "A later edit");
    assert!(app.finish_pending_changes());
    fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn a_pending_save_can_be_discarded_when_the_lists_folder_stays_unavailable() {
    for obstructed in [false, true] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let original_path = app.lists[0].path.clone();
        let lists = app.workspace.join("lists");
        let offline = app.workspace.join("offline");
        fs::rename(&lists, &offline).unwrap();
        if obstructed {
            fs::write(&lists, "Obstruction").unwrap();
        }
        app.lists[0].data.tasks.push(Task::new("Pending edit"));
        assert!(!app.save_list_index(0));
        assert!(!app.has_save_conflict(list));
        let palette = app.palette();
        let output = run_frame(&ctx, Default::default(), |ui| app.status_ui(ui, palette));
        let shown = |label: &str| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.text() == label
                )
            })
        };
        assert!(shown("Waiting for the folder. Your changes are unsaved."));
        assert!(shown("Retry save"));
        assert!(shown("Discard all unsaved changes"));
        assert!(!shown("Keep local copy and reload"));
        assert!(!app.finish_pending_changes());
        assert!(app.discard_local_change_and_reload(list));
        let backup = discarded_list(&app, original_path.file_name().unwrap());
        assert_eq!(backup.tasks[0].text, "Pending edit");
        assert!(app.finish_pending_changes());
        assert!(!lists.is_dir());
        assert_eq!(fs::read_dir(offline).unwrap().count(), 1);
        fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
        fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn discard_waits_for_a_successful_backup_before_clearing_local_changes() {
    let (_, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Saved task");
    let task_id = task.id;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    app.open_task_context(list, task_id, false);
    app.task_context.as_mut().unwrap().text = "Unsubmitted notes".into();
    app.open_quick_capture(Some(list));
    app.quick_capture.as_mut().unwrap().text = "Unsubmitted capture".into();
    let path = app.lists[0].path.clone();
    let mut remote = storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks[0].text = "Synced title".into();
    storage::save_list(&remote).unwrap();
    let disk = fs::read(&path).unwrap();
    app.lists[0].data.tasks[0].toggle();
    assert!(!app.save_list_index(0));
    let backups = storage::backups_dir(&app.workspace);
    fs::write(backups.join("conflicted"), "blocks the backup directory").unwrap();

    assert!(!app.discard_local_change_and_reload(list));
    assert!(app.status.as_deref().unwrap().contains("Could not back up"));
    assert!(app.persistence.dirty_lists.contains(&list));
    assert!(app.has_save_conflict(list));
    assert!(app.lists[0].data.tasks[0].completed);
    assert_eq!(app.task_context.as_ref().unwrap().text, "Unsubmitted notes");
    assert_eq!(
        app.quick_capture.as_ref().unwrap().text,
        "Unsubmitted capture"
    );
    assert_eq!(fs::read(&path).unwrap(), disk);

    fs::remove_file(backups.join("conflicted")).unwrap();
    assert!(app.discard_local_change_and_reload(list));
    assert!(app.task_context.is_none());
    assert!(app.quick_capture.is_none());
    let backup = discarded_list(&app, path.file_name().unwrap());
    assert_eq!(backup.tasks[0].text, "Unsubmitted capture");
    assert_eq!(backup.tasks[1].context, "Unsubmitted notes");
    assert!(backup.tasks[1].completed);
    assert_eq!(fs::read(&path).unwrap(), disk);
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn a_retained_list_does_not_replace_the_synced_order_of_a_different_list_at_its_path() {
    let (_, mut app) = test_app();
    let original = app.lists[0].key;
    let imported = app.lists[0].path.with_file_name("imported.json");
    fs::rename(&app.lists[0].path, &imported).unwrap();
    let mut remote =
        storage::create_list(&app.workspace, "Remote", crate::model::Accent::Sky).unwrap();
    let first = Task::new("First");
    let second = Task::new("Second");
    let synced_order = [second.id, first.id];
    remote.data.tasks = vec![first, second];
    storage::save_list(&remote).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();

    remote.data.tasks.swap(0, 1);
    storage::save_list(&remote).unwrap();
    fs::write(&imported, fs::read(&remote.path).unwrap()).unwrap();
    fs::remove_file(&remote.path).unwrap();
    let index = app.list_index(original).unwrap();
    app.lists[index].data.tasks.push(Task::new("Pending edit"));
    assert!(!app.save_list_index(index));
    app.next_scan = Instant::now();
    app.scan_external_changes();
    let index = app.list_index(remote.key).unwrap();
    app.lists[index].data.tasks[0].toggle();
    assert!(app.save_list_index(index));
    let saved = storage::load_existing_workspace(&app.workspace).unwrap();
    let order = saved.lists[0]
        .data
        .tasks
        .iter()
        .map(|task| task.id)
        .collect::<Vec<_>>();
    assert_eq!(order, synced_order);
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn a_workspace_can_reopen_with_an_unreadable_list_and_a_pending_save() {
    let (_, mut app) = test_app();
    let original = app.lists[0].key;
    let path = app.lists[0].path.clone();
    let lists = app.workspace.join("lists");
    let offline = app.workspace.join("offline");
    fs::rename(&lists, &offline).unwrap();
    app.lists[0]
        .data
        .tasks
        .push(Task::new("Pending local edit"));
    assert!(!app.save_list_index(0));
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert!(app.persistence.workspace_failure.is_some());

    let other = storage::create_list(&app.workspace, "Synced", crate::model::Accent::Sky).unwrap();
    fs::create_dir(&path).unwrap();
    app.retry_workspace();
    assert!(app.persistence.workspace_failure.is_none());
    assert!(app.list_index(other.key).is_some());
    assert!(app.persistence.dirty_lists.contains(&original));
    assert!(!app.retry_dirty_lists());
    assert!(app.has_save_conflict(original));
    assert!(app.keep_local_copy_and_reload(original));
    assert!(path.is_dir());
    assert!(app.finish_pending_changes());
    let reloaded = storage::load_workspace(&app.workspace).unwrap();
    let copy = reloaded
        .lists
        .iter()
        .find(|list| list.key != other.key)
        .unwrap();
    assert_eq!(copy.data.tasks[0].text, "Pending local edit");
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
    app.persistence.dirty_lists.insert(list);
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

#[test]
fn retry_save_only_appears_for_independent_recoverable_writes() {
    for independent in ["none", "dirty list", "preferences", "filename"] {
        let (ctx, mut app) = test_app();
        let conflicted = app.lists[0].key;
        let mut remote = storage::load_workspace(&app.workspace)
            .unwrap()
            .lists
            .remove(0);
        remote.data.tasks.push(Task::new("Remote task"));
        storage::save_list(&remote).unwrap();
        let remote_bytes = fs::read(&remote.path).unwrap();
        app.lists[0].data.tasks.push(Task::new("Local task"));
        assert!(!app.save_list_index(0));
        let mut other_id = None;
        if matches!(independent, "dirty list" | "filename") {
            let other =
                storage::create_list(&app.workspace, "Independent", crate::model::Accent::Mint)
                    .unwrap();
            other_id = Some(other.key);
            app.lists.push(other);
            if independent == "dirty list" {
                app.lists[1].data.tasks.push(Task::new("Retry this task"));
                app.persistence.dirty_lists.insert(other_id.unwrap());
            } else {
                app.lists[1].data.title = "Renamed independent list".into();
                app.persistence.pending_filenames.insert(other_id.unwrap());
            }
        } else if independent == "preferences" {
            app.settings.font_size = 24.0;
            app.persistence.settings_dirty = true;
        }
        let palette = app.palette();
        let mut output = None;
        for _ in 0..3 {
            output = Some(run_frame(&ctx, Default::default(), |ui| {
                app.status_ui(ui, palette)
            }));
        }
        let output = output.unwrap();
        let shown = |label| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.text() == label
                )
            })
        };
        assert!(shown("Keep local copy and reload"), "{independent}");
        assert!(shown("Discard all unsaved changes"), "{independent}");
        assert_eq!(shown("Retry save"), independent != "none", "{independent}");
        assert!(!app.retry_dirty_lists());
        assert!(app.persistence.dirty_lists.contains(&conflicted));
        assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
        if let Some(other_id) = other_id {
            assert!(!app.persistence.dirty_lists.contains(&other_id));
            assert!(!app.persistence.pending_filenames.contains(&other_id));
            let other = storage::load_workspace(&app.workspace)
                .unwrap()
                .lists
                .into_iter()
                .find(|list| list.key == other_id)
                .unwrap();
            if independent == "dirty list" {
                assert_eq!(other.data.tasks[0].text, "Retry this task");
            } else {
                assert!(
                    other
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("renamed-independent-list--")
                );
            }
        } else if independent == "preferences" {
            assert_eq!(storage::load_settings().0.font_size, 24.0);
        }
        fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn conflicted_context_and_quick_add_offer_recovery_without_retry_actions() {
    for context in [true, false] {
        let (ctx, mut app) = test_app();
        let task = Task::new("Existing task");
        let task_id = task.id;
        let list = app.lists[0].key;
        app.lists[0].data.tasks.push(task);
        assert!(app.save_list_index(0));
        let mut remote = storage::load_workspace(&app.workspace)
            .unwrap()
            .lists
            .remove(0);
        remote.data.tasks[0].text = "Remote title".into();
        storage::save_list(&remote).unwrap();
        let remote_bytes = fs::read(&remote.path).unwrap();
        if context {
            app.open_task_context(list, task_id, false);
            app.task_context.as_mut().unwrap().text = "Local context draft".into();
            assert!(!app.save_task_context());
        } else {
            app.open_task_context(list, task_id, false);
            app.task_context.as_mut().unwrap().text = "Unsubmitted notes".into();
            app.open_quick_capture(Some(list));
            app.quick_capture.as_mut().unwrap().text = "Quick add draft".into();
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
        }
        assert!(app.has_save_conflict(list));
        let mut output = None;
        for _ in 0..3 {
            output = Some(run_frame(&ctx, Default::default(), |_| {
                app.task_context_ui(&ctx);
                app.overlay_ui(&ctx, app.palette());
            }));
        }
        let output = output.unwrap();
        let shown = |label| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.text() == label
                )
            })
        };
        assert!(shown("Keep local copy and reload"));
        assert!(shown("Discard all unsaved changes"));
        assert!(!shown("Retry save"));
        assert!(!shown(if context { "Save" } else { "Add" }));
        if !context {
            run_frame(
                &ctx,
                egui::RawInput {
                    events: crate::app::tests::chord(egui::Key::Enter, egui::Modifiers::NONE),
                    ..Default::default()
                },
                |_| app.overlay_ui(&ctx, app.palette()),
            );
            assert_eq!(app.quick_capture.as_ref().unwrap().text, "Quick add draft");
        }
        assert_eq!(fs::read(&remote.path).unwrap(), remote_bytes);
        fs::remove_dir_all(app.workspace).unwrap();
    }
}
