//! Multi-line paste into the task, checklist, and Quick add entry fields.

use super::*;
use crate::{
    app::tests::{chord, frame_input, run_app_frame, test_app},
    model::{Subtask, TaskSort, TodoList},
    storage,
};
use egui::{Key, Modifiers};
use std::fs;

struct Fixture {
    ctx: egui::Context,
    app: MinimalistApp,
    list: Uuid,
}

fn frame(f: &mut Fixture, events: Vec<Event>) {
    let input = frame_input(
        egui::vec2(430.0, 500.0),
        f.ctx.input(|input| input.time) + 0.3,
        events,
    );
    f.app.update_motion(&f.ctx);
    run_app_frame(&f.ctx, &mut f.app, input);
}

fn idle(f: &mut Fixture) {
    for _ in 0..3 {
        frame(f, Vec::new());
    }
}

fn paste(f: &mut Fixture, text: &str) {
    frame(f, vec![Event::Paste(text.into())]);
    idle(f);
}

fn focused(f: &Fixture) -> Option<Id> {
    f.ctx.memory(|memory| memory.focused())
}

fn focus(f: &mut Fixture, id: Id) {
    f.app.request_task_focus = false;
    f.app.keyboard.pending = Some(id);
    idle(f);
    assert_eq!(focused(f), Some(id));
}

/// Opens the first list with the given tasks and focuses its task entry.
fn fixture(existing: &[&str]) -> Fixture {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = existing.iter().copied().map(Task::new).collect();
    assert!(app.save_list_index(0));
    if !matches!(app.screen, Screen::List(id) if id == list) {
        app.open_list(list);
    }
    let mut f = Fixture { ctx, app, list };
    focus(&mut f, Id::new(("new-task", list)));
    f
}

fn titles(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|task| task.text.as_str()).collect()
}

fn saved(f: &Fixture) -> TodoList {
    serde_json::from_slice(&fs::read(&f.app.lists[0].path).unwrap()).unwrap()
}

fn notice(f: &Fixture) -> Option<&str> {
    f.app
        .persistence
        .notice
        .as_ref()
        .map(|(message, _)| message.as_str())
}

