//! Headless keyboard journeys using real egui widgets and persisted task files.

use super::*;
use crate::app::tests::test_app;
use crate::model::{Accent, ArchivedTask, Task};
use crate::storage;

fn frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<Event>,
    width: f32,
) -> egui::FullOutput {
    let input = egui::RawInput {
        time: Some(ctx.input(|input| input.time) + 0.3),
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 500.0),
        )),
        events,
        ..Default::default()
    };
    app.update_motion(ctx);
    let mut output = ctx.run_ui(input, |ui| {
        // eframe provides this hidden constructor for headless App tests.
        // Review its compatibility when upgrading the locked eframe version.
        <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut MinimalistApp) {
    for _ in 0..3 {
        frame(ctx, app, Vec::new(), 430.0);
    }
}

fn key(ctx: &egui::Context, app: &mut MinimalistApp, key: Key, modifiers: Modifiers) {
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect();
    frame(ctx, app, events, 430.0);
    idle(ctx, app);
}

fn focus(ctx: &egui::Context, app: &mut MinimalistApp, id: Id) {
    app.request_task_focus = false;
    app.keyboard.pending = Some(id);
    idle(ctx, app);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(id));
}

fn type_text(ctx: &egui::Context, app: &mut MinimalistApp, text: &str) {
    frame(ctx, app, vec![Event::Text(text.to_owned())], 430.0);
}

fn add_list(app: &mut MinimalistApp, name: &str) -> Uuid {
    let mut list = storage::create_list(&app.workspace, name, Accent::Mint).unwrap();
    list.data.created_at_unix = app
        .lists
        .iter()
        .map(|list| list.data.created_at_unix)
        .max()
        .unwrap_or(0)
        + 1;
    storage::save_list(&list).unwrap();
    let id = list.key;
    app.lists.push(list);
    storage::sort_lists(&mut app.lists);
    id
}

fn cleanup(app: &MinimalistApp) {
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn projects_add_back_tasks_add_uses_only_keys() {
    let (ctx, mut app) = test_app();
    let tasks = app.lists[0].key;
    app.lists[0].data.title = "Tasks".into();
    let projects = add_list(&mut app, "projects");
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == tasks));
    type_text(&ctx, &mut app, "x");
    key(&ctx, &mut app, Key::Backspace, Modifiers::NONE);
    assert!(app.new_task.is_empty());
    assert!(matches!(app.screen, Screen::List(id) if id == tasks));
    key(
        &ctx,
        &mut app,
        BACK_TO_LISTS.logical_key,
        BACK_TO_LISTS.modifiers,
    );
    assert!(matches!(app.screen, Screen::Overview));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(tasks).id())
    );
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(projects).id())
    );
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(matches!(app.screen, Screen::List(id) if id == projects));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("new-task", projects)))
    );
    type_text(&ctx, &mut app, "Plan project");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    key(
        &ctx,
        &mut app,
        BACK_TO_LISTS.logical_key,
        BACK_TO_LISTS.modifiers,
    );
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(projects).id())
    );
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::NONE);
    key(&ctx, &mut app, Key::Space, Modifiers::NONE);
    type_text(&ctx, &mut app, "Buy groceries");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(
        saved
            .lists
            .iter()
            .find(|l| l.key == projects)
            .unwrap()
            .data
            .tasks[0]
            .text,
        "Plan project"
    );
    assert_eq!(
        saved
            .lists
            .iter()
            .find(|l| l.key == tasks)
            .unwrap()
            .data
            .tasks[0]
            .text,
        "Buy groceries"
    );
    assert_eq!(
        saved
            .lists
            .iter()
            .map(|l| l.data.tasks.len())
            .sum::<usize>(),
        2
    );
    cleanup(&app);
}

#[test]
fn opening_a_card_accepts_text_on_the_very_next_frame() {
    let (ctx, mut app) = test_app();
    app.go_to_overview();
    idle(&ctx, &mut app);
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        430.0,
    );
    type_text(&ctx, &mut app, "Immediate typing");
    assert_eq!(app.new_task, "Immediate typing");
    cleanup(&app);
}

