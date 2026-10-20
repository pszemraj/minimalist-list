//! Editor focus, destructive shortcuts, text-field focus rendering, and recovery focus.

use super::*;

#[test]
fn clicking_away_from_a_subtask_editor_saves_without_stealing_focus() {
    for target_is_task_entry in [true, false] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Parent");
        task.subtasks.push(crate::model::Subtask::new("Original"));
        let parent = task.id;
        let child = task.subtasks[0].id;
        app.lists[0].data.tasks.push(task);
        app.subtasks_for = Some((list, parent));
        app.begin_subtask_edit(list, parent, child);
        idle(&ctx, &mut app);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(Id::new(("edit-subtask", list, parent, child)))
        );
        app.details.editor.as_mut().unwrap().text = "Saved on click".into();
        let target = if target_is_task_entry {
            Id::new(("new-task", list))
        } else {
            Id::new(("new-subtask", list, parent))
        };
        let pos = ctx.read_response(target).unwrap().rect.center();
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                430.0,
            );
        }
        idle(&ctx, &mut app);
        assert!(app.details.editor.is_none());
        assert_eq!(ctx.memory(|m| m.focused()), Some(target));
        key(&ctx, &mut app, Key::Space, Modifiers::NONE);
        type_text(&ctx, &mut app, "Still typing here");
        assert_eq!(ctx.memory(|m| m.focused()), Some(target));
        assert!(
            if target_is_task_entry {
                &app.new_task
            } else {
                &app.new_subtask
            }
            .contains("Still typing here")
        );
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(
            saved.lists[0].data.tasks[0].subtasks[0].text,
            "Saved on click"
        );
        assert!(!saved.lists[0].data.tasks[0].subtasks[0].completed);
        cleanup(&app);
    }
}

#[test]
fn subtask_enter_returns_focus_to_its_completion_control() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks.push(crate::model::Subtask::new("Original"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    app.subtasks_for = Some((list, parent));
    app.begin_subtask_edit(list, parent, child);
    idle(&ctx, &mut app);
    app.details.editor.as_mut().unwrap().text = "Saved with Enter".into();
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.details.editor.is_none());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Subtask(list, parent, child).id())
    );
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(
        saved.lists[0].data.tasks[0].subtasks[0].text,
        "Saved with Enter"
    );
    assert!(!saved.lists[0].data.tasks[0].subtasks[0].completed);
    cleanup(&app);
}

#[test]
fn archive_shortcut_leaves_task_and_subtask_text_editors_untouched() {
    for (editor, text) in [
        (0, "Draft"),
        (0, " "),
        (1, "Draft"),
        (1, ""),
        (1, " "),
        (2, "Draft"),
        (2, ""),
        (2, " "),
        (3, "Draft"),
        (3, " "),
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Completed parent");
        task.toggle();
        task.subtasks.push(crate::model::Subtask::new("Child"));
        let parent = task.id;
        let child = task.subtasks[0].id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        match editor {
            0 => {
                app.new_task = text.into();
                focus(&ctx, &mut app, Id::new(("new-task", list)));
            }
            1 => {
                app.editing = Some((list, parent));
                app.edit_text = text.into();
                app.request_edit_focus = true;
                idle(&ctx, &mut app);
            }
            2 => {
                app.subtasks_for = Some((list, parent));
                app.begin_subtask_edit(list, parent, child);
                app.details.editor.as_mut().unwrap().text = text.into();
                idle(&ctx, &mut app);
            }
            _ => {
                app.subtasks_for = Some((list, parent));
                app.new_subtask = text.into();
                focus(&ctx, &mut app, Id::new(("new-subtask", list, parent)));
            }
        }
        assert!(ctx.text_edit_focused());
        key(
            &ctx,
            &mut app,
            Key::Backspace,
            Modifiers::COMMAND | Modifiers::SHIFT,
        );
        assert!(app.clear_animation.is_none());
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks.len(), 1);
        assert!(saved.lists[0].data.archive.is_empty());
        app.finish_inline_edit();
        focus(&ctx, &mut app, FocusItem::Task(list, parent).id());
        key(
            &ctx,
            &mut app,
            Key::Backspace,
            Modifiers::COMMAND | Modifiers::SHIFT,
        );
        assert!(
            app.clear_animation.is_some(),
            "the shortcut must still work outside text editors"
        );
        app.archive_completed(list);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert!(saved.lists[0].data.tasks.is_empty());
        assert_eq!(saved.lists[0].data.archive[0].task.id, parent);
        cleanup(&app);
    }
}

