//! Visible child previews, independently collapsed parents, and draft ownership.

use super::*;
use crate::{app::tests::test_app, model::Subtask};
use std::fs;

fn fixture() -> (egui::Context, MinimalistApp, Uuid, [Task; 2]) {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let tasks = ["First parent", "Second parent"].map(|title| {
        let mut task = Task::new(title);
        task.subtasks.push(Subtask::new(format!("{title} child")));
        task
    });
    app.lists[0].data.tasks = tasks.to_vec();
    app.save_list_index(0);
    (ctx, app, list, tasks)
}

fn children_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    list: Uuid,
    tasks: &[Task],
) -> egui::FullOutput {
    crate::app::tests::run_frame(
        ctx,
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(440.0, 700.0),
            )),
            ..Default::default()
        },
        |ui| {
            let palette = app.palette();
            for task in tasks {
                app.task_children_below(ui, list, task, palette);
            }
        },
    )
}

fn text_count(output: &egui::FullOutput, expected: &str) -> usize {
    output.shapes.iter().filter(|shape|
        matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == expected)
    ).count()
}

#[test]
fn preference_shows_every_checklist_without_allocating_multiple_entry_drafts() {
    let (ctx, mut app, list, tasks) = fixture();
    app.settings.show_subtasks = true;
    app.new_subtask = "Unowned draft must not appear under either parent".into();
    let before = fs::read(&app.lists[0].path).unwrap();
    children_frame(&ctx, &mut app, list, &tasks);
    let output = children_frame(&ctx, &mut app, list, &tasks);
    assert_eq!(text_count(&output, "First parent child"), 1);
    assert_eq!(text_count(&output, "Second parent child"), 1);
    assert_eq!(text_count(&output, "Edit subtasks"), 2);
    assert!(app.subtasks_for.is_none());
    assert!(app.details.editor.is_none());
    assert_eq!(
        app.new_subtask,
        "Unowned draft must not appear under either parent"
    );
    assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn collapsing_one_parent_preserves_other_previews_and_its_scoped_entry_draft() {
    let (ctx, mut app, list, tasks) = fixture();
    app.settings.show_subtasks = true;
    app.switch_subtask_panel(Some((list, tasks[0].id)));
    app.new_subtask = "First parent's unfinished child".into();
    app.toggle_task_subtasks(list, tasks[0].id);
    assert!(app.subtasks_for.is_none());
    assert!(!app.preview_is_visible(list, &tasks[0]));
    assert!(app.preview_is_visible(list, &tasks[1]));
    children_frame(&ctx, &mut app, list, &tasks);
    let output = children_frame(&ctx, &mut app, list, &tasks);
    assert_eq!(text_count(&output, "First parent child"), 0);
    assert_eq!(text_count(&output, "Second parent child"), 1);
    app.toggle_task_subtasks(list, tasks[0].id);
    assert!(app.preview_is_visible(list, &tasks[0]));
    assert!(app.subtasks_for.is_none());
    app.switch_subtask_panel(Some((list, tasks[1].id)));
    app.new_subtask = "Second parent's unfinished child".into();
    app.switch_subtask_panel(Some((list, tasks[0].id)));
    assert_eq!(app.new_subtask, "First parent's unfinished child");
    app.switch_subtask_panel(Some((list, tasks[1].id)));
    assert_eq!(app.new_subtask, "Second parent's unfinished child");
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn disabled_preference_keeps_existing_explicit_expand_and_collapse_behavior() {
    let (ctx, mut app, list, tasks) = fixture();
    assert!(!app.settings.show_subtasks);
    assert!(!app.preview_is_visible(list, &tasks[0]));
    app.toggle_task_subtasks(list, tasks[0].id);
    assert_eq!(app.subtasks_for, Some((list, tasks[0].id)));
    children_frame(&ctx, &mut app, list, &tasks);
    let output = children_frame(&ctx, &mut app, list, &tasks);
    assert_eq!(text_count(&output, "First parent child"), 1);
    assert_eq!(text_count(&output, "Second parent child"), 0);
    app.toggle_task_subtasks(list, tasks[0].id);
    assert!(app.subtasks_for.is_none());
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn large_checklist_counts_reserve_their_full_measured_control_width() {
    let (ctx, mut app, list, mut tasks) = fixture();
    tasks[0].subtasks = (0..1_000).map(|_| Subtask::new("Child")).collect();
    app.settings.font_size = 30.0;
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let task = &tasks[0];
        let width = app.task_controls_width(ui, task);
        let row = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(320.0, 44.0));
        app.task_checklist_control(ui, row, list, task, app.palette());
        let response = ui
            .ctx()
            .read_response(Id::new(("task-checklist", list, task.id)))
            .unwrap();
        let progress =
            ui.painter()
                .layout_no_wrap("0/1000".into(), app.font_id(17.0), egui::Color32::WHITE);
        assert!(response.rect.width() >= progress.size().x + 26.0);
        assert!(row.right() - width + 44.0 < response.rect.left());
        assert_eq!(response.rect.height(), 32.0);
    });
    output.textures_delta.clear();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn preview_completion_is_drawn_with_vectors_and_wrap_reserves_the_marker_column() {
    for font in [
        crate::model::FontChoice::Sans,
        crate::model::FontChoice::Mono,
    ] {
        for completed in [false, true] {
            let (ctx, mut app) = test_app();
            app.settings.font = font;
            app.settings.font_size = 30.0;
            let mut child =
                Subtask::new("Long child text wraps beside its completion marker".repeat(3));
            child.completed = completed;
            let mut bounds = Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(200.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    bounds = app.child_preview_row(ui, &child, app.palette());
                },
            );
            let circles: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::epaint::Shape::Circle(circle) = &shape.shape {
                        Some(circle)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(circles.len(), if completed { 2 } else { 1 });
            assert_eq!(
                output
                    .shapes
                    .iter()
                    .filter(|shape| matches!(&shape.shape, egui::epaint::Shape::LineSegment { .. }))
                    .count(),
                if completed { 2 } else { 0 }
            );
            let text = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        Some(text)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(text.galley.text(), child.text);
            assert!(text.galley.rows.len() > 1);
            assert!(text.pos.x >= circles[0].center.x + 12.0);
            assert!(text.pos.x + text.galley.size().x <= bounds.right() + 0.1);
            assert!(text.pos.y + text.galley.size().y <= bounds.bottom() + 0.1);
            output.textures_delta.clear();
            fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}
