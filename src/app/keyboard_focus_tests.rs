//! Regressions for navigation return targets and Settings keyboard transitions.

use super::*;

#[test]
fn held_control_activation_waits_across_completion_and_subtask_removal() {
    for action in [Key::Enter, Key::Space] {
        for control in 0..3 {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let mut task = Task::new("Parent");
            task.subtasks = ["First", "Second"].map(crate::model::Subtask::new).to_vec();
            let task_id = task.id;
            let first = task.subtasks[0].id;
            let second = task.subtasks[1].id;
            app.lists[0].data.tasks.push(task);
            app.save_list_index(0);
            if control == 0 {
                focus(&ctx, &mut app, FocusItem::Task(list, task_id).id());
                key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
            } else {
                app.subtasks_for = Some((list, task_id));
                focus(
                    &ctx,
                    &mut app,
                    FocusItem::Subtask(list, task_id, first).id(),
                );
                if control == 2 {
                    // Checklist text is now an editable button between completion and Delete.
                    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
                    assert_eq!(
                        ctx.memory(|memory| memory.focused()),
                        Some(Id::new(("edit-subtask-button", list, task_id, first)))
                    );
                    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
                }
            }
            key_edge(&ctx, &mut app, action, true);
            idle(&ctx, &mut app);
            let task = &app.lists[0].data.tasks[0];
            assert_eq!(task.completed, control == 0);
            if control == 2 {
                assert_eq!(task.subtasks.len(), 1);
                assert_eq!(task.subtasks[0].id, second);
                assert_eq!(
                    ctx.memory(|m| m.focused()),
                    Some(FocusItem::Subtask(list, task_id, second).id())
                );
            }
            assert_eq!(task.subtasks[0].completed, control == 1);
            let path = app.lists[0].path.clone();
            let saved = std::fs::read(&path).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            for _ in 0..5 {
                key_edge(&ctx, &mut app, action, true);
                idle(&ctx, &mut app);
                assert_eq!(std::fs::read(&path).unwrap(), saved);
                assert_eq!(
                    std::fs::metadata(&path).unwrap().modified().unwrap(),
                    modified
                );
            }
            key_edge(&ctx, &mut app, action, false);
            key(&ctx, &mut app, action, Modifiers::NONE);
            let loaded = storage::load_workspace(&app.workspace).unwrap();
            let task = &loaded.lists[0].data.tasks[0];
            assert!(!task.completed);
            assert_eq!(task.subtasks[0].completed, control == 2);
            cleanup(&app);
        }
    }
}

#[test]
fn held_child_edit_activation_keeps_the_editor_open_until_a_new_submit_press() {
    for action in [Key::Enter, Key::Space] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Parent");
        task.subtasks
            .push(crate::model::Subtask::new("Original child"));
        task.subtasks[0].completed = true;
        let task_id = task.id;
        let child = task.subtasks[0].id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        app.subtasks_for = Some((list, task_id));
        focus(
            &ctx,
            &mut app,
            Id::new(("edit-subtask-button", list, task_id, child)),
        );
        key_edge(&ctx, &mut app, action, true);
        idle(&ctx, &mut app);
        assert!(app.details.editor.is_some());
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(Id::new(("edit-subtask", list, task_id, child)))
        );
        key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
        type_text(&ctx, &mut app, "Corrected child");
        let saved = std::fs::read(&app.lists[0].path).unwrap();
        for _ in 0..5 {
            key_edge(&ctx, &mut app, action, true);
            idle(&ctx, &mut app);
            assert!(app.details.editor.is_some());
            assert_eq!(std::fs::read(&app.lists[0].path).unwrap(), saved);
        }
        key_edge(&ctx, &mut app, action, false);
        key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
        assert!(app.details.editor.is_none());
        let loaded = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(loaded.lists[0].data.tasks[0].subtasks[0].id, child);
        assert!(loaded.lists[0].data.tasks[0].subtasks[0].completed);
        assert_eq!(
            loaded.lists[0].data.tasks[0].subtasks[0].text,
            "Corrected child"
        );
        cleanup(&app);
    }
}