#[test]
fn immediate_arrows_override_entry_and_return_focus_requests() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    idle(&ctx, &mut app);
    type_text(&ctx, &mut app, "First");
    let press = |ctx: &egui::Context, app: &mut MinimalistApp, key, modifiers| {
        frame(
            ctx,
            app,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            430.0,
        )
    };
    press(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    press(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let task = app.lists[0].data.tasks[0].id;
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task).id())
    );
    add_list(&mut app, "Second");
    press(
        &ctx,
        &mut app,
        BACK_TO_LISTS.logical_key,
        BACK_TO_LISTS.modifiers,
    );
    press(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(app.lists[1].key).id())
    );
    cleanup(&app);
}

#[test]
fn settings_round_trip_preserves_destination_and_rename_for_ctrl_and_cmd() {
    for command in [
        Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        },
        Modifiers {
            mac_cmd: true,
            command: true,
            ..Modifiers::NONE
        },
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        idle(&ctx, &mut app);
        key(&ctx, &mut app, Key::Comma, command);
        key(&ctx, &mut app, Key::Comma, command);
        assert!(matches!(app.screen, Screen::Settings));
        app.rename_input = "Renamed".into();
        key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        assert!(matches!(app.screen, Screen::List(id) if id == list));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(Id::new(("new-task", list)))
        );
        assert_eq!(app.lists[0].data.title, "Renamed");
        key(
            &ctx,
            &mut app,
            BACK_TO_LISTS.logical_key,
            BACK_TO_LISTS.modifiers,
        );
        key(&ctx, &mut app, Key::Comma, command);
        key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
        assert!(matches!(app.screen, Screen::Overview));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(FocusItem::Card(list).id())
        );
        cleanup(&app);
    }
}

#[test]
fn grid_navigation_obeys_columns_and_edges() {
    for (width, columns) in [(430.0, 1), (700.0, 2), (980.0, 3)] {
        let (ctx, mut app) = test_app();
        for name in ["Second", "Third", "Fourth", "Fifth"] {
            add_list(&mut app, name);
        }
        app.go_to_overview();
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new(), width);
        }
        let start = app.lists[0].key;
        ctx.memory_mut(|m| m.request_focus(FocusItem::Card(start).id()));
        for (key, expected) in [
            (Key::ArrowLeft, 0),
            (Key::ArrowUp, 0),
            (Key::ArrowDown, columns),
        ] {
            frame(
                &ctx,
                &mut app,
                vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                width,
            );
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(FocusItem::Card(app.lists[expected].key).id())
            );
        }
        if columns > 1 {
            ctx.memory_mut(|m| m.request_focus(FocusItem::Card(app.lists[columns - 1].key).id()));
            for key in [Key::ArrowUp, Key::ArrowRight] {
                frame(
                    &ctx,
                    &mut app,
                    vec![Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                    width,
                );
                assert_eq!(
                    ctx.memory(|m| m.focused()),
                    Some(FocusItem::Card(app.lists[columns - 1].key).id())
                );
            }
        }
        cleanup(&app);
    }
}

