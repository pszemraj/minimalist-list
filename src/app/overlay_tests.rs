//! Search and overlay regression tests.
use super::*;
use crate::model::{Accent, ArchivedTask, Subtask, TodoList};
use std::path::PathBuf;

#[test]
fn shortcut_reference_keyboard_scrolls_within_a_narrow_viewport() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.open_shortcuts();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 420.0));
    let mut draw = |key: Option<Key>| {
        let events = key
            .map(|key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .into_iter()
            .collect();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                time: Some(ctx.input(|input| input.time) + 0.3),
                events,
                ..Default::default()
            },
            |_| {
                app.handle_app_shortcuts(&ctx);
                app.overlay_ui(&ctx, app.palette());
                app.finish_keyboard_frame(&ctx);
            },
        );
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|clipped| {
                if let egui::epaint::Shape::Text(text) = clipped.shape
                    && clipped
                        .clip_rect
                        .intersects(egui::Rect::from_min_size(text.pos, text.galley.size()))
                {
                    Some((text.galley.text().to_owned(), text.pos))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    for _ in 0..3 {
        draw(None);
    }
    let top = draw(None);
    assert!(top.iter().any(|(text, _)| text == "General"));
    assert!(!top.iter().any(|(text, _)| text == "Copy task text"));
    assert!(top.iter().all(|(_, pos)| viewport.contains(*pos)));
    draw(Some(Key::PageDown));
    for _ in 0..3 {
        draw(None);
    }
    assert!(!draw(None).iter().any(|(text, _)| text == "General"));
    draw(Some(Key::End));
    for _ in 0..3 {
        draw(None);
    }
    assert!(draw(None).iter().any(|(text, _)| text == "Copy task text"));
    draw(Some(Key::Home));
    for _ in 0..3 {
        draw(None);
    }
    assert!(draw(None).iter().any(|(text, _)| text == "General"));
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

fn stored_list(title: &str) -> StoredList {
    let data = TodoList::new(title, Accent::Mint);
    StoredList {
        key: data.id,
        path: PathBuf::from(format!("{title}.json")),
        data,
        stamp: Default::default(),
        source_bytes: Default::default(),
    }
}

#[test]
fn find_grows_after_typing_and_keeps_results_and_help_visible() {
    for size in [egui::vec2(520.0, 760.0), egui::vec2(320.0, 280.0)] {
        let (ctx, mut app) = super::super::tests::test_app();
        app.settings.font_size = 21.0;
        app.lists[0].data.tasks = ["Project first", "Project second", "Project third"]
            .into_iter()
            .map(Task::new)
            .collect();
        app.open_find();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut draw = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    time: Some(ctx.input(|input| input.time) + 0.3),
                    events,
                    ..Default::default()
                },
                |_| app.overlay_ui(&ctx, app.palette()),
            );
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|clipped| {
                    let egui::epaint::Shape::Text(text) = clipped.shape else {
                        return None;
                    };
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    (clipped.clip_rect.contains_rect(rect) && viewport.contains_rect(rect))
                        .then(|| text.galley.text().to_owned())
                })
                .collect::<Vec<_>>()
        };
        for _ in 0..3 {
            draw(Vec::new());
        }
        draw(vec![egui::Event::Text("project".into())]);
        for _ in 0..3 {
            draw(Vec::new());
        }
        let visible = draw(Vec::new());
        assert!(visible.iter().any(|text| text == "Find tasks"));
        assert!(
            visible
                .iter()
                .any(|text| text == "Up/Down selects · Enter opens · Esc closes"),
            "Find help must remain visible after results appear at {size:?}: {visible:?}"
        );
        if size.y > 500.0 {
            for title in ["Project first", "Project second", "Project third"] {
                assert!(visible.iter().any(|text| text.starts_with(title)));
            }
        }
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}

