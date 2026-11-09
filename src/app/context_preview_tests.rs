//! Delayed notes previews, pointer transfer, and constrained tooltip scrolling.

use super::{tests::app_frame, *};
use crate::app::tests::test_app;
use std::fs;

#[test]
fn wrap_preview_waits_two_seconds_and_includes_saved_context() {
    for mode in [TitleOverflow::Wrap, TitleOverflow::Scroll] {
        let (ctx, mut app) = test_app();
        let mut task = Task::new("Task title for metadata preview");
        task.toggle_started();
        task.context = "Saved notes for the delayed preview".into();
        let id = task.id;
        app.lists[0].data.tasks.push(task);
        app.view_zoom = 1.0;
        app.request_task_focus = false;
        app.settings.title_overflow = mode;
        app_frame(&ctx, &mut app, vec![], 1.0);
        let rect = ctx.read_response(Id::new(("task-text", id))).unwrap().rect;
        app_frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(rect.center())],
            1.2,
        );
        let early = app_frame(&ctx, &mut app, vec![], 3.0);
        assert!(!early.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("Created"))));
        app_frame(&ctx, &mut app, vec![], 3.3);
        let late = app_frame(&ctx, &mut app, vec![], 3.4);
        let metadata = late.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("In progress · 0/0 subtasks")));
        assert_eq!(metadata, mode == TitleOverflow::Wrap);
        let notes = late.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("Saved notes for the delayed preview")));
        assert_eq!(notes, mode == TitleOverflow::Wrap);
        fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn long_wrap_preview_stays_bounded_and_scrollable_until_the_pointer_leaves() {
    let (ctx, mut app) = test_app();
    let mut task = Task::new("Long notes preview");
    task.context = (0..100).map(|i| format!("Note line {i}\n")).collect();
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.settings.title_overflow = TitleOverflow::Wrap;
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    app_frame(&ctx, &mut app, vec![], 1.0);
    let response = ctx.read_response(Id::new(("task-text", id))).unwrap();
    app_frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(response.rect.center())],
        1.2,
    );
    for time in [3.3, 3.4, 3.5] {
        app_frame(&ctx, &mut app, vec![], time);
    }
    let tooltip_id = egui::Tooltip::tooltip_id(response.id, 0);
    let preview = ctx.memory(|memory| memory.area_rect(tooltip_id)).unwrap();
    assert!(preview.height() <= 440.0, "oversized preview: {preview:?}");
    assert!(ctx.content_rect().contains_rect(preview));
    app_frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(preview.center())],
        3.6,
    );
    let output = app_frame(&ctx, &mut app, vec![], 3.7);
    let before = notes_y(&output);
    app_frame(
        &ctx,
        &mut app,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -160.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }],
        3.8,
    );
    assert!(ctx.read_response(response.id).unwrap().is_tooltip_open());
    let output = app_frame(&ctx, &mut app, vec![], 4.1);
    let after = notes_y(&output);
    assert!(after < before, "notes did not scroll: {before} -> {after}");
    app_frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(510.0, 690.0))],
        4.4,
    );
    app_frame(&ctx, &mut app, vec![], 4.7);
    assert!(!ctx.read_response(response.id).unwrap().is_tooltip_open());
    fs::remove_dir_all(&app.workspace).unwrap();
}

fn notes_y(output: &egui::FullOutput) -> f32 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.text().contains("Note line 0") => {
                Some(text.pos.y)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn preview_accepts_diagonal_pointer_transfer_and_closes_when_its_owner_is_disabled() {
    let (ctx, mut app) = test_app();
    app.settings.title_overflow = TitleOverflow::Wrap;
    let mut task = Task::new("Wide hovered title");
    task.context = "Scrollable notes\n".repeat(100);
    let draw = |events, time, enabled| {
        crate::app::tests::run_frame(
            &ctx,
            crate::app::tests::frame_input(egui::vec2(800.0, 700.0), time, events),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let response =
                        ui.allocate_response(egui::vec2(620.0, 60.0), egui::Sense::hover());
                    app.wrapped_task_preview(ui, &response, &task, app.palette());
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(Id::new("preview-owner"), response));
                });
            },
        );
    };
    draw(vec![], 1.0, true);
    let response = ctx
        .data(|data| data.get_temp::<egui::Response>(Id::new("preview-owner")))
        .unwrap();
    let origin = response.rect.right_bottom() - egui::vec2(2.0, 2.0);
    draw(vec![egui::Event::PointerMoved(origin)], 1.2, true);
    for time in [3.3, 3.4, 3.5] {
        draw(vec![], time, true);
    }
    let preview = ctx
        .memory(|memory| memory.area_rect(egui::Tooltip::tooltip_id(response.id, 0)))
        .unwrap();
    let between = origin.lerp(preview.center(), 0.15);
    assert!(!response.rect.contains(between) && !preview.expand(4.0).contains(between));
    draw(vec![egui::Event::PointerMoved(between)], 3.6, true);
    assert!(ctx.read_response(response.id).unwrap().is_tooltip_open());
    draw(vec![egui::Event::PointerMoved(preview.center())], 3.8, true);
    assert!(ctx.read_response(response.id).unwrap().is_tooltip_open());
    draw(vec![], 4.0, false);
    draw(vec![], 4.2, false);
    assert!(!ctx.read_response(response.id).unwrap().is_tooltip_open());
    fs::remove_dir_all(&app.workspace).unwrap();
}