#[test]
fn task_commands_edit_complete_reorder_delete_and_keep_identity() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["First", "Second", "Third"].map(Task::new).to_vec();
    app.save_list_index(0);
    idle(&ctx, &mut app);
    let path = app.lists[0].path.clone();
    let first = app.lists[0].data.tasks[0].id;
    for (edge, movement) in [(Key::Home, Key::ArrowUp), (Key::End, Key::ArrowDown)] {
        focus(&ctx, &mut app, FocusItem::Task(list, first).id());
        key(&ctx, &mut app, edge, Modifiers::NONE);
        let focused = ctx.memory(|memory| memory.focused());
        let order: Vec<_> = app.lists[0].data.tasks.iter().map(|task| task.id).collect();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000),
                ),
            )
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: movement,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::ALT,
            }],
            430.0,
        );
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: movement,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: Modifiers::ALT,
            }],
            430.0,
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(ctx.memory(|memory| memory.focused()), focused);
        assert_eq!(
            app.lists[0]
                .data
                .tasks
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            order
        );
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: movement,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::ALT,
            }],
            430.0,
        );
        assert_eq!(
            storage::load_workspace(&app.workspace).unwrap().lists[0]
                .data
                .tasks
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            order
        );
    }
    focus(&ctx, &mut app, Id::new(("new-task", list)));
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let first = app.lists[0].data.tasks[0].id;
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, first).id())
    );
    key(&ctx, &mut app, Key::Space, Modifiers::NONE);
    assert!(app.lists[0].data.tasks[0].completed);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Edited");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(app.lists[0].data.tasks[0].text, "Edited");
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, first).id())
    );
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(app.lists[0].data.tasks[1].id, first);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, first).id())
    );
    key(&ctx, &mut app, Key::End, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, app.lists[0].data.tasks[2].id).id())
    );
    key(&ctx, &mut app, Key::Home, Modifiers::NONE);
    key(&ctx, &mut app, Key::ArrowDown, Modifiers::NONE);
    let next = app.lists[0].data.tasks[2].id;
    key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    app.delete_animation.as_mut().unwrap().2 =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, next).id())
    );
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .tasks
            .len(),
        2
    );
    cleanup(&app);
}

#[test]
fn specific_shortcuts_do_not_open_quick_add_or_move_text_focus() {
    let (ctx, mut app) = test_app();
    let first = app.lists[0].key;
    let second = add_list(&mut app, "Second");
    for (index, list) in app.lists.iter_mut().enumerate() {
        list.data.created_at_unix = index as u64 + 1;
        storage::save_list(list).unwrap();
    }
    idle(&ctx, &mut app);
    key(
        &ctx,
        &mut app,
        Key::N,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(app.creating_list && app.quick_capture.is_none());
    type_text(&ctx, &mut app, "New keyboard list");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(app.lists.len(), 3);
    key(&ctx, &mut app, Key::Tab, Modifiers::CTRL);
    assert!(matches!(app.screen, Screen::List(id) if id == first));
    key(&ctx, &mut app, Key::Tab, Modifiers::CTRL);
    assert!(matches!(app.screen, Screen::List(id) if id == second));
    key(&ctx, &mut app, Key::Tab, Modifiers::CTRL | Modifiers::SHIFT);
    assert!(matches!(app.screen, Screen::List(id) if id == first));
    for ctrl in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::COMMAND] {
        key(&ctx, &mut app, Key::Tab, ctrl);
        assert!(matches!(app.screen, Screen::List(id) if id == second));
        key(&ctx, &mut app, Key::Tab, ctrl | Modifiers::SHIFT);
        assert!(matches!(app.screen, Screen::List(id) if id == first));
    }
    key(
        &ctx,
        &mut app,
        Key::Tab,
        Modifiers::MAC_CMD | Modifiers::COMMAND,
    );
    assert!(matches!(app.screen, Screen::List(id) if id == first));
    key(&ctx, &mut app, Key::Tab, Modifiers::CTRL);
    type_text(&ctx, &mut app, "a b");
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::NONE);
    key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
    assert_eq!(app.new_task, "a ");
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("new-task", second)))
    );
    cleanup(&app);
}

#[test]
fn modal_dismissal_restores_focus_and_search_activation_focuses_target() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("needle");
    app.lists[0].data.tasks.push(task.clone());
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    assert!(!matches!(app.screen, Screen::Settings));
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(app.quick_capture.is_none());
    assert!(matches!(app.screen, Screen::List(_)));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    key(&ctx, &mut app, Key::F, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "needle");
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.find.is_none());
    assert!(app.editing.is_none());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    cleanup(&app);
}

#[test]
fn reload_preserves_focus_then_chooses_next_surviving_task() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["First", "Middle", "Last"].map(Task::new).to_vec();
    app.save_list_index(0);
    let middle = app.lists[0].data.tasks[1].id;
    let last = app.lists[0].data.tasks[2].id;
    focus(&ctx, &mut app, FocusItem::Task(list, middle).id());
    let mut external = app.lists[0].clone();
    external.data.tasks.insert(0, Task::new("Inserted"));
    storage::save_list(&external).unwrap();
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, middle).id())
    );
    external.data.tasks.retain(|task| task.id != middle);
    storage::save_list(&external).unwrap();
    app.next_scan = std::time::Instant::now();
    app.scan_external_changes();
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, last).id())
    );
    cleanup(&app);
}