#[test]
fn delete_list_confirmation_requires_a_second_physical_press() {
    for action in [Key::Enter, Key::Space] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let path = app.lists[0].path.clone();
        let survivor = add_list(&mut app, "Survivor");
        idle(&ctx, &mut app);
        app.open_settings(Some(list));
        idle(&ctx, &mut app);
        let button = tab_to_label(&ctx, &mut app, "Delete list");
        key_edge(&ctx, &mut app, action, true);
        assert!(app.delete_armed);
        let saved = std::fs::read(&path).unwrap();
        for _ in 0..5 {
            key_edge(&ctx, &mut app, action, true);
            assert!(app.delete_armed);
            assert!(app.list_index(list).is_some());
            assert_eq!(std::fs::read(&path).unwrap(), saved);
            assert_eq!(ctx.memory(|m| m.focused()), Some(button));
        }
        key_edge(&ctx, &mut app, action, false);
        key(&ctx, &mut app, action, Modifiers::NONE);
        assert!(!path.exists());
        assert!(app.list_index(list).is_none());
        let loaded = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(loaded.lists.len(), 1);
        assert_eq!(loaded.lists[0].key, survivor);
        cleanup(&app);
    }
}

#[test]
fn held_text_input_preserves_spaces_and_editing_repeats() {
    let (ctx, mut app) = test_app();
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "A");
    for _ in 0..3 {
        key_edge(&ctx, &mut app, Key::Space, true);
        type_text(&ctx, &mut app, " ");
    }
    assert_eq!(app.new_task, "A   ");
    key_edge(&ctx, &mut app, Key::Space, false);
    for _ in 0..3 {
        key_edge(&ctx, &mut app, Key::Backspace, true);
    }
    assert_eq!(app.new_task, "A");
    key_edge(&ctx, &mut app, Key::Backspace, false);
    type_text(&ctx, &mut app, "BCD");
    key(&ctx, &mut app, Key::Home, Modifiers::NONE);
    for _ in 0..3 {
        key_edge(&ctx, &mut app, Key::Delete, true);
    }
    assert_eq!(app.new_task, "D");
    assert!(app.lists[0].data.tasks.is_empty());
    cleanup(&app);
}

#[test]
fn held_row_actions_wait_for_release_before_acting_again() {
    for (action, cancel_edit) in [
        (Key::Delete, false),
        (Key::Space, false),
        (Key::Enter, false),
        (Key::Enter, true),
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["A", "B", "C", "D"].map(Task::new).to_vec();
        app.save_list_index(0);
        let first = app.lists[0].data.tasks[0].id;
        let second = app.lists[0].data.tasks[1].id;
        focus(&ctx, &mut app, FocusItem::Task(list, first).id());
        let press = |ctx: &egui::Context, app: &mut MinimalistApp| {
            frame(
                ctx,
                app,
                vec![Event::Key {
                    key: action,
                    physical_key: None,
                    pressed: true,
                    repeat: false, // egui detects repeats until a release arrives.
                    modifiers: Modifiers::NONE,
                }],
                430.0,
            );
        };
        press(&ctx, &mut app);
        if action == Key::Enter {
            assert_eq!(app.editing, Some((list, first)));
            if cancel_edit {
                key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
            } else {
                key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
                type_text(&ctx, &mut app, "Unsubmitted edit");
            }
        }
        if let Some((_, _, started)) = &mut app.delete_animation {
            *started = std::time::Instant::now() - std::time::Duration::from_secs(1);
        }
        idle(&ctx, &mut app);
        let expected = if action == Key::Delete { second } else { first };
        let count = if action == Key::Delete { 3 } else { 4 };
        assert_eq!(app.lists[0].data.tasks.len(), count);
        assert_eq!(app.lists[0].data.tasks[0].completed, action == Key::Space);
        let path = app.lists[0].path.clone();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000),
                ),
            )
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let saved = std::fs::read(&path).unwrap();
        for _ in 0..5 {
            press(&ctx, &mut app);
            assert!(
                ctx.input(|input| input.raw.events.iter().any(|event| matches!(event,
                    Event::Key { key, pressed: true, repeat: true, .. } if *key == action
                )))
            );
            if let Some((_, _, started)) = &mut app.delete_animation {
                *started = std::time::Instant::now() - std::time::Duration::from_secs(1);
            }
            idle(&ctx, &mut app);
            let editing = action == Key::Enter && !cancel_edit;
            assert_eq!(app.editing, editing.then_some((list, first)));
            assert_eq!(app.lists[0].data.tasks.len(), count, "held {action:?}");
            assert_eq!(app.lists[0].data.tasks[0].completed, action == Key::Space);
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(if editing {
                    Id::new(("edit-task", list, first))
                } else {
                    FocusItem::Task(list, expected).id()
                })
            );
            assert_eq!(std::fs::read(&path).unwrap(), saved);
            assert_eq!(
                std::fs::metadata(&path).unwrap().modified().unwrap(),
                modified
            );
            assert!(app.lists[0].data.archive.is_empty());
        }
        // A release followed by a fresh press must still perform the action.
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: action,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
        key(&ctx, &mut app, action, Modifiers::NONE);
        if action == Key::Enter {
            if cancel_edit {
                assert_eq!(app.editing, Some((list, first)));
            } else {
                assert!(app.editing.is_none());
                assert_eq!(app.lists[0].data.tasks[0].text, "Unsubmitted edit");
                assert_eq!(
                    storage::load_workspace(&app.workspace).unwrap().lists[0]
                        .data
                        .tasks[0]
                        .text,
                    "Unsubmitted edit"
                );
            }
        } else if action == Key::Space {
            assert!(!app.lists[0].data.tasks[0].completed);
        } else {
            app.delete_animation.as_mut().unwrap().2 =
                std::time::Instant::now() - std::time::Duration::from_secs(1);
            idle(&ctx, &mut app);
            assert_eq!(app.lists[0].data.tasks.len(), 2);
            assert_eq!(
                storage::load_workspace(&app.workspace).unwrap().lists[0]
                    .data
                    .tasks
                    .len(),
                2
            );
        }
        cleanup(&app);
    }
}