#[test]
fn quick_add_long_list_name_keeps_entry_and_add_inside_small_viewport() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.lists[0].data.title = "A very long list name describing preparations for an international workshop and all its participating research groups".to_owned();
    app.open_quick_capture(Some(app.lists[0].key));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 280.0));
    let palette = app.palette();
    for frame in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |_| app.overlay_ui(&ctx, palette),
        );
        output.textures_delta.clear();
        if frame < 2 {
            continue;
        }
        let entry = ctx.read_response(Id::new("quick-capture-text")).unwrap();
        assert!(viewport.contains_rect(entry.rect));
        let add = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == "Add" => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .expect("Add must remain visible");
        assert!(viewport.contains_rect(add));
    }
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn searches_active_completed_archived_and_subtask_text() {
    let mut list = stored_list("Work");
    list.data.tasks.push(Task::new("Active needle"));

    let mut completed = Task::new("Completed needle");
    completed.toggle();
    list.data.tasks.push(completed);

    let mut parent = Task::new("Parent");
    parent.subtasks.push(Subtask::new("Subtask needle"));
    list.data.tasks.push(parent);

    let archived = Task::new("Archived needle");
    list.data.archive.push(ArchivedTask::new(archived));

    let hits = search_hits(&[list], "NEEDLE");
    assert_eq!(hits.len(), 4);
    assert_eq!(hits[0].state, HitState::Active);
    assert_eq!(hits[1].state, HitState::Completed);
    assert!(hits[2].subtask_id.is_some());
    assert_eq!(hits[3].state, HitState::Archived);
}

#[test]
fn empty_search_has_no_results() {
    assert!(search_hits(&[stored_list("Tasks")], "  ").is_empty());
}

fn noted_task(title: &str, context: &str) -> Task {
    let mut task = Task::new(title);
    task.context = context.into();
    task
}

#[test]
fn multi_word_query_requires_every_term_in_one_item() {
    let mut list = stored_list("Work");
    list.data.tasks.push(Task::new("Book flights"));
    list.data.tasks.push(noted_task("Other", "hotel in Lisbon"));
    assert!(search_hits(&[list.clone()], "flights hotel").is_empty());

    list.data
        .tasks
        .push(noted_task("Plan trip", "Lisbon hotel"));
    let hits = search_hits(&[list], "TRIP  hotel");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].task_text, "Plan trip");
    assert_eq!(hits[0].context_snippet.as_deref(), Some("Lisbon hotel"));
}

#[test]
fn context_only_match_reports_its_line() {
    let mut list = stored_list("Work");
    let long = format!("passport {}", "x".repeat(200));
    list.data.tasks.push(noted_task(
        "Plan trip",
        &format!("first line\n   Hotel: Lisbon   \nlater hotel\n{long}"),
    ));
    let hits = search_hits(&[list.clone()], "hotel");
    assert_eq!(hits[0].context_snippet.as_deref(), Some("Hotel: Lisbon"));

    let hits = search_hits(&[list], "trip passport");
    let snippet = hits[0].context_snippet.as_deref().unwrap();
    assert_eq!(snippet.chars().count(), 121);
    assert!(snippet.starts_with("passport xxx") && snippet.ends_with('…'));
}

#[test]
fn title_match_has_no_notes_snippet() {
    let mut list = stored_list("Work");
    list.data
        .tasks
        .push(noted_task("Hotel booking", "hotel in Lisbon"));
    let hits = search_hits(&[list], "hotel");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].context_snippet, None);
}

#[test]
fn archived_context_is_searched_and_children_ignore_parent_context() {
    let mut list = stored_list("Work");
    let mut archived = noted_task("Old trip", "visa deadline");
    archived.subtasks.push(Subtask::new("Pack bag"));
    list.data.archive.push(ArchivedTask::new(archived));
    let hits = search_hits(&[list.clone()], "visa");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].state, HitState::Archived);
    assert_eq!(hits[0].context_snippet.as_deref(), Some("visa deadline"));

    let hits = search_hits(&[list], "pack visa");
    assert!(hits.is_empty());
}

