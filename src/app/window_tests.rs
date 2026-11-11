//! Native window event and shutdown tests.

use super::*;
use crate::app::tests::test_app;
use crate::model::Task;

#[test]
fn tray_quit_retains_failed_capture_until_its_retry_succeeds() {
    let (ctx, mut app) = test_app();
    app.open_quick_capture(None);
    let draw = |app: &mut MinimalistApp, events| {
        crate::app::tests::run_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 700.0),
                )),
                time: Some(ctx.input(|input| input.time) + 0.3),
                events,
                ..Default::default()
            },
            |_| app.overlay_ui(&ctx, app.palette()),
        );
    };
    let enter = || {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    };
    draw(&mut app, vec![]);
    draw(
        &mut app,
        vec![egui::Event::Text("Retained tray capture".into())],
    );
    std::fs::rename(app.workspace.join("lists"), app.workspace.join("offline")).unwrap();
    std::fs::write(app.workspace.join("lists"), "unavailable").unwrap();
    draw(&mut app, enter());
    assert!(app.quick_capture.is_some() && app.status.is_some());
    app.window_hidden = true;
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events = vec![egui::ViewportEvent::Close];
    let output = ctx.run_logic(&input, |ctx| {
        app.handle_window_events(ctx, false, &[Action::Quit], None);
    });
    assert!(!app.quitting && !app.window_hidden);
    assert!(app.quick_capture.is_some());
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(!commands.contains(&ViewportCommand::Close));
    std::fs::remove_file(app.workspace.join("lists")).unwrap();
    std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists")).unwrap();
    draw(&mut app, vec![]);
    draw(&mut app, enter());
    assert!(app.quick_capture.is_none());
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .occluded = Some(true);
    let output = ctx.run_logic(&input, |ctx| {
        app.handle_window_events(
            ctx,
            false,
            &[Action::QuickAdd, Action::Quit, Action::Show],
            None,
        );
    });
    assert!(app.quitting);
    assert!(app.global_quick_add_requested.is_none());
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(commands.contains(&ViewportCommand::Close));
    assert!(!commands.contains(&ViewportCommand::CancelClose));
    assert!(!commands.contains(&ViewportCommand::Focus));
    let saved = crate::storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks.len(), 1);
    assert_eq!(saved.lists[0].data.tasks[0].text, "Retained tray capture");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn duplicate_launch_reveals_despite_stale_minimize_and_close_input() {
    for close_requested in [false, true] {
        let (ctx, mut app) = test_app();
        let state = app.workspace.join("instance-state");
        let crate::instance::LaunchOutcome::Primary(guard) =
            crate::instance::claim(&app.workspace, &state, false).unwrap()
        else {
            panic!("expected a new test instance");
        };
        app.set_instance_guard(guard, &ctx);
        app.window_hidden = true;
        assert!(matches!(
            crate::instance::claim(&app.workspace, &state, false).unwrap(),
            crate::instance::LaunchOutcome::Existing
        ));
        let mut input = egui::RawInput::default();
        if close_requested {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events = vec![egui::ViewportEvent::Close];
        }

        let output = ctx.run_logic(&input, |ctx| app.handle_window_actions(ctx, true));
        assert!(!app.window_hidden);
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(commands.contains(&ViewportCommand::Visible(true)));
        assert!(commands.contains(&ViewportCommand::Minimized(false)));
        assert!(!commands.contains(&ViewportCommand::Visible(false)));
        if close_requested {
            assert!(commands.contains(&ViewportCommand::CancelClose));
        }
        app.instance = None;
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn show_and_quick_add_override_native_close_and_minimize_in_either_order() {
    for actions in [
        &[Action::QuickAdd][..],
        &[Action::Show, Action::QuickAdd],
        &[Action::QuickAdd, Action::Show],
    ] {
        let (ctx, mut app) = test_app();
        app.window_hidden = true;
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events = vec![egui::ViewportEvent::Close];

        let output = ctx.run_logic(&input, |ctx| {
            app.handle_window_events(ctx, true, actions, Some(Availability::Available));
        });
        assert!(!app.window_hidden);
        assert_eq!(app.global_quick_add_requested, Some(actions.len() == 1));
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(commands.contains(&ViewportCommand::CancelClose));
        assert!(commands.contains(&ViewportCommand::Minimized(false)));
        assert!(!commands.contains(&ViewportCommand::Visible(false)));
        assert_eq!(
            commands
                .iter()
                .filter(|c| **c == ViewportCommand::Focus)
                .count(),
            1
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn revealing_reapplies_the_saved_window_level_after_mapping() {
    for pinned in [false, true] {
        let (ctx, mut app) = test_app();
        app.settings.always_on_top = pinned;
        app.last_pin_state = Some(pinned);
        app.last_decorations_state = Some(app.settings.window_decorations);
        app.window_hidden = true;

        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.show_window(ctx);
            app.apply_window_preferences(ctx);
        });
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        let mapped = commands
            .iter()
            .position(|command| *command == ViewportCommand::Visible(true))
            .unwrap();
        let level = if pinned {
            egui::viewport::WindowLevel::AlwaysOnTop
        } else {
            egui::viewport::WindowLevel::Normal
        };
        let restored = commands
            .iter()
            .position(|command| *command == ViewportCommand::WindowLevel(level))
            .unwrap();
        assert!(mapped < restored);
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn tray_quick_add_reveals_a_hidden_window_and_focuses_the_entry_field() {
    let (ctx, mut app) = test_app();
    app.window_hidden = true;
    let output = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, false, &[Action::QuickAdd], None);
    });
    assert!(!app.window_hidden);
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    let mapped = commands
        .iter()
        .position(|command| *command == ViewportCommand::Visible(true))
        .unwrap();
    let focused = commands
        .iter()
        .position(|command| *command == ViewportCommand::Focus)
        .unwrap();
    assert!(mapped < focused);
    crate::app::tests::settle_keyboard(&ctx, &mut app);
    assert!(app.quick_capture.is_some());
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("quick-capture-text"))
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn tray_quick_add_leaves_deferred_input_with_the_inline_editor() {
    use crate::app::{
        keyboard::FocusItem,
        tests::{chord, focus_widget, frame_input, keyboard_frame, run_frame, settle_keyboard},
    };

    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Original");
    let task_id = task.id;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    focus_widget(&ctx, &mut app, FocusItem::Task(list, task_id).id());
    keyboard_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Enter, egui::Modifiers::NONE),
        430.0,
    );
    settle_keyboard(&ctx, &mut app);
    assert_eq!(app.editing, Some((list, task_id)));

    let input = frame_input(
        egui::vec2(430.0, 500.0),
        ctx.input(|input| input.time) + 0.3,
        vec![
            egui::Event::Paste("One\nTwo".into()),
            egui::Event::Copy,
            egui::Event::Text("Three".into()),
        ],
    );
    run_frame(&ctx, input, |ui| {
        // Native eframe runs logic and then UI with the same input batch.
        app.handle_window_events(ui.ctx(), false, &[Action::QuickAdd], None);
        <MinimalistApp as eframe::App>::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
    });
    settle_keyboard(&ctx, &mut app);

    let saved = crate::storage::load_workspace(&app.workspace).unwrap();
    let tasks = &saved.lists[0].data.tasks;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].text, "OriginalOne TwoThree");
    assert!(app.quick_capture.as_ref().unwrap().text.is_empty());
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("quick-capture-text"))
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn native_close_and_tray_quit_save_input_from_the_last_visible_frame() {
    use crate::app::{
        keyboard::FocusItem,
        tests::{chord, focus_widget, frame_input, keyboard_frame, run_frame, settle_keyboard},
    };

    for tray_quit in [false, true] {
        for deferred in [false, true] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let task = Task::new("Original");
            let task_id = task.id;
            app.lists[0].data.tasks.push(task);
            assert!(app.save_list_index(0));
            focus_widget(&ctx, &mut app, FocusItem::Task(list, task_id).id());
            keyboard_frame(
                &ctx,
                &mut app,
                chord(egui::Key::Enter, egui::Modifiers::NONE),
                430.0,
            );
            settle_keyboard(&ctx, &mut app);
            assert_eq!(app.editing, Some((list, task_id)));

            let mut events = vec![egui::Event::Paste("One\nTwo".into())];
            if deferred {
                events.extend([egui::Event::Copy, egui::Event::Text("Three".into())]);
            }
            let size = egui::vec2(430.0, 500.0);
            let mut input = frame_input(size, ctx.input(|input| input.time) + 0.3, events);
            let actions: &[_] = if tray_quit {
                &[Action::Quit]
            } else {
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .events
                    .push(egui::ViewportEvent::Close);
                &[]
            };
            let output = run_frame(&ctx, input, |ui| {
                app.handle_window_events(ui.ctx(), false, actions, Some(Availability::Unavailable));
                eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
            });
            assert_eq!(app.quitting, !deferred);
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert_eq!(
                commands.contains(&ViewportCommand::CancelClose),
                deferred && !tray_quit
            );
            assert_eq!(commands.contains(&ViewportCommand::Close), !deferred);

            if deferred {
                let input = frame_input(size, ctx.input(|input| input.time) + 0.3, vec![]);
                let output = run_frame(&ctx, input, |ui| {
                    app.handle_window_actions(ui.ctx(), false);
                    eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
                });
                assert!(app.quitting);
                assert!(
                    output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .contains(&ViewportCommand::Close)
                );
            }

            let saved = crate::storage::load_workspace(&app.workspace).unwrap();
            let expected = if deferred {
                "OriginalOne TwoThree"
            } else {
                "OriginalOne Two"
            };
            assert_eq!(saved.lists[0].data.tasks[0].text, expected);
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }
}

