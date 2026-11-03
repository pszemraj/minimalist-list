//! History deletion, Undo, clearing, archive age, and offscreen row regressions.

use super::*;
use crate::{
    app::{
        Screen,
        context::relative_age,
        tests::{chord, frame_input, run_app_frame, snapshot_files, test_app},
    },
    model::{Subtask, TitleOverflow, TodoList},
    storage,
};
use egui::{Event, Key, Modifiers};
use std::fs;

fn frame(ctx: &egui::Context, app: &mut MinimalistApp, events: Vec<Event>) {
    let time = ctx.input(|input| input.time) + 0.3;
    run_app_frame(
        ctx,
        app,
        frame_input(egui::vec2(430.0, 550.0), time, events),
    );
}

fn idle(ctx: &egui::Context, app: &mut MinimalistApp) {
    for _ in 0..4 {
        frame(ctx, app, Vec::new());
    }
}

fn key_edge(ctx: &egui::Context, app: &mut MinimalistApp, key: Key, pressed: bool) {
    frame(
        ctx,
        app,
        vec![Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false, // egui derives genuine repeats from the held-key state.
            modifiers: Modifiers::NONE,
        }],
    );
}

fn focus(ctx: &egui::Context, app: &mut MinimalistApp, id: Id) {
    app.request_task_focus = false;
    app.keyboard.pending = Some(id);
    idle(ctx, app);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(id));
}

fn saved(app: &MinimalistApp) -> TodoList {
    serde_json::from_slice(&fs::read(&app.lists[0].path).unwrap()).unwrap()
}

/// Fills the first list's History with `count` entries archived at distinct times and opens it.
fn open_history(app: &mut MinimalistApp, count: usize) -> (Uuid, Vec<Uuid>) {
    let list = app.lists[0].key;
    app.lists[0].data.archive = (0..count)
        .map(|index| {
            let mut item = ArchivedTask::new(Task::new(format!("Archived {index}")));
            item.archived_at_unix = 1_000 + index as u64;
            item
        })
        .collect();
    let ids = app.lists[0]
        .data
        .archive
        .iter()
        .map(|item| item.task.id)
        .collect();
    app.screen = Screen::List(list);
    app.history_open = true;
    app.save_list_index(0);
    (list, ids)
}

