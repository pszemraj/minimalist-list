//! Overview pointer, creation, and layout journeys.
use super::*;

fn grid_frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    width: f32,
    time: f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 760.0),
        )),
        time: Some(time),
        events,
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(
        ctx.data(|data| data.get_temp::<f32>(Id::new("native-scale")))
            .unwrap_or(1.0),
    );
    crate::app::tests::run_frame(ctx, input, |ui| app.overview_ui(ui, app.palette()))
}

fn pointer(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(point),
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn root_frame(ctx: &egui::Context, app: &mut MinimalistApp, events: Vec<egui::Event>) {
    app.update_motion(ctx);
    crate::app::tests::run_frame(
        ctx,
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520.0, 760.0),
            )),
            time: Some(ctx.input(|input| input.time) + 0.1),
            events,
            ..Default::default()
        },
        |ui| <MinimalistApp as eframe::App>::ui(app, ui, &mut eframe::Frame::_new_kittest()),
    );
}

#[test]
fn native_frame_event_batches_commit_card_drags_instead_of_opening_lists() {
    for batching in [0, 1, 2, 3] {
        let (ctx, mut app) = super::super::tests::test_app();
        let ids = three_lists(&mut app);
        for _ in 0..3 {
            root_frame(&ctx, &mut app, vec![]);
        }
        let source = ctx
            .read_response(FocusItem::Card(ids[0]).id())
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(FocusItem::Card(ids[1]).id())
            .unwrap()
            .rect
            .center();
        if batching == 0 {
            let mut events = pointer(source, true);
            events.extend(pointer(target, false));
            root_frame(&ctx, &mut app, events);
        } else {
            root_frame(&ctx, &mut app, pointer(source, true));
            if batching == 2 {
                root_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(target)]);
            }
            let release = pointer(target, false);
            root_frame(
                &ctx,
                &mut app,
                if batching == 3 {
                    vec![release[1].clone()]
                } else {
                    release
                },
            );
        }
        assert!(
            matches!(app.screen, super::super::Screen::Overview),
            "batching={batching}"
        );
        assert_eq!(
            app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
            vec![ids[1], ids[0], ids[2]],
            "batching={batching}"
        );
        assert!(!app.settings.list_order.is_empty());
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn native_card_clicks_settings_and_navigation_do_not_become_reorders() {
    for journey in ["click", "settings drag", "keyboard cancel"] {
        let (ctx, mut app) = super::super::tests::test_app();
        let ids = three_lists(&mut app);
        for _ in 0..3 {
            root_frame(&ctx, &mut app, vec![]);
        }
        let source = ctx
            .read_response(FocusItem::Card(ids[0]).id())
            .unwrap()
            .rect;
        let target = ctx
            .read_response(FocusItem::Card(ids[1]).id())
            .unwrap()
            .rect
            .center();
        match journey {
            "click" => {
                let mut events = pointer(source.center(), true);
                events.extend(pointer(source.center() + egui::vec2(1.0, 1.0), false));
                root_frame(&ctx, &mut app, events);
                assert!(matches!(app.screen, super::super::Screen::List(id) if id == ids[0]));
            }
            "settings drag" => {
                let settings = source.right_top() + egui::vec2(-18.0, 18.0);
                let mut events = pointer(settings, true);
                events.extend(pointer(target, false));
                root_frame(&ctx, &mut app, events);
                assert!(matches!(app.screen, super::super::Screen::Overview));
            }
            _ => {
                root_frame(&ctx, &mut app, pointer(source.center(), true));
                root_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(target)]);
                assert!(app.overview_drag.as_ref().unwrap().started);
                root_frame(
                    &ctx,
                    &mut app,
                    vec![egui::Event::Key {
                        key: egui::Key::Comma,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::COMMAND,
                    }],
                );
                root_frame(&ctx, &mut app, pointer(target, false));
                assert!(matches!(app.screen, super::super::Screen::Settings));
            }
        }
        assert!(app.overview_drag.is_none(), "journey={journey}");
        assert!(app.settings.list_order.is_empty(), "journey={journey}");
        assert_eq!(
            app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
            ids
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

fn three_lists(app: &mut MinimalistApp) -> Vec<Uuid> {
    for title in ["Second", "Third"] {
        app.lists
            .push(storage::create_list(&app.workspace, title, Accent::Sky).unwrap());
    }
    app.screen = super::super::Screen::Overview;
    app.lists.iter().map(|list| list.key).collect()
}

#[test]
fn grid_card_clicks_and_settings_keep_their_hit_regions_at_both_pixel_scales() {
    for scale in [1.0, 2.0] {
        for width in [320.0, 700.0] {
            for combined in [false, true] {
                let (ctx, mut app) = super::super::tests::test_app();
                // Native DPI changes do not apply egui's separate UI zoom transform.
                ctx.data_mut(|data| data.insert_temp(Id::new("native-scale"), scale));
                let ids = three_lists(&mut app);
                for time in [0.0, 0.1] {
                    grid_frame(&ctx, &mut app, width, time, vec![]);
                }
                let response = ctx.read_response(FocusItem::Card(ids[1]).id()).unwrap();
                let point = response.rect.left_top() + egui::vec2(50.0, 30.0);
                if combined {
                    let mut events = pointer(point, true);
                    events.extend(pointer(point + egui::vec2(1.0, 1.0), false));
                    grid_frame(&ctx, &mut app, width, 0.2, events);
                } else {
                    grid_frame(&ctx, &mut app, width, 0.2, pointer(point, true));
                    grid_frame(
                        &ctx,
                        &mut app,
                        width,
                        0.4,
                        pointer(point + egui::vec2(1.0, 1.0), false),
                    );
                }
                assert!(
                    matches!(app.screen, super::super::Screen::List(id) if id == ids[1]),
                    "scale={scale}, width={width}, combined={combined}"
                );
                assert!(app.overview_drag.is_none());
                app.go_to_overview();
                grid_frame(&ctx, &mut app, width, 0.6, vec![]);
                let card = ctx
                    .read_response(FocusItem::Card(ids[2]).id())
                    .unwrap()
                    .rect;
                let settings = card.right_top() + egui::vec2(-18.0, 18.0);
                grid_frame(&ctx, &mut app, width, 0.7, pointer(settings, true));
                grid_frame(
                    &ctx,
                    &mut app,
                    width,
                    0.9,
                    pointer(settings + egui::vec2(0.5, 0.5), false),
                );
                assert_eq!(app.settings_list_id, Some(ids[2]));
                assert!(app.overview_drag.is_none());
                std::fs::remove_dir_all(app.workspace).unwrap();
            }
        }
    }
}

#[test]
fn dragging_a_card_moves_its_identity_and_restores_order_after_restart() {
    let (ctx, mut app) = super::super::tests::test_app();
    let ids = three_lists(&mut app);
    grid_frame(&ctx, &mut app, 700.0, 0.0, vec![]);
    let source = ctx
        .read_response(FocusItem::Card(ids[0]).id())
        .unwrap()
        .rect
        .center();
    let target = ctx
        .read_response(FocusItem::Card(ids[2]).id())
        .unwrap()
        .rect
        .center();
    grid_frame(&ctx, &mut app, 700.0, 0.1, pointer(source, true));
    grid_frame(
        &ctx,
        &mut app,
        700.0,
        0.2,
        vec![egui::Event::PointerMoved(target)],
    );
    assert_eq!(app.overview_drag.as_ref().unwrap().list_id, ids[0]);
    grid_frame(&ctx, &mut app, 700.0, 0.3, pointer(target, false));
    let expected = vec![ids[1], ids[2], ids[0]];
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        expected
    );
    assert!(matches!(app.screen, super::super::Screen::Overview));
    let settings = storage::load_settings().unwrap();
    assert_eq!(
        settings.list_order[app.workspace.to_string_lossy().as_ref()],
        expected
    );
    let restarted = MinimalistApp::new(&egui::Context::default(), settings, None, None);
    assert_eq!(
        restarted
            .lists
            .iter()
            .map(|list| list.key)
            .collect::<Vec<_>>(),
        expected
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn navigation_or_a_removed_source_cancels_card_drag_without_reordering() {
    for removed in [false, true] {
        let (ctx, mut app) = super::super::tests::test_app();
        let ids = three_lists(&mut app);
        grid_frame(&ctx, &mut app, 700.0, 0.0, vec![]);
        let source = ctx
            .read_response(FocusItem::Card(ids[0]).id())
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(FocusItem::Card(ids[1]).id())
            .unwrap()
            .rect
            .center();
        grid_frame(&ctx, &mut app, 700.0, 0.1, pointer(source, true));
        grid_frame(
            &ctx,
            &mut app,
            700.0,
            0.2,
            vec![egui::Event::PointerMoved(target)],
        );
        assert!(app.overview_drag.is_some());
        if removed {
            app.lists.remove(0);
            grid_frame(&ctx, &mut app, 700.0, 0.3, vec![]);
        } else {
            app.open_settings(None);
        }
        assert!(app.overview_drag.is_none());
        assert!(app.settings.list_order.is_empty());
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn wrap_card_information_waits_two_seconds_and_scroll_mode_does_not_show_it() {
    for mode in [TitleOverflow::Wrap, TitleOverflow::Scroll] {
        let (ctx, mut app) = super::super::tests::test_app();
        app.screen = super::super::Screen::Overview;
        app.settings.title_overflow = mode;
        let id = app.lists[0].key;
        grid_frame(&ctx, &mut app, 320.0, 0.0, vec![]);
        let point = ctx
            .read_response(FocusItem::Card(id).id())
            .unwrap()
            .rect
            .center();
        let info_visible = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.text().contains("archived")
                )
            })
        };
        grid_frame(
            &ctx,
            &mut app,
            320.0,
            1.0,
            vec![egui::Event::PointerMoved(point)],
        );
        assert!(!info_visible(&grid_frame(
            &ctx,
            &mut app,
            320.0,
            2.9,
            vec![]
        )));
        grid_frame(&ctx, &mut app, 320.0, 3.1, vec![]);
        assert_eq!(
            info_visible(&grid_frame(&ctx, &mut app, 320.0, 3.2, vec![])),
            mode == TitleOverflow::Wrap
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn dragging_from_the_settings_target_does_not_open_or_reorder_a_list() {
    let (ctx, mut app) = super::super::tests::test_app();
    let ids = three_lists(&mut app);
    grid_frame(&ctx, &mut app, 700.0, 0.0, vec![]);
    let source = ctx
        .read_response(FocusItem::Card(ids[0]).id())
        .unwrap()
        .rect
        .right_top()
        + egui::vec2(-18.0, 18.0);
    let target = ctx
        .read_response(FocusItem::Card(ids[1]).id())
        .unwrap()
        .rect
        .center();
    grid_frame(&ctx, &mut app, 700.0, 0.1, pointer(source, true));
    grid_frame(
        &ctx,
        &mut app,
        700.0,
        0.2,
        vec![egui::Event::PointerMoved(target)],
    );
    grid_frame(&ctx, &mut app, 700.0, 0.3, pointer(target, false));
    assert!(matches!(app.screen, super::super::Screen::Overview));
    assert!(app.overview_drag.is_none());
    assert!(app.settings.list_order.is_empty());
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        ids
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn manual_tile_order_is_independent_for_each_workspace() {
    let (_, mut app) = super::super::tests::test_app();
    let original = app.workspace.clone();
    let first = three_lists(&mut app);
    app.reorder_list(first[0], first[2]);
    let first_order = vec![first[1], first[2], first[0]];
    let next = original.join("second-workspace");
    app.switch_workspace(next.to_str().unwrap());
    let second = three_lists(&mut app);
    app.reorder_list(second[2], second[0]);
    let second_order = vec![second[2], second[0], second[1]];
    app.switch_workspace(original.to_str().unwrap());
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        first_order
    );
    app.switch_workspace(next.to_str().unwrap());
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        second_order
    );
    let settings = storage::load_settings().unwrap();
    assert_eq!(
        settings.list_order[original.to_string_lossy().as_ref()],
        first_order
    );
    assert_eq!(
        settings.list_order[next.to_string_lossy().as_ref()],
        second_order
    );
    std::fs::remove_dir_all(original).unwrap();
}

#[test]
fn dragging_near_the_grid_bottom_scrolls_to_more_tiles() {
    let (ctx, mut app) = super::super::tests::test_app();
    for index in 0..12 {
        app.lists.push(
            storage::create_list(&app.workspace, &format!("List {index}"), Accent::Sky).unwrap(),
        );
    }
    app.screen = super::super::Screen::Overview;
    let id = app.lists[0].key;
    grid_frame(&ctx, &mut app, 320.0, 0.0, vec![]);
    let source = ctx.read_response(FocusItem::Card(id).id()).unwrap().rect;
    let target = egui::pos2(source.center().x, 746.0);
    grid_frame(&ctx, &mut app, 320.0, 0.1, pointer(source.center(), true));
    grid_frame(
        &ctx,
        &mut app,
        320.0,
        0.2,
        vec![egui::Event::PointerMoved(target)],
    );
    for index in 0..30 {
        grid_frame(&ctx, &mut app, 320.0, 0.3 + index as f64 * 0.05, vec![]);
    }
    assert!(
        ctx.read_response(FocusItem::Card(id).id())
            .unwrap()
            .rect
            .top()
            < source.top() - 80.0
    );
    grid_frame(&ctx, &mut app, 320.0, 2.0, pointer(target, false));
    assert!(app.overview_drag.is_none());
    assert_ne!(app.lists[0].key, id);
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn list_sort_views_preserve_manual_order_and_stable_ties_across_restart() {
    let (_, mut app) = super::super::tests::test_app();
    let ids = three_lists(&mut app);
    for (index, (title, timestamp)) in [("Zulu", 100), ("alpha", 300), ("ALPHA", 300)]
        .into_iter()
        .enumerate()
    {
        app.lists[index].data.title = title.into();
        app.lists[index].data.created_at_unix = timestamp;
        storage::save_list(&app.lists[index]).unwrap();
    }
    app.reorder_list(ids[2], ids[0]);
    let manual = vec![ids[2], ids[0], ids[1]];
    app.keyboard.overview_list = Some(ids[0]);
    for (mode, expected) in [
        (ListSort::Title, vec![ids[2], ids[1], ids[0]]),
        (ListSort::Newest, vec![ids[2], ids[1], ids[0]]),
        (ListSort::Oldest, vec![ids[0], ids[2], ids[1]]),
    ] {
        app.set_list_sort(mode);
        assert_eq!(
            app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            app.settings.list_order[app.workspace.to_string_lossy().as_ref()],
            manual
        );
        assert_eq!(app.keyboard.pending, Some(FocusItem::Card(ids[0]).id()));
        let restarted = MinimalistApp::new(
            &egui::Context::default(),
            storage::load_settings().unwrap(),
            None,
            None,
        );
        assert_eq!(restarted.list_sort(), mode);
        assert_eq!(
            restarted
                .lists
                .iter()
                .map(|list| list.key)
                .collect::<Vec<_>>(),
            expected
        );
    }
    app.reorder_list(ids[0], ids[1]);
    assert_eq!(
        app.settings.list_order[app.workspace.to_string_lossy().as_ref()],
        manual
    );
    app.set_list_sort(ListSort::Manual);
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        manual
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn sort_lists_menu_changes_the_view_and_manual_restores_tile_order() {
    let (ctx, mut app) = super::super::tests::test_app();
    let ids = three_lists(&mut app);
    app.lists[0].data.title = "Zulu".into();
    app.lists[1].data.title = "Alpha".into();
    app.lists[2].data.title = "Middle".into();
    click_label(&ctx, &mut app, "Sort lists · Manual order");
    click_label(&ctx, &mut app, "Title A–Z");
    assert_eq!(app.list_sort(), ListSort::Title);
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        vec![ids[1], ids[2], ids[0]]
    );
    click_label(&ctx, &mut app, "Sort lists · Title A–Z");
    click_label(&ctx, &mut app, "Manual order");
    assert_eq!(app.list_sort(), ListSort::Manual);
    assert_eq!(
        app.lists.iter().map(|list| list.key).collect::<Vec<_>>(),
        ids
    );
    std::fs::remove_dir_all(app.workspace).unwrap();
}

fn frame(
    ctx: &egui::Context,
    app: &mut MinimalistApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    crate::app::tests::run_frame(
        ctx,
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 760.0),
            )),
            time: Some(ctx.input(|input| input.time) + 0.3),
            events,
            ..Default::default()
        },
        |ui| app.overview_ui(ui, app.palette()),
    )
}