#[test]
fn tab_reaches_subtasks_history_and_settings_controls() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let mut task = Task::new("Parent");
    task.subtasks.push(crate::model::Subtask::new("Child"));
    let subtask = task.subtasks[0].id;
    let archived = Task::new("Archived");
    app.lists[0]
        .data
        .archive
        .push(ArchivedTask::new(archived.clone()));
    app.lists[0].data.tasks.push(task.clone());
    app.subtasks_for = Some((list, task.id));
    app.history_open = true;
    idle(&ctx, &mut app);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..24 {
        key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
        seen.extend(ctx.memory(|m| m.focused()));
    }
    assert!(seen.contains(&FocusItem::Task(list, task.id).id()));
    assert!(seen.contains(&FocusItem::Subtask(list, task.id, subtask).id()));
    assert!(seen.contains(&FocusItem::Archived(list, archived.id).id()));
    focus(&ctx, &mut app, FocusItem::Archived(list, archived.id).id());
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, archived.id).id())
    );
    key(&ctx, &mut app, Key::Comma, Modifiers::COMMAND);
    let start = ctx.memory(|m| m.focused());
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert_ne!(ctx.memory(|m| m.focused()), start);
    key(&ctx, &mut app, Key::Tab, Modifiers::SHIFT);
    assert_eq!(ctx.memory(|m| m.focused()), start);
    cleanup(&app);
}

#[test]
fn quick_add_dropdown_escape_is_local_and_submit_restores_entry() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    add_list(&mut app, "Second");
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Captured");
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(egui::Popup::is_any_open(&ctx));
    for _ in 0..6 {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(app.quick_capture.is_some());
    }
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        430.0,
    );
    assert!(!egui::Popup::is_any_open(&ctx));
    assert!(app.quick_capture.is_some());
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert!(app.quick_capture.is_none());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::new(("new-task", list)))
    );
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .tasks[0]
            .text,
        "Captured"
    );
    cleanup(&app);
}

#[test]
fn immediate_tab_enter_opens_capture_selector_without_submitting() {
    let (ctx, mut app) = test_app();
    add_list(&mut app, "Second");
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Not submitted yet");
    for key in [Key::Tab, Key::Enter] {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
    }
    assert!(egui::Popup::is_any_open(&ctx));
    assert!(app.quick_capture.is_some());
    assert!(app.lists.iter().all(|list| list.data.tasks.is_empty()));
    cleanup(&app);
}

#[test]
fn consecutive_tabs_after_opening_settings_keep_advancing() {
    let (ctx, mut app) = test_app();
    idle(&ctx, &mut app);
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: Key::Comma,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
        430.0,
    );
    let mut seen = std::collections::HashSet::new();
    for _ in 0..5 {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
        assert!(
            seen.insert(ctx.memory(|memory| memory.focused())),
            "Tab must advance instead of restoring the workspace field"
        );
    }
    cleanup(&app);
}

#[test]
fn capture_destination_selection_closes_popup_and_returns_to_text() {
    let (ctx, mut app) = test_app();
    let first = app.lists[0].key;
    let second = add_list(&mut app, "Second");
    app.open_list(second);
    idle(&ctx, &mut app);
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    type_text(&ctx, &mut app, "Destination task");
    for input_key in [Key::Tab, Key::Enter, Key::ArrowDown, Key::Enter] {
        key(&ctx, &mut app, input_key, Modifiers::NONE);
    }
    assert!(!egui::Popup::is_any_open(&ctx));
    assert!(app.quick_capture.is_some());
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(Id::new("quick-capture-text"))
    );
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    assert_eq!(
        app.lists
            .iter()
            .find(|list| list.key == first)
            .unwrap()
            .data
            .tasks[0]
            .text,
        "Destination task"
    );
    assert!(
        app.lists
            .iter()
            .find(|list| list.key == second)
            .unwrap()
            .data
            .tasks
            .is_empty()
    );
    cleanup(&app);
}