#[test]
fn held_arrows_keep_navigating_and_reordering() {
    for modifiers in [Modifiers::NONE, Modifiers::ALT] {
        for movement in [Key::ArrowDown, Key::ArrowUp] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            app.lists[0].data.tasks = ["A", "B", "C", "D"].map(Task::new).to_vec();
            app.save_list_index(0);
            let start = if movement == Key::ArrowDown { 0 } else { 3 };
            let task = app.lists[0].data.tasks[start].id;
            focus(&ctx, &mut app, FocusItem::Task(list, task).id());
            for step in 1..=2 {
                frame(
                    &ctx,
                    &mut app,
                    vec![Event::Key {
                        key: movement,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    430.0,
                );
                let position = if start == 0 { step } else { start - step };
                let expected = if modifiers == Modifiers::ALT {
                    assert_eq!(app.lists[0].data.tasks[position].id, task);
                    assert_eq!(
                        storage::load_workspace(&app.workspace).unwrap().lists[0]
                            .data
                            .tasks[position]
                            .id,
                        task
                    );
                    task
                } else {
                    app.lists[0].data.tasks[position].id
                };
                assert_eq!(
                    ctx.memory(|m| m.focused()),
                    Some(FocusItem::Task(list, expected).id())
                );
            }
            cleanup(&app);
        }
    }
}

#[test]
fn pointer_openers_restore_the_clicked_control() {
    for (label, card) in [
        ("add", false),
        ("find", false),
        ("...", false),
        ("...", true),
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let task = Task::new("Pointer focus return");
        let row = FocusItem::Task(list, task.id).id();
        app.lists[0].data.tasks.push(task);
        let owner = if card {
            app.go_to_overview();
            FocusItem::Card(list).id()
        } else {
            row
        };
        focus(&ctx, &mut app, owner);
        let output = frame(&ctx, &mut app, Vec::new(), 430.0);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.text() == label
                        && (!card || ctx.read_response(owner).unwrap().rect.contains(text.pos)) =>
                {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap();
        let pointer = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // Discover the real button ID through keyboard focus, without opening it.
        let mut opener = None;
        for _ in 0..16 {
            key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
            let id = ctx.memory(|memory| memory.focused()).unwrap();
            if ctx.read_response(id).unwrap().rect.contains(pos) {
                opener = Some(id);
                break;
            }
        }
        let opener = opener.expect("header control is keyboard reachable");

        for editing in [false, true] {
            if card && editing {
                continue;
            }
            focus(&ctx, &mut app, owner);
            if editing {
                key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
            }
            // A fast click can press and release between two rendered frames.
            frame(
                &ctx,
                &mut app,
                vec![Event::PointerMoved(pos), pointer(true), pointer(false)],
                430.0,
            );
            assert!(
                app.quick_capture.is_some()
                    || app.find.is_some()
                    || matches!(app.screen, Screen::Settings),
                "{label} must open its layer"
            );
            key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(opener),
                "return from {label}, editing={editing}"
            );
            key(&ctx, &mut app, Key::Slash, Modifiers::COMMAND);
            key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(opener));
        }
        cleanup(&app);
    }
}

