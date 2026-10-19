//! Headless application helpers and task interaction regressions.
use super::*;

/// Runs a headless UI frame and discards texture updates without a renderer.
///
/// # Arguments
///
/// - `ctx` - Context retaining focus and widget state between frames.
/// - `input` - Simulated frame input.
/// - `render` - UI to render during the frame.
///
/// # Returns
/// The frame output without texture updates.
pub(super) fn run_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, render);
    output.textures_delta.clear();
    output
}

/// Creates press and release events for a key with the same modifiers.
///
/// # Arguments
///
/// - `key` - Key to press and release.
/// - `modifiers` - Modifiers held for both events.
///
/// # Returns
///
/// The key press followed by its release, without repeat or physical-key data.
pub(super) fn chord(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}

#[test]
fn focused_enter_submits_without_nested_context_input() {
    let ctx = egui::Context::default();
    let field_id = egui::Id::new("text-input-shortcut-test");
    let mut title = "Second".to_owned();

    run_frame(&ctx, Default::default(), |ui| {
        let response = ui.add(egui::TextEdit::singleline(&mut title).id(field_id));
        response.request_focus();
    });

    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
        ..Default::default()
    };
    let mut submitted = false;
    run_frame(&ctx, input, |ui| {
        let response = ui.add(egui::TextEdit::singleline(&mut title).id(field_id));
        assert!(response.lost_focus());
        assert!(!response.has_focus());
        let (submit, cancel) = text_input_shortcuts(ui, &response);
        submitted = submit;
        assert!(!cancel);
    });

    // Nesting the focus query inside `Ui::input` panics or hangs before this can become true.
    assert!(submitted);
}

/// Creates an application whose task files are confined to a temporary workspace.
///
/// # Returns
///
/// A context and application ready for isolated UI tests.
pub(super) fn test_app() -> (egui::Context, MinimalistApp) {
    let ctx = egui::Context::default();
    let workspace = std::env::temp_dir().join(format!("minimalist-list-ui-{}", Uuid::new_v4()));
    let settings = Settings {
        workspace_path: workspace.to_string_lossy().into_owned(),
        ..Settings::default()
    };
    let app = MinimalistApp::new(&ctx, settings, None, None);
    (ctx, app)
}

fn enter_events() -> Vec<egui::Event> {
    chord(egui::Key::Enter, egui::Modifiers::default())
}

#[test]
fn view_changes_and_task_removal_cancel_drags() {
    let (ctx, mut app) = test_app();
    let list_id = app.lists[0].key;
    let mut task = crate::model::Task::new("Completed last task");
    task.completed = true;
    let task_id = task.id;
    app.lists[0].data.tasks = vec![crate::model::Task::new("Active"), task];
    let drag = DragState {
        list_id,
        task_id,
        source: 1,
        target: 1,
        delta: Vec2::ZERO,
        source_center: 200.0,
        height: 36.0,
    };
    app.drag = Some(drag.clone());
    app.begin_transition(1.0);
    assert!(app.drag.is_none());

    app.drag = Some(drag);
    app.archive_completed(list_id);
    assert!(app.drag.is_none());
    assert_eq!(app.lists[0].data.tasks.len(), 1);
    let palette = app.palette();
    run_frame(&ctx, Default::default(), |ui| {
        app.list_ui(ui, list_id, palette)
    });

    app.drag = Some(DragState {
        list_id,
        task_id: app.lists[0].data.tasks[0].id,
        source: 0,
        target: 0,
        delta: Vec2::ZERO,
        source_center: 160.0,
        height: 36.0,
    });
    app.remove_task(list_id, app.lists[0].data.tasks[0].id);
    assert!(app.drag.is_none());
    run_frame(&ctx, Default::default(), |ui| {
        app.list_ui(ui, list_id, palette)
    });
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn task_row_focus_survives_insertion_and_reordering() {
    let (ctx, mut app) = test_app();
    let list_id = app.lists[0].key;
    app.lists[0].data.tasks = vec![
        crate::model::Task::new("Packing"),
        crate::model::Task::new("Laundry"),
    ];
    app.request_task_focus = true;
    let palette = app.palette();
    run_frame(&ctx, Default::default(), |ui| {
        app.list_ui(ui, list_id, palette)
    });
    for _ in 0..2 {
        run_frame(
            &ctx,
            egui::RawInput {
                events: chord(egui::Key::Tab, egui::Modifiers::default()),
                ..Default::default()
            },
            |ui| app.list_ui(ui, list_id, palette),
        );
    }
    let focused = ctx.memory(|memory| memory.focused()).unwrap();
    let before = ctx.read_response(focused).unwrap().rect;
    assert!(before.top() > 100.0);
    assert!(before.height() > 20.0);
    app.lists[0]
        .data
        .tasks
        .insert(0, crate::model::Task::new("New task"));
    app.lists[0].data.tasks.reverse();
    for _ in 0..2 {
        run_frame(&ctx, Default::default(), |ui| {
            app.list_ui(ui, list_id, palette)
        });
    }
    let after = ctx.read_response(focused).unwrap();
    assert!(after.has_focus());
    assert!(
        after.rect.top() > before.top(),
        "{focused:?}: {before:?} -> {:?}",
        after.rect
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn subtask_capture_keeps_focus_after_rows_are_inserted() {
    let (ctx, mut app) = test_app();
    let list_id = app.lists[0].key;
    let task = crate::model::Task::new("Packing");
    app.lists[0].data.tasks.push(task.clone());
    let field = egui::Id::new(("new-subtask", list_id, task.id));
    let palette = app.palette();
    run_frame(&ctx, Default::default(), |ui| {
        app.subtasks_panel(ui, list_id, &task, palette, 1.0)
    });
    ctx.memory_mut(|memory| memory.request_focus(field));
    for text in ["Charger", "Passport"] {
        let task = app.lists[0].data.tasks[0].clone();
        run_frame(
            &ctx,
            egui::RawInput {
                events: vec![egui::Event::Text(text.to_owned())],
                ..Default::default()
            },
            |ui| app.subtasks_panel(ui, list_id, &task, palette, 1.0),
        );
        run_frame(
            &ctx,
            egui::RawInput {
                events: enter_events(),
                ..Default::default()
            },
            |ui| app.subtasks_panel(ui, list_id, &task, palette, 1.0),
        );
        let task = app.lists[0].data.tasks[0].clone();
        run_frame(&ctx, Default::default(), |ui| {
            app.subtasks_panel(ui, list_id, &task, palette, 1.0)
        });
        assert!(ctx.memory(|memory| memory.has_focus(field)));
        assert!(app.new_subtask.is_empty());
    }
    assert_eq!(
        app.lists[0].data.tasks[0]
            .subtasks
            .iter()
            .map(|subtask| subtask.text.as_str())
            .collect::<Vec<_>>(),
        ["Charger", "Passport"]
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}