#[test]
fn archive_shortcut_works_in_empty_capture_after_open_add_and_child_capture() {
    for scenario in 0..3 {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Completed parent");
        task.toggle();
        let parent = task.id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        app.open_list(list);
        idle(&ctx, &mut app);
        if scenario == 1 {
            type_text(&ctx, &mut app, "New active task");
            key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
            assert_eq!(app.lists[0].data.tasks.len(), 2);
            assert!(app.new_task.is_empty());
        } else if scenario == 2 {
            app.subtasks_for = Some((list, parent));
            focus(&ctx, &mut app, Id::new(("new-subtask", list, parent)));
            assert!(app.new_subtask.is_empty());
        }
        assert!(ctx.text_edit_focused());
        key(
            &ctx,
            &mut app,
            Key::Backspace,
            Modifiers::COMMAND | Modifiers::SHIFT,
        );
        assert!(app.clear_animation.is_some());
        app.clear_animation.as_mut().unwrap().1 =
            std::time::Instant::now() - std::time::Duration::from_secs(1);
        idle(&ctx, &mut app);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.archive[0].task.id, parent);
        assert_eq!(saved.lists[0].data.tasks.len(), usize::from(scenario == 1));
        cleanup(&app);
    }
}

#[test]
fn text_before_archive_shortcut_in_one_frame_protects_the_capture_draft() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Keep completed task active");
    task.toggle();
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    focus(&ctx, &mut app, Id::new(("new-task", list)));
    let mut events = vec![Event::Text("Typed before archive".into())];
    events.extend(chord(Key::Backspace, Modifiers::COMMAND | Modifiers::SHIFT));
    frame(&ctx, &mut app, events, 430.0);
    assert_eq!(app.new_task, "Typed before archive");
    assert!(app.clear_animation.is_none());
    idle(&ctx, &mut app);
    assert!(app.clear_animation.is_none());
    assert_eq!(app.lists[0].data.tasks.len(), 1);
    assert!(app.lists[0].data.archive.is_empty());
    cleanup(&app);
}

fn focus_ring_drawn(ctx: &egui::Context, app: &mut MinimalistApp, id: Id) -> bool {
    let output = frame(ctx, app, Vec::new(), 430.0);
    assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    let ring = ctx.read_response(id).unwrap().rect.expand(1.0);
    output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Rect(rect)
            if rect.rect == ring && rect.stroke.width == 1.5)
    })
}

#[test]
fn self_framed_text_fields_skip_the_keyboard_focus_ring() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks.push(crate::model::Subtask::new("Child"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    let entry = Id::new(("new-task", list));
    focus(&ctx, &mut app, entry);
    assert!(!focus_ring_drawn(&ctx, &mut app, entry));
    // The frameless inline task editor has no outline of its own.
    app.editing = Some((list, parent));
    app.edit_text = "Parent".into();
    app.request_edit_focus = true;
    idle(&ctx, &mut app);
    assert!(focus_ring_drawn(
        &ctx,
        &mut app,
        Id::new(("edit-task", list, parent))
    ));
    app.editing = None;
    app.subtasks_for = Some((list, parent));
    let entry = Id::new(("new-subtask", list, parent));
    focus(&ctx, &mut app, entry);
    assert!(!focus_ring_drawn(&ctx, &mut app, entry));
    app.begin_subtask_edit(list, parent, child);
    idle(&ctx, &mut app);
    assert!(!focus_ring_drawn(
        &ctx,
        &mut app,
        Id::new(("edit-subtask", list, parent, child))
    ));
    app.cancel_subtask_edit();
    app.subtasks_for = None;
    app.open_settings(Some(list));
    let rename = Id::new(("settings-list-name", list));
    focus(&ctx, &mut app, rename);
    assert!(!focus_ring_drawn(&ctx, &mut app, rename));
    app.settings_list_id = None;
    for id in [
        Id::new("settings-workspace"),
        Id::new("settings-global-shortcut"),
    ] {
        focus(&ctx, &mut app, id);
        assert!(!focus_ring_drawn(&ctx, &mut app, id));
    }
    app.go_to_overview();
    app.creating_list = true;
    app.request_list_focus = true;
    idle(&ctx, &mut app);
    assert!(!focus_ring_drawn(&ctx, &mut app, Id::new("new-list-title")));
    cleanup(&app);
}

#[test]
fn clicking_retry_workspace_returns_typing_to_the_task_entry() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    let lists = app.workspace.join("lists");
    let offline = app.workspace.join("lists-offline");
    std::fs::rename(&lists, &offline).unwrap();
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    assert!(app.persistence.workspace_failure.is_some());
    let output = frame(&ctx, &mut app, Vec::new(), 430.0);
    let retry = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == "Retry workspace" => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
            }
            _ => None,
        })
        .expect("recovery offers Retry workspace");
    std::fs::rename(&offline, &lists).unwrap();
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(retry),
                Event::PointerButton {
                    pos: retry,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            430.0,
        );
    }
    idle(&ctx, &mut app);
    assert!(app.persistence.workspace_failure.is_none());
    assert!(matches!(app.screen, Screen::List(id) if id == list));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("new-task", list)))
    );
    type_text(&ctx, &mut app, "Typed after recovery");
    assert_eq!(app.new_task, "Typed after recovery");
    cleanup(&app);
}
