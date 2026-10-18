//! Adversarial persistence and transition journeys using isolated task files.

use super::*;
use crate::model::Task;

fn ui_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    app.view_zoom = 1.0;
    let input = egui::RawInput {
        time: Some(ctx.input(|input| input.time) + 0.3),
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(520.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut MinimalistApp) {
    for _ in 0..3 {
        ui_frame(ctx, app, Vec::new());
    }
}

fn key_and_text(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    let mut events: Vec<_> = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect();
    events.push(egui::Event::Text("Native".into()));
    events
}

#[test]
fn escape_from_settings_and_typing_in_one_frame_reaches_the_returned_entry() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    app.open_settings(None);
    idle(&ctx, &mut app);
    app.settings.always_on_top = !app.settings.always_on_top;
    app.settings.window_decorations = !app.settings.window_decorations;
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(matches!(app.screen, Screen::List(_)));
    assert_eq!(app.new_task, "Native");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_settings_and_typing_in_one_frame_reaches_its_workspace_field() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::Comma, egui::Modifiers::COMMAND),
    );
    assert!(matches!(app.screen, Screen::Settings));
    assert!(
        app.workspace_input.contains("Native"),
        "{}",
        app.workspace_input
    );
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_find_and_typing_in_one_frame_reaches_the_query() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::F, egui::Modifiers::COMMAND),
    );
    idle(&ctx, &mut app);
    let output = ui_frame(&ctx, &mut app, Vec::new());
    assert!(app.find.is_some());
    let query = ctx.read_response(egui::Id::new("find-query")).unwrap().rect;
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Native" && query.contains(text.pos))
    }));
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn opening_quick_capture_and_typing_in_one_frame_reaches_only_capture() {
    let (ctx, mut app) = tests::test_app();
    idle(&ctx, &mut app);
    ui_frame(
        &ctx,
        &mut app,
        key_and_text(egui::Key::N, egui::Modifiers::COMMAND),
    );
    idle(&ctx, &mut app);
    let output = ui_frame(&ctx, &mut app, Vec::new());
    let input = ctx
        .read_response(egui::Id::new("quick-capture-text"))
        .unwrap()
        .rect;
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Native" && input.contains(text.pos))
    }));
    assert!(app.new_task.is_empty());
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn repeated_stale_instance_saves_preserve_a_newer_workspace_preference() {
    let (_, mut first) = tests::test_app();
    first.save_settings();
    let settings = storage::load_settings().unwrap();
    let ctx = egui::Context::default();
    let mut stale = MinimalistApp::new(&ctx, settings, None, None);
    let original = first.workspace.clone();
    let original_list = stale.lists[0].key;
    first.switch_workspace(&original.join("new-selection").to_string_lossy());
    let newer_selection = first.workspace.to_string_lossy().into_owned();
    for _ in 0..3 {
        stale.open_list(original_list);
        assert_eq!(
            storage::load_settings().unwrap().workspace_path,
            newer_selection
        );
    }
    std::fs::remove_dir_all(original).unwrap();
}

#[test]
fn workspace_switch_flushes_accepted_deletion_and_archive_animations() {
    for archive in [false, true] {
        let (_, mut app) = tests::test_app();
        let original = app.workspace.clone();
        let list = app.lists[0].key;
        let mut task = Task::new("Accepted mutation before folder change");
        task.completed = archive;
        let task_id = task.id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        if archive {
            app.begin_clear_completed(list);
            assert!(app.clear_animation.is_some());
        } else {
            app.begin_delete_task(list, task_id);
            assert!(app.delete_animation.is_some());
        }
        app.switch_workspace(&original.join("next-workspace").to_string_lossy());
        assert_ne!(app.workspace, original);
        let saved = storage::load_workspace(&original).unwrap();
        assert!(saved.lists[0].data.tasks.is_empty());
        assert_eq!(saved.lists[0].data.archive.len(), usize::from(archive));
        assert!(app.delete_animation.is_none());
        assert!(app.clear_animation.is_none());
        std::fs::remove_dir_all(original).unwrap();
    }
}
