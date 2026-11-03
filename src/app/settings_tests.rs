//! List-options and settings-screen regressions.

use super::*;

#[test]
fn translucent_settings_header_hints_and_version_have_no_backing() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.settings.background_opacity = 0.2;
    app.settings.font = FontChoice::Mono;
    app.open_settings(None);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 1600.0),
            )),
            ..Default::default()
        },
        |ui| app.settings_ui(ui, app.palette()),
    );
    output.textures_delta.clear();
    for label in [
        "Settings",
        "Up/Down: rows · Left/Right: choose or adjust",
        "Current list options",
        "App settings",
        concat!("Version ", env!("CARGO_PKG_VERSION")),
        "Workspace, window, appearance, and shortcuts apply to every list.",
    ] {
        let (index, text_rect) = output
            .shapes
            .iter()
            .enumerate()
            .find_map(|(index, shape)| {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    Some((
                        index,
                        egui::Rect::from_min_size(text.pos, text.galley.size()),
                    ))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing {label}"));
        assert!(!output.shapes[..index].iter().any(|shape| matches!(&shape.shape,
            egui::epaint::Shape::Rect(rect) if rect.fill.a() > 0 && rect.rect.contains_rect(text_rect)
        )), "Unexpected backing under {label}");
    }
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn list_deletion_refuses_same_identity_copies_before_and_after_restart() {
    for restart in [false, true] {
        let (ctx, mut app) = super::super::tests::test_app();
        let list = app.lists[0].key;
        let original = app.lists[0].path.clone();
        let survivor = storage::create_list(&app.workspace, "Survivor", Accent::Charcoal).unwrap();
        let survivor_path = survivor.path.clone();
        app.lists.push(survivor);
        app.open_settings(Some(list));
        let mut stale = app.lists[0].clone();
        stale.data.title = "Renamed".into();
        stale.path = original.with_file_name(format!("renamed--{list}.json"));
        stale.stamp.set(None);
        storage::save_list(&stale).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&stale.path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000))
            .unwrap();
        app.lists[0]
            .data
            .tasks
            .push(crate::model::Task::new("Latest saved task"));
        app.rename_input = "Renamed".into();
        app.rename_list(list);
        assert!(!app.persistence.pending_filenames.contains(&list));
        if restart {
            app = MinimalistApp::new(&ctx, app.settings.clone(), None, None);
            assert!(app.persistence.pending_filenames.is_empty());
        }
        let source_bytes = std::fs::read(&original).unwrap();
        let stale_bytes = std::fs::read(&stale.path).unwrap();
        app.delete_list(list);
        assert!(app.list_index(list).is_some());
        assert!(
            app.status
                .as_ref()
                .unwrap()
                .contains("duplicate list identity")
        );
        assert_eq!(std::fs::read(&original).unwrap(), source_bytes);
        assert_eq!(std::fs::read(&stale.path).unwrap(), stale_bytes);
        let loaded = storage::load_workspace(&app.workspace).unwrap();
        let current = loaded.lists.iter().find(|item| item.key == list).unwrap();
        assert_eq!(current.path, original);
        assert_eq!(current.data.tasks[0].text, "Latest saved task");
        // Resolving the duplicate permits ordinary deletion without reviving it.
        std::fs::remove_file(&stale.path).unwrap();
        app.delete_list(list);
        assert!(app.list_index(list).is_none());
        assert!(!app.persistence.pending_filenames.contains(&list));
        assert!(!original.exists());
        let loaded = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(loaded.lists.len(), 1);
        assert_eq!(loaded.lists[0].path, survivor_path);
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn deleting_a_formerly_unsaved_list_removes_its_pending_retry() {
    let (_ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    let original_path = app.lists[0].path.clone();
    let survivor = storage::create_list(&app.workspace, "Survivor", Accent::Charcoal).unwrap();
    app.lists.push(survivor);
    let blocker = app.workspace.join("blocked-parent");
    std::fs::write(&blocker, "file blocks directory").unwrap();
    app.lists[0].path = blocker.join("list.json");
    app.lists[0]
        .data
        .tasks
        .push(crate::model::Task::new("Unsaved"));
    assert!(!app.save_list_index(0));
    assert!(app.persistence.dirty_lists.contains(&list));
    app.lists[0].path = original_path.clone();
    app.delete_list(list);
    assert!(!original_path.exists());
    assert!(!app.persistence.dirty_lists.contains(&list));
    assert!(app.retry_dirty_lists());
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

fn settings_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520.0, 1600.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.settings_ui(ui, app.palette()),
    );
    output.textures_delta.clear();
    output
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            egui::epaint::Shape::Text(text) => Some((
                text.galley.text().to_owned(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
            )),
            _ => None,
        })
        .collect()
}

fn click(ctx: &egui::Context, app: &mut MinimalistApp, label: &str) {
    let position = settings_frame(ctx, app, Vec::new())
        .into_iter()
        .find(|(text, _)| text == label)
        .unwrap()
        .1
        .center();
    for pressed in [true, false] {
        settings_frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn list_options_and_global_preferences_are_distinct_and_switching_commits_rename() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    app.open_settings(None);
    let global = settings_frame(&ctx, &mut app, Vec::new());
    assert!(global.iter().any(|(text, _)| text == "Workspace"));
    assert!(!global.iter().any(|(text, _)| text == "Delete list"));
    click(&ctx, &mut app, "Current list options");
    assert_eq!(app.settings_list_id, Some(list));
    let options = settings_frame(&ctx, &mut app, Vec::new());
    assert!(options.iter().any(|(text, _)| text == "Delete list"));
    assert!(!options.iter().any(|(text, _)| text == "Workspace"));
    app.rename_input = "Renamed through list options".into();
    click(&ctx, &mut app, "App settings");
    assert!(app.settings_list_id.is_none());
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .title,
        "Renamed through list options"
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn deletion_confirmation_names_all_affected_content() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    let mut parent = crate::model::Task::new("Active parent");
    parent.subtasks.push(crate::model::Subtask::new("Child"));
    let mut done = crate::model::Task::new("Completed parent");
    done.completed = true;
    app.lists[0].data.tasks = vec![parent.clone(), done];
    app.lists[0]
        .data
        .archive
        .push(crate::model::ArchivedTask::new(parent));
    app.open_settings(Some(list));
    app.delete_armed = true;
    let text = settings_frame(&ctx, &mut app, Vec::new());
    assert!(
        text.iter()
            .any(|(text, _)| text == &format!("Delete “{}”?", app.lists[0].data.title))
    );
    assert!(text.iter().any(|(text, _)| text == "1 active tasks, 1 completed tasks, 1 archived tasks, and 2 subtasks will be removed. A copy is kept in local backups."));
    assert!(text.iter().any(|(text, _)| text == "Cancel deletion"));
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn opacity_slider_exposes_percentage_values_and_steps() {
    let (ctx, mut app) = super::super::tests::test_app();
    ctx.enable_accesskit();
    for (stored, percent) in [(0.2, 20.0), (0.85, 85.0), (1.0, 100.0)] {
        app.settings.background_opacity = stored;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| app.settings_contents(ui, app.palette()),
        );
        output.textures_delta.clear();
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        let (_, slider) = tree
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Slider
                    && node.label() == Some("Background opacity")
            })
            .unwrap();
        assert_eq!(slider.numeric_value(), Some(percent));
        assert_eq!(slider.min_numeric_value(), Some(20.0));
        assert_eq!(slider.max_numeric_value(), Some(100.0));
        assert_eq!(slider.numeric_value_step(), Some(1.0));
        assert_eq!(app.settings.background_opacity, stored);
    }
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn appearance_choices_and_sliders_have_accessible_states_and_names() {
    let (ctx, mut app) = super::super::tests::test_app();
    ctx.enable_accesskit();
    app.settings.font = FontChoice::Mono;
    app.settings.bold_text = true;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520.0, 1600.0),
            )),
            ..Default::default()
        },
        |ui| app.settings_contents(ui, app.palette()),
    );
    output.textures_delta.clear();
    let tree = output.platform_output.accesskit_update.as_ref().unwrap();
    for (label, selected) in [("Sans", false), ("Mono", true), ("Bold", true)] {
        let node = tree
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .unwrap_or_else(|| panic!("Missing {label}"));
        assert_eq!(
            node.1.toggled(),
            Some(if selected {
                egui::accesskit::Toggled::True
            } else {
                egui::accesskit::Toggled::False
            })
        );
    }
    for label in ["Text size", "Row spacing", "Background opacity"] {
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Slider
                    && node.label() == Some(label)),
            "Missing associated name {label}"
        );
    }
    output.textures_delta.clear();
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

