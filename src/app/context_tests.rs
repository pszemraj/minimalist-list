//! Context compatibility, clipboard, and recovery regressions.

use super::*;
use crate::{app::tests::test_app, model::Subtask};
use std::fs;

#[test]
fn context_defaults_for_old_json_and_empty_context_keeps_original_format() {
    let task = Task::new("Legacy task");
    let serialized = serde_json::to_value(&task).unwrap();
    assert!(serialized.get("context").is_none());
    let loaded: Task = serde_json::from_value(serialized).unwrap();
    assert!(loaded.context.is_empty());
    let mut with_context = loaded;
    with_context.context = "Reference link\nSupporting details".into();
    let value = serde_json::to_value(&with_context).unwrap();
    assert_eq!(value["context"], "Reference link\nSupporting details");
    assert_eq!(
        serde_json::from_value::<Task>(value).unwrap().context,
        with_context.context
    );
}

#[test]
fn full_copy_includes_parent_context_and_each_child_completion() {
    let mut task = Task::new("Parent title");
    task.context = "Supporting details\nhttps://example.com/reference".into();
    task.subtasks = ["First child", "Second child\nwith detail"]
        .map(Subtask::new)
        .to_vec();
    task.subtasks[1].completed = true;
    assert_eq!(
        task.full_text(),
        "[ ] Parent title\n\nSupporting details\nhttps://example.com/reference\n\n- [ ] First child\n- [x] Second child\nwith detail"
    );
    task.toggle();
    assert!(task.full_text().starts_with("[x] Parent title"));
}

#[test]
fn copying_uses_a_brief_notice_and_preserves_save_errors() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Copy me");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.copy_task(&ctx, list, id, false);
    assert!(app.status.is_none());
    assert_eq!(
        app.persistence.notice.as_ref().unwrap().0,
        "Copied complete task"
    );
    app.status = Some("Unrelated save failed".into());
    app.copy_task(&ctx, list, id, false);
    assert_eq!(app.status.as_deref(), Some("Unrelated save failed"));
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn failed_context_save_retains_exact_draft_and_retry_saves_once() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Parent");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.open_task_context(list, id, false);
    let draft = "  Long context\nSecond paragraph\nhttps://example.com  ";
    app.task_context.as_mut().unwrap().text = draft.into();
    let original = app.lists[0].path.clone();
    fs::rename(&original, original.with_extension("backup")).unwrap();
    fs::create_dir(&original).unwrap();
    assert!(!app.save_task_context());
    assert_eq!(app.task_context.as_ref().unwrap().text, draft);
    assert!(app.task_context.as_ref().unwrap().failure.is_some());
    assert!(app.lists[0].data.tasks[0].context.is_empty());
    fs::remove_dir(&original).unwrap();
    assert!(app.save_task_context());
    assert!(app.task_context.is_none());
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(original).unwrap()).unwrap();
    assert_eq!(saved.tasks.len(), 1);
    assert_eq!(saved.tasks[0].id, id);
    assert_eq!(saved.tasks[0].context, draft);
    assert_eq!(saved.format_version, 1);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn archived_context_is_read_only_and_complete_copy_keeps_it() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Archived parent");
    task.context = "Archived supporting details".into();
    task.subtasks.push(Subtask::new("Archived child"));
    let id = task.id;
    app.lists[0]
        .data
        .archive
        .push(crate::model::ArchivedTask::new(task));
    app.save_list_index(0);
    let before = fs::read(&app.lists[0].path).unwrap();
    app.open_task_context(list, id, true);
    assert!(app.task_context.as_ref().unwrap().archived);
    let mut output = ctx.run_ui(Default::default(), |ui| {
        app.copy_task(ui.ctx(), list, id, true);
        app.task_context_ui(ui.ctx());
    });
    assert!(output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text.contains("Archived supporting details") && text.contains("Archived child"))));
    output.textures_delta.clear();
    assert!(app.save_task_context());
    assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
    assert!(app.lists[0].data.tasks.is_empty());
    fs::remove_dir_all(&app.workspace).unwrap();
}

