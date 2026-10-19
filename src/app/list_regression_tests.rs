//! Pointer, focus, and scroll journeys through the real task-list widgets.

use super::*;
use crate::app::{Screen, tests::test_app};
use crate::model::Accent;
use crate::storage;
use egui::{Event, Key, Modifiers};

fn frame(ctx: &egui::Context, app: &mut MinimalistApp, events: Vec<Event>) {
    app.view_zoom = 1.0;
    let input = egui::RawInput {
        time: Some(ctx.input(|input| input.time) + 0.3),
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(520.0, 600.0))),
        events,
        ..Default::default()
    };
    app.update_motion(ctx);
    let mut output = ctx.run_ui(input, |ui| {
        <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
    output.textures_delta.clear();
}

fn idle(ctx: &egui::Context, app: &mut MinimalistApp) {
    for _ in 0..4 {
        frame(ctx, app, Vec::new());
    }
}

fn key(ctx: &egui::Context, app: &mut MinimalistApp, key: Key, modifiers: Modifiers) {
    frame(
        ctx,
        app,
        [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
            .collect(),
    );
}

fn click(ctx: &egui::Context, app: &mut MinimalistApp, pos: Pos2, split: bool) {
    let pointer = |pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    if split {
        frame(ctx, app, vec![Event::PointerMoved(pos), pointer(true)]);
        frame(ctx, app, vec![pointer(false)]);
    } else {
        frame(
            ctx,
            app,
            vec![Event::PointerMoved(pos), pointer(true), pointer(false)],
        );
    }
    idle(ctx, app);
}

#[test]
fn clicking_another_title_commits_then_opens_in_both_directions() {
    for split in [false, true] {
        for (source, target) in [(0, 1), (1, 0)] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            app.lists[0].data.tasks = ["Upper original", "Lower original"].map(Task::new).to_vec();
            app.save_list_index(0);
            let source_id = app.lists[0].data.tasks[source].id;
            let target_id = app.lists[0].data.tasks[target].id;
            idle(&ctx, &mut app);
            let source_pos = ctx
                .read_response(Id::new(("task-text", source_id)))
                .unwrap()
                .rect
                .center();
            click(&ctx, &mut app, source_pos, split);
            assert_eq!(app.editing, Some((list, source_id)));
            key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
            frame(
                &ctx,
                &mut app,
                vec![Event::Text("Saved outgoing edit".into())],
            );
            let target_pos = ctx
                .read_response(Id::new(("task-text", target_id)))
                .unwrap()
                .rect
                .center();
            click(&ctx, &mut app, target_pos, split);
            assert_eq!(app.editing, Some((list, target_id)));
            assert_eq!(app.edit_text, app.lists[0].data.tasks[target].text);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(Id::new(("edit-task", list, target_id)))
            );
            let saved = storage::load_workspace(&app.workspace).unwrap();
            assert_eq!(
                saved.lists[0].data.tasks[source].text,
                "Saved outgoing edit"
            );
            assert_eq!(saved.lists[0].data.tasks[target].id, target_id);
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}

#[test]
fn clicking_completion_also_commits_the_other_rows_editor() {
    for source in [0, 1] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["Upper", "Lower"].map(Task::new).to_vec();
        app.save_list_index(0);
        let task = app.lists[0].data.tasks[source].id;
        app.apply_row_action(RowAction::BeginEdit(list, task, "Original".into()));
        idle(&ctx, &mut app);
        key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
        frame(
            &ctx,
            &mut app,
            vec![Event::Text("Saved before completion".into())],
        );
        let target = app.lists[0].data.tasks[1 - source].id;
        let row = ctx
            .read_response(FocusItem::Task(list, target).id())
            .unwrap()
            .rect;
        click(
            &ctx,
            &mut app,
            Pos2::new(row.left() + 22.0, row.center().y),
            false,
        );
        assert!(app.editing.is_none());
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(
            saved.lists[0].data.tasks[source].text,
            "Saved before completion"
        );
        assert!(saved.lists[0].data.tasks[1 - source].completed);
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn blank_enter_keeps_main_entry_ready_for_immediate_typing() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    for draft in ["", "   "] {
        app.new_task = draft.into();
        key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(Id::new(("new-task", list)))
        );
        frame(
            &ctx,
            &mut app,
            vec![Event::Text("Immediately captured".into())],
        );
        assert_eq!(app.new_task, format!("{draft}Immediately captured"));
        assert!(app.lists[0].data.tasks.is_empty());
    }
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks[0].text, "Immediately captured");
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn lists_retain_independent_scroll_positions() {
    let (ctx, mut app) = test_app();
    app.lists[0].data.tasks = (0..40)
        .map(|i| Task::new(format!("First list {i}")))
        .collect();
    let first = app.lists[0].key;
    let first_task = app.lists[0].data.tasks[0].id;
    let mut second = storage::create_list(&app.workspace, "Second", Accent::Mint).unwrap();
    second.data.tasks = (0..40)
        .map(|i| Task::new(format!("Second list {i}")))
        .collect();
    let second_id = second.key;
    let second_task = second.data.tasks[0].id;
    app.lists.push(second);
    idle(&ctx, &mut app);
    let top = ctx
        .read_response(FocusItem::Task(first, first_task).id())
        .unwrap()
        .rect
        .top();
    let scroll = |ctx: &egui::Context, app: &mut MinimalistApp, amount| {
        frame(
            ctx,
            app,
            vec![
                Event::PointerMoved(Pos2::new(240.0, 400.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, amount),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        for _ in 0..20 {
            frame(ctx, app, Vec::new());
        }
    };
    scroll(&ctx, &mut app, -500.0);
    let first_scrolled = ctx
        .read_response(FocusItem::Task(first, first_task).id())
        .unwrap()
        .rect
        .top();
    assert!(first_scrolled < top - 400.0);
    app.open_list(second_id);
    idle(&ctx, &mut app);
    let second_top = ctx
        .read_response(FocusItem::Task(second_id, second_task).id())
        .unwrap()
        .rect
        .top();
    assert!((second_top - top).abs() < 1.0);
    scroll(&ctx, &mut app, -200.0);
    let second_scrolled = ctx
        .read_response(FocusItem::Task(second_id, second_task).id())
        .unwrap()
        .rect
        .top();
    app.open_list(first);
    idle(&ctx, &mut app);
    let restored = ctx
        .read_response(FocusItem::Task(first, first_task).id())
        .unwrap()
        .rect
        .top();
    assert!(
        (restored - first_scrolled).abs() < 1.0,
        "restored {restored}, wanted {first_scrolled}, top {top}"
    );
    app.open_list(second_id);
    idle(&ctx, &mut app);
    let restored = ctx
        .read_response(FocusItem::Task(second_id, second_task).id())
        .unwrap()
        .rect
        .top();
    assert!((restored - second_scrolled).abs() < 1.0);
    assert!(matches!(app.screen, Screen::List(id) if id == second_id));
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn rendering_cancels_a_drag_whose_source_was_removed_or_replaced() {
    for source in [0, 4] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks.push(Task::new("Survivor"));
        app.drag = Some(DragState {
            list_id: list,
            task_id: Uuid::new_v4(),
            source,
            target: source,
            delta: Vec2::new(0.0, 150.0),
            source_center: 120.0,
            height: 42.0,
        });
        idle(&ctx, &mut app);
        assert!(app.drag.is_none());
        assert_eq!(app.lists[0].data.tasks[0].text, "Survivor");
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn dragging_at_the_bottom_scrolls_and_saves_the_original_task_identity() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = (0..40)
        .map(|i| Task::new(format!("Reorder task {i}")))
        .collect();
    app.save_list_index(0);
    let task = app.lists[0].data.tasks[0].id;
    idle(&ctx, &mut app);
    let row = ctx
        .read_response(FocusItem::Task(list, task).id())
        .unwrap()
        .rect;
    let start = Pos2::new(row.left() + 40.0, row.center().y);
    let end = Pos2::new(start.x, 580.0);
    let pointer = |pos, pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut app,
        vec![Event::PointerMoved(start), pointer(start, true)],
    );
    frame(&ctx, &mut app, vec![Event::PointerMoved(end)]);
    for _ in 0..24 {
        frame(&ctx, &mut app, Vec::new());
    }
    assert_eq!(app.drag.as_ref().map(|drag| drag.task_id), Some(task));
    let scrolled = ctx
        .read_response(FocusItem::Task(list, task).id())
        .unwrap()
        .rect
        .top();
    assert!(scrolled < row.top() - 100.0, "{scrolled} vs {}", row.top());
    frame(&ctx, &mut app, vec![pointer(end, false)]);
    idle(&ctx, &mut app);
    assert!(app.drag.is_none());
    let saved = storage::load_workspace(&app.workspace).unwrap();
    let position = saved.lists[0]
        .data
        .tasks
        .iter()
        .position(|item| item.id == task)
        .unwrap();
    assert!(
        position > 8,
        "edge scrolling must reach rows outside the initial viewport"
    );
    assert_eq!(saved.lists[0].data.tasks.len(), 40);
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn navigation_and_archiving_during_a_real_drag_leave_saved_tasks_usable() {
    for (archive, partial_batch) in [(false, false), (true, false), (false, true), (true, true)] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["Survivor", "Dragged"].map(Task::new).to_vec();
        app.lists[0].data.tasks[1].toggle();
        app.save_list_index(0);
        let task = app.lists[0].data.tasks[1].id;
        let second = storage::create_list(&app.workspace, "Other", Accent::Sky).unwrap();
        let second_id = second.key;
        app.lists.push(second);
        idle(&ctx, &mut app);
        let row = ctx
            .read_response(FocusItem::Task(list, task).id())
            .unwrap()
            .rect;
        let start = Pos2::new(row.left() + 40.0, row.center().y);
        let end = start + Vec2::new(0.0, 70.0);
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        if partial_batch {
            frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
            frame(
                &ctx,
                &mut app,
                vec![pointer(start, true), Event::PointerMoved(end)],
            );
        } else {
            frame(
                &ctx,
                &mut app,
                vec![Event::PointerMoved(start), pointer(start, true)],
            );
            frame(&ctx, &mut app, vec![Event::PointerMoved(end)]);
        }
        assert_eq!(app.drag.as_ref().map(|drag| drag.task_id), Some(task));
        if archive {
            key(
                &ctx,
                &mut app,
                Key::Backspace,
                Modifiers::COMMAND | Modifiers::SHIFT,
            );
            app.clear_animation.as_mut().unwrap().1 =
                Instant::now() - std::time::Duration::from_secs(1);
        } else {
            key(&ctx, &mut app, Key::Tab, Modifiers::CTRL);
            assert!(matches!(app.screen, Screen::List(id) if id == second_id));
        }
        idle(&ctx, &mut app);
        assert!(app.drag.is_none());
        frame(&ctx, &mut app, vec![pointer(end, false)]);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        let original = saved.lists.iter().find(|item| item.key == list).unwrap();
        assert_eq!(original.data.tasks.len(), if archive { 1 } else { 2 });
        if archive {
            assert_eq!(original.data.archive[0].task.id, task);
        }
        app.open_list(list);
        idle(&ctx, &mut app);
        frame(
            &ctx,
            &mut app,
            vec![Event::Text("After canceled drag".into())],
        );
        key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        let original = saved.lists.iter().find(|item| item.key == list).unwrap();
        assert_eq!(original.data.tasks[0].text, "After canceled drag");
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn task_swipes_from_title_and_row_gutter_preserve_fast_native_gestures() {
    for batching in [0, 1, 2] {
        for hit in ["title", "blank", "gutter"] {
            let (ctx, mut app) = test_app();
            let list = app.lists[0].key;
            let task = Task::new("Swipe this task");
            let id = task.id;
            app.lists[0].data.tasks.push(task);
            app.save_list_index(0);
            app.settings.font_size = 30.0;
            idle(&ctx, &mut app);
            let row = ctx
                .read_response(FocusItem::Task(list, id).id())
                .unwrap()
                .rect;
            let title = ctx.read_response(Id::new(("task-text", id))).unwrap().rect;
            let start = match hit {
                "title" => title.left_center() + Vec2::new(12.0, 0.0),
                "blank" => title.right_center() - Vec2::new(8.0, 0.0),
                _ => Pos2::new(row.left() + 40.0, row.center().y),
            };
            let end = start + Vec2::new(110.0, 0.0);
            let pointer = |pos, pressed| Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            if batching == 1 {
                frame(
                    &ctx,
                    &mut app,
                    vec![
                        Event::PointerMoved(start),
                        pointer(start, true),
                        Event::PointerMoved(end),
                        pointer(end, false),
                    ],
                );
            } else if batching == 2 {
                frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
                frame(
                    &ctx,
                    &mut app,
                    vec![pointer(start, true), Event::PointerMoved(end)],
                );
                frame(&ctx, &mut app, vec![pointer(end, false)]);
            } else {
                frame(
                    &ctx,
                    &mut app,
                    vec![Event::PointerMoved(start), pointer(start, true)],
                );
                frame(&ctx, &mut app, vec![Event::PointerMoved(end)]);
                frame(&ctx, &mut app, vec![pointer(end, false)]);
            }
            idle(&ctx, &mut app);
            assert!(
                storage::load_workspace(&app.workspace).unwrap().lists[0]
                    .data
                    .tasks[0]
                    .completed,
                "hit={hit}, batching={batching}, editing={:?}",
                app.editing
            );
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }
}

#[test]
fn coalesced_swipes_reveal_actions_without_capturing_row_controls() {
    for (hit, partial_batch) in [
        "title",
        "completion",
        "copy",
        "context",
        "checklist",
        "delete",
    ]
    .into_iter()
    .flat_map(|hit| [(hit, false), (hit, true)])
    {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let mut task = Task::new("Swipe target");
        task.context = "Preserved notes".into();
        task.subtasks.push(crate::model::Subtask::new("Child"));
        let id = task.id;
        app.lists[0].data.tasks.push(task);
        app.save_list_index(0);
        let before = std::fs::read(&app.lists[0].path).unwrap();
        idle(&ctx, &mut app);
        let row = ctx
            .read_response(FocusItem::Task(list, id).id())
            .unwrap()
            .rect;
        let start = match hit {
            "title" => ctx
                .read_response(Id::new(("task-text", id)))
                .unwrap()
                .rect
                .center(),
            "completion" => Pos2::new(row.left() + 22.0, row.center().y),
            "delete" => Pos2::new(row.right() - 17.0, row.center().y),
            _ => {
                let control = match hit {
                    "copy" => Id::new(("copy-task", list, id, false)),
                    "context" => Id::new(("task-context-presence", id)),
                    _ => Id::new(("task-checklist", list, id)),
                };
                ctx.read_response(control).unwrap().rect.center()
            }
        };
        let end = start - Vec2::new(100.0, 0.0);
        let mut events = vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerMoved(end),
            Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        if partial_batch {
            let release = events.pop().unwrap();
            frame(&ctx, &mut app, events);
            frame(&ctx, &mut app, vec![release]);
        } else {
            frame(&ctx, &mut app, events);
        }
        idle(&ctx, &mut app);
        assert_eq!(
            app.revealed,
            (hit == "title").then_some((list, id)),
            "hit={hit}"
        );
        assert_eq!(
            std::fs::read(&app.lists[0].path).unwrap(),
            before,
            "hit={hit}"
        );
        assert!(app.editing.is_none() && app.task_context.is_none() && app.subtasks_for.is_none());
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn final_pointer_movement_in_the_release_frame_is_included_in_the_gesture() {
    for (horizontal, batching) in [
        (false, 0),
        (true, 0),
        (false, 1),
        (true, 1),
        (false, 2),
        (true, 2),
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["Dragged", "Middle", "Last"].map(Task::new).to_vec();
        app.save_list_index(0);
        let task = app.lists[0].data.tasks[0].id;
        let last = app.lists[0].data.tasks[2].id;
        idle(&ctx, &mut app);
        let first_rect = ctx
            .read_response(FocusItem::Task(list, task).id())
            .unwrap()
            .rect;
        let last_rect = ctx
            .read_response(FocusItem::Task(list, last).id())
            .unwrap()
            .rect;
        let start = Pos2::new(first_rect.left() + 40.0, first_rect.center().y);
        let movement = if horizontal {
            Vec2::new(12.0, 0.0)
        } else {
            Vec2::new(0.0, 12.0)
        };
        let end = if horizontal {
            start + Vec2::new(110.0, 0.0)
        } else {
            Pos2::new(start.x, last_rect.center().y)
        };
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        if batching == 1 {
            frame(
                &ctx,
                &mut app,
                vec![
                    Event::PointerMoved(start),
                    pointer(start, true),
                    Event::PointerMoved(end),
                    pointer(end, false),
                ],
            );
        } else if batching == 2 {
            frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
            frame(
                &ctx,
                &mut app,
                vec![pointer(start, true), Event::PointerMoved(start + movement)],
            );
            frame(
                &ctx,
                &mut app,
                vec![Event::PointerMoved(end), pointer(end, false)],
            );
        } else {
            frame(
                &ctx,
                &mut app,
                vec![Event::PointerMoved(start), pointer(start, true)],
            );
            frame(&ctx, &mut app, vec![Event::PointerMoved(start + movement)]);
            assert!(app.drag.is_some());
            frame(
                &ctx,
                &mut app,
                vec![Event::PointerMoved(end), pointer(end, false)],
            );
        }
        idle(&ctx, &mut app);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        if horizontal {
            assert!(saved.lists[0].data.tasks[0].completed);
        } else {
            assert_eq!(saved.lists[0].data.tasks[2].id, task);
        }
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn workspace_switch_transfers_instance_ownership_and_reopen_reveals_the_window() {
    use crate::instance::{LaunchOutcome, claim};
    let (ctx, mut app) = test_app();
    let original = app.workspace.clone();
    let state_path = storage::window_state_path();
    let state = state_path.parent().unwrap();
    let LaunchOutcome::Primary(guard) = claim(&original, state, false).unwrap() else {
        panic!("test workspace must have no existing owner");
    };
    let wake = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notify = wake.clone();
    let repaint_ctx = egui::Context::default();
    repaint_ctx.set_request_repaint_callback(move |_| {
        notify.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    app.set_instance_guard(guard, &repaint_ctx);
    app.switch_workspace(&original.join("lists/..").to_string_lossy());
    assert!(app.instance.as_ref().unwrap().owns_workspace(&original));
    assert!(!app.instance.as_ref().unwrap().take_show_request());

    let other =
        storage::normalize_workspace_path(&original.join("other").to_string_lossy()).unwrap();
    let LaunchOutcome::Primary(other_owner) = claim(&other, state, false).unwrap() else {
        panic!("second test workspace must have no existing owner");
    };
    app.switch_workspace(&other.to_string_lossy());
    assert_eq!(app.workspace, original);
    assert!(other_owner.take_show_request());
    drop(other_owner);

    app.switch_workspace(&other.to_string_lossy());
    assert_eq!(app.workspace, other);
    assert!(app.instance.as_ref().unwrap().owns_workspace(&other));
    let LaunchOutcome::Primary(released) = claim(&original, state, false).unwrap() else {
        panic!("switch must release the previous workspace claim");
    };
    app.window_hidden = true;
    assert!(matches!(
        claim(&other, state, false).unwrap(),
        LaunchOutcome::Existing
    ));
    assert!(
        wake.load(std::sync::atomic::Ordering::SeqCst),
        "the transferred claim must wake the native event loop"
    );
    <MinimalistApp as eframe::App>::logic(&mut app, &ctx, &mut eframe::Frame::_new_kittest());
    assert!(
        !app.window_hidden,
        "reopening the selected workspace must reveal its owner"
    );
    drop(released);
    drop(app.instance.take());
    std::fs::remove_dir_all(original).unwrap();
}