fn archived_pair(app: &mut MinimalistApp) {
    app.lists[0]
        .data
        .tasks
        .push(crate::model::Task::new("Active"));
    app.lists[0].data.archive = ["Done one", "Done two"]
        .map(|text| crate::model::ArchivedTask::new(crate::model::Task::new(text)))
        .to_vec();
    app.save_list_index(0);
}

fn rect_of(texts: &[(String, egui::Rect)], label: &str) -> egui::Rect {
    texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Missing {label}"))
        .1
}

#[test]
fn clear_history_sits_above_delete_list_and_only_with_archived_tasks() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    app.open_settings(Some(list));
    let empty = settings_frame(&ctx, &mut app, Vec::new());
    assert!(!empty.iter().any(|(text, _)| text == "Clear History"));
    archived_pair(&mut app);
    let shown = settings_frame(&ctx, &mut app, Vec::new());
    assert!(rect_of(&shown, "Clear History").top() < rect_of(&shown, "Delete list").top());
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clear_history_arms_cancels_and_confirms_in_two_steps() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    archived_pair(&mut app);
    app.open_settings(Some(list));
    click(&ctx, &mut app, "Clear History");
    assert!(app.clear_history_armed);
    let armed = settings_frame(&ctx, &mut app, Vec::new());
    for label in [
        "Remove all 2 archived tasks from History?",
        "Confirm clear History",
        "Cancel",
    ] {
        rect_of(&armed, label);
    }
    assert_eq!(app.lists[0].data.archive.len(), 2);
    click(&ctx, &mut app, "Cancel");
    assert!(!app.clear_history_armed);
    assert_eq!(app.lists[0].data.archive.len(), 2);
    click(&ctx, &mut app, "Clear History");
    click(&ctx, &mut app, "Confirm clear History");
    assert!(!app.clear_history_armed);
    let stored = storage::load_workspace(&app.workspace).unwrap();
    assert!(stored.lists[0].data.archive.is_empty());
    assert_eq!(stored.lists[0].data.tasks[0].text, "Active");
    let after = settings_frame(&ctx, &mut app, Vec::new());
    assert!(!after.iter().any(|(text, _)| text == "Clear History"));
    assert!(after.iter().any(|(text, _)| text == "Delete list"));
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn arming_clear_history_and_delete_list_disarms_the_other() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    archived_pair(&mut app);
    app.open_settings(Some(list));
    click(&ctx, &mut app, "Clear History");
    click(&ctx, &mut app, "Delete list");
    assert!(app.delete_armed && !app.clear_history_armed);
    click(&ctx, &mut app, "Clear History");
    assert!(app.clear_history_armed && !app.delete_armed);
    click(&ctx, &mut app, "App settings");
    click(&ctx, &mut app, "Current list options");
    assert_eq!(app.settings_list_id, Some(list));
    assert!(!app.clear_history_armed && !app.delete_armed);
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