fn app_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
    time: f64,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
        },
    );
    output.textures_delta.clear();
    output
}

fn chord(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
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

fn copied(output: &egui::FullOutput) -> Option<&str> {
    output.platform_output.commands.iter().find_map(|command| {
        if let egui::OutputCommand::CopyText(text) = command {
            Some(text.as_str())
        } else {
            None
        }
    })
}

#[test]
fn full_copy_shortcut_resolves_parent_child_controls_and_child_entry() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent task");
    task.context = "Hidden support".into();
    task.subtasks.push(Subtask::new("Child task"));
    task.subtasks[0].completed = true;
    let parent = task.id;
    let child = task.subtasks[0].id;
    let expected = task.full_text();
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    app.switch_subtask_panel(Some((list, parent)));
    app.new_subtask = "Unsubmitted child draft".into();
    app_frame(&ctx, &mut app, vec![], 1.0);
    let ids = [
        FocusItem::Task(list, parent).id(),
        Id::new(("task-text", parent)),
        Id::new(("copy-task", list, parent, false)),
        FocusItem::Subtask(list, parent, child).id(),
        Id::new(("edit-subtask-button", list, parent, child)),
        Id::new(("new-subtask", list, parent)),
    ];
    let mut time = 2.0;
    for id in ids {
        app.keyboard.pending = Some(id);
        app_frame(&ctx, &mut app, vec![], time);
        time += 0.3;
        app_frame(&ctx, &mut app, vec![], time);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(id));
        time += 0.3;
        let output = app_frame(
            &ctx,
            &mut app,
            chord(
                egui::Key::C,
                egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                },
            ),
            time,
        );
        assert_eq!(
            copied(&output),
            Some(expected.as_str()),
            "missing copy from {id:?}"
        );
        assert_eq!(app.new_subtask, "Unsubmitted child draft");
        time += 0.3;
    }
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn complete_copy_from_editors_keeps_drafts_and_normal_selection_copy() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Saved title");
    task.context = "Saved context".into();
    task.subtasks.push(Subtask::new("Saved child"));
    let parent = task.id;
    let child = task.subtasks[0].id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    app.editing = Some((list, parent));
    app.edit_text = "Current title draft".into();
    app.request_edit_focus = true;
    app_frame(&ctx, &mut app, vec![], 1.0);
    app_frame(&ctx, &mut app, vec![], 1.3);
    let full = app_frame(
        &ctx,
        &mut app,
        chord(
            egui::Key::C,
            egui::Modifiers {
                shift: true,
                ..egui::Modifiers::COMMAND
            },
        ),
        1.6,
    );
    assert!(
        copied(&full)
            .unwrap()
            .starts_with("[ ] Current title draft\n\nSaved context")
    );
    assert!(app.editing.is_some());
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::A, egui::Modifiers::COMMAND),
        1.9,
    );
    let selected = app_frame(&ctx, &mut app, vec![egui::Event::Copy], 2.2);
    assert_eq!(copied(&selected), Some("Current title draft"));
    app.finish_inline_edit();
    app.switch_subtask_panel(Some((list, parent)));
    app.begin_subtask_edit(list, parent, child);
    app.details.editor.as_mut().unwrap().text = "Current child draft".into();
    app_frame(&ctx, &mut app, vec![], 2.5);
    app_frame(&ctx, &mut app, vec![], 2.8);
    let full = app_frame(
        &ctx,
        &mut app,
        chord(
            egui::Key::C,
            egui::Modifiers {
                shift: true,
                ..egui::Modifiers::COMMAND
            },
        ),
        3.1,
    );
    assert!(copied(&full).unwrap().contains("- [ ] Current child draft"));
    assert!(app.details.editor.is_some());
    assert_eq!(app.lists[0].data.tasks[0].subtasks[0].text, "Saved child");
    app.cancel_subtask_edit();
    app.open_task_context(list, parent, false);
    app.task_context.as_mut().unwrap().text = "Current context draft".into();
    app_frame(&ctx, &mut app, vec![], 3.4);
    let full = app_frame(
        &ctx,
        &mut app,
        chord(
            egui::Key::C,
            egui::Modifiers {
                shift: true,
                ..egui::Modifiers::COMMAND
            },
        ),
        3.7,
    );
    assert!(copied(&full).unwrap().contains("Current context draft"));
    assert!(app.task_context.is_some());
    assert_eq!(app.lists[0].data.tasks[0].context, "Saved context");
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn complete_copy_includes_editor_input_before_the_shortcut_in_the_same_batch() {
    for editor in ["title", "child", "context"] {
        for input in ["text", "paste", "ime"] {
            for native_copy in [false, true] {
                let (ctx, mut app) = test_app();
                let list = app.lists[0].key;
                let mut task = Task::new("Saved title");
                task.context = "Saved context".into();
                task.subtasks.push(Subtask::new("Saved child"));
                let parent = task.id;
                let child = task.subtasks[0].id;
                app.lists[0].data.tasks.push(task);
                app.save_list_index(0);
                let before = fs::read(&app.lists[0].path).unwrap();
                app.view_zoom = 1.0;
                app.request_task_focus = false;
                match editor {
                    "title" => {
                        app.editing = Some((list, parent));
                        app.edit_text = "Saved title".into();
                        app.request_edit_focus = true;
                    }
                    "child" => {
                        app.switch_subtask_panel(Some((list, parent)));
                        app.begin_subtask_edit(list, parent, child);
                    }
                    _ => app.open_task_context(list, parent, false),
                }
                app_frame(&ctx, &mut app, vec![], 1.0);
                app_frame(&ctx, &mut app, vec![], 1.3);
                app_frame(
                    &ctx,
                    &mut app,
                    chord(egui::Key::A, egui::Modifiers::COMMAND),
                    1.6,
                );
                let latest = "Latest editor text";
                let mut events = match input {
                    "text" => vec![egui::Event::Text(latest.into())],
                    "paste" => vec![egui::Event::Paste(latest.into())],
                    _ => vec![
                        egui::Event::Ime(egui::ImeEvent::Preedit {
                            text: "Candidate".into(),
                            active_range_chars: Some(0..9),
                        }),
                        egui::Event::Ime(egui::ImeEvent::Commit(latest.into())),
                    ],
                };
                let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
                if native_copy {
                    events.extend([
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Copy,
                        egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                    ]);
                } else {
                    events.extend(chord(egui::Key::C, modifiers));
                }
                let first = app_frame(&ctx, &mut app, events, 1.9);
                let next = app_frame(&ctx, &mut app, vec![], 2.2);
                let text = copied(&first).or_else(|| copied(&next)).unwrap();
                assert!(
                    text.contains(latest),
                    "{editor}, {input}, native={native_copy}: {text}"
                );
                assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
                assert!(match editor {
                    "title" => app.editing.is_some(),
                    "child" => app.details.editor.is_some(),
                    _ => app.task_context.is_some(),
                });
                fs::remove_dir_all(app.workspace).unwrap();
            }
        }
    }
}

