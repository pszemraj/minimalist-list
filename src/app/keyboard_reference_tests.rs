//! Shortcut reference interactions and platform-specific key labels.

use super::*;

#[test]
fn shortcut_reference_displays_configured_and_disabled_global_shortcuts() {
    for width in [320.0, 520.0] {
        let (ctx, mut app) = test_app();
        let default = crate::hotkey::label(&app.settings.global_shortcut);
        app.open_shortcuts();
        for (configured, expected) in [
            (
                "CommandOrControl+Alt+KeyK",
                if cfg!(target_os = "macos") {
                    "Cmd+Option+K"
                } else {
                    "Ctrl+Alt+K"
                },
            ),
            ("", "Disabled"),
        ] {
            app.settings.global_shortcut = configured.into();
            for _ in 0..3 {
                frame(&ctx, &mut app, Vec::new(), width);
            }
            let output = frame(&ctx, &mut app, Vec::new(), width);
            let text: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            assert!(text.contains(&"Show window (global)"), "{text:?}");
            assert!(
                text.contains(&expected),
                "Missing {expected} at width {width}"
            );
            assert!(!text.contains(&default.as_str()));
        }
        cleanup(&app);
    }
}

#[test]
fn held_enter_stays_with_the_layer_that_received_the_first_press() {
    let (ctx, mut app) = test_app();
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "Unsubmitted list draft");
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Capture task");
    key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
    key_edge(&ctx, &mut app, Key::Enter, true);
    assert!(app.shortcuts.is_none());
    for _ in 0..5 {
        key_edge(&ctx, &mut app, Key::Enter, true);
        assert!(app.quick_capture.is_some());
        assert!(
            storage::load_workspace(&app.workspace).unwrap().lists[0]
                .data
                .tasks
                .is_empty()
        );
    }
    key_edge(&ctx, &mut app, Key::Enter, false);
    key_edge(&ctx, &mut app, Key::Enter, true);
    assert!(app.quick_capture.is_none());
    assert_eq!(app.lists[0].data.tasks.len(), 1);
    let path = app.lists[0].path.clone();
    let saved = std::fs::read(&path).unwrap();
    for _ in 0..5 {
        key_edge(&ctx, &mut app, Key::Enter, true);
        assert_eq!(app.new_task, "Unsubmitted list draft");
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    key_edge(&ctx, &mut app, Key::Enter, false);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.new_task.is_empty());
    let loaded = storage::load_workspace(&app.workspace).unwrap();
    let titles: Vec<_> = loaded.lists[0]
        .data
        .tasks
        .iter()
        .map(|task| task.text.as_str())
        .collect();
    assert_eq!(titles, ["Unsubmitted list draft", "Capture task"]);
    cleanup(&app);
}

#[test]
fn opening_overlays_during_a_pointer_drag_cancels_without_saving_and_allows_reload() {
    for open_key in [Key::N, Key::F, Key::Slash, Key::Comma] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["First", "Second"].map(Task::new).to_vec();
        app.save_list_index(0);
        idle(&ctx, &mut app);
        let task = app.lists[0].data.tasks[1].id;
        let rect = ctx
            .read_response(FocusItem::Task(list, task).id())
            .unwrap()
            .rect;
        let origin = egui::pos2(rect.left() + 4.0, rect.bottom() - 3.0);
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut app,
            vec![Event::PointerMoved(origin), pointer(origin, true)],
            430.0,
        );
        let moved = origin + egui::vec2(0.0, -50.0);
        for _ in 0..2 {
            frame(&ctx, &mut app, vec![Event::PointerMoved(moved)], 430.0);
        }
        assert!(app.drag.is_some());
        let path = app.lists[0].path.clone();
        let saved = std::fs::read(&path).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        key(&ctx, &mut app, open_key, Modifiers::COMMAND);
        frame(&ctx, &mut app, vec![pointer(moved, false)], 430.0);
        key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        assert!(app.drag.is_none(), "opening {open_key:?} cancels the drag");
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        let mut external = app.lists[0].clone();
        external.data.title = "Changed externally after drag".into();
        storage::save_list(&external).unwrap();
        app.next_scan = std::time::Instant::now();
        app.scan_external_changes();
        assert_eq!(app.lists[0].data.title, external.data.title);
        assert_eq!(app.lists[0].data.tasks[1].id, task);
        cleanup(&app);
    }
}

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
        (BACKSPACE_TO_LISTS, "Backspace", "Delete"),
        (
            KeyboardShortcut::new(Modifiers::NONE, Key::PageUp),
            "PageUp",
            "Fn+Up",
        ),
        (
            KeyboardShortcut::new(Modifiers::NONE, Key::PageDown),
            "PageDown",
            "Fn+Down",
        ),
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
    ] {
        let (_, label) = groups
            .iter()
            .flat_map(|(_, rows)| rows)
            .find(|(name, _)| *name == action)
            .unwrap();
        assert_eq!(*label, shortcut(binding));
    }
    for (action, binding) in [
        (
            "Copy task text",
            KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::C),
        ),
        (
            "Undo deletion",
            KeyboardShortcut::new(Modifiers::COMMAND, Key::Z),
        ),
    ] {
        let (_, label) = groups
            .iter()
            .flat_map(|(_, rows)| rows)
            .find(|(name, _)| *name == action)
            .unwrap();
        assert_eq!(*label, shortcut(binding));
    }
    assert!(
        groups
            .iter()
            .flat_map(|(_, rows)| rows)
            .all(|(_, binding)| ![
                "Enter",
                "Escape",
                "Arrow keys",
                "Up / Down",
                "Enter / Space"
            ]
            .contains(&binding.as_str()))
    );
}
