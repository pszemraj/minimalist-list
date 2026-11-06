//! Notes-editor save and Find navigation regressions.

use super::*;
use crate::{
    app::{
        Screen,
        tests::{chord, frame_input, run_app_frame, test_app},
    },
    model::Accent,
    storage,
};
use std::{fs, path::PathBuf};

struct Fixture {
    ctx: egui::Context,
    app: MinimalistApp,
    source: Uuid,
    source_task: Uuid,
    source_path: PathBuf,
    destination: Uuid,
    target: Uuid,
}

fn frame(f: &mut Fixture, events: Vec<egui::Event>) {
    let input = frame_input(
        egui::vec2(520.0, 700.0),
        f.ctx.input(|input| input.time) + 0.3,
        events,
    );
    run_app_frame(&f.ctx, &mut f.app, input);
}

fn settle(f: &mut Fixture) {
    for _ in 0..3 {
        frame(f, Vec::new());
    }
}

fn fixture() -> Fixture {
    let (ctx, mut app) = test_app();
    let source = app.lists[0].key;
    let mut task = Task::new("Source task");
    task.context = "Original notes".into();
    let source_task = task.id;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    let source_path = app.lists[0].path.clone();
    let mut other = storage::create_list(&app.workspace, "Other", Accent::Sky).unwrap();
    let destination = other.key;
    let target_task = Task::new("Destination task");
    let target = target_task.id;
    other.data.tasks.push(target_task);
    storage::save_list(&other).unwrap();
    app.lists.push(other);
    app.open_list(source);
    app.open_task_context(source, source_task, false);
    let mut f = Fixture {
        ctx,
        app,
        source,
        source_task,
        source_path,
        destination,
        target,
    };
    settle(&mut f);
    f
}

fn open_find_and_activate_target(f: &mut Fixture) {
    frame(f, chord(egui::Key::F, egui::Modifiers::COMMAND));
    frame(f, vec![egui::Event::Text("Destination task".into())]);
    frame(f, chord(egui::Key::Enter, egui::Modifiers::NONE));
    settle(f);
}

#[test]
fn unchanged_notes_close_without_writing_an_externally_changed_list() {
    let mut f = fixture();
    let mut external: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.source_path).unwrap()).unwrap();
    external["tasks"][0]["text"] = "Changed on another device".into();
    external["tasks"][0]["context"] = "Notes updated elsewhere".into();
    let bytes = serde_json::to_vec_pretty(&external).unwrap();
    fs::write(&f.source_path, &bytes).unwrap();

    assert!(f.app.save_task_context());
    assert!(f.app.task_context.is_none());
    assert!(!f.app.has_save_conflict(f.source));
    assert!(f.app.persistence.dirty_lists.is_empty());
    assert_eq!(
        f.app.keyboard.pending,
        Some(FocusItem::Task(f.source, f.source_task).id())
    );
    assert_eq!(fs::read(&f.source_path).unwrap(), bytes);
    fs::remove_dir_all(f.app.workspace).unwrap();
}

#[test]
fn closing_unchanged_notes_still_flushes_unrelated_dirty_changes() {
    let mut f = fixture();
    let index = f.app.list_index(f.source).unwrap();
    f.app.lists[index].data.tasks[0].text = "Unrelated pending title".into();
    f.app.persistence.dirty_lists.insert(f.source);

    assert!(f.app.finish_pending_changes());
    assert!(f.app.task_context.is_none());
    assert!(f.app.persistence.dirty_lists.is_empty());
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.source_path).unwrap()).unwrap();
    assert_eq!(saved["tasks"][0]["text"], "Unrelated pending title");
    assert_eq!(saved["tasks"][0]["context"], "Original notes");
    fs::remove_dir_all(f.app.workspace).unwrap();
}

#[test]
fn activating_find_from_notes_saves_the_draft_then_focuses_the_result() {
    let mut f = fixture();
    f.app.task_context.as_mut().unwrap().text = "Unsaved note before navigation".into();
    open_find_and_activate_target(&mut f);

    assert!(f.app.find.is_none());
    assert!(f.app.task_context.is_none());
    assert!(matches!(f.app.screen, Screen::List(id) if id == f.destination));
    assert_eq!(
        f.ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(f.destination, f.target).id())
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.source_path).unwrap()).unwrap();
    assert_eq!(
        saved["tasks"][0]["context"],
        "Unsaved note before navigation"
    );
    fs::remove_dir_all(f.app.workspace).unwrap();
}

#[test]
fn failed_notes_save_keeps_the_draft_visible_without_opening_the_find_result() {
    let mut f = fixture();
    f.app.task_context.as_mut().unwrap().text = "Keep this unsaved draft".into();
    let mut external: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.source_path).unwrap()).unwrap();
    external["tasks"][0]["text"] = "Changed on another device".into();
    let bytes = serde_json::to_vec_pretty(&external).unwrap();
    fs::write(&f.source_path, &bytes).unwrap();
    open_find_and_activate_target(&mut f);

    assert!(f.app.find.is_none());
    assert!(matches!(f.app.screen, Screen::List(id) if id == f.source));
    let editor = f.app.task_context.as_ref().unwrap();
    assert_eq!((editor.list, editor.task), (f.source, f.source_task));
    assert_eq!(editor.text, "Keep this unsaved draft");
    assert!(editor.failure.is_some());
    assert!(f.app.has_save_conflict(f.source));
    assert_eq!(
        f.ctx.memory(|memory| memory.focused()),
        Some(Id::new("task-context-text"))
    );
    assert_eq!(fs::read(&f.source_path).unwrap(), bytes);
    fs::remove_dir_all(f.app.workspace).unwrap();
}