#[test]
fn context_draft_returns_after_find_quick_add_and_shortcuts_without_writing() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Parent");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    let before = fs::read(&app.lists[0].path).unwrap();
    app.open_task_context(list, id, false);
    app.task_context.as_mut().unwrap().text = "Preserve this task's context draft".into();
    app_frame(&ctx, &mut app, vec![], 1.0);
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::F, egui::Modifiers::COMMAND),
        1.3,
    );
    assert!(app.find.is_some());
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::N, egui::Modifiers::COMMAND),
        1.6,
    );
    assert!(app.quick_capture.is_some() && app.find.is_none());
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Escape, egui::Modifiers::NONE),
        1.9,
    );
    app_frame(&ctx, &mut app, vec![], 2.2);
    assert!(app.quick_capture.is_none());
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new("task-context-text"))
    );
    let editor = app.task_context.as_ref().unwrap();
    assert_eq!((editor.list, editor.task), (list, id));
    assert_eq!(editor.text, "Preserve this task's context draft");
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Slash, egui::Modifiers::COMMAND),
        2.5,
    );
    assert!(app.shortcuts.is_some());
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Escape, egui::Modifiers::NONE),
        2.8,
    );
    app_frame(&ctx, &mut app, vec![], 3.1);
    assert_eq!(
        app.task_context.as_ref().unwrap().text,
        "Preserve this task's context draft"
    );
    assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Escape, egui::Modifiers::NONE),
        3.4,
    );
    assert!(app.task_context.is_none());
    assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn wrap_preview_waits_two_seconds_and_never_displays_context() {
    for mode in [TitleOverflow::Wrap, TitleOverflow::Scroll] {
        let (ctx, mut app) = test_app();
        let mut task = Task::new("Task title for metadata preview");
        task.context = "PRIVATE_CONTEXT_NOT_FOR_HOVER".into();
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
        let metadata = late.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("Created")));
        assert_eq!(metadata, mode == TitleOverflow::Wrap);
        assert!(!late.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("PRIVATE_CONTEXT_NOT_FOR_HOVER"))));
        fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn failed_context_modal_keeps_save_and_cancel_reachable_at_minimum_window_size() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("A long title ".repeat(80));
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.open_task_context(list, id, false);
    let editor = app.task_context.as_mut().unwrap();
    editor.text = "Long context paragraph\n".repeat(200);
    editor.failure = Some("A failure path with many wrapped details ".repeat(20));
    app.settings.font_size = 30.0;
    app.settings.font = crate::model::FontChoice::Mono;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 280.0),
            )),
            ..Default::default()
        },
        |ui| app.task_context_ui(ui.ctx()),
    );
    // A second pass establishes the centered modal's actual position.
    output.textures_delta.clear();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 280.0),
            )),
            ..Default::default()
        },
        |ui| app.task_context_ui(ui.ctx()),
    );
    let mut found = 0;
    for shape in &output.shapes {
        if let egui::epaint::Shape::Text(text) = &shape.shape
            && matches!(text.galley.text(), "Retry save" | "Cancel")
        {
            found += 1;
            assert!(
                text.pos.y >= 0.0 && text.pos.y + text.galley.size().y <= 280.0,
                "context action escaped viewport: {:?}",
                text.pos
            );
        }
    }
    assert_eq!(found, 2);
    output.textures_delta.clear();
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn copy_shortcut_reaches_each_parent_and_child_action_without_activation() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.context = "Supporting context".into();
    task.subtasks.push(Subtask::new("Child"));
    let parent = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    let before = fs::read(&app.lists[0].path).unwrap();
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    app.switch_subtask_panel(Some((list, parent)));
    app.keyboard.pending = Some(FocusItem::Task(list, parent).id());
    app_frame(&ctx, &mut app, vec![], 1.0);
    let mut time = 1.3;
    for _ in 0..8 {
        app_frame(
            &ctx,
            &mut app,
            chord(egui::Key::Tab, egui::Modifiers::NONE),
            time,
        );
        time += 0.3;
        assert_eq!(app.task_copy_target(&ctx), Some((list, parent, false)));
        let output = app_frame(
            &ctx,
            &mut app,
            chord(
                egui::Key::C,
                egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                },
            ),
            time,
        );
        assert!(copied(&output).unwrap().contains("Supporting context"));
        assert_eq!(fs::read(&app.lists[0].path).unwrap(), before);
        time += 0.3;
    }
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn native_copy_event_with_shift_copies_whole_task_and_plain_copy_keeps_selection() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Task title");
    task.context = "Supporting context".into();
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    app.editing = Some((list, id));
    app.edit_text = "Selected editor text".into();
    app.request_edit_focus = true;
    app_frame(&ctx, &mut app, vec![], 1.0);
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::A, egui::Modifiers::COMMAND),
        1.3,
    );
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    shift: true,
                    ..egui::Modifiers::COMMAND
                }),
                egui::Event::Copy,
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
            ..Default::default()
        },
        |ui| {
            <MinimalistApp as eframe::App>::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
        },
    );
    assert_eq!(
        copied(&output),
        Some("[ ] Selected editor text\n\nSupporting context")
    );
    assert_eq!(
        output
            .platform_output
            .commands
            .iter()
            .filter(|command| matches!(command, egui::OutputCommand::CopyText(_)))
            .count(),
        1
    );
    output.textures_delta.clear();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
                egui::Event::Copy,
            ],
            ..Default::default()
        },
        |ui| {
            <MinimalistApp as eframe::App>::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
        },
    );
    assert_eq!(copied(&output), Some("Selected editor text"));
    output.textures_delta.clear();
    let mut events = vec![egui::Event::Text("Latest selected title".into())];
    events.extend(chord(egui::Key::A, egui::Modifiers::COMMAND));
    events.extend([
        egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
        egui::Event::Copy,
        egui::Event::ModifiersChanged(egui::Modifiers::NONE),
    ]);
    let first = app_frame(&ctx, &mut app, events, 2.2);
    let next = app_frame(&ctx, &mut app, vec![], 2.5);
    assert_eq!(
        copied(&first).or_else(|| copied(&next)),
        Some("Latest selected title")
    );
    assert!(app.editing.is_some());
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clicking_the_modal_backdrop_keeps_the_explicit_context_draft() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Parent");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.open_task_context(list, id, false);
    app.task_context.as_mut().unwrap().text = "Do not lose notes on an outside click".into();
    app_frame(&ctx, &mut app, vec![], 1.0);
    app_frame(&ctx, &mut app, vec![], 1.3);
    let position = egui::pos2(1.0, 1.0);
    app_frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        1.6,
    );
    assert_eq!(
        app.task_context.as_ref().unwrap().text,
        "Do not lose notes on an outside click"
    );
    assert!(app.lists[0].data.tasks[0].context.is_empty());
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn saving_context_in_a_sorted_view_preserves_manual_json_order() {
    let (_ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["Zulu", "Alpha", "Middle"].map(Task::new).to_vec();
    let original: Vec<_> = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
    app.save_list_index(0);
    app.sort_tasks(list, crate::model::TaskSort::Title);
    app.open_task_context(list, original[2], false);
    app.task_context.as_mut().unwrap().text = "Edited while sorted".into();
    assert!(app.save_task_context());
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(&app.lists[0].path).unwrap()).unwrap();
    assert_eq!(
        saved.tasks.iter().map(|task| task.id).collect::<Vec<_>>(),
        original
    );
    assert_eq!(saved.tasks[2].context, "Edited while sorted");
    assert_eq!(
        app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| task.text.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Middle", "Zulu"]
    );
    app.sort_tasks(list, crate::model::TaskSort::Manual);
    assert_eq!(
        app.lists[0]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        original
    );
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn context_save_after_quick_add_preserves_both_tasks_and_manual_order() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["Zulu parent", "Alpha sibling"].map(Task::new).to_vec();
    let parent = app.lists[0].data.tasks[0].id;
    let sibling = app.lists[0].data.tasks[1].id;
    app.save_list_index(0);
    app.sort_tasks(list, crate::model::TaskSort::Title);
    app.open_task_context(list, parent, false);
    app.task_context.as_mut().unwrap().text = "Draft survives another tool writing".into();
    app_frame(&ctx, &mut app, vec![], 1.0);
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::N, egui::Modifiers::COMMAND),
        1.3,
    );
    app_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text("Captured during context edit".into())],
        1.6,
    );
    app_frame(
        &ctx,
        &mut app,
        chord(egui::Key::Enter, egui::Modifiers::NONE),
        1.9,
    );
    assert!(app.quick_capture.is_none());
    assert_eq!(app.task_context.as_ref().unwrap().task, parent);
    assert!(app.save_task_context());
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(&app.lists[0].path).unwrap()).unwrap();
    assert_eq!(saved.tasks.len(), 3);
    assert_eq!(saved.tasks[0].text, "Captured during context edit");
    assert_eq!((saved.tasks[1].id, saved.tasks[2].id), (parent, sibling));
    assert_eq!(
        saved.tasks[1].context,
        "Draft survives another tool writing"
    );
    assert_eq!(app.task_sort_mode(list), crate::model::TaskSort::Title);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn context_presence_control_is_distinct_and_opens_exact_task_at_narrow_width() {
    let (ctx, mut app) = test_app();
    let mut task = Task::new("A wrapped task title");
    task.context = "SECRET_CONTEXT_WITHOUT_ROW_EXCERPT".into();
    let id = task.id;
    let list = app.lists[0].key;
    app.lists[0].data.tasks.push(task);
    app.settings.title_overflow = TitleOverflow::Wrap;
    app.settings.font_size = 30.0;
    app.view_zoom = 1.0;
    app.request_task_focus = false;
    let mut draw = |events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                <MinimalistApp as eframe::App>::ui(
                    &mut app,
                    ui,
                    &mut eframe::Frame::_new_kittest(),
                );
            },
        );
        output.textures_delta.clear();
        output
    };
    draw(vec![]);
    let output = draw(vec![]);
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text().contains("SECRET_CONTEXT"))));
    let presence = ctx
        .read_response(Id::new(("task-context-presence", id)))
        .unwrap()
        .rect;
    let title = ctx.read_response(Id::new(("task-text", id))).unwrap().rect;
    let copy = ctx
        .read_response(Id::new(("copy-task", list, id, false)))
        .unwrap()
        .rect;
    assert!(presence.left() >= title.right());
    assert!(presence.right() <= copy.left());
    let position = presence.center();
    draw(vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    assert_eq!(app.task_context.as_ref().unwrap().task, id);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn unrelated_popups_never_copy_a_stale_task_menu_target() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Earlier context menu owner");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    let task_menu = Id::new("old-task-menu");
    app.details.menu_task = Some((list, id, false));
    app.details.menu_popup = Some(task_menu);
    egui::Popup::open_id(&ctx, task_menu);
    assert_eq!(app.task_copy_target(&ctx), Some((list, id, false)));
    egui::Popup::close_id(&ctx, task_menu);
    egui::Popup::open_id(&ctx, Id::new("unrelated-sort-menu"));
    assert_eq!(app.task_copy_target(&ctx), None);
    fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn failed_context_save_blocks_workspace_switch_and_native_close_until_retry() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Original workspace task");
    let id = task.id;
    app.lists[0].data.tasks.push(task);
    app.save_list_index(0);
    app.open_task_context(list, id, false);
    app.task_context.as_mut().unwrap().text = "Recover after native close attempt".into();
    let original_workspace = app.workspace.clone();
    let original_path = app.lists[0].path.clone();
    fs::rename(&original_path, original_path.with_extension("backup")).unwrap();
    fs::create_dir(&original_path).unwrap();
    app.switch_workspace(original_workspace.join("next").to_str().unwrap());
    assert_eq!(app.workspace, original_workspace);
    assert!(!original_workspace.join("next").exists());
    let output = ctx.run_logic(&Default::default(), |ctx| app.close_window(ctx));
    assert!(!app.window_hidden && !app.quitting);
    assert!(
        output
            .viewport_commands
            .values()
            .flatten()
            .all(|command| *command != egui::ViewportCommand::Close
                && *command != egui::ViewportCommand::Visible(false))
    );
    assert_eq!(
        app.task_context.as_ref().unwrap().text,
        "Recover after native close attempt"
    );
    fs::remove_dir(&original_path).unwrap();
    assert!(app.finish_pending_changes());
    let saved: crate::model::TodoList =
        serde_json::from_slice(&fs::read(original_path).unwrap()).unwrap();
    assert_eq!(saved.tasks[0].context, "Recover after native close attempt");
    assert!(app.task_context.is_none());
    fs::remove_dir_all(&app.workspace).unwrap();
}