#[test]
fn deleted_history_entry_returns_to_its_position_with_its_archive_time() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 3);
    app.expand_archived(list, ids[1]);
    app.delete_archived(list, ids[1]);
    let remaining: Vec<_> = saved(&app)
        .archive
        .iter()
        .map(|item| item.task.id)
        .collect();
    assert_eq!(remaining, [ids[0], ids[2]]);
    assert!(!app.details.expanded_history.contains(&(list, ids[1])));
    assert!(app.history_open);
    app.undo_last();
    let restored = saved(&app).archive;
    assert_eq!(
        restored.iter().map(|item| item.task.id).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(restored[1].archived_at_unix, 1_001);
    assert!(app.details.undo.is_none());
    assert!(app.history_open);
    assert_eq!(
        app.keyboard.pending,
        Some(FocusItem::Archived(list, ids[1]).id())
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn deleting_the_last_history_entry_closes_history_and_undo_reopens_it() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 1);
    app.delete_archived(list, ids[0]);
    assert!(!app.history_open);
    assert!(saved(&app).archive.is_empty());
    app.undo_last();
    assert!(app.history_open);
    assert_eq!(saved(&app).archive[0].task.id, ids[0]);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn history_undo_refuses_an_identity_that_reappeared_and_stays_available() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 2);
    app.delete_archived(list, ids[0]);
    let mut duplicate = Task::new("Same identity");
    duplicate.id = ids[0];
    app.lists[0].data.tasks.push(duplicate);
    app.undo_last();
    assert_eq!(app.lists[0].data.archive.len(), 1);
    assert!(matches!(app.details.undo, Some(UndoItem::Archived { .. })));
    assert_eq!(
        app.status.as_deref(),
        Some("The deleted task already exists in the workspace.")
    );
    app.lists[0].data.tasks.clear();
    app.undo_last();
    assert_eq!(app.lists[0].data.archive[0].task.id, ids[0]);
    assert!(app.details.undo.is_none());
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn delete_key_on_history_controls_removes_one_row_per_physical_press() {
    for title in [false, true] {
        let (ctx, mut app) = test_app();
        let (list, ids) = open_history(&mut app, 4);
        idle(&ctx, &mut app);
        let control = if title {
            history_title_id(list, ids[1])
        } else {
            FocusItem::Archived(list, ids[1]).id()
        };
        focus(&ctx, &mut app, control);
        key_edge(&ctx, &mut app, Key::Delete, true);
        for _ in 0..5 {
            key_edge(&ctx, &mut app, Key::Delete, true);
        }
        key_edge(&ctx, &mut app, Key::Delete, false);
        idle(&ctx, &mut app);
        let remaining: Vec<_> = saved(&app)
            .archive
            .iter()
            .map(|item| item.task.id)
            .collect();
        assert_eq!(remaining, [ids[0], ids[2], ids[3]]);
        // Focus moves to the next row's Restore action rather than falling out.
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(FocusItem::Archived(list, ids[2]).id())
        );
        key_edge(&ctx, &mut app, Key::Delete, true);
        key_edge(&ctx, &mut app, Key::Delete, false);
        idle(&ctx, &mut app);
        assert_eq!(app.lists[0].data.archive.len(), 2);
        // The shared slot holds the latest deletion; Undo restores its original place.
        app.undo_last();
        assert_eq!(app.lists[0].data.archive[1].task.id, ids[2]);
        fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn held_history_delete_leaves_the_task_entry_draft_intact() {
    let (ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 1);
    let entry = Id::new(("new-task", list));
    focus(&ctx, &mut app, entry);
    frame(&ctx, &mut app, vec![Event::Text("Keep this draft".into())]);
    let mut editor = egui::TextEdit::load_state(&ctx, entry).unwrap();
    editor
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(0),
        )));
    editor.store(&ctx, entry);
    focus(&ctx, &mut app, FocusItem::Archived(list, ids[0]).id());

    key_edge(&ctx, &mut app, Key::Delete, true);
    idle(&ctx, &mut app);
    assert!(saved(&app).archive.is_empty());
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(entry));
    for _ in 0..5 {
        key_edge(&ctx, &mut app, Key::Delete, true);
    }
    key_edge(&ctx, &mut app, Key::Delete, false);
    assert_eq!(app.new_task, "Keep this draft");

    key_edge(&ctx, &mut app, Key::Delete, true);
    key_edge(&ctx, &mut app, Key::Delete, true);
    key_edge(&ctx, &mut app, Key::Delete, false);
    assert_eq!(app.new_task, "ep this draft");
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clearing_history_empties_the_archive_and_keeps_live_tasks() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 3);
    app.lists[0].data.tasks.push(Task::new("Still active"));
    app.expand_archived(list, ids[0]);
    let other = Uuid::new_v4();
    app.details.expanded_history.insert((other, ids[0]));
    app.clear_history(list);
    let stored = saved(&app);
    assert!(stored.archive.is_empty());
    assert_eq!(stored.tasks[0].text, "Still active");
    assert!(!app.history_open);
    assert!(!app.details.expanded_history.contains(&(list, ids[0])));
    assert!(app.details.expanded_history.contains(&(other, ids[0])));
    fs::remove_dir_all(&app.workspace).unwrap();
}

/// Orders the snapshots by their numeric timestamps, oldest first.
fn snapshots_by_time(app: &MinimalistApp, list: Uuid) -> Vec<std::path::PathBuf> {
    let mut files = snapshot_files(app, list);
    files.sort_by_key(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str()?.parse::<u64>().ok())
    });
    files
}