#[test]
fn deletion_immediately_after_modal_navigation_restores_next_row() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    app.lists[0].data.tasks = ["First", "Second", "Last"].map(Task::new).to_vec();
    let next = app.lists[0].data.tasks[1].id;
    let last = app.lists[0].data.tasks[2].id;
    focus(&ctx, &mut app, FocusItem::Task(list, last).id());
    key(&ctx, &mut app, Key::N, Modifiers::COMMAND);
    for key in [Key::Escape, Key::Home, Key::Delete] {
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            430.0,
        );
    }
    app.delete_animation.as_mut().unwrap().2 =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    idle(&ctx, &mut app);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(FocusItem::Task(list, next).id())
    );
    cleanup(&app);
}

#[test]
fn deleting_last_rows_and_clearing_completed_restore_surviving_focus() {
    for command in [
        Modifiers::CTRL | Modifiers::COMMAND,
        Modifiers::MAC_CMD | Modifiers::COMMAND,
    ] {
        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0].data.tasks = ["First", "Second", "Third", "Last"].map(Task::new).to_vec();
        let first = app.lists[0].data.tasks[0].id;
        let third = app.lists[0].data.tasks[2].id;
        focus(&ctx, &mut app, FocusItem::Task(list, first).id());
        key(&ctx, &mut app, Key::Backspace, command | Modifiers::SHIFT);
        key(&ctx, &mut app, Key::Space, Modifiers::NONE);
        if let Some((_, started)) = &mut app.clear_animation {
            *started = std::time::Instant::now() - std::time::Duration::from_secs(1);
        }
        idle(&ctx, &mut app);
        assert_eq!(
            app.lists[0].data.tasks.len(),
            4,
            "an empty clear must not archive a task completed afterward"
        );
        assert!(app.lists[0].data.tasks[0].completed);
        assert!(app.lists[0].data.archive.is_empty());
        app.lists[0].data.tasks[1].completed = true;
        focus(&ctx, &mut app, FocusItem::Task(list, first).id());
        key(&ctx, &mut app, Key::Backspace, command | Modifiers::SHIFT);
        app.clear_animation.as_mut().unwrap().1 =
            std::time::Instant::now() - std::time::Duration::from_secs(1);
        idle(&ctx, &mut app);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(FocusItem::Task(list, third).id())
        );
        key(&ctx, &mut app, Key::End, Modifiers::NONE);
        for expected in [
            Some(FocusItem::Task(list, third).id()),
            Some(Id::new(("new-task", list))),
        ] {
            key(&ctx, &mut app, Key::Delete, Modifiers::NONE);
            app.delete_animation.as_mut().unwrap().2 =
                std::time::Instant::now() - std::time::Duration::from_secs(1);
            idle(&ctx, &mut app);
            assert_eq!(ctx.memory(|m| m.focused()), expected);
        }
        cleanup(&app);
    }
}

#[test]
fn tab_saves_edits_and_cancelled_list_creation_restores_card() {
    let (ctx, mut app) = test_app();
    let list = app.lists[0].key;
    let task = Task::new("Before");
    app.lists[0].data.tasks.push(task.clone());
    focus(&ctx, &mut app, FocusItem::Task(list, task.id).id());
    key(&ctx, &mut app, Key::Enter, Modifiers::NONE);
    app.edit_text = "After".into();
    key(&ctx, &mut app, Key::Tab, Modifiers::NONE);
    assert_eq!(app.lists[0].data.tasks[0].text, "After");
    assert!(app.editing.is_none());
    assert_ne!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Task(list, task.id).id())
    );
    key(
        &ctx,
        &mut app,
        Key::N,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    key(&ctx, &mut app, Key::Escape, Modifiers::NONE);
    assert!(!app.creating_list);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(FocusItem::Card(list).id())
    );
    cleanup(&app);
}

#[path = "keyboard_focus_tests.rs"]
mod focus_regressions;

#[path = "keyboard_reference_tests.rs"]
mod reference;