#[test]
fn window_events_save_hide_restore_and_reload() {
    let (ctx, mut app) = test_app();
    let list_id = app.lists[0].key;
    let task = Task::new("Before editing");
    app.editing = Some((list_id, task.id));
    app.edit_text = "Saved before hiding".into();
    app.lists[0].data.tasks = vec![task];

    let output = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, true, &[], Some(Availability::Available));
    });
    assert!(app.window_hidden);
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT]
            .contains(&ViewportCommand::Visible(false))
    );
    assert!(app.editing.is_none());
    let mut saved = crate::storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks[0].text, "Saved before hiding");

    saved.lists[0].data.tasks[0].text = "Changed on another computer".into();
    crate::storage::save_list(&saved.lists[0]).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert_eq!(
        app.lists[0].data.tasks[0].text,
        "Changed on another computer"
    );

    // A window manager can discard skip-taskbar state while hidden. A
    // reveal must invalidate the cache even when its polling deadline is later.
    app.taskbar_hidden = Some(true);
    app.next_taskbar_check = Instant::now() + SCAN_INTERVAL;

    // A tray Show wins over stale minimize and close input in the same tick.
    let mut close_input = egui::RawInput::default();
    close_input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events = vec![egui::ViewportEvent::Close];
    let output = ctx.run_logic(&close_input, |ctx| {
        app.handle_window_events(ctx, true, &[Action::Show], None);
    });
    assert!(!app.window_hidden);
    assert_eq!(app.taskbar_hidden, None);
    assert!(app.next_taskbar_check <= Instant::now());
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    for command in [
        ViewportCommand::Visible(true),
        ViewportCommand::Minimized(false),
        ViewportCommand::Focus,
        ViewportCommand::CancelClose,
    ] {
        assert!(commands.contains(&command));
    }
    assert!(!commands.contains(&ViewportCommand::Visible(false)));

    let _ = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, true, &[], Some(Availability::Available));
    });
    app.next_scan = Instant::now() + SCAN_INTERVAL;
    let _ = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, false, &[], None);
    });
    assert!(app.window_hidden);
    app.next_scan = Instant::now();
    let output = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, false, &[], Some(Availability::Unavailable));
    });
    assert!(!app.window_hidden);
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Visible(true))
    );

    // No tray is an ordinary fallback: minimize stays visible to the taskbar,
    // and native close is not canceled.
    app.status = None;
    app.setup_tray(&ctx, false);
    assert!(app.status.is_none());
    assert_eq!(app.tray_availability(), Availability::Unavailable);
    let output = ctx.run_logic(&Default::default(), |ctx| app.start_hidden(ctx));
    assert!(!app.window_hidden);
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Visible(true))
    );
    let output = ctx.run_logic(&Default::default(), |ctx| {
        app.handle_window_events(ctx, true, &[], Some(Availability::Unavailable));
    });
    assert!(!app.window_hidden);
    assert!(
        output
            .viewport_commands
            .values()
            .flatten()
            .all(|command| *command != ViewportCommand::Visible(false))
    );
    let output = ctx.run_logic(&close_input, |ctx| {
        app.handle_window_events(ctx, false, &[], Some(Availability::Unavailable));
        app.finish_native_shutdown_after_input(ctx);
    });
    assert!(app.quitting);
    assert!(
        output
            .viewport_commands
            .values()
            .flatten()
            .all(|command| *command != ViewportCommand::CancelClose)
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn unchecked_or_inconclusive_tray_checks_do_not_restore_a_hidden_window() {
    for availability in [
        None,
        Some(Availability::Unknown),
        Some(Availability::Available),
    ] {
        let (ctx, mut app) = test_app();
        app.window_hidden = true;
        // The deadline may pass after handle_window_actions chooses not to probe.
        app.next_scan = Instant::now();
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[], availability);
        });
        assert!(app.window_hidden);
        assert!(app.status.is_none());
        assert!(output.viewport_commands.values().flatten().all(|command| {
            !matches!(
                command,
                ViewportCommand::Visible(true) | ViewportCommand::Focus
            )
        }));
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn an_inconclusive_probe_does_not_hide_or_exit() {
    for close in [false, true] {
        let (ctx, mut app) = test_app();
        let mut input = egui::RawInput::default();
        if close {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events = vec![egui::ViewportEvent::Close];
        }
        // Only an already-created native tray can produce Unknown in the application.
        let output = ctx.run_logic(&input, |ctx| {
            app.handle_window_events(ctx, !close, &[], Some(Availability::Unknown));
            app.finish_native_shutdown_after_input(ctx);
        });
        assert!(!app.window_hidden);
        assert!(!app.quitting);
        assert!(app.status.is_none());
        let mut commands = output.viewport_commands.values().flatten();
        if close {
            assert!(commands.any(|command| *command == ViewportCommand::CancelClose));
        }
        assert!(output.viewport_commands.values().flatten().all(|command| {
            !matches!(
                command,
                ViewportCommand::Visible(false) | ViewportCommand::Close
            )
        }));
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn shutdown_flushes_inline_edit_and_pending_mutations_to_disk() {
    for tray_quit in [false, true] {
        let (ctx, mut app) = test_app();
        let list_id = app.lists[0].key;
        let edited = Task::new("Before editing");
        let mut completed = Task::new("Archive on quit");
        completed.completed = true;
        let deleted = Task::new("Delete on quit");
        app.editing = Some((list_id, edited.id));
        app.edit_text = "After editing".into();
        app.lists[0].data.tasks = vec![edited, completed, deleted.clone()];
        app.delete_animation = Some((list_id, deleted.id, Instant::now()));
        app.clear_animation = Some((list_id, Instant::now()));
        app.window_hidden = true;
        let output = ctx.run_logic(&Default::default(), |ctx| {
            if tray_quit {
                app.handle_window_events(ctx, false, &[Action::Quit], None);
            } else {
                app.close_window(ctx);
            }
        });
        assert!(app.quitting);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Close)
        );
        eframe::App::on_exit(&mut app, None);
        let saved = crate::storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks.len(), 1);
        assert_eq!(saved.lists[0].data.tasks[0].text, "After editing");
        assert_eq!(saved.lists[0].data.archive.len(), 1);
        assert_eq!(saved.lists[0].data.archive[0].task.text, "Archive on quit");
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events = vec![egui::ViewportEvent::Close];
        let output = ctx.run_logic(&input, |ctx| app.handle_window_actions(ctx, true));
        assert!(
            !output
                .viewport_commands
                .values()
                .flatten()
                .any(|cmd| *cmd == ViewportCommand::CancelClose)
        );
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn command_w_closes_window_above_overlays_after_saving_edits() {
    for overlay in [false, true] {
        let (ctx, mut app) = test_app();
        let list_id = app.lists[0].key;
        let edited = Task::new("Before editing");
        app.editing = Some((list_id, edited.id));
        app.edit_text = "After editing".into();
        app.lists[0].data.tasks = vec![edited];
        if overlay {
            app.open_quick_capture(None);
        }
        let output = crate::app::tests::run_app_frame(
            &ctx,
            &mut app,
            egui::RawInput {
                events: crate::app::tests::chord(egui::Key::W, egui::Modifiers::COMMAND),
                ..Default::default()
            },
        );
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Close)
        );
        let saved = crate::storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks[0].text, "After editing");
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}