#[test]
fn clearing_history_snapshots_the_archive_even_right_after_another_snapshot() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 3);
    // Opening History saved the list, so a throttled snapshot is already under an hour old.
    assert_eq!(snapshot_files(&app, list).len(), 1);
    app.clear_history(list);
    assert!(app.status.is_none());
    assert!(saved(&app).archive.is_empty());
    let files = snapshots_by_time(&app, list);
    // A same-second name is bumped into the future, so the save may add one more copy.
    assert!(files.len() >= 2);
    let kept: TodoList = serde_json::from_slice(&fs::read(files.last().unwrap()).unwrap()).unwrap();
    assert_eq!(
        kept.archive
            .iter()
            .map(|item| item.task.id)
            .collect::<Vec<_>>(),
        ids
    );
    fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clearing_history_keeps_its_snapshot_when_the_clock_has_moved_back() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 1);
    let original = snapshot_files(&app, list).pop().unwrap();
    let directory = original.parent().unwrap();
    let earlier_contents = fs::read(&original).unwrap();
    fs::remove_file(&original).unwrap();
    let future = now_unix_seconds() + 86_400;
    for timestamp in future..future + 100 {
        fs::write(
            directory.join(format!("{timestamp}.json")),
            &earlier_contents,
        )
        .unwrap();
    }

    app.clear_history(list);
    assert!(saved(&app).archive.is_empty());
    assert!(app.status.is_none());
    app.lists[0].data.tasks.push(Task::new("Later edit"));
    assert!(app.save_list_index(0));

    let snapshots = snapshot_files(&app, list);
    assert_eq!(snapshots.len(), 100);
    assert!(snapshots.iter().any(|path| {
        let backup: TodoList = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        backup.archive.iter().any(|item| item.task.id == ids[0])
    }));
    fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clearing_history_keeps_newly_archived_tasks_recoverable_after_a_write_outage() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let lists = app.workspace.join("lists");
    let offline = app.workspace.join("offline");
    fs::rename(&lists, &offline).unwrap();
    fs::write(&lists, "a file blocks the folder").unwrap();

    let mut task = Task::new("Archived while offline");
    let id = task.id;
    task.completed = true;
    app.lists[0].data.tasks.push(task);
    app.archive_completed(list);
    assert!(app.persistence.dirty_lists.contains(&list));
    assert_eq!(app.lists[0].data.archive[0].task.id, id);
    app.clear_history(list);
    assert_eq!(app.lists[0].data.archive[0].task.id, id);

    fs::remove_file(&lists).unwrap();
    fs::rename(&offline, &lists).unwrap();
    assert!(saved(&app).archive.is_empty());
    app.clear_history(list);
    assert!(saved(&app).archive.is_empty());
    assert!(app.lists[0].data.archive.is_empty());
    assert!(app.status.is_none());
    let kept: TodoList =
        serde_json::from_slice(&fs::read(snapshots_by_time(&app, list).last().unwrap()).unwrap())
            .unwrap();
    assert_eq!(kept.archive[0].task.id, id);

    fs::remove_dir_all(storage::backups_dir(&app.workspace)).unwrap();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clearing_history_is_refused_when_no_backup_can_be_written() {
    let (_ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 3);
    let backups = storage::backups_dir(&app.workspace);
    fs::remove_dir_all(&backups).unwrap();
    fs::create_dir_all(&backups).unwrap();
    fs::write(backups.join("snapshots"), "a file blocks the folder").unwrap();
    app.clear_history(list);
    assert!(
        app.status
            .as_deref()
            .unwrap()
            .starts_with("History was kept because no backup could be written")
    );
    assert_eq!(app.lists[0].data.archive.len(), ids.len());
    assert_eq!(saved(&app).archive.len(), ids.len());
    assert!(app.history_open);
    fs::remove_dir_all(backups).unwrap();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn relative_age_names_whole_days_hours_minutes_or_just_now() {
    assert_eq!(relative_age("Created", 100, 159), "Created just now");
    assert_eq!(relative_age("Created", 100, 160), "Created 1 minutes ago");
    assert_eq!(relative_age("Archived", 0, 7_200), "Archived 2 hours ago");
    assert_eq!(
        relative_age("Archived", 0, 3 * 86_400 + 5),
        "Archived 3 days ago"
    );
    assert_eq!(relative_age("Archived", 500, 100), "Archived just now");
}

#[test]
fn history_rows_show_one_truncated_age_line_and_keep_controls_reachable() {
    for overflow in [TitleOverflow::Scroll, TitleOverflow::Wrap] {
        for width in [200.0, 320.0, 520.0] {
            let (ctx, mut app) = test_app();
            app.settings.title_overflow = overflow;
            app.settings.font_size = 30.0;
            let (list, ids) = open_history(&mut app, 3);
            app.lists[0].data.archive[0].task.text = "A long archived title ".repeat(10);
            app.lists[0].data.archive[0].archived_at_unix =
                crate::model::now_unix_seconds().saturating_sub(3 * 86_400 + 60);
            let palette = app.palette();
            let output = run_frame_at(&ctx, width, |ui| app.history_ui(ui, list, palette, 1.0));
            let ages: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text)
                        if text.galley.text().starts_with("Archived ") =>
                    {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size())).map(|rect| {
                            (text.galley.text().to_owned(), text.galley.rows.len(), rect)
                        })
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(ages.len(), 3, "{overflow:?} at {width}");
            assert!(ages.iter().all(|(_, rows, _)| *rows == 1));
            if width >= 320.0 {
                assert_eq!(ages[0].0, "Archived 3 days ago");
            }
            let spacing = ctx.global_style().spacing.item_spacing.y;
            for (index, id) in ids.iter().enumerate() {
                let copy = ctx
                    .read_response(Id::new(("copy-task", list, *id, true)))
                    .unwrap()
                    .rect;
                let restore = ctx
                    .read_response(FocusItem::Archived(list, *id).id())
                    .unwrap()
                    .rect;
                assert!(ages[index].2.right() <= copy.left(), "age overlaps copy");
                assert!(copy.right() <= restore.left());
                assert!(restore.right() <= width, "Restore escaped at {width}");
                if let Some(next) = ids.get(index + 1) {
                    // The reserved height is exactly the built title, gap, and age line.
                    let next = ctx
                        .read_response(FocusItem::Archived(list, *next).id())
                        .unwrap()
                        .rect;
                    assert!(
                        (next.top() - ages[index].2.bottom() - spacing).abs() < 0.6,
                        "row {index} reserved a different height than it built"
                    );
                }
            }
            fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}

fn run_frame_at(
    ctx: &egui::Context,
    width: f32,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    crate::app::tests::run_frame(
        ctx,
        frame_input(egui::vec2(width, 40_000.0), 0.0, Vec::new()),
        render,
    )
}

#[test]
fn offscreen_history_rows_skip_titles_but_keyboard_navigation_reaches_them() {
    let (ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 400);
    let (first, last) = (ids[0], ids[399]);
    for _ in 0..6 {
        frame(&ctx, &mut app, Vec::new());
    }
    assert!(ctx.read_response(history_title_id(list, first)).is_some());
    assert!(ctx.read_response(history_title_id(list, last)).is_none());
    let reserved = ctx
        .read_response(FocusItem::Archived(list, last).id())
        .unwrap()
        .rect;
    assert!(reserved.top() > 550.0);
    focus(&ctx, &mut app, FocusItem::Archived(list, first).id());
    frame(&ctx, &mut app, chord(Key::End, Modifiers::NONE));
    for _ in 0..6 {
        frame(&ctx, &mut app, Vec::new());
    }
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Archived(list, last).id())
    );
    let title = ctx
        .read_response(history_title_id(list, last))
        .unwrap()
        .rect;
    assert!(title.top() >= 0.0 && title.bottom() <= 550.0);
    // The row that left the viewport is skipped again, yet its reserved space is unchanged.
    assert!(ctx.read_response(history_title_id(list, first)).is_none());
    frame(&ctx, &mut app, chord(Key::ArrowUp, Modifiers::NONE));
    for _ in 0..4 {
        frame(&ctx, &mut app, Vec::new());
    }
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Archived(list, ids[398]).id())
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn offscreen_history_restore_keeps_its_accessible_name_and_role() {
    let (ctx, mut app) = test_app();
    ctx.enable_accesskit();
    let (list, ids) = open_history(&mut app, 40);
    idle(&ctx, &mut app);
    let time = ctx.input(|input| input.time) + 0.3;
    let output = run_app_frame(
        &ctx,
        &mut app,
        frame_input(egui::vec2(430.0, 550.0), time, Vec::new()),
    );
    assert!(ctx.read_response(history_title_id(list, ids[0])).is_some());
    assert!(ctx.read_response(history_title_id(list, ids[39])).is_none());
    let tree = output.platform_output.accesskit_update.unwrap();
    fs::remove_dir_all(&app.workspace).unwrap();
    for position in [0, 39] {
        let id = FocusItem::Archived(list, ids[position]).id().accesskit_id();
        let (_, node) = tree
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .unwrap();
        assert_eq!(node.role(), egui::accesskit::Role::Button);
        assert_eq!(
            node.label(),
            Some(format!("Restore: Archived {position}").as_str())
        );
    }
}