#[test]
fn disappeared_return_targets_release_focus_after_external_reload() {
    for overlay in [Key::Comma, Key::N, Key::F, Key::Slash] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = [Task::new("Removed"), Task::new("Survivor")].to_vec();
        app.save_list_index(0);
        let removed = app.lists[0].data.tasks[0].id;
        focus(&ctx, &mut app, FocusItem::Task(list, removed).id());
        key(&ctx, &mut app, overlay, Modifiers::COMMAND);
        let mut external = app.lists[0].clone();
        external.data.tasks.remove(0);
        storage::save_list(&external).unwrap();
        app.next_scan = std::time::Instant::now();
        app.scan_external_changes();
        key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        idle(&ctx, &mut app);
        assert!(
            app.keyboard.pending.is_none(),
            "stale return from {overlay:?}"
        );
        let entry = Id::new(("new-task", list));
        let pos = ctx.read_response(entry).unwrap().rect.center();
        frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            430.0,
        );
        type_text(&ctx, &mut app, "Still editable");
        assert_eq!(app.new_task, "Still editable");
        key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
        assert_ne!(ctx.memory(|m| m.focused()), Some(entry));
        cleanup(&app);
    }
}

#[test]
fn overlays_opened_during_removal_do_not_restore_the_removed_row() {
    for remove in [Key::Delete, Key::Backspace] {
        for overlay in [Key::Comma, Key::N, Key::F, Key::Slash] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let mut task = Task::new("Removed during overlay");
            task.completed = remove == Key::Backspace;
            let id = task.id;
            app.lists[0].data.tasks.push(task);
            focus(&ctx, &mut app, FocusItem::Task(list, id).id());
            let modifiers = if remove == Key::Backspace {
                Modifiers::COMMAND | Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            key(&ctx, &mut app, remove, modifiers);
            key(&ctx, &mut app, overlay, Modifiers::COMMAND);
            let elapsed = std::time::Instant::now() - std::time::Duration::from_secs(1);
            if let Some((_, _, started)) = &mut app.delete_animation {
                *started = elapsed;
            }
            if let Some((_, started)) = &mut app.clear_animation {
                *started = elapsed;
            }
            idle(&ctx, &mut app);
            key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
            idle(&ctx, &mut app);
            assert!(app.keyboard.pending.is_none());
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(Id::new(("new-task", list))),
                "return after {remove:?} and {overlay:?}"
            );
            type_text(&ctx, &mut app, "After removal");
            assert_eq!(app.new_task, "After removal");
            assert!(app.lists[0].data.tasks.is_empty());
            cleanup(&app);
        }
    }
}

#[test]
fn settings_rename_keeps_external_changes_unless_a_local_draft_is_pending() {
    for close in [Key::Escape, Key::N] {
        for draft in ["untouched", "pending", "submitted", "left", "reverted"] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let original = app.lists[0].data.title.clone();
            app.open_settings(Some(list));
            idle(&ctx, &mut app);
            if draft != "untouched" {
                focus(&ctx, &mut app, Id::new(("settings-list-name", list)));
                key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
                type_text(&ctx, &mut app, "Local rename");
                match draft {
                    "submitted" => key(&ctx, &mut app, Key::Enter, Modifiers::NONE),
                    "left" => key(&ctx, &mut app, Key::Tab, Modifiers::NONE),
                    "reverted" => {
                        key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
                        type_text(&ctx, &mut app, &original);
                    }
                    _ => {}
                }
            }

            let mut external = app.lists[0].clone();
            external.data.title = "Renamed elsewhere while Settings is open".into();
            storage::save_list(&external).unwrap();
            app.next_scan = std::time::Instant::now();
            app.scan_external_changes();
            assert_eq!(app.lists[0].data.title, external.data.title);
            let expected = if draft == "pending" {
                "Local rename"
            } else {
                &external.data.title
            };
            assert_eq!(app.rename_input, expected, "visible draft: {draft}");
            let modifiers = if close == Key::N {
                Modifiers::COMMAND | Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            key(&ctx, &mut app, close, modifiers);
            assert_eq!(app.lists[0].data.title, expected, "{draft}, {close:?}");
            assert_eq!(
                storage::load_workspace(&app.workspace).unwrap().lists[0]
                    .data
                    .title,
                expected,
                "saved title: {draft}, {close:?}"
            );
            cleanup(&app);
        }
    }
}

