//! Keyboard Move and Undo journeys through real widgets and saved list files.

use super::*;
use crate::app::tests::label_center;
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
    let position = label_center(&output, label);
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
    storage::delete_list(&f.app.workspace, &removed).unwrap();
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
fn a_dialog_whose_task_vanished_closes_and_returns_focus_to_task_entry() {
    let mut f = fixture(["A", "B", "C"]);
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[1]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(f.app.details.move_task.is_some());
    f.app.lists[0].data.tasks.remove(1);
    f.app.request_task_focus = false;
    idle(&f.ctx, &mut f.app);
    assert!(f.app.details.move_task.is_none());
    assert_eq!(focused(&f.ctx), Some(Id::new(("new-task", f.list))));
    cleanup(&f.app);
}

#[test]
fn a_vanished_destination_keeps_the_dialog_open_and_can_be_replaced() {
    let mut f = fixture(["A", "B", "C"]);
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[1]).id());
    key(
        &f.ctx,
        &mut f.app,
        Key::M,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    key(&f.ctx, &mut f.app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(destination(&f.app), f.third);
    let removed = f.app.lists.remove(f.app.list_index(f.third).unwrap());
    storage::delete_list(&f.app.workspace, &removed).unwrap();
    key(&f.ctx, &mut f.app, Key::Enter, Modifiers::NONE);
    assert!(f.app.details.move_task.is_some());
    assert_eq!(
        f.app.status.as_deref(),
        Some("That list is no longer available. Choose another list.")
    );
    assert_eq!(titles(&saved(&f.app, f.list)), ["A", "B", "C"]);
    key(&f.ctx, &mut f.app, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(destination(&f.app), f.other);
    key(&f.ctx, &mut f.app, Key::Enter, Modifiers::NONE);
    assert!(f.app.details.move_task.is_none());
    assert_eq!(titles(&saved(&f.app, f.other)), ["B"]);
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

#[test]
fn global_quick_add_keeps_move_return_focus() {
    for source_survives in [true, false] {
        let mut f = fixture(["A", "B", "C"]);
        let source_task = f.tasks[1];
        let source_row = FocusItem::Task(f.list, source_task).id();
        focus(&f.ctx, &mut f.app, source_row);
        frame(&f.ctx, &mut f.app, move_chord(), 430.0);
        idle(&f.ctx, &mut f.app);
        assert!(f.app.details.move_task.is_some());
        assert_ne!(focused(&f.ctx), Some(source_row));

        let return_focus = if source_survives {
            Some(source_row)
        } else {
            let index = f.app.list_index(f.list).unwrap();
            f.app.lists[index]
                .data
                .tasks
                .retain(|task| task.id != source_task);
            assert!(f.app.save_list_index(index));
            f.app.default_focus()
        };
        let source_before = saved(&f.app, f.list);
        let _ = f.ctx.run_logic(&Default::default(), |ctx| {
            f.app.request_global_quick_add(ctx);
        });
        idle(&f.ctx, &mut f.app);
        assert!(f.app.details.move_task.is_none());
        assert_eq!(focused(&f.ctx), Some(Id::new("quick-capture-text")));
        key(&f.ctx, &mut f.app, Key::Escape, Modifiers::NONE);
        assert!(f.app.quick_capture.is_none());
        assert_eq!(focused(&f.ctx), return_focus);
        assert_eq!(titles(&saved(&f.app, f.list)), titles(&source_before));
        assert!(saved(&f.app, f.other).tasks.is_empty());
        cleanup(&f.app);
    }
}

#[test]
fn move_without_a_destination_keeps_the_title_editor_open() {
    let mut f = fixture(["A", "B", "C"]);
    f.app.lists.truncate(1);
    let task = f.tasks[1];
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, task).id());
    key(&f.ctx, &mut f.app, Key::Enter, Modifiers::NONE);
    type_text(&f.ctx, &mut f.app, " draft");
    let editor = Id::new(("edit-task", f.list, task));
    let draft = f.app.edit_text.clone();
    assert_eq!(focused(&f.ctx), Some(editor));
    frame(&f.ctx, &mut f.app, move_chord(), 430.0);
    idle(&f.ctx, &mut f.app);
    assert!(f.app.details.move_task.is_none());
    assert_eq!(focused(&f.ctx), Some(editor));
    assert_eq!(f.app.edit_text, draft);
    assert_eq!(titles(&saved(&f.app, f.list)), ["A", "B", "C"]);
    cleanup(&f.app);
}

#[test]
fn undo_move_from_another_list_leaves_its_focused_row_alone() {
    let mut f = fixture(["A", "B", "C"]);
    let unrelated = Task::new("Unrelated");
    let row = FocusItem::Task(f.third, unrelated.id).id();
    let index = f.app.list_index(f.third).unwrap();
    f.app.lists[index].data.tasks.push(unrelated);
    assert!(f.app.save_list_index(index));
    focus(&f.ctx, &mut f.app, FocusItem::Task(f.list, f.tasks[1]).id());
    frame(&f.ctx, &mut f.app, move_chord(), 430.0);
    idle(&f.ctx, &mut f.app);
    choose_with_keys(&mut f, 0);
    f.app.open_list(f.third);
    focus(&f.ctx, &mut f.app, row);
    key(&f.ctx, &mut f.app, Key::Z, Modifiers::COMMAND);
    idle(&f.ctx, &mut f.app);
    assert_eq!(titles(&saved(&f.app, f.list)), ["A", "B", "C"]);
    assert!(saved(&f.app, f.other).tasks.is_empty());
    assert_eq!(focused(&f.ctx), Some(row));
    cleanup(&f.app);
}

#[test]
fn long_move_titles_keep_the_picker_and_buttons_inside_a_narrow_window() {
    for height in [280.0, 500.0] {
        let long_title = "A long task title for the narrow Move dialog. ".repeat(35);
        let mut f = fixture([&long_title, "B", "C"]);
        f.app.begin_move_task(f.list, f.tasks[0]);
        let size = egui::vec2(320.0, height);
        let mut output = Default::default();
        for _ in 0..4 {
            let input = crate::app::tests::frame_input(
                size,
                f.ctx.input(|input| input.time) + 0.3,
                Vec::new(),
            );
            output = crate::app::tests::run_app_frame(&f.ctx, &mut f.app, input);
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for label in ["Other", "Move", "Cancel"] {
            let position = label_center(&output, label);
            assert!(
                screen.contains(position),
                "{label} at {position} outside {screen:?}"
            );
        }
        let position = label_center(&output, "Move");
        let events = vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        let input =
            crate::app::tests::frame_input(size, f.ctx.input(|input| input.time) + 0.3, events);
        crate::app::tests::run_app_frame(&f.ctx, &mut f.app, input);
        assert!(f.app.details.move_task.is_none());
        assert_eq!(saved(&f.app, f.other).tasks[0].text, long_title);
        assert_eq!(titles(&saved(&f.app, f.list)), ["B", "C"]);
        cleanup(&f.app);
    }
}

#[test]
fn long_move_destinations_keep_the_picker_and_popup_inside_a_narrow_window() {
    let mut f = fixture(["A", "B", "C"]);
    let long_title = "Long destination list title ".repeat(20);
    let destination = f.app.list_index(f.other).unwrap();
    f.app.lists[destination].data.title.clone_from(&long_title);
    f.ctx.enable_accesskit();
    f.app.begin_move_task(f.list, f.tasks[0]);
    let size = egui::vec2(320.0, 280.0);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let render = |f: &mut Fixture, events| {
        let input =
            crate::app::tests::frame_input(size, f.ctx.input(|input| input.time) + 0.3, events);
        crate::app::tests::run_app_frame(&f.ctx, &mut f.app, input)
    };
    for _ in 0..4 {
        render(&mut f, Vec::new());
    }
    let picker = f.ctx.read_response(focused(&f.ctx).unwrap()).unwrap();
    assert!(
        screen.contains_rect(picker.rect),
        "destination picker {:?} outside {screen:?}",
        picker.rect
    );
    let position = picker.rect.center();
    render(
        &mut f,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    for _ in 0..3 {
        render(&mut f, Vec::new());
    }
    let output = render(&mut f, Vec::new());
    let tree = output.platform_output.accesskit_update.unwrap();
    let choice = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(long_title.as_str()))
        .expect("the full destination name remains accessible");
    let bounds = choice.1.bounds().unwrap();
    assert!(bounds.x0 >= 0.0 && bounds.x1 <= f64::from(size.x));
    assert_eq!(
        f.app.details.move_task.as_ref().unwrap().destination,
        f.other
    );
    cleanup(&f.app);
}