fn overlay_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> Vec<String> {
    let output = crate::app::tests::run_frame(
        ctx,
        crate::app::tests::frame_input(
            egui::vec2(430.0, 500.0),
            ctx.input(|input| input.time) + 0.3,
            events,
        ),
        |_| {
            app.handle_app_shortcuts(ctx);
            app.overlay_ui(ctx, app.palette());
            app.finish_keyboard_frame(ctx);
        },
    );
    output
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn enter() -> Vec<egui::Event> {
    crate::app::tests::chord(Key::Enter, egui::Modifiers::NONE)
}

#[test]
fn failed_quick_add_keeps_text_and_retry_saves_exactly_one_task() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    app.open_quick_capture(Some(list));
    app.quick_capture.as_mut().unwrap().text = "Capture survives unavailable folder".into();
    overlay_frame(&ctx, &mut app, vec![]);
    std::fs::rename(app.workspace.join("lists"), app.workspace.join("offline")).unwrap();
    std::fs::write(app.workspace.join("lists"), "unavailable").unwrap();
    overlay_frame(&ctx, &mut app, enter());
    let state = app.quick_capture.as_ref().unwrap();
    assert_eq!(state.text, "Capture survives unavailable folder");
    assert!(state.failure.is_some());
    assert!(app.lists[0].data.tasks.is_empty());
    for _ in 0..3 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    let text = overlay_frame(&ctx, &mut app, vec![]);
    assert!(
        text.iter().any(|text| text.starts_with("Not saved.")),
        "visible text: {text:?}"
    );
    assert!(
        text.iter().any(|text| text == "Retry save"),
        "visible text: {text:?}"
    );
    std::fs::remove_file(app.workspace.join("lists")).unwrap();
    std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists")).unwrap();
    overlay_frame(&ctx, &mut app, enter());
    assert!(app.quick_capture.is_none());
    let saved = crate::storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks.len(), 1);
    assert_eq!(
        saved.lists[0].data.tasks[0].text,
        "Capture survives unavailable folder"
    );
    assert!(app.status.is_none());
    assert_eq!(
        app.persistence
            .notice
            .as_ref()
            .map(|(message, _)| message.as_str()),
        Some("Added to Tasks")
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn failed_quick_add_blocks_shutdown_until_retry_without_losing_destination() {
    for suspended in [false, true] {
        let (ctx, mut app) = super::super::tests::test_app();
        let destination =
            crate::storage::create_list(&app.workspace, "Destination", Accent::Sky).unwrap();
        let list = destination.key;
        app.lists.push(destination);
        app.open_quick_capture(Some(list));
        app.quick_capture.as_mut().unwrap().text = "Keep the failed capture".into();
        overlay_frame(&ctx, &mut app, vec![]);
        std::fs::rename(app.workspace.join("lists"), app.workspace.join("offline")).unwrap();
        std::fs::write(app.workspace.join("lists"), "unavailable").unwrap();
        overlay_frame(&ctx, &mut app, enter());
        assert!(app.quick_capture.as_ref().unwrap().failure.is_some());
        if suspended {
            app.open_find();
        }
        assert!(!app.finish_pending_changes(), "suspended={suspended}");
        let output = ctx.run_logic(&Default::default(), |ctx| app.close_window(ctx));
        assert!(
            output
                .viewport_commands
                .values()
                .flatten()
                .all(|command| *command != egui::ViewportCommand::Close)
        );
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events = vec![egui::ViewportEvent::Close];
        let output = ctx.run_logic(&input, |ctx| {
            app.handle_window_actions(ctx, false);
            app.finish_window_actions_after_input(ctx);
        });
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(!app.quitting && !app.window_hidden);
        let state = app.quick_capture.as_ref().unwrap();
        assert_eq!(state.text, "Keep the failed capture");
        assert_eq!(state.selected_list_id, Some(list));
        assert!(state.failure.is_some());
        std::fs::remove_file(app.workspace.join("lists")).unwrap();
        std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists")).unwrap();
        overlay_frame(&ctx, &mut app, vec![]);
        overlay_frame(&ctx, &mut app, enter());
        assert!(app.quick_capture.is_none());
        assert!(app.finish_pending_changes());
        let saved = crate::storage::load_workspace(&app.workspace).unwrap();
        let destination = saved.lists.iter().find(|item| item.key == list).unwrap();
        assert_eq!(destination.data.tasks.len(), 1);
        assert_eq!(destination.data.tasks[0].text, "Keep the failed capture");
        let output = ctx.run_logic(&Default::default(), |ctx| app.close_window(ctx));
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Close)
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn unsubmitted_or_explicitly_cancelled_quick_add_does_not_block_shutdown() {
    for failed_then_cancelled in [false, true] {
        let (ctx, mut app) = super::super::tests::test_app();
        app.open_quick_capture(None);
        app.quick_capture.as_mut().unwrap().text = "Ordinary unsubmitted draft".into();
        overlay_frame(&ctx, &mut app, vec![]);
        if failed_then_cancelled {
            app.quick_capture.as_mut().unwrap().failure = Some("Previous save failed".into());
            overlay_frame(
                &ctx,
                &mut app,
                vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(app.quick_capture.is_none() && app.capture_draft.is_none());
        }
        assert!(app.finish_pending_changes());
        let output = ctx.run_logic(&Default::default(), |ctx| app.close_window(ctx));
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Close)
        );
        assert!(app.lists[0].data.tasks.is_empty());
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn quick_add_backdrop_preserves_drafts_until_explicit_cancellation() {
    for (text, batch) in [("", false), ("Call the bank", false), ("", true)] {
        let (ctx, mut app) = super::super::tests::test_app();
        app.open_quick_capture(None);
        let state = app.quick_capture.as_mut().unwrap();
        state.text = text.into();
        if batch {
            state.pending_batch = vec!["One".into(), "Two".into()];
            state.failure = Some("Previous save failed".into());
        }
        for _ in 0..3 {
            overlay_frame(&ctx, &mut app, vec![]);
        }
        let pos = egui::pos2(1.0, 1.0);
        overlay_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        if batch || !text.is_empty() {
            let state = app.quick_capture.as_ref().unwrap();
            assert_eq!(state.text, text);
            assert_eq!(!state.pending_batch.is_empty(), batch);
            overlay_frame(
                &ctx,
                &mut app,
                crate::app::tests::chord(Key::Escape, egui::Modifiers::NONE),
            );
        }
        assert!(app.quick_capture.is_none());
        assert!(app.lists[0].data.tasks.is_empty());
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn cancelling_quick_add_preserves_a_notes_save_conflict() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    let task = Task::new("Original");
    let task_id = task.id;
    app.lists[0].data.tasks.push(task);
    assert!(app.save_list_index(0));
    let mut remote = crate::storage::load_workspace(&app.workspace)
        .unwrap()
        .lists
        .remove(0);
    remote.data.tasks[0].text = "Remote title".into();
    crate::storage::save_list(&remote).unwrap();
    app.open_task_context(list, task_id, false);
    app.task_context.as_mut().unwrap().text = "Unsaved notes".into();
    assert!(!app.save_task_context());
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert_eq!(app.lists[0].data.tasks[0].text, "Original");
    app.open_quick_capture(Some(list));
    for _ in 0..3 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    overlay_frame(
        &ctx,
        &mut app,
        crate::app::tests::chord(Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.quick_capture.is_none());
    assert!(app.has_save_conflict(list));
    assert_eq!(app.task_context.as_ref().unwrap().text, "Unsaved notes");
    app.task_context = None;
    app.forget_save_conflict(list);
    app.scan_external_changes();
    assert_eq!(app.lists[0].data.tasks[0].text, "Remote title");
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn quick_add_success_preserves_unrelated_diagnostics_including_after_retry() {
    for fail_first in [false, true] {
        for diagnostic in [
            "Global shortcut registration failed",
            "Duplicate list files were found",
            "Could not read an external list file",
        ] {
            let (ctx, mut app) = super::super::tests::test_app();
            let list = app.lists[0].key;
            app.open_quick_capture(Some(list));
            app.quick_capture.as_mut().unwrap().text = "Preserve this capture".into();
            overlay_frame(&ctx, &mut app, vec![]);
            if fail_first {
                std::fs::rename(app.workspace.join("lists"), app.workspace.join("offline"))
                    .unwrap();
                std::fs::write(app.workspace.join("lists"), "unavailable").unwrap();
                overlay_frame(&ctx, &mut app, enter());
                assert!(app.quick_capture.as_ref().unwrap().failure.is_some());
                assert!(app.lists[0].data.tasks.is_empty());
                std::fs::remove_file(app.workspace.join("lists")).unwrap();
                std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists"))
                    .unwrap();
            }
            app.status = Some(diagnostic.into());
            overlay_frame(&ctx, &mut app, enter());
            assert!(app.quick_capture.is_none());
            assert_eq!(
                app.status.as_deref(),
                Some(diagnostic),
                "retry={fail_first}"
            );
            let saved = crate::storage::load_workspace(&app.workspace).unwrap();
            assert_eq!(saved.lists[0].data.tasks.len(), 1);
            assert_eq!(saved.lists[0].data.tasks[0].text, "Preserve this capture");
            assert_eq!(app.persistence.notice.as_ref().unwrap().0, "Added to Tasks");
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }
}

#[test]
fn empty_overlay_enter_keeps_focus_for_immediate_typing() {
    for find in [false, true] {
        let (ctx, mut app) = super::super::tests::test_app();
        if find {
            app.open_find();
        } else {
            app.open_quick_capture(None);
        }
        overlay_frame(&ctx, &mut app, vec![]);
        overlay_frame(&ctx, &mut app, enter());
        overlay_frame(&ctx, &mut app, vec![egui::Event::Text("next input".into())]);
        if find {
            assert_eq!(app.find.as_ref().unwrap().query, "next input");
        } else {
            assert_eq!(app.quick_capture.as_ref().unwrap().text, "next input");
        }
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn switching_tools_and_opening_results_preserves_session_drafts_and_selection() {
    let (ctx, mut app) = super::super::tests::test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks.push(Task::new("needle one"));
    app.lists[0].data.tasks.push(Task::new("needle two"));
    app.open_quick_capture(Some(list));
    app.quick_capture.as_mut().unwrap().text = "unfinished capture".into();
    app.open_find();
    app.find.as_mut().unwrap().query = "needle".into();
    app.find.as_mut().unwrap().selected = 1;
    app.open_quick_capture(Some(list));
    assert_eq!(
        app.quick_capture.as_ref().unwrap().text,
        "unfinished capture"
    );
    app.open_find();
    assert_eq!(app.find.as_ref().unwrap().selected, 1);
    overlay_frame(&ctx, &mut app, vec![]);
    overlay_frame(&ctx, &mut app, enter());
    assert!(app.find.is_none());
    app.open_find();
    assert_eq!(app.find.as_ref().unwrap().query, "needle");
    assert_eq!(app.find.as_ref().unwrap().selected, 1);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn child_search_uses_its_own_completion_and_keeps_history_separate() {
    let mut list = stored_list("Work");
    let mut parent = Task::new("parent");
    let mut child = Subtask::new("needle completed");
    child.completed = true;
    parent.subtasks.push(child);
    list.data.tasks.push(parent);
    let mut done_parent = Task::new("done parent");
    done_parent.toggle();
    done_parent.subtasks.push(Subtask::new("needle active"));
    list.data.tasks.push(done_parent.clone());
    list.data.archive.push(ArchivedTask::new(done_parent));
    let hits = search_hits(&[list], "needle");
    assert_eq!(hits[0].state, HitState::Completed);
    assert_eq!(hits[1].state, HitState::Active);
    assert_eq!(hits[2].state_label(), "history · active");
}

#[test]
fn pointer_scroll_in_find_stays_put_until_keyboard_selection_changes() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.lists[0].data.tasks = (0..70)
        .map(|index| Task::new(format!("needle {index}")))
        .collect();
    app.open_find();
    app.find.as_mut().unwrap().query = "needle".into();
    for _ in 0..5 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    let first = app.find.as_ref().unwrap().result_ids[0];
    let top = ctx.read_response(first).unwrap().rect.top();
    overlay_frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(egui::pos2(210.0, 280.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(0.0, -900.0),
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    for _ in 0..20 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    let scrolled = ctx.read_response(first).unwrap().rect.top();
    assert!(
        scrolled < top - 300.0,
        "pointer scroll must stay away from selected first hit: {top} -> {scrolled}"
    );
    overlay_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for _ in 0..10 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    let selected = app.find.as_ref().unwrap().result_ids[1];
    assert!(ctx.read_response(selected).unwrap().rect.top() >= 0.0);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn an_empty_suspended_capture_uses_the_current_list_when_reopened() {
    let (_, mut app) = super::super::tests::test_app();
    let first = app.lists[0].key;
    let second =
        crate::storage::create_list(&app.workspace, "Second", crate::model::Accent::Mint).unwrap();
    let second_id = second.key;
    app.lists.push(second);
    app.open_quick_capture(Some(first));
    app.open_find();
    app.find = None;
    app.open_list(second_id);
    app.open_quick_capture(Some(second_id));
    assert_eq!(
        app.quick_capture.as_ref().unwrap().selected_list_id,
        Some(second_id)
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn deleting_a_capture_destination_requires_an_explicit_replacement_for_its_draft() {
    let (ctx, mut app) = super::super::tests::test_app();
    let first = app.lists[0].key;
    let second = crate::storage::create_list(&app.workspace, "Second", Accent::Mint).unwrap();
    let second_id = second.key;
    app.lists.push(second);
    app.open_quick_capture(Some(first));
    app.quick_capture.as_mut().unwrap().text = "Keep this draft in its intended destination".into();
    overlay_frame(&ctx, &mut app, vec![]);
    crate::storage::delete_list(&app.workspace, &app.lists[0]).unwrap();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert!(app.list_index(first).is_none());
    let text = overlay_frame(&ctx, &mut app, vec![]);
    assert!(
        text.iter()
            .any(|text| text == "Choose a destination list to save this draft.")
    );
    assert!(
        app.quick_capture
            .as_ref()
            .unwrap()
            .selected_list_id
            .is_none()
    );
    overlay_frame(&ctx, &mut app, enter());
    assert_eq!(
        app.quick_capture.as_ref().unwrap().text,
        "Keep this draft in its intended destination"
    );
    assert!(app.lists[0].data.tasks.is_empty());
    app.quick_capture.as_mut().unwrap().selected_list_id = Some(second_id);
    app.open_quick_add_here(&ctx);
    overlay_frame(&ctx, &mut app, vec![]);
    overlay_frame(&ctx, &mut app, enter());
    assert!(
        app.quick_capture.is_none(),
        "focus={:?}, selected={:?}, failure={:?}",
        ctx.memory(|memory| memory.focused()),
        app.quick_capture
            .as_ref()
            .map(|capture| capture.selected_list_id),
        app.quick_capture.as_ref().map(|capture| &capture.failure)
    );
    let saved = crate::storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks.len(), 1);
    assert_eq!(
        saved.lists[0].data.tasks[0].text,
        "Keep this draft in its intended destination"
    );
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn clicking_an_archived_child_result_retains_its_query_and_selection_on_return() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.lists[0]
        .data
        .tasks
        .push(Task::new("Needle first result"));
    let mut archived = Task::new("Archived parent");
    archived
        .subtasks
        .push(Subtask::new("Needle archived child"));
    app.lists[0].data.archive.push(ArchivedTask::new(archived));
    app.open_find();
    app.find.as_mut().unwrap().query = "needle".into();
    for _ in 0..3 {
        overlay_frame(&ctx, &mut app, vec![]);
    }
    let clicked = app.find.as_ref().unwrap().result_ids[1];
    let response = ctx.read_response(clicked).unwrap();
    let point = response.rect.center();
    assert!(response.interact_rect.contains(point));
    for pressed in [true, false] {
        overlay_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(app.find.is_none());
    assert!(app.history_open);
    assert_eq!(app.lists[0].data.archive.len(), 1);
    app.open_find();
    assert_eq!(app.find.as_ref().unwrap().query, "needle");
    assert_eq!(app.find.as_ref().unwrap().selected, 1);
    overlay_frame(&ctx, &mut app, vec![]);
    assert_eq!(app.find.as_ref().unwrap().selected, 1);
    std::fs::remove_dir_all(app.workspace).unwrap();
}