#[test]
fn new_list_from_settings_saves_rename_and_clears_return_session() {
    for command in [
        Modifiers::CTRL | Modifiers::COMMAND,
        Modifiers::MAC_CMD | Modifiers::COMMAND,
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        idle(&ctx, &mut app);
        app.open_settings(Some(list));
        idle(&ctx, &mut app);
        focus(&ctx, &mut app, Id::new(("settings-list-name", list)));
        key(&ctx, &mut app, Key::A, command);
        type_text(&ctx, &mut app, "Renamed before new list");
        key(&ctx, &mut app, Key::N, command | Modifiers::SHIFT);
        assert!(matches!(app.screen, Screen::Overview));
        assert!(app.creating_list);
        assert!(app.return_screen.is_none());
        assert!(app.settings_list_id.is_none());
        assert_eq!(
            storage::load_workspace(&app.workspace).unwrap().lists[0]
                .data
                .title,
            "Renamed before new list"
        );
        key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        cleanup(&app);
    }
}

#[test]
fn quoted_workspace_input_opens_existing_lists_and_survives_restart() {
    for quote in ['\'', '"'] {
        for use_button in [false, true] {
            let (ctx, mut app) = test_app();
            let old_workspace = app.workspace.clone();
            let destination = old_workspace.join("backup and setup/minimalist-task-lists");
            let existing =
                storage::create_list(&destination, "Existing workspace", Accent::Mint).unwrap();
            idle(&ctx, &mut app);
            key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
            key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
            type_text(
                &ctx,
                &mut app,
                &format!("{quote}{}{quote}", destination.display()),
            );
            if use_button {
                tab_to_label(&ctx, &mut app, "Use typed path");
                key(&ctx, &mut app, Key::Space, Modifiers::NONE);
            } else {
                key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
            }
            assert_eq!(app.workspace, destination.canonicalize().unwrap());
            assert_eq!(app.lists[0].key, existing.key);
            assert_eq!(app.workspace_input, app.settings.workspace_path);
            assert!(old_workspace.join("lists").is_dir());
            let saved = storage::load_settings().unwrap();
            assert_eq!(saved.workspace_path, app.settings.workspace_path);
            let restarted = MinimalistApp::new(&egui::Context::default(), saved, None, None);
            assert_eq!(restarted.workspace, app.workspace);
            assert_eq!(restarted.lists[0].key, existing.key);
            std::fs::remove_dir_all(&old_workspace).unwrap();
        }
    }
}

#[test]
fn unchanged_workspace_enter_preserves_list_and_launch_override() {
    let (ctx, mut app) = test_app();
    let second = add_list(&mut app, "Second");
    app.persisted_workspace_path = "Saved workspace before launch override".into();
    app.open_list(second);
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::Settings));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new("settings-workspace"))
    );
    let original_path = app.workspace_input.clone();
    type_text(&ctx, &mut app, " still editing");
    assert_eq!(
        app.workspace_input,
        format!("{original_path} still editing")
    );
    assert_eq!(app.settings.last_list_id, Some(second));
    assert_eq!(
        app.persisted_workspace_path,
        "Saved workspace before launch override"
    );
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == second));
    cleanup(&app);
}

#[test]
fn unavailable_return_focus_keeps_a_deliberate_click() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.open_settings(Some(list));
    idle(&ctx, &mut app);
    let chosen = Id::new(("settings-list-name", app.lists[0].key));
    frame(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(egui::pos2(200.0, 350.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -800.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
        430.0,
    );
    idle(&ctx, &mut app);
    let response = ctx.read_response(chosen).unwrap();
    let pos = response.rect.center();
    assert!(response.interact_rect.contains(pos), "{response:?}");
    let missing = Id::new("missing-return-control");
    app.keyboard.pending = Some(missing);
    app.keyboard.missing_request = Some((missing, 2));
    frame(
        &ctx,
        &mut app,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        430.0,
    );
    assert!(app.keyboard.pending.is_none());
    assert_eq!(ctx.memory(|m| m.focused()), Some(chosen));
    idle(&ctx, &mut app);
    assert_eq!(ctx.memory(|m| m.focused()), Some(chosen));
    cleanup(&app);
}

#[test]
fn overview_fallback_remembers_a_surviving_card() {
    let (ctx, mut app) = test_app();
    let second = add_list(&mut app, "Second");
    app.go_to_overview();
    focus(&ctx, &mut app, FocusItem::Card(second).id());
    app.keyboard.pending = Some(Id::new("missing-return-control"));
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(second).id())
    );
    app.lists.retain(|list| list.key != second);
    app.keyboard.pending = Some(Id::new("another-missing-return-control"));
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(app.lists[0].key).id())
    );
    cleanup(&app);
}

