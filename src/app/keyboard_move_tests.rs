//! Keyboard Move and Undo journeys through real widgets and saved list files.

use super::*;
use crate::model::{Subtask, TaskSort, TodoList};

struct Fixture {
    ctx: egui::Context,
    app: MinimalistApp,
    list: Uuid,
    other: Uuid,
    third: Uuid,
    tasks: Vec<Uuid>,
}

fn fixture(titles: [&str; 3]) -> Fixture {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.title = "Tasks".into();
    app.lists[0].data.tasks = titles.map(Task::new).to_vec();
    app.lists[0].data.tasks[1]
        .subtasks
        .push(Subtask::new("Child"));
    let tasks = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
    assert!(app.save_list_index(0));
    let other = add_list(&mut app, "Other");
    let third = add_list(&mut app, "Third");
    app.request_task_focus = false;
    idle(&ctx, &mut app);
    Fixture {
        ctx,
        app,
        list,
        other,
        third,
        tasks,
    }
}

fn saved(app: &MinimalistApp, list: Uuid) -> TodoList {
    let index = app.list_index(list).unwrap();
    serde_json::from_slice(&std::fs::read(&app.lists[index].path).unwrap()).unwrap()
}

fn titles(list: &TodoList) -> Vec<&str> {
    list.tasks.iter().map(|task| task.text.as_str()).collect()
}

fn move_chord() -> Vec<Event> {
    chord(Key::M, Modifiers::COMMAND | Modifiers::SHIFT)
}

fn focused(ctx: &egui::Context) -> Option<Id> {
    ctx.memory(|memory| memory.focused())
}

fn destination(app: &MinimalistApp) -> Uuid {
    app.details.move_task.as_ref().unwrap().destination
}

/// Chooses the next destination with the arrow keys and confirms with Enter.
fn choose_with_keys(f: &mut Fixture, downs: usize) {
    for _ in 0..downs {
        key(&f.ctx, &mut f.app, Key::ArrowDown, Modifiers::NONE);
    }
    key(&f.ctx, &mut f.app, Key::Enter, Modifiers::NONE);
    assert!(f.app.details.move_task.is_none());
}

fn click_label(ctx: &egui::Context, app: &mut MinimalistApp, label: &str) {
    let output = frame(ctx, app, Vec::new(), 430.0);
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label} control is visible"));
    let button = |pressed| Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![Event::PointerMoved(position), button(true), button(false)],
        430.0,
    );
    idle(ctx, app);
}

