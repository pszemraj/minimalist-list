//! Keyboard navigation through list levels and Settings rows without Tab chains.

use super::*;

#[test]
fn opening_settings_preserves_fractional_preferences_until_a_slider_is_edited() {
    let (ctx, mut app) = test_app();
    app.settings.font_size = 19.4;
    app.settings.row_padding = 7.4;
    app.settings.background_opacity = 0.83;
    app.save_settings();
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    for _ in 0..2 {
        key(&ctx, &mut app, Key::PageDown, Modifiers::NONE);
    }
    let saved = storage::load_settings().unwrap();
    assert_eq!(saved.font_size, 19.4);
    assert_eq!(saved.row_padding, 7.4);
    assert_eq!(saved.background_opacity, 0.83);
    assert_eq!(app.settings.font_size, saved.font_size);
    assert_eq!(app.settings.row_padding, saved.row_padding);
    assert_eq!(app.settings.background_opacity, saved.background_opacity);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.settings.font_size, 20.0);
    assert_eq!(storage::load_settings().unwrap().font_size, 20.0);
    cleanup(&app);
}

#[test]
fn task_secondary_controls_navigate_to_primary_rows_without_activating_them() {
    for delete_control in [false, true] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let first = Task::new("First");
        let second = Task::new("Second");
        app.lists[0].data.tasks = vec![first.clone(), second.clone()];
        focus(&ctx, &mut app, FocusItem::Task(list, first.id).id());
        if delete_control {
            tab_to_label(&ctx, &mut app, "x");
        } else {
            key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
        }
        key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(FocusItem::Task(list, second.id).id())
        );
        key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(FocusItem::Task(list, first.id).id())
        );
        assert_eq!(app.lists[0].data.tasks.len(), 2);
        assert!(!app.lists[0].data.tasks[0].completed);
        cleanup(&app);
    }
}

#[test]
fn history_pointer_open_keeps_toggle_focus_and_last_restore_allows_backspace() {
    let (ctx, mut app) = test_app();
    let archived = Task::new("Archived");
    app.lists[0]
        .data
        .archive
        .push(ArchivedTask::new(archived.clone()));
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let toggle = ctx.memory(|m| m.focused()).unwrap();
    let pos = ctx.read_response(toggle).unwrap().rect.center();
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
    idle(&ctx, &mut app);
    assert!(app.history_open);
    assert_eq!(ctx.memory(|m| m.focused()), Some(toggle));
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(!app.history_open);
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::Overview));
    cleanup(&app);
}

#[test]
fn checklist_navigation_enters_children_preserves_drafts_and_returns_to_parent() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks = ["First", "Second"].map(crate::model::Subtask::new).to_vec();
    let first = task.subtasks[0].id;
    let second = task.subtasks[1].id;
    app.lists[0].data.tasks.push(task.clone());
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Subtask(list, task.id, first).id())
    );
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::Space, Modifiers::NONE);
    assert!(app.lists[0].data.tasks[0].subtasks[1].completed);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    type_text(&ctx, &mut app, "Draft child");
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Subtask(list, task.id, second).id())
    );
    assert_eq!(app.new_subtask, "Draft child");
    key(&ctx, &mut app, Key::Home, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.new_subtask, "Draft child");
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Subtask(list, task.id, first).id())
    );
    key(&ctx, &mut app, Key::End, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(app.lists[0].data.tasks[0].subtasks[2].text, "Draft child");
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    assert!(app.subtasks_for.is_none());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    cleanup(&app);
}

#[test]
fn history_navigation_works_with_only_archived_tasks_and_closes_the_focused_panel() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let archived = Task::new("Archived");
    app.lists[0]
        .data
        .archive
        .push(ArchivedTask::new(archived.clone()));
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let toggle = ctx.memory(|m| m.focused());
    assert_ne!(toggle, Some(Id::new(("new-task", list))));
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    assert!(app.history_open);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Archived(list, archived.id).id())
    );
    let parent = Task::new("Unrelated open checklist");
    app.lists[0].data.tasks.push(parent.clone());
    app.subtasks_for = Some((list, parent.id));
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(!app.history_open);
    assert_eq!(app.subtasks_for, Some((list, parent.id)));
    assert_eq!(ctx.memory(|m| m.focused()), toggle);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(!app.history_open);
    assert_eq!(app.subtasks_for, Some((list, parent.id)));
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, archived.id).id())
    );
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .tasks[1]
            .text,
        "Archived"
    );
    assert!(!app.history_open);
    cleanup(&app);
}

#[test]
fn find_arrows_and_enter_agree_after_tabbing_to_a_result() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let first = Task::new("Needle first");
    let second = Task::new("Needle second");
    app.lists[0].data.tasks = vec![first.clone(), second.clone()];
    key(&ctx, &mut app, Key::F, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Needle");
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.find.is_none());
    assert_eq!(app.spotlight.as_ref().unwrap().task_id, second.id);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, second.id).id())
    );
    cleanup(&app);
}

#[test]
fn list_creation_arrows_choose_color_then_return_to_name_for_submission() {
    let (ctx, mut app) = test_app();
    key(&ctx, &mut app, NEW_LIST.logical_key, NEW_LIST.modifiers);
    type_text(&ctx, &mut app, "Arrow-created list");
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    assert!(!ctx.text_edit_focused());
    key(&ctx, &mut app, Key::ArrowRight, Modifiers::NONE);
    key_edge(&ctx, &mut app, Key::Enter, true);
    for _ in 0..3 {
        key_edge(&ctx, &mut app, Key::Enter, true);
    }
    assert!(app.creating_list);
    assert_eq!(app.new_list_accent, Accent::Charcoal);
    assert_eq!(ctx.memory(|m| m.focused()), Some(Id::new("new-list-title")));
    key_edge(&ctx, &mut app, Key::Enter, false);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(!app.creating_list);
    let created = app
        .lists
        .iter()
        .find(|list| list.data.title == "Arrow-created list")
        .unwrap();
    assert_eq!(created.data.accent, Accent::Charcoal);
    cleanup(&app);
}

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
    assert_eq!(app.settings.font_size, font_size + 1.0);
    let font_size = app.settings.font_size;
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let row_padding = app.settings.row_padding;
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(app.settings.row_padding, row_padding - 1.0);
    assert_eq!(app.settings.font_size, font_size);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let opacity = app.settings.background_opacity;
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    assert!((app.settings.background_opacity - (opacity - 0.01)).abs() < 0.001);
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
