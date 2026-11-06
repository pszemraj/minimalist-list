//! Editor focus, destructive shortcuts, focus-ring rendering, and recovery focus.

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
fn opening_an_empty_checklist_focuses_its_add_field() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Parent");
    let parent = task.id;
    app.lists[0].data.tasks.push(task);
    idle(&ctx, &mut app);
    let opener = Id::new(("task-checklist", list, parent));
    let pos = ctx.read_response(opener).unwrap().rect.center();
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
    let entry = Id::new(("new-subtask", list, parent));
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(entry));
    type_text(&ctx, &mut app, "First child");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(entry));
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks[0].subtasks[0].text, "First child");
    cleanup(&app);
}

#[test]
fn tab_reaches_child_text_and_enter_opens_its_editor() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks.push(crate::model::Subtask::new("Original"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    app.switch_subtask_panel(Some((list, parent)));
    focus(&ctx, &mut app, FocusItem::Subtask(list, parent, child).id());
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new(("edit-subtask-button", list, parent, child)))
    );
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new(("edit-subtask", list, parent, child)))
    );
    assert!(app.details.editor.is_some());
    cleanup(&app);
}

#[test]
fn right_from_the_checklist_control_enters_its_first_child() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks.push(crate::model::Subtask::new("Child"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    let opener = Id::new(("task-checklist", list, parent));
    focus(&ctx, &mut app, opener);
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.subtasks_for, Some((list, parent)));
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Subtask(list, parent, child).id())
    );
    cleanup(&app);
}

#[test]
fn existing_child_editors_keep_navigation_keys_for_the_text_cursor() {
    for navigation in [
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::Home,
        Key::End,
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Parent");
        task.subtasks.push(crate::model::Subtask::new("Original"));
        task.subtasks[0].completed = true;
        let parent = task.id;
        let child = task.subtasks[0].id;
        app.lists[0].data.tasks.push(task);
        app.switch_subtask_panel(Some((list, parent)));
        app.begin_subtask_edit(list, parent, child);
        idle(&ctx, &mut app);
        let editor = Id::new(("edit-subtask", list, parent, child));
        key(&ctx, &mut app, navigation, Modifiers::NONE);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(editor),
            "{navigation:?}"
        );
        assert_eq!(app.subtasks_for, Some((list, parent)), "{navigation:?}");
        assert!(app.details.editor.is_some(), "{navigation:?}");
        type_text(&ctx, &mut app, "!");
        key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        let saved_child = &saved.lists[0].data.tasks[0].subtasks[0];
        assert!(saved_child.text.contains('!'));
        assert!(saved_child.completed);
        cleanup(&app);
    }
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

#[test]
fn scrolled_settings_focus_outline_stays_in_the_visible_region() {
    let (ctx, mut app) = test_app();
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    idle(&ctx, &mut app);
    // The workspace field draws its own glow, so use the frameless button below it.
    let control = tab_to_label(&ctx, &mut app, "Choose folder");
    let output = frame(&ctx, &mut app, Vec::new(), 430.0);
    let response = ctx.read_response(control).unwrap();
    assert!(output.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::epaint::Shape::Rect(rect)
            if rect.rect == response.rect.expand(1.0)
                && rect.stroke.width == 1.5
                && clipped.clip_rect.contains_rect(rect.rect))
    }));
    // Scroll until the control is hidden, back by points until it reappears, then one
    // point away, so it ends just past the clip edge where its outline could linger.
    let pos = response.rect.center();
    let steps = [-24.0, 1.0, -1.0];
    let mut phase = 0;
    for _ in 0..60 {
        let visible = ctx
            .read_response(control)
            .unwrap()
            .interact_rect
            .is_positive();
        if visible == (phase == 1) {
            phase += 1;
            if phase == steps.len() {
                break;
            }
        }
        frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, steps[phase]),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
            430.0,
        );
        idle(&ctx, &mut app);
    }
    assert_eq!(
        phase,
        steps.len(),
        "the control must end just past the clip edge"
    );
    let output = frame(&ctx, &mut app, Vec::new(), 430.0);
    let response = ctx.read_response(control).unwrap();
    assert_eq!(ctx.memory(|m| m.focused()), Some(control));
    assert!(response.rect.intersects(ctx.content_rect()));
    assert!(!response.interact_rect.is_positive(), "{response:?}");
    for clipped in output.shapes {
        if let egui::epaint::Shape::Rect(rect) = clipped.shape
            && rect.rect == response.rect.expand(1.0)
            && rect.stroke.width == 1.5
        {
            assert!(
                !rect.rect.intersect(clipped.clip_rect).is_positive(),
                "scrolled-away focus is visible through {:?}",
                clipped.clip_rect
            );
        }
    }
    cleanup(&app);
}