#[test]
fn enter_on_an_offscreen_history_row_in_the_frame_that_focuses_it_restores_it() {
    let (ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 400);
    for _ in 0..6 {
        frame(&ctx, &mut app, Vec::new());
    }
    let last = ids[399];
    assert!(ctx.read_response(history_title_id(list, last)).is_none());
    app.request_task_focus = false;
    app.keyboard.pending = Some(FocusItem::Archived(list, last).id());
    frame(&ctx, &mut app, chord(Key::Enter, Modifiers::NONE));
    idle(&ctx, &mut app);
    assert_eq!(app.lists[0].data.archive.len(), 399);
    assert!(
        app.lists[0]
            .data
            .archive
            .iter()
            .all(|item| item.task.id != last)
    );
    assert!(app.lists[0].data.tasks.iter().any(|task| task.id == last));
    assert_eq!(saved(&app).archive.len(), 399);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn skipped_history_rows_reserve_the_same_space_as_built_rows() {
    let (ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 3);
    app.lists[0].data.archive[1].task.text = "Wrapped archived title ".repeat(12);
    let palette = app.palette();
    let mut built = Vec::new();
    for height in [40_000.0, 1.0] {
        // A widget stays readable for one extra pass, so settle each viewport twice.
        for _ in 0..2 {
            crate::app::tests::run_frame(
                &ctx,
                frame_input(egui::vec2(400.0, height), 0.0, Vec::new()),
                |ui| {
                    let clip =
                        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(400.0, height));
                    ui.set_clip_rect(clip);
                    app.history_ui(ui, list, palette, 1.0);
                },
            );
        }
        built.push(
            ids.iter()
                .map(|id| {
                    ctx.read_response(FocusItem::Archived(list, *id).id())
                        .unwrap()
                        .rect
                })
                .collect::<Vec<_>>(),
        );
    }
    // Nothing is visible under the tiny clip, yet every Restore control sits where it did.
    assert_eq!(built[0], built[1]);
    for id in &ids {
        assert!(ctx.read_response(history_title_id(list, *id)).is_none());
    }
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn expanded_history_rows_keep_children_even_when_offscreen() {
    let (ctx, mut app) = test_app();
    let (list, ids) = open_history(&mut app, 120);
    let mut task = Task::new("Deep parent");
    task.subtasks.push(Subtask::new("Offscreen child"));
    let target = ids[110];
    app.lists[0].data.archive[110].task = Task { id: target, ..task };
    app.expand_archived(list, target);
    for _ in 0..6 {
        frame(&ctx, &mut app, Vec::new());
    }
    assert!(ctx.read_response(history_title_id(list, target)).is_some());
    fs::remove_dir_all(&app.workspace).unwrap();
}