fn click_label(ctx: &egui::Context, app: &mut MinimalistApp, label: &str) {
    let output = frame(ctx, app, Vec::new());
    let point = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::epaint::Shape::Text(text) = &shape.shape
                && text.galley.text() == label
            {
                Some(text.pos + text.galley.size() * 0.5)
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("Missing button {label}"));
    for pressed in [true, false] {
        frame(
            ctx,
            app,
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
}

#[test]
fn creation_buttons_create_cancel_and_leave_blank_submission_focused() {
    let (ctx, mut app) = super::super::tests::test_app();
    app.creating_list = true;
    app.request_list_focus = true;
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new());
    }
    frame(
        &ctx,
        &mut app,
        crate::app::tests::chord(egui::Key::Enter, egui::Modifiers::NONE),
    );
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text("Still focused".into())],
    );
    assert_eq!(app.new_list_title, "Still focused");
    app.new_list_title.clear();
    let before = app.lists.len();
    click_label(&ctx, &mut app, "Create list");
    assert!(app.creating_list);
    assert_eq!(app.lists.len(), before);
    app.new_list_title = "Pointer-created list".into();
    click_label(&ctx, &mut app, "Create list");
    assert!(!app.creating_list);
    assert_eq!(app.lists.len(), before + 1);
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert!(
        saved
            .lists
            .iter()
            .any(|list| list.data.title == "Pointer-created list")
    );
    app.creating_list = true;
    app.new_list_title = "Must not be created".into();
    for _ in 0..3 {
        frame(&ctx, &mut app, Vec::new());
    }
    click_label(&ctx, &mut app, "Cancel");
    assert!(!app.creating_list);
    assert!(app.new_list_title.is_empty());
    assert_eq!(app.lists.len(), before + 1);
    std::fs::remove_dir_all(&app.workspace).unwrap();
}

