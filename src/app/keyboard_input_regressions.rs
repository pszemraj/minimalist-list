//! Editor focus, destructive shortcuts, and capture-entry focus rendering.

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

#[test]
fn task_entry_uses_its_frame_glow_without_an_inner_focus_ring() {
    let (ctx, app) = test_app();
    let list = app.lists[0].key;
    for (id, expected_ring) in [
        (Id::new(("new-task", list)), false),
        (Id::new("other-field"), true),
    ] {
        let mut text = String::new();
        let palette = app.palette();
        let mut target_rect = egui::Rect::NOTHING;
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = crate::app::tests::run_frame(&ctx, Default::default(), |ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
                response.request_focus();
                target_rect = response.rect.expand(1.0);
                app.keyboard_focus_ui(ui, palette);
            });
        }
        let has_inner_ring = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::epaint::Shape::Rect(rect)
                if rect.rect == target_rect
                    && rect.stroke.width == 1.5
                    && rect.stroke.color == palette.accent)
        });
        assert_eq!(has_inner_ring, expected_ring);
    }
    cleanup(&app);
}