fn app_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let time = ctx.input(|input| input.time) + 0.3;
    app.update_motion(ctx);
    super::super::tests::run_app_frame(
        ctx,
        app,
        super::super::tests::frame_input(egui::vec2(520.0, 1600.0), time, events),
    )
    .shapes
    .into_iter()
    .filter_map(|shape| match shape.shape {
        egui::epaint::Shape::Text(text) => Some((
            text.galley.text().to_owned(),
            egui::Rect::from_min_size(text.pos, text.galley.size()),
        )),
        _ => None,
    })
    .collect()
}

fn key_edge(key: egui::Key, pressed: bool) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

/// Focuses the unarmed Clear History control through the keyboard focus request.
fn focus_clear_history(ctx: &egui::Context, app: &mut MinimalistApp) -> egui::Id {
    let mut texts = Vec::new();
    for _ in 0..6 {
        texts = app_frame(ctx, app, Vec::new());
    }
    let center = rect_of(&texts, "Clear History").center();
    let control = app
        .keyboard
        .settings_rows
        .iter()
        .flatten()
        .copied()
        .find(|id| {
            ctx.read_response(*id)
                .is_some_and(|response| response.rect.contains(center))
        })
        .unwrap();
    app.request_task_focus = false;
    app.keyboard.pending = Some(control);
    for _ in 0..4 {
        app_frame(ctx, app, Vec::new());
    }
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(control));
    control
}