#[test]
fn long_cards_keep_title_preview_footer_and_options_inside_the_card() {
    for width in [210.0, 288.0, 488.0] {
        for font in [
            crate::model::FontChoice::Sans,
            crate::model::FontChoice::Mono,
        ] {
            for title in [
                "Long title with many words and line breaks\n".repeat(12),
                "W".repeat(1000),
            ] {
                let (ctx, mut app) = super::super::tests::test_app();
                app.settings.font = font;
                app.settings.bold_text = true;
                let id = app.lists[0].key;
                let mut bounds = Rect::NOTHING;
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let (card, options) = app.list_card(
                        ui,
                        id,
                        &title,
                        Accent::Charcoal,
                        3,
                        0,
                        &["Long preview ".repeat(80); 1],
                        width,
                    );
                    bounds = card.rect;
                    assert!(card.rect.contains_rect(options.rect));
                });
                for shape in &output.shapes {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        let rect = Rect::from_min_size(text.pos, text.galley.size());
                        assert!(
                            bounds.contains_rect(rect),
                            "{font:?}, width={width}, {rect:?} outside {bounds:?}"
                        );
                        if text.galley.text() == title {
                            assert!(text.galley.rows.len() <= 2);
                        }
                    }
                }
                output.textures_delta.clear();
                std::fs::remove_dir_all(&app.workspace).unwrap();
            }
        }
    }
}

#[test]
fn completed_only_card_says_all_done() {
    let (ctx, mut app) = super::super::tests::test_app();
    let id = app.lists[0].key;
    let mut output = ctx.run_ui(Default::default(), |ui| {
        app.list_card(ui, id, "Completed", Accent::Charcoal, 0, 2, &[], 288.0);
    });
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "All done")));
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Empty")));
    output.textures_delta.clear();
    std::fs::remove_dir_all(&app.workspace).unwrap();
}
