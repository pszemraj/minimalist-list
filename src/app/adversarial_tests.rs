//! Adversarial persistence and transition journeys using isolated task files.

use super::*;
use crate::model::Task;

fn ui_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    app.view_zoom = 1.0;
    let input = egui::RawInput {
        time: Some(ctx.input(|input| input.time) + 0.3),
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(520.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut MinimalistApp) {
    for _ in 0..3 {
        ui_frame(ctx, app, Vec::new());
    }
}

fn key_and_text(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    let mut events: Vec<_> = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect();
    events.push(egui::Event::Text("Native".into()));
    events
}

#[test]
fn expiring_notice_keeps_settings_opener_in_the_accessibility_tree() {
    let (ctx, mut app) = tests::test_app();
    ctx.enable_accesskit();
    app.acknowledge("Moved to destination".into());
    let output = ui_frame(&ctx, &mut app, Vec::new());
    let tree = output.platform_output.accesskit_update.unwrap();
    let (opener, node) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("List options"))
        .unwrap();
    let opener = *opener;
    let bounds = node.bounds().unwrap();
    let pos = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    ui_frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert!(matches!(app.screen, Screen::Settings));
    idle(&ctx, &mut app);
    app.persistence.notice = None;
    ui_frame(&ctx, &mut app, Vec::new());
    let events = key_and_text(egui::Key::Escape, egui::Modifiers::NONE)
        .into_iter()
        .take(2)
        .collect();
    let output = ui_frame(&ctx, &mut app, events);
    let tree = output.platform_output.accesskit_update.unwrap();
    assert!(
        tree.nodes.iter().any(|(id, _)| *id == tree.focus),
        "focused node {:?} is missing from the accessibility tree",
        tree.focus
    );
    assert_eq!(tree.focus, opener);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn settings_return_keeps_checklist_openers_in_every_accessibility_frame() {
    for show_previews in [false, true] {
        for control in ["completion", "text", "entry", "editor"] {
            let (ctx, mut app) = tests::test_app();
            ctx.enable_accesskit();
            let list = app.lists[0].key;
            let mut task = Task::new("Parent");
            let mut child = crate::model::Subtask::new("Edited child");
            child.completed = true;
            let child_id = child.id;
            task.subtasks.push(child);
            let task_id = task.id;
            app.lists[0].data.tasks.push(task);
            app.save_list_index(0);
            app.settings.show_subtasks = show_previews;
            app.switch_subtask_panel(Some((list, task_id)));
            app.new_subtask = "Unsubmitted child".into();
            let opener = match control {
                "completion" | "editor" => {
                    keyboard::FocusItem::Subtask(list, task_id, child_id).id()
                }
                "text" => egui::Id::new(("edit-subtask-button", list, task_id, child_id)),
                _ => egui::Id::new(("new-subtask", list, task_id)),
            };
            let initial_focus = if control == "editor" {
                app.begin_subtask_edit(list, task_id, child_id);
                app.details.editor.as_mut().unwrap().text = "Saved child edit".into();
                egui::Id::new(("edit-subtask", list, task_id, child_id))
            } else {
                opener
            };
            app.request_task_focus = false;
            app.keyboard.pending = Some(initial_focus);
            let render = |app: &mut MinimalistApp, events| {
                let output = ui_frame(&ctx, app, events);
                let tree = output.platform_output.accesskit_update.unwrap();
                assert!(
                    tree.nodes.iter().any(|(id, _)| *id == tree.focus),
                    "{control}, previews={show_previews}: missing focused node {:?}",
                    tree.focus
                );
            };
            for _ in 0..3 {
                render(&mut app, Vec::new());
            }
            for key in [egui::Key::Comma, egui::Key::Escape] {
                let modifiers = if key == egui::Key::Comma {
                    egui::Modifiers::COMMAND
                } else {
                    egui::Modifiers::NONE
                };
                let events = key_and_text(key, modifiers).into_iter().take(2).collect();
                render(&mut app, events);
                for _ in 0..3 {
                    render(&mut app, Vec::new());
                }
            }
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(opener));
            assert_eq!(app.subtasks_for, Some((list, task_id)));
            assert_eq!(app.new_subtask, "Unsubmitted child");
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }
}

#[test]
fn externally_removed_settings_opener_never_receives_accessibility_focus() {
    let (ctx, mut app) = tests::test_app();
    ctx.enable_accesskit();
    let list = app.lists[0].key;
    let task = Task::new("Removed externally");
    let removed = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.request_task_focus = false;
    app.keyboard.pending = Some(keyboard::FocusItem::Task(list, removed).id());
    idle(&ctx, &mut app);
    app.open_settings(None);
    idle(&ctx, &mut app);
    let mut external = app.lists[0].clone();
    external.data.tasks.clear();
    storage::save_list(&external).unwrap();
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    let events = key_and_text(egui::Key::Escape, egui::Modifiers::NONE)
        .into_iter()
        .take(2)
        .collect();
    for events in [events, Vec::new(), Vec::new(), Vec::new()] {
        let output = ui_frame(&ctx, &mut app, events);
        let tree = output.platform_output.accesskit_update.unwrap();
        assert!(
            tree.nodes.iter().any(|(id, _)| *id == tree.focus),
            "missing focused node {:?}",
            tree.focus
        );
    }
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new(("new-task", list)))
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn escape_from_settings_and_typing_in_one_frame_reaches_the_returned_entry() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    app.open_settings(None);
    idle(&ctx, &mut app);
    app.settings.always_on_top = !app.settings.always_on_top;
    app.settings.window_decorations = !app.settings.window_decorations;
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(matches!(app.screen, Screen::List(_)));
    assert_eq!(app.new_task, "Native");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn text_before_escape_is_applied_to_list_name_before_closing_options() {
    let (ctx, mut app) = tests::test_app();
    let list = app.lists[0].key;
    app.open_settings(Some(list));
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new(("settings-list-name", list)))
    );
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::A, egui::Modifiers::COMMAND)
            .into_iter()
            .take(2)
            .collect(),
    );
    let mut events = vec![egui::Event::Text("New name before Escape".into())];
    events.extend(
        key_and_text(egui::Key::Escape, egui::Modifiers::NONE)
            .into_iter()
            .take(2),
    );
    ui_frame(&ctx, &mut app, events);
    assert_eq!(app.rename_input, "New name before Escape");
    idle(&ctx, &mut app);
    assert!(matches!(app.screen, Screen::List(_)));
    assert_eq!(app.lists[0].data.title, "New name before Escape");
    assert!(app.new_task.is_empty());
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.title, "New name before Escape");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn task_text_before_find_is_saved_while_later_text_reaches_the_query() {
    let (ctx, mut app) = tests::test_app();
    let list = app.lists[0].key;
    let task = Task::new("Original parent");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.editing = Some((list, id));
    app.edit_text = "Original parent".into();
    app.request_edit_focus = true;
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::A, egui::Modifiers::COMMAND)
            .into_iter()
            .take(2)
            .collect(),
    );
    let mut events = vec![egui::Event::Text("New parent before Find".into())];
    events.extend(key_and_text(egui::Key::F, egui::Modifiers::COMMAND));
    ui_frame(&ctx, &mut app, events);
    assert_eq!(app.edit_text, "New parent before Find");
    ui_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text(" fresh input".into())],
    );
    assert!(app.find.is_some());
    assert_eq!(app.lists[0].data.tasks[0].text, "New parent before Find");
    idle(&ctx, &mut app);
    let output = ui_frame(&ctx, &mut app, Vec::new());
    let query = ctx.read_response(egui::Id::new("find-query")).unwrap().rect;
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Native fresh input" && query.contains(text.pos))
    }));
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks[0].text, "New parent before Find");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn native_selection_paste_and_ime_commit_finish_before_escape() {
    for ime in [false, true] {
        let (ctx, mut app) = tests::test_app();
        let list = app.lists[0].key;
        app.open_settings(Some(list));
        idle(&ctx, &mut app);
        let mut events: Vec<_> = key_and_text(egui::Key::A, egui::Modifiers::COMMAND)
            .into_iter()
            .take(2)
            .collect();
        if ime {
            events.extend([
                egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "候補".into(),
                    active_range_chars: Some(0..2),
                }),
                egui::Event::Ime(egui::ImeEvent::Commit("新しい名前".into())),
            ]);
        } else {
            events.push(egui::Event::Paste("Pasted list name".into()));
        }
        events.extend(
            key_and_text(egui::Key::Escape, egui::Modifiers::NONE)
                .into_iter()
                .take(2),
        );
        ui_frame(&ctx, &mut app, events);
        idle(&ctx, &mut app);
        assert!(matches!(app.screen, Screen::List(_)));
        let expected = if ime {
            "新しい名前"
        } else {
            "Pasted list name"
        };
        assert_eq!(app.lists[0].data.title, expected);
        assert!(app.new_task.is_empty());
        assert_eq!(
            storage::load_workspace(&app.workspace).unwrap().lists[0]
                .data
                .title,
            expected
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn parent_edit_before_control_tab_saves_once_and_later_text_reaches_next_list() {
    let (ctx, mut app) = tests::test_app();
    let list = app.lists[0].key;
    let other =
        storage::create_list(&app.workspace, "Other list", crate::model::Accent::Mint).unwrap();
    let destination = other.key;
    app.lists.push(other);
    let parent = Task::new("Parent before edit");
    let id = parent.id;
    app.lists[0].data.tasks.push(parent);
    app.editing = Some((list, id));
    app.edit_text = "Parent before edit".into();
    app.request_edit_focus = true;
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::A, egui::Modifiers::COMMAND)
            .into_iter()
            .take(2)
            .collect(),
    );
    let mut events = vec![egui::Event::Paste("Parent after paste".into())];
    events.extend(key_and_text(egui::Key::Tab, egui::Modifiers::CTRL));
    ui_frame(&ctx, &mut app, events);
    idle(&ctx, &mut app);
    assert!(matches!(app.screen, Screen::List(active) if active == destination));
    assert_eq!(app.new_task, "Native");
    let source = app.list_index(list).unwrap();
    assert_eq!(app.lists[source].data.tasks.len(), 1);
    assert_eq!(app.lists[source].data.tasks[0].text, "Parent after paste");
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(
        saved
            .lists
            .iter()
            .find(|item| item.key == list)
            .unwrap()
            .data
            .tasks[0]
            .text,
        "Parent after paste"
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_settings_and_typing_in_one_frame_reaches_its_workspace_field() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::Comma, egui::Modifiers::COMMAND),
    );
    assert!(matches!(app.screen, Screen::Settings));
    assert!(
        app.workspace_input.contains("Native"),
        "{}",
        app.workspace_input
    );
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_find_and_typing_in_one_frame_reaches_the_query() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::F, egui::Modifiers::COMMAND),
    );
    idle(&ctx, &mut app);
    let output = ui_frame(&ctx, &mut app, Vec::new());
    assert!(app.find.is_some());
    let query = ctx.read_response(egui::Id::new("find-query")).unwrap().rect;
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Native" && query.contains(text.pos))
    }));
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_quick_capture_and_typing_in_one_frame_reaches_only_capture() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::N, egui::Modifiers::COMMAND),
    );
    idle(&ctx, &mut app);
    let output = ui_frame(&ctx, &mut app, Vec::new());
    let input = ctx
        .read_response(egui::Id::new("quick-capture-text"))
        .unwrap()
        .rect;
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Native" && input.contains(text.pos))
    }));
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn repeated_stale_instance_saves_preserve_a_newer_workspace_preference() {
    let (_, mut first) = tests::test_app();
    first.save_settings();
    let settings = storage::load_settings().unwrap();
    let ctx = egui::Context::default();
    let mut stale = MinimalistApp::new(&ctx, settings, None, None);
    let original = first.workspace.clone();
    let original_list = stale.lists[0].key;
    first.switch_workspace(&original.join("new-selection").to_string_lossy());
    let newer_selection = first.workspace.to_string_lossy().into_owned();
    for _ in 0..3 {
        stale.open_list(original_list);
        assert_eq!(
            storage::load_settings().unwrap().workspace_path,
            newer_selection
        );
    }
    std::fs::remove_dir_all(original).unwrap();
}

#[test]
fn workspace_switch_flushes_accepted_deletion_and_archive_animations() {
    for archive in [false, true] {
        let (_, mut app) = tests::test_app();
        let original = app.workspace.clone();
        let list = app.lists[0].key;
        let mut task = Task::new("Accepted mutation before folder change");
        task.completed = archive;
        let task_id = task.id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        if archive {
            app.begin_clear_completed(list);
            assert!(app.clear_animation.is_some());
        } else {
            app.begin_delete_task(list, task_id);
            assert!(app.delete_animation.is_some());
        }
        app.switch_workspace(&original.join("next-workspace").to_string_lossy());
        assert_ne!(app.workspace, original);
        let saved = storage::load_workspace(&original).unwrap();
        assert!(saved.lists[0].data.tasks.is_empty());
        assert_eq!(saved.lists[0].data.archive.len(), usize::from(archive));
        assert!(app.delete_animation.is_none());
        assert!(app.clear_animation.is_none());
        std::fs::remove_dir_all(original).unwrap();
    }
}