#[test]
fn held_enter_or_space_arms_clear_history_but_never_confirms_it() {
    for action in [egui::Key::Enter, egui::Key::Space] {
        let (ctx, mut app) = super::super::tests::test_app();
        let list = app.lists[0].key;
        archived_pair(&mut app);
        app.open_settings(Some(list));
        let control = focus_clear_history(&ctx, &mut app);
        app_frame(&ctx, &mut app, key_edge(action, true));
        assert!(app.clear_history_armed);
        for _ in 0..5 {
            app_frame(&ctx, &mut app, key_edge(action, true));
            assert!(app.clear_history_armed);
            assert_eq!(app.lists[0].data.archive.len(), 2);
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(control));
        }
        app_frame(&ctx, &mut app, key_edge(action, false));
        for _ in 0..2 {
            app_frame(
                &ctx,
                &mut app,
                super::super::tests::chord(action, egui::Modifiers::NONE),
            );
        }
        assert!(app.lists[0].data.archive.is_empty());
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn keyboard_confirmed_clear_history_keeps_focus_so_arrow_keys_still_move() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    archived_pair(&mut app);
    app.open_settings(Some(list));
    focus_clear_history(&ctx, &mut app);
    for pressed in [true, false] {
        app_frame(&ctx, &mut app, key_edge(egui::Key::Enter, pressed));
    }
    assert!(app.clear_history_armed);
    app_frame(
        &ctx,
        &mut app,
        super::super::tests::chord(egui::Key::Enter, egui::Modifiers::NONE),
    );
    for _ in 0..4 {
        app_frame(&ctx, &mut app, Vec::new());
    }
    assert!(app.lists[0].data.archive.is_empty());
    let name = egui::Id::new(("settings-list-name", list));
    let focused = || ctx.memory(|memory| memory.focused());
    assert_eq!(focused(), Some(name));
    let row = app
        .keyboard
        .settings_rows
        .iter()
        .position(|ids| ids.contains(&name))
        .unwrap();
    let below = app.keyboard.settings_rows[row + 1][0];
    for (key, expected) in [(egui::Key::ArrowDown, below), (egui::Key::ArrowUp, name)] {
        app_frame(
            &ctx,
            &mut app,
            super::super::tests::chord(key, egui::Modifiers::NONE),
        );
        for _ in 0..3 {
            app_frame(&ctx, &mut app, Vec::new());
        }
        assert_eq!(focused(), Some(expected));
    }
    std::fs::remove_dir_all(&app.workspace).unwrap();
}
