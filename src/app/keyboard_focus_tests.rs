//! Regressions for navigation return targets and Settings keyboard transitions.

use super::*;

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
fn new_list_from_settings_saves_rename_and_clears_return_session() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    focus(&ctx, &mut app, Id::new(("settings-list-name", list)));
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Renamed before new list");
    key(
        &ctx,
        &mut app,
        Key::N,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
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
    assert_eq!(app.settings.last_list_id, Some(second));
    assert_eq!(
        app.persisted_workspace_path,
        "Saved workspace before launch override"
    );
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == second));
    cleanup(&app);
}
