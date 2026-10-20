//! Offscreen row culling retains keyboard navigation and narrow-row actions.

use super::*;
use crate::app::tests::{chord, test_app};

fn frame(ctx: &egui::Context, app: &mut MinimalistApp, width: f32, events: Vec<egui::Event>) {
    crate::app::tests::run_frame(
        ctx,
        egui::RawInput {
            time: Some(ctx.input(|input| input.time) + 0.3),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 550.0))),
            events,
            ..Default::default()
        },
        |ui| {
            <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
        },
    );
}

#[test]
fn offscreen_rows_skip_title_widgets_but_end_navigation_reaches_them() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = (0..400).map(|i| Task::new(format!("Task {i}"))).collect();
    let first = app.lists[0].data.tasks[0].id;
    let last = app.lists[0].data.tasks[399].id;
    for _ in 0..4 {
        frame(&ctx, &mut app, 520.0, vec![]);
    }
    assert!(ctx.read_response(Id::new(("task-text", first))).is_some());
    assert!(ctx.read_response(Id::new(("task-text", last))).is_none());
    assert!(
        ctx.read_response(FocusItem::Task(list, last).id())
            .is_some()
    );
    app.keyboard.pending = Some(FocusItem::Task(list, first).id());
    frame(&ctx, &mut app, 520.0, vec![]);
    frame(
        &ctx,
        &mut app,
        520.0,
        chord(egui::Key::End, egui::Modifiers::NONE),
    );
    for _ in 0..5 {
        frame(&ctx, &mut app, 520.0, vec![]);
    }
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(list, last).id())
    );
    let title = ctx
        .read_response(Id::new(("task-text", last)))
        .unwrap()
        .rect;
    assert!(title.top() >= 0.0 && title.bottom() <= 550.0);
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn narrow_rows_recover_copy_space_and_still_copy_with_the_shortcut() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Task 2 title is readable");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    for _ in 0..4 {
        frame(&ctx, &mut app, 320.0, vec![]);
    }
    let narrow = ctx.read_response(Id::new(("task-text", id))).unwrap().rect;
    assert!(
        ctx.read_response(Id::new(("copy-task", list, id, false)))
            .is_none()
    );
    app.keyboard.pending = Some(FocusItem::Task(list, id).id());
    frame(&ctx, &mut app, 320.0, vec![]);
    let output = crate::app::tests::run_frame(
        &ctx,
        egui::RawInput {
            events: chord(
                egui::Key::C,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            ),
            ..Default::default()
        },
        |ui| {
            <MinimalistApp as eframe::App>::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
        },
    );
    assert!(output.platform_output.commands.iter().any(|command|
        matches!(command, egui::OutputCommand::CopyText(text) if text == "[ ] Task 2 title is readable")));
    for _ in 0..3 {
        frame(&ctx, &mut app, 520.0, vec![]);
    }
    let wide = ctx.read_response(Id::new(("task-text", id))).unwrap().rect;
    assert!(
        ctx.read_response(Id::new(("copy-task", list, id, false)))
            .is_some()
    );
    assert!((wide.width() - narrow.width() - 169.0).abs() < 2.0);
    std::fs::remove_dir_all(&app.workspace).unwrap();
}
