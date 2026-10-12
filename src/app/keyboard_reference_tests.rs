//! Shortcut reference interactions and platform-specific key labels.

use super::*;

#[test]
fn shortcuts_reference_round_trips_from_main_screens_for_ctrl_and_cmd() {
    for (command, slash_modifiers) in [
        Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        },
        Modifiers {
            mac_cmd: true,
            command: true,
            ..Modifiers::NONE
        },
    ]
    .into_iter()
    .flat_map(|command| [(command, command), (command, command | Modifiers::SHIFT)])
    {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks.push(Task::new("Keep this task"));
        idle(&ctx, &mut app);
        key(&ctx, &mut app, Key::Slash, Modifiers::NONE);
        assert!(app.shortcuts.is_none());
        key(&ctx, &mut app, Key::Slash, Modifiers::SHIFT);
        assert!(app.shortcuts.is_none());
        for screen in 0..3 {
            match screen {
                0 => type_text(&ctx, &mut app, "Draft / text"),
                1 => key(
                    &ctx,
                    &mut app,
                    BACK_TO_LISTS.logical_key,
                    BACK_TO_LISTS.modifiers,
                ),
                _ => key(&ctx, &mut app, Key::Comma, command),
            }
            let original = ctx.memory(|memory| memory.focused());
            key(&ctx, &mut app, Key::Slash, slash_modifiers);
            assert!(app.shortcuts.is_some());
            assert_ne!(ctx.memory(|memory| memory.focused()), original);
            key(&ctx, &mut app, Key::Slash, slash_modifiers);
            key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
            key(&ctx, &mut app, Key::N, command);
            key(
                &ctx,
                &mut app,
                BACK_TO_LISTS.logical_key,
                BACK_TO_LISTS.modifiers,
            );
            assert!(app.shortcuts.is_some());
            assert!(app.quick_capture.is_none());
            assert_eq!(app.lists[0].data.tasks.len(), 1);
            key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
            assert!(app.shortcuts.is_none());
            assert_eq!(ctx.memory(|memory| memory.focused()), original);
            assert_eq!(app.new_task, "Draft / text");
            assert!(match screen {
                0 => matches!(app.screen, Screen::List(id) if id == list),
                1 => matches!(app.screen, Screen::Overview),
                _ => matches!(app.screen, Screen::Settings),
            });
        }
        cleanup(&app);
    }
}

#[test]
fn shortcuts_reference_preserves_capture_and_find_drafts_and_return_focus() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Captured after reading shortcuts");
    let query = ctx.memory(|memory| memory.focused());
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    for _ in 0..6 {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
        assert!(app.shortcuts.is_none());
        assert!(app.quick_capture.is_some());
    }
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        430.0,
    );
    idle(&ctx, &mut app);
    assert!(app.quick_capture.is_some());
    assert_eq!(ctx.memory(|memory| memory.focused()), query);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        app.lists[0].data.tasks[0].text,
        "Captured after reading shortcuts"
    );
    let task = app.lists[0].data.tasks[0].id;
    key(&ctx, &mut app, Key::F, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Captured after");
    let query = ctx.memory(|memory| memory.focused());
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(app.find.is_some());
    assert_eq!(ctx.memory(|memory| memory.focused()), query);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(list, task).id())
    );
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    for _ in 0..6 {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
        assert!(app.shortcuts.is_none());
        assert!(matches!(app.screen, Screen::Settings));
    }
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        430.0,
    );
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == list));
    cleanup(&app);
}

#[test]
fn shortcuts_reference_close_button_returns_to_saved_inline_edit() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Before");
    app.lists[0].data.tasks.push(task.clone());
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "After / editing");
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    assert!(app.editing.is_none());
    assert_eq!(app.lists[0].data.tasks[0].text, "After / editing");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.shortcuts.is_none());
    assert!(app.editing.is_none());
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .tasks[0]
            .text,
        "After / editing"
    );
    cleanup(&app);
}

#[test]
fn shortcuts_reference_dismissal_accepts_the_very_next_input() {
    let (ctx, mut app) = test_app();
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "Start");
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    let press = |key| {
        [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .collect()
    };
    frame(&ctx, &mut app, press(Key::Escape), 430.0);
    type_text(&ctx, &mut app, " finish");
    assert_eq!(app.new_task, "Start finish");
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Immediate capture");
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    frame(&ctx, &mut app, press(Key::Escape), 430.0);
    frame(&ctx, &mut app, press(Key::Enter), 430.0);
    assert!(app.quick_capture.is_none());
    assert_eq!(app.lists[0].data.tasks[0].text, "Immediate capture");
    let task = app.lists[0].data.tasks[0].id;
    let list = app.lists[0].key;
    key(&ctx, &mut app, Key::F, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Immediate capture");
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    frame(&ctx, &mut app, press(Key::Escape), 430.0);
    frame(&ctx, &mut app, press(Key::Enter), 430.0);
    assert!(app.find.is_none());
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(list, task).id())
    );
    cleanup(&app);
}

#[test]
fn reference_uses_shared_bindings_and_native_key_names() {
    for (binding, regular, mac) in [
        (SHOW_SHORTCUTS, "Ctrl+/", "Cmd+/"),
        (NEXT_LIST, "Ctrl+Tab", "Ctrl+Tab"),
        (PREVIOUS_LIST, "Ctrl+Shift+Tab", "Ctrl+Shift+Tab"),
        (CLEAR_COMPLETED, "Ctrl+Shift+Backspace", "Shift+Cmd+Delete"),
        (DELETE_TASK, "Delete", "Fn+Delete"),
        (MOVE_TASK_UP, "Alt+Up", "Option+Up"),
        (
            KeyboardShortcut::new(Modifiers::ALT, Key::ArrowLeft),
            "Alt+Left",
            "Option+Left",
        ),
        (
            KeyboardShortcut::new(Modifiers::COMMAND, Key::OpenBracket),
            "Ctrl+[",
            "Cmd+[",
        ),
        (
            KeyboardShortcut::new(Modifiers::NONE, Key::Home),
            "Home",
            "Fn+Left",
        ),
        (
            KeyboardShortcut::new(Modifiers::NONE, Key::End),
            "End",
            "Fn+Right",
        ),
    ] {
        assert_eq!(shortcut_for_platform(binding, false), regular);
        assert_eq!(shortcut_for_platform(binding, true), mac);
    }
    let groups = shortcut_groups();
    for (action, binding) in [
        ("Keyboard shortcuts", SHOW_SHORTCUTS),
        ("Settings (main screens)", OPEN_SETTINGS),
        ("Quick add", QUICK_ADD),
        ("Find tasks and History", FIND),
        ("New list (Lists, list, Settings)", NEW_LIST),
        ("Back to Lists", BACK_TO_LISTS),
        ("Move completed tasks to History", CLEAR_COMPLETED),
        ("Delete the task", DELETE_TASK),
    ] {
        let (_, label) = groups
            .iter()
            .flat_map(|(_, rows)| rows)
            .find(|(name, _)| *name == action)
            .unwrap();
        assert_eq!(*label, shortcut(binding));
    }
    let (_, first_last) = groups
        .iter()
        .flat_map(|(_, rows)| rows)
        .find(|(name, _)| *name == "First / last task")
        .unwrap();
    assert_eq!(
        *first_last,
        format!(
            "{} / {}",
            shortcut(KeyboardShortcut::new(Modifiers::NONE, Key::Home)),
            shortcut(KeyboardShortcut::new(Modifiers::NONE, Key::End)),
        )
    );
}
