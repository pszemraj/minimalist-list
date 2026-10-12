//! Keyboard navigation through list levels and Settings rows without Tab chains.

use super::*;

#[test]
fn backspace_returns_from_empty_entry_and_task_rows_to_their_overview_card() {
    for from_task in [false, true] {
        let (ctx, mut app) = test_app();
        let list = add_list(&mut app, "Projects");
        app.open_list(list);
        idle(&ctx, &mut app);
        if from_task {
            type_text(&ctx, &mut app, "Keep this task");
            key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
            key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
        }
        key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
        assert!(matches!(app.screen, Screen::Overview));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(FocusItem::Card(list).id())
        );
        key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
        assert!(matches!(app.screen, Screen::List(id) if id == list));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(Id::new(("new-task", list)))
        );
        if from_task {
            assert_eq!(
                storage::load_workspace(&app.workspace).unwrap().lists[1]
                    .data
                    .tasks[0]
                    .text,
                "Keep this task"
            );
        }
        cleanup(&app);
    }
}

#[test]
fn settings_arrows_move_rows_and_page_keys_jump_sections() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    assert!(!ctx.text_edit_focused());
    let choose_folder = ctx.memory(|m| m.focused());
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_ne!(ctx.memory(|m| m.focused()), choose_folder);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let pin = ctx.memory(|m| m.focused());
    key(&ctx, &mut app, Key::Space, Modifiers::NONE);
    assert!(app.settings.always_on_top);
    assert!(storage::load_settings().unwrap().always_on_top);
    key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    assert!(!ctx.text_edit_focused());
    key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("settings-list-name", list)))
    );
    let response = ctx
        .read_response(Id::new(("settings-list-name", list)))
        .unwrap();
    assert!(response.interact_rect.is_positive());
    key(&ctx, &mut app, Key::PageUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::PageUp, Modifiers::NONE);
    assert_eq!(ctx.memory(|m| m.focused()), pin);
    key(&ctx, &mut app, Key::PageUp, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new("settings-workspace"))
    );
    cleanup(&app);
}

#[test]
fn held_backspace_edits_to_empty_without_navigating_until_a_new_press() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "ab");
    key_edge(&ctx, &mut app, Key::Backspace, true);
    assert_eq!(app.new_task, "a");
    for _ in 0..4 {
        key_edge(&ctx, &mut app, Key::Backspace, true);
        assert_eq!(app.new_task, "");
        assert!(matches!(app.screen, Screen::List(id) if id == list));
    }
    key_edge(&ctx, &mut app, Key::Backspace, false);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::Overview));
    cleanup(&app);
}

#[test]
fn backspace_stays_inside_editors_subtasks_and_overlays() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "Original");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    let task = app.lists[0].data.tasks[0].id;
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert_eq!(app.editing, Some((list, task)));
    assert_eq!(app.edit_text, "");
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert_eq!(app.subtasks_for, Some((list, task)));
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(app.quick_capture.is_some());
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    key(&ctx, &mut app, Key::F, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(app.find.is_some());
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == list));
    assert_eq!(app.lists[0].data.tasks[0].text, "Original");
    cleanup(&app);
}

#[test]
fn settings_horizontal_arrows_edit_text_and_adjust_sliders() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    for _ in 0..2 {
        key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    }
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    key(&ctx, &mut app, Key::Space, Modifiers::NONE);
    assert_eq!(app.settings.font, crate::model::FontChoice::Mono);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let font_size = app.settings.font_size;
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert!(app.settings.font_size > font_size);
    let font_size = app.settings.font_size;
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let row_padding = app.settings.row_padding;
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    assert!(app.settings.row_padding < row_padding);
    assert_eq!(app.settings.font_size, font_size);
    key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "abcd");
    key(&ctx, &mut app, Key::Home, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert_eq!(app.rename_input, "cd");
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("settings-list-name", list)))
    );
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .title,
        "cd"
    );
    for _ in 0..2 {
        key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    }
    let accent = ctx.memory(|m| m.focused());
    key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    assert_eq!(ctx.memory(|m| m.focused()), accent);
    cleanup(&app);
}