/// Changes the list file elsewhere, then fails a local save so recovery is offered.
fn conflict(f: &mut Fixture) {
    let mut remote = storage::load_workspace(&f.app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.title = "Changed elsewhere".into();
    storage::save_list(&remote).unwrap();
    f.app.lists[0].data.title = "Changed here".into();
    assert!(!f.app.save_list_index(0));
    assert!(f.app.has_save_conflict(f.list));
}

fn cleanup(f: Fixture) {
    fs::remove_dir_all(&f.app.workspace).unwrap();
}

#[test]
fn pasted_items_split_lines_and_strip_one_marker() {
    let items = |text| pasted_items(text);
    assert_eq!(items("Milk\nEggs"), ["Milk", "Eggs"]);
    assert_eq!(items("Milk\r\nEggs\rBread"), ["Milk", "Eggs", "Bread"]);
    assert_eq!(
        items("  - Milk  \n\n * Eggs\n\t• Bread\r\n   \n"),
        ["Milk", "Eggs", "Bread"]
    );
    assert_eq!(
        items("1. One\n2) Two\n10. Ten\n007) Bond"),
        ["One", "Two", "Ten", "Bond"]
    );
    assert_eq!(
        items("- [ ] Open\n* [x] Done\n[X] Shout\n[ ]   Bare box\n1. [ ] Numbered box"),
        ["Open", "Done", "Shout", "Bare box", "Numbered box"]
    );
    // Only one bullet and one checkbox are removed, and only with a gap after it.
    assert_eq!(
        items("- - Nested\n[ ] [x] Twice\n-5 degrees\n*bold*\n1.5 liters\n2024\n[x]no gap"),
        [
            "- Nested",
            "[x] Twice",
            "-5 degrees",
            "*bold*",
            "1.5 liters",
            "2024",
            "[x]no gap"
        ]
    );
    // Markers alone leave nothing to add.
    assert!(items("-\n*\n3.\n[ ]\n- [x]\n\n").is_empty());
    assert_eq!(items("Buy milk\n"), ["Buy milk"]);
    assert_eq!(items("One line"), ["One line"]);
    assert!(items("").is_empty());
}

#[test]
fn task_entry_paste_adds_one_task_per_line_at_the_top_with_one_save() {
    let mut f = fixture(&["Old A", "Old B"]);
    f.app.new_task = "half-typed".into();
    paste(&mut f, "- Milk\r\n- [ ] Eggs\n\n3. Bread\n");
    let expected = ["Milk", "Eggs", "Bread", "Old A", "Old B"];
    assert_eq!(titles(&f.app.lists[0].data.tasks), expected);
    assert_eq!(titles(&saved(&f).tasks), expected);
    assert_eq!(f.app.new_task, "half-typed", "the draft is untouched");
    assert_eq!(notice(&f), Some("Added 3 tasks"));
    assert_eq!(focused(&f), Some(Id::new(("new-task", f.list))));
    let tasks = &f.app.lists[0].data.tasks;
    assert!(tasks.iter().all(|task| !task.completed));
    assert_eq!(
        tasks[0].created_at_unix, tasks[2].created_at_unix,
        "a paste shares one creation time"
    );
    // The next ordinary submission still adds above the pasted tasks.
    f.app.new_task.clear();
    frame(&mut f, vec![Event::Text("Single".into())]);
    frame(&mut f, chord(Key::Enter, Modifiers::NONE));
    assert_eq!(titles(&saved(&f).tasks)[..2], ["Single", "Milk"]);
    cleanup(f);
}

#[test]
fn newest_sort_keeps_pasted_order_in_view_and_manual_order_on_disk() {
    let mut f = fixture(&[]);
    let (mut old_a, mut old_b) = (Task::new("Old A"), Task::new("Old B"));
    (old_a.created_at_unix, old_b.created_at_unix) = (100, 200);
    f.app.lists[0].data.tasks = vec![old_a, old_b];
    assert!(f.app.save_list_index(0));
    f.app.sort_tasks(f.list, TaskSort::Newest);
    idle(&mut f);
    assert_eq!(titles(&f.app.lists[0].data.tasks), ["Old B", "Old A"]);
    paste(&mut f, "Milk\nEggs\nBread");
    assert_eq!(
        titles(&f.app.lists[0].data.tasks),
        ["Milk", "Eggs", "Bread", "Old B", "Old A"]
    );
    assert_eq!(
        titles(&saved(&f).tasks),
        ["Milk", "Eggs", "Bread", "Old A", "Old B"]
    );
    f.app.sort_tasks(f.list, TaskSort::Manual);
    assert_eq!(
        titles(&f.app.lists[0].data.tasks),
        ["Milk", "Eggs", "Bread", "Old A", "Old B"]
    );
    cleanup(f);
}

#[test]
fn single_line_pastes_are_left_to_the_text_field() {
    for text in ["Buy milk", "Buy milk\n", "\r\n- Buy milk\r\n", "Buy milk\r"] {
        let mut f = fixture(&["Existing"]);
        paste(&mut f, text);
        assert_eq!(titles(&f.app.lists[0].data.tasks), ["Existing"], "{text:?}");
        assert_eq!(f.app.new_task, text.replace(['\r', '\n'], " "), "{text:?}");
        assert!(notice(&f).is_none());
        cleanup(f);
    }
}

#[test]
fn multiline_paste_elsewhere_is_left_to_the_focused_widget() {
    let mut f = fixture(&["Existing"]);
    let (list, task) = (f.list, f.app.lists[0].data.tasks[0].id);
    focus(
        &mut f,
        crate::app::keyboard::FocusItem::Task(list, task).id(),
    );
    paste(&mut f, "One\nTwo\nThree");
    assert_eq!(titles(&f.app.lists[0].data.tasks), ["Existing"]);
    focus(&mut f, Id::new(("new-task", list)));
    f.app.open_find();
    idle(&mut f);
    paste(&mut f, "One\nTwo\nThree");
    assert_eq!(titles(&f.app.lists[0].data.tasks), ["Existing"]);
    assert!(f.app.find.is_some());
    cleanup(f);
}

#[test]
fn paste_before_a_shortcut_in_one_frame_still_creates_the_tasks() {
    let mut f = fixture(&[]);
    let mut events = vec![Event::Paste("One\nTwo".into())];
    events.extend(chord(Key::F, Modifiers::COMMAND));
    frame(&mut f, events);
    idle(&mut f);
    assert_eq!(titles(&saved(&f).tasks), ["One", "Two"]);
    assert!(f.app.find.is_some(), "the deferred shortcut still runs");
    cleanup(f);
}

fn open_checklist(f: &mut Fixture, children: &[&str]) -> Uuid {
    let mut task = Task::new("Parent");
    task.subtasks = children.iter().copied().map(Subtask::new).collect();
    let id = task.id;
    f.app.lists[0].data.tasks = vec![task];
    assert!(f.app.save_list_index(0));
    f.app.switch_subtask_panel(Some((f.list, id)));
    idle(f);
    focus(f, Id::new(("new-subtask", f.list, id)));
    id
}

#[test]
fn checklist_paste_appends_subtasks_in_order_and_keeps_the_draft() {
    let mut f = fixture(&[]);
    let parent = open_checklist(&mut f, &["Existing"]);
    f.app.new_subtask = "half-typed".into();
    paste(&mut f, "- [x] A\r\n* B\n2) C");
    let expected = ["Existing", "A", "B", "C"];
    let children = |tasks: &[Task]| {
        tasks[0]
            .subtasks
            .iter()
            .map(|child| child.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(children(&f.app.lists[0].data.tasks), expected);
    assert_eq!(children(&saved(&f).tasks), expected);
    assert!(saved(&f).tasks[0].subtasks.iter().all(|c| !c.completed));
    assert_eq!(f.app.new_subtask, "half-typed");
    assert_eq!(notice(&f), Some("Added 3 subtasks"));
    assert_eq!(titles(&saved(&f).tasks), ["Parent"], "no tasks are created");
    assert_eq!(focused(&f), Some(Id::new(("new-subtask", f.list, parent))));
    f.app.new_subtask.clear();
    paste(&mut f, "Last\n");
    assert_eq!(f.app.new_subtask, "Last ");
    assert_eq!(saved(&f).tasks[0].subtasks.len(), 4);
    cleanup(f);
}

fn open_quick_add(f: &mut Fixture, draft: &str) {
    f.app.open_quick_capture(Some(f.list));
    f.app.quick_capture.as_mut().unwrap().text = draft.into();
    idle(f);
    assert_eq!(focused(f), Some(Id::new("quick-capture-text")));
}

#[test]
fn quick_add_paste_saves_to_the_destination_and_closes_when_the_draft_is_empty() {
    let mut f = fixture(&["Existing"]);
    open_quick_add(&mut f, "  ");
    paste(&mut f, "- One\n- Two\r\nThree");
    assert!(f.app.quick_capture.is_none());
    assert_eq!(
        titles(&saved(&f).tasks),
        ["One", "Two", "Three", "Existing"]
    );
    assert_eq!(notice(&f), Some("Added 3 tasks to Tasks"));
    assert_eq!(f.app.settings.last_capture_list_id, Some(f.list));
    cleanup(f);
}

#[test]
fn quick_add_paste_keeps_a_nonempty_draft_open() {
    let mut f = fixture(&[]);
    open_quick_add(&mut f, "Unfinished");
    paste(&mut f, "One\nTwo");
    let state = f.app.quick_capture.as_ref().unwrap();
    assert_eq!(state.text, "Unfinished");
    assert_eq!(state.selected_list_id, Some(f.list));
    assert_eq!(titles(&saved(&f).tasks), ["One", "Two"]);
    assert_eq!(notice(&f), Some("Added 2 tasks to Tasks"));
    assert_eq!(focused(&f), Some(Id::new("quick-capture-text")));
    // Enter still adds the draft as its own task, above the pasted ones.
    frame(&mut f, chord(Key::Enter, Modifiers::NONE));
    assert!(f.app.quick_capture.is_none());
    assert_eq!(titles(&saved(&f).tasks), ["Unfinished", "One", "Two"]);
    assert_eq!(notice(&f), Some("Added to Tasks"));
    cleanup(f);
}

#[test]
fn quick_add_paste_with_a_conflicted_destination_stays_in_the_draft() {
    let mut f = fixture(&[]);
    open_quick_add(&mut f, "");
    conflict(&mut f);
    paste(&mut f, "One\nTwo");
    assert_eq!(f.app.quick_capture.as_ref().unwrap().text, "One Two");
    assert!(saved(&f).tasks.is_empty());
    cleanup(f);
}

#[test]
fn quick_add_retries_a_failed_batch_without_resubmitting_the_typed_draft() {
    let mut f = fixture(&[]);
    open_quick_add(&mut f, "Unfinished");
    let (lists, offline) = (
        f.app.workspace.join("lists"),
        f.app.workspace.join("offline"),
    );
    fs::rename(&lists, &offline).unwrap();
    fs::write(&lists, "unavailable").unwrap();
    paste(&mut f, "One\nTwo");
    let state = f.app.quick_capture.as_ref().unwrap();
    assert_eq!(state.text, "Unfinished");
    assert!(state.failure.is_some());
    assert!(f.app.status.is_some(), "the storage error is shown");
    assert!(f.app.lists[0].data.tasks.is_empty());
    assert!(notice(&f).is_none());
    f.app.open_find();
    assert!(!f.app.finish_pending_changes());
    assert!(f.app.quick_capture.is_some());

    fs::remove_file(&lists).unwrap();
    fs::rename(&offline, &lists).unwrap();
    idle(&mut f);
    frame(&mut f, chord(Key::Enter, Modifiers::NONE));
    let state = f.app.quick_capture.as_ref().unwrap();
    assert_eq!(state.text, "Unfinished");
    assert!(state.failure.is_none());
    let reloaded = storage::load_workspace(&f.app.workspace).unwrap();
    assert_eq!(titles(&reloaded.lists[0].data.tasks), ["One", "Two"]);
    idle(&mut f);
    frame(&mut f, chord(Key::Enter, Modifiers::NONE));
    assert!(f.app.quick_capture.is_none());
    assert_eq!(titles(&saved(&f).tasks), ["Unfinished", "One", "Two"]);
    cleanup(f);
}

#[test]
fn quick_add_paste_conflict_recovery_keeps_the_batch_and_typed_draft_once() {
    let mut f = fixture(&["Existing"]);
    open_quick_add(&mut f, "Unfinished");
    let mut remote = storage::load_workspace(&f.app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks.push(Task::new("Remote"));
    storage::save_list(&remote).unwrap();

    paste(&mut f, "One\nTwo");
    assert!(f.app.has_save_conflict(f.list));
    assert!(f.app.keep_local_copy_and_reload(f.list));
    assert!(f.app.quick_capture.is_none());
    let reloaded = storage::load_workspace(&f.app.workspace).unwrap();
    let original = reloaded
        .lists
        .iter()
        .find(|list| list.key == f.list)
        .unwrap();
    let copy = reloaded
        .lists
        .iter()
        .find(|list| list.key != f.list)
        .unwrap();
    assert_eq!(titles(&original.data.tasks), ["Existing", "Remote"]);
    assert_eq!(
        titles(&copy.data.tasks),
        ["One", "Two", "Unfinished", "Existing"]
    );
    cleanup(f);
}

#[test]
fn a_subtask_edit_keeps_pasted_lines_in_one_item() {
    let mut f = fixture(&[]);
    let parent = open_checklist(&mut f, &["Original"]);
    let child = f.app.lists[0].data.tasks[0].subtasks[0].id;
    f.app.begin_subtask_edit(f.list, parent, child);
    idle(&mut f);
    f.app.details.editor.as_mut().unwrap().text.clear();
    paste(&mut f, "one\ntwo");
    assert_eq!(f.app.details.editor.as_ref().unwrap().text, "one\ntwo");
    frame(&mut f, chord(Key::Enter, Modifiers::NONE));
    idle(&mut f);
    assert!(f.app.details.editor.is_none());
    let subtasks = &saved(&f).tasks[0].subtasks;
    assert_eq!(
        subtasks.len(),
        1,
        "an inline edit never splits into new items"
    );
    assert_eq!(subtasks[0].text, "one\ntwo");
    cleanup(f);
}