#[test]
fn scrolled_settings_focus_outline_stays_in_the_visible_region() {
    let (ctx, mut app) = test_app();
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    idle(&ctx, &mut app);
    let workspace = Id::new("settings-workspace");
    let output = frame(&ctx, &mut app, Vec::new(), 430.0);
    let response = ctx.read_response(workspace).unwrap();
    assert!(output.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::epaint::Shape::Rect(rect)
            if rect.rect == response.rect.expand(1.0)
                && rect.stroke.width == 1.5
                && clipped.clip_rect.contains_rect(rect.rect))
    }));
    // The displayed fixture path can wrap. Scroll until the field is clipped,
    // in small steps so its outline remains beneath the header for this check.
    let pos = response.rect.center();
    for _ in 0..20 {
        frame(
            &ctx,
            &mut app,
            vec![
                Event::PointerMoved(pos),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -24.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
            430.0,
        );
        idle(&ctx, &mut app);
        if !ctx
            .read_response(workspace)
            .unwrap()
            .interact_rect
            .is_positive()
        {
            break;
        }
    }
    let output = frame(&ctx, &mut app, Vec::new(), 430.0);
    let response = ctx.read_response(workspace).unwrap();
    assert_eq!(ctx.memory(|m| m.focused()), Some(workspace));
    assert!(response.rect.intersects(ctx.content_rect()));
    assert!(!response.interact_rect.is_positive(), "{response:?}");
    let mut outlines = 0;
    for clipped in output.shapes {
        if let egui::epaint::Shape::Rect(rect) = clipped.shape
            && rect.rect == response.rect.expand(1.0)
            && rect.stroke.width == 1.5
        {
            outlines += 1;
            assert!(
                !rect.rect.intersect(clipped.clip_rect).is_positive(),
                "scrolled-away focus is visible through {:?}",
                clipped.clip_rect
            );
        }
    }
    assert!(outlines > 0);
    cleanup(&app);
}

#[test]
fn escape_cancels_one_layer_and_navigation_saves_inline_edits() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Original");
    app.lists[0].data.tasks.push(task.clone());
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("new-subtask", list, task.id)))
    );
    type_text(&ctx, &mut app, "Child");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    app.history_open = true;
    let escape = |ctx: &egui::Context, app: &mut MinimalistApp, pressed| {
        frame(
            ctx,
            app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed,
                repeat: false, // egui derives repeats from the uninterrupted key state.
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
    };
    escape(&ctx, &mut app, true);
    assert!(matches!(app.screen, Screen::List(_)));
    assert!(app.subtasks_for.is_none());
    for _ in 0..5 {
        escape(&ctx, &mut app, true);
        assert!(
            ctx.input(|input| input.raw.events.iter().any(|event| matches!(
                event,
                Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    repeat: true,
                    ..
                }
            )))
        );
        assert!(app.history_open, "one hold must not dismiss the next layer");
        assert!(matches!(app.screen, Screen::List(id) if id == list));
    }
    // Release, then press again: the next layer may close.
    escape(&ctx, &mut app, false);
    assert!(app.history_open);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(!app.history_open);
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(app.editing, Some((list, task.id)));
    app.edit_text = "Cancelled".into();
    app.subtasks_for = Some((list, task.id));
    for _ in 0..6 {
        escape(&ctx, &mut app, true);
        assert!(app.editing.is_none());
        assert_eq!(app.subtasks_for, Some((list, task.id)));
        assert!(matches!(app.screen, Screen::List(id) if id == list));
    }
    escape(&ctx, &mut app, false);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(app.subtasks_for.is_none());
    assert_eq!(app.lists[0].data.tasks[0].text, "Original");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    app.edit_text = "Saved on navigation".into();
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert_eq!(app.lists[0].data.tasks[0].text, "Saved on navigation");
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == list));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    app.edit_text = "Saved on Back".into();
    key(
        &ctx,
        &mut app,
        BACK_TO_LISTS.logical_key,
        BACK_TO_LISTS.modifiers,
    );
    assert!(matches!(app.screen, Screen::Overview));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(list).id())
    );
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .tasks[0]
            .text,
        "Saved on Back"
    );
    cleanup(&app);
}