#[test]
fn shortcut_from_a_focused_row_moves_with_arrows_and_enter() {
    let mut f = fixture(["A", "B", "C"]);
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[1]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    let dialog = f.app.details.move_task.as_ref().unwrap();
    assert_eq!((dialog.source, dialog.task), (f.list, f.tasks[1]));
    assert_eq!(dialog.destination, f.other);
    assert_ne!(
        focused(&f.ctx),
        Some(FocusItem::Task(f.list, f.tasks[1]).id())
    );
    key(&f.ctx, &mut f.app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(destination(&f.app), f.third);
    key(&f.ctx, &mut f.app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(destination(&f.app), f.other, "Down wraps to the first list");
    key(&f.ctx, &mut f.app, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(destination(&f.app), f.third, "Up wraps to the last list");
    assert!(f.app.details.move_task.is_some(), "arrows only choose");
    key(&f.ctx, &mut f.app, Key::Enter, Modifiers::NONE);
    assert!(f.app.details.move_task.is_none());
    assert_eq!(titles(&saved(&f.app, f.list)), ["A", "C"]);
    assert_eq!(titles(&saved(&f.app, f.third)), ["B"]);
    assert!(saved(&f.app, f.other).tasks.is_empty());
    assert_eq!(
        focused(&f.ctx),
        Some(FocusItem::Task(f.list, f.tasks[2]).id()),
        "focus moves to the neighbouring row"
    );
    cleanup(&f.app);
}

#[test]
fn shortcut_from_a_child_completion_control_moves_the_parent() {
    let mut f = fixture(["A", "B", "C"]);
    f.app.switch_subtask_panel(Some((f.list, f.tasks[1])));
    let child = f.app.lists[0].data.tasks[1].subtasks[0].id;
    focus(
        &f.ctx,
        &mut f.app,
        FocusItem::Subtask(f.list, f.tasks[1], child).id(),
    );
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(f.app.details.move_task.as_ref().unwrap().task, f.tasks[1]);
    choose_with_keys(&mut f, 1);
    assert_eq!(titles(&saved(&f.app, f.list)), ["A", "C"]);
    let moved = saved(&f.app, f.third);
    assert_eq!(titles(&moved), ["B"]);
    assert_eq!(moved.tasks[0].subtasks[0].text, "Child");
    cleanup(&f.app);
}

#[test]
fn shortcut_from_the_title_editor_saves_typed_text_before_moving() {
    let mut f = fixture(["A", "B", "C"]);
    f.app.editing = Some((f.list, f.tasks[1]));
    f.app.edit_text = "B".into();
    f.app.request_edit_focus = true;
    idle(&f.ctx, &mut f.app);
    assert_eq!(
        focused(&f.ctx),
        Some(Id::new(("edit-task", f.list, f.tasks[1])))
    );
    // Typing and the shortcut arrive in one batch; the text must land first.
    let mut events = vec![Event::Text(" edited".into())];
    events.extend(move_chord());
    frame(&f.ctx, &mut f.app, events, 430.0);
    idle(&f.ctx, &mut f.app);
    assert!(f.app.editing.is_none());
    assert_eq!(f.app.details.move_task.as_ref().unwrap().task, f.tasks[1]);
    assert!(saved(&f.app, f.list).tasks[1].text.contains("edited"));
    choose_with_keys(&mut f, 1);
    let remaining = saved(&f.app, f.list);
    assert_eq!(titles(&remaining), ["A", "C"]);
    let moved = saved(&f.app, f.third);
    assert_eq!(moved.tasks.len(), 1);
    assert!(moved.tasks[0].text.contains("edited"));
    cleanup(&f.app);
}

#[test]
fn escape_and_cancel_leave_files_alone_and_return_focus_to_the_row() {
    for pointer in [false, true] {
        let mut f = fixture(["A", "B", "C"]);
        let row = FocusItem::Task(f.list, f.tasks[1]).id();
        focus(&f.ctx, &mut f.app, row);
        let before = std::fs::read(&f.app.lists[0].path).unwrap();
        key(
            &f.ctx,
            &mut f.app,
            Key::M,
            Modifiers::COMMAND | Modifiers::SHIFT,
        );
        assert!(f.app.details.move_task.is_some());
        if pointer {
            click_label(&f.ctx, &mut f.app, "Cancel");
        } else {
            key(&f.ctx, &mut f.app, Key::Escape, Modifiers::NONE);
        }
        assert!(f.app.details.move_task.is_none());
        assert_eq!(focused(&f.ctx), Some(row));
        assert_eq!(std::fs::read(&f.app.lists[0].path).unwrap(), before);
        assert!(saved(&f.app, f.other).tasks.is_empty());
        cleanup(&f.app);
    }
}

#[test]
fn the_move_button_moves_and_the_last_destination_is_preselected() {
    let mut f = fixture(["A", "B", "C"]);
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[1]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    key(&f.ctx, &mut f.app, Key::ArrowDown, Modifiers::NONE);
    click_label(&f.ctx, &mut f.app, "Move");
    assert_eq!(titles(&saved(&f.app, f.third)), ["B"]);
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[0]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(destination(&f.app), f.third, "the last destination returns");
    key(&f.ctx, &mut f.app, Key::Escape, Modifiers::NONE);
    // A deleted last destination falls back to the first other list.
    let removed = f.app.lists.remove(f.app.list_index(f.third).unwrap());
    storage::delete_list(&removed).unwrap();
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[0]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(destination(&f.app), f.other);
    cleanup(&f.app);
}

#[test]
fn shortcut_ignores_archived_rows_and_unfocused_screens() {
    let mut f = fixture(["A", "B", "C"]);
    f.app.lists[0].data.tasks[0].toggle();
    f.app.archive_completed(f.list);
    f.app.history_open = true;
    idle(&f.ctx, &mut f.app);
    focus(
        &f.ctx,
        &mut f.app,
        FocusItem::Archived(f.list, f.tasks[0]).id(),
    );
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(f.app.details.move_task.is_none());
    focus(&f.ctx, &mut f.app, Id::new(("new-task", f.list)));
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(f.app.details.move_task.is_none());
    cleanup(&f.app);
}

#[test]
fn command_z_and_the_toast_undo_restore_the_original_position_in_every_sort() {
    for sort in [TaskSort::Manual, TaskSort::Title] {
        for toast in [false, true] {
            let mut f = fixture(["Zulu", "Middle", "Alpha"]);
            let list = f.list;
            f.app.sort_tasks(list, sort);
            focus(&f.ctx, &mut f.app, FocusItem::Task(list, f.tasks[1]).id());
            key(
                &f.ctx,
                &mut f.app,
                Key::M,
                Modifiers::COMMAND | Modifiers::SHIFT,
            );
            choose_with_keys(&mut f, 1);
            assert_eq!(titles(&saved(&f.app, list)), ["Zulu", "Alpha"]);
            assert_eq!(titles(&saved(&f.app, f.third)), ["Middle"]);
            if toast {
                click_label(&f.ctx, &mut f.app, "Undo");
            } else {
                key(&f.ctx, &mut f.app, Key::Z, Modifiers::COMMAND);
            }
            assert_eq!(
                titles(&saved(&f.app, list)),
                ["Zulu", "Middle", "Alpha"],
                "{sort:?} (toast: {toast})"
            );
            assert!(saved(&f.app, f.third).tasks.is_empty());
            assert!(f.app.details.move_task.is_none());
            cleanup(&f.app);
        }
    }
}