fn context_fixture() -> (egui::Context, MinimalistApp, Uuid, Vec<Uuid>) {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["A", "B", "C"].map(Task::new).to_vec();
    app.lists[0].data.tasks[1]
        .subtasks
        .push(crate::model::Subtask::new("Child"));
    let tasks = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
    assert!(app.save_list_index(0));
    app.request_task_focus = false;
    idle(&ctx, &mut app);
    (ctx, app, list, tasks)
}

fn saved_tasks(app: &MinimalistApp) -> Vec<Task> {
    storage::load_workspace(&app.workspace).unwrap().lists[0]
        .data
        .tasks
        .clone()
}

#[test]
fn task_context_shortcut_opens_the_focused_row_editor_and_saves() {
    let (ctx, mut app, list, tasks) = context_fixture();
    let row = FocusItem::Task(list, tasks[1]).id();
    focus(&ctx, &mut app, row);
    key(&ctx, &mut app, Key::I, Modifiers::COMMAND);
    let editor = app.task_context.as_ref().unwrap();
    assert_eq!(
        (editor.list, editor.task, editor.archived),
        (list, tasks[1], false)
    );
    assert!(editor.text.is_empty(), "the shortcut inserts no text");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new("task-context-text"))
    );
    type_text(&ctx, &mut app, "Booking ref 42");
    assert!(app.save_task_context());
    idle(&ctx, &mut app);
    assert!(app.task_context.is_none());
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(row));
    let saved = saved_tasks(&app);
    assert_eq!(saved[1].context, "Booking ref 42");
    assert!(saved[0].context.is_empty() && saved[2].context.is_empty());
    cleanup(&app);
}

#[test]
fn task_context_shortcut_from_a_child_control_opens_the_parent_editor() {
    let (ctx, mut app, list, tasks) = context_fixture();
    app.switch_subtask_panel(Some((list, tasks[1])));
    let child = app.lists[0].data.tasks[1].subtasks[0].id;
    focus(
        &ctx,
        &mut app,
        FocusItem::Subtask(list, tasks[1], child).id(),
    );
    key(&ctx, &mut app, Key::I, Modifiers::COMMAND);
    let editor = app.task_context.as_ref().unwrap();
    assert_eq!((editor.list, editor.task), (list, tasks[1]));
    cleanup(&app);
}

#[test]
fn task_context_shortcut_saves_typed_title_text_before_opening() {
    let (ctx, mut app, list, tasks) = context_fixture();
    app.editing = Some((list, tasks[1]));
    app.edit_text = "B".into();
    app.request_edit_focus = true;
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new(("edit-task", list, tasks[1])))
    );
    // Typing and the shortcut arrive in one batch; the text must land first.
    let mut events = vec![Event::Text(" edited".into())];
    events.extend(chord(Key::I, Modifiers::COMMAND));
    frame(&ctx, &mut app, events, 430.0);
    idle(&ctx, &mut app);
    assert!(app.editing.is_none());
    let editor = app.task_context.as_ref().unwrap();
    assert_eq!((editor.list, editor.task), (list, tasks[1]));
    assert!(editor.text.is_empty());
    assert_eq!(saved_tasks(&app)[1].text, "B edited");
    cleanup(&app);
}

#[test]
fn task_context_shortcut_opens_history_rows_read_only() {
    let (ctx, mut app, list, tasks) = context_fixture();
    app.lists[0].data.tasks[0].context = "Saved notes".into();
    app.lists[0].data.tasks[0].toggle();
    app.archive_completed(list);
    app.history_open = true;
    idle(&ctx, &mut app);
    let row = FocusItem::Archived(list, tasks[0]).id();
    focus(&ctx, &mut app, row);
    key(&ctx, &mut app, Key::I, Modifiers::COMMAND);
    let editor = app.task_context.as_ref().unwrap();
    assert_eq!(
        (editor.list, editor.task, editor.archived),
        (list, tasks[0], true)
    );
    assert_eq!(editor.text, "Saved notes");
    type_text(&ctx, &mut app, "ignored");
    assert_eq!(app.task_context.as_ref().unwrap().text, "Saved notes");
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(app.task_context.is_none());
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(row));
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .archive[0]
            .task
            .context,
        "Saved notes"
    );
    cleanup(&app);
}

#[test]
fn task_context_shortcut_does_nothing_from_task_entry() {
    let (ctx, mut app, list, _) = context_fixture();
    let entry = Id::new(("new-task", list));
    focus(&ctx, &mut app, entry);
    key(&ctx, &mut app, Key::I, Modifiers::COMMAND);
    assert!(app.task_context.is_none());
    assert!(app.new_task.is_empty());
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(entry));
    cleanup(&app);
}
