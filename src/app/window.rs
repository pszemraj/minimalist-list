//! Tray hiding, restoration, and explicit application shutdown.

use super::MinimalistApp;
#[cfg(any(target_os = "linux", test))]
use super::SCAN_INTERVAL;
use crate::tray::{Action, Availability, Tray};
use eframe::egui::{self, ViewportCommand};
#[cfg(any(target_os = "linux", target_os = "windows", test))]
use std::time::Instant;

/// A native close or quit awaiting the current frame's editor input.
#[derive(Clone, Copy)]
pub(super) enum NativeShutdown {
    Close(Availability),
    Quit,
}

impl MinimalistApp {
    /// Retains the workspace claim and attaches launch requests to this event loop.
    ///
    /// # Arguments
    /// - `guard` - Claim for the loaded workspace.
    /// - `ctx` - Native context to wake on reopening.
    pub fn set_instance_guard(
        &mut self,
        guard: crate::instance::InstanceGuard,
        ctx: &egui::Context,
    ) {
        guard.set_repaint_context(ctx.clone());
        self.instance = Some(guard);
        #[cfg(target_os = "macos")]
        match crate::macos::ReopenHandler::install(ctx) {
            Ok(handler) => self.reopen = Some(handler),
            Err(error) => self.status = Some(error),
        }
    }

    /// Installs the native tray after the window's event loop has started.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Native application's egui context.
    /// - `supports_hiding` - Whether the selected window backend supports hide and restore.
    pub fn setup_tray(&mut self, ctx: &egui::Context, supports_hiding: bool) {
        // A desktop without a usable tray uses ordinary window behavior quietly.
        if !supports_hiding {
            log::debug!("Tray setup skipped for this desktop");
            return;
        }
        match Tray::new(ctx) {
            Ok(tray) => {
                self.tray = Some(tray);
            }
            Err(error) => {
                log::warn!("Could not install the native tray: {error}");
                let message = format!("Tray unavailable: {error}");
                self.status = Some(match self.status.take() {
                    Some(status) => format!("{status}\n{message}"),
                    None => message,
                });
            }
        }
    }

    /// Keeps Windows and X11 windows out of the taskbar while a usable tray exists.
    ///
    /// # Arguments
    /// - `window` - Current root window; unavailable windows are retried next tick.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub(crate) fn update_taskbar_presence(&mut self, window: Option<&winit::window::Window>) {
        let Some(window) = window else {
            return;
        };
        if Instant::now() < self.next_taskbar_check {
            return;
        }
        let availability = self.tray_availability();
        let hidden = availability == Availability::Available;
        if self.taskbar_hidden == Some(hidden) {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowExtWindows;
            window.set_skip_taskbar(hidden);
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = crate::linux::set_taskbar_hidden(window, hidden) {
            self.next_taskbar_check = Instant::now() + SCAN_INTERVAL;
            self.status
                .get_or_insert_with(|| format!("Could not update taskbar visibility: {error}"));
            return;
        }
        self.taskbar_hidden = Some(hidden);
    }

    /// Starts in the tray, or shows the window if no usable tray exists.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Native application's egui context.
    pub fn start_hidden(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.tray {
            tray.wait_for_startup();
        }
        if !self.hide_window(ctx, self.tray_availability()) {
            log::info!("Hidden startup unavailable; showing the window");
            self.restore_window(ctx);
        }
    }

    /// Reports an absent tray separately from an inconclusive check of an existing one.
    fn tray_availability(&self) -> Availability {
        self.tray
            .as_ref()
            .map_or(Availability::Unavailable, Tray::availability)
    }

    /// Describes the close control's current behavior.
    ///
    /// # Returns
    ///
    /// The tooltip for hiding to the tray or closing without a tray.
    pub(super) fn close_hint(&self) -> &'static str {
        if self.tray_availability().can_hide() {
            "Hide to tray"
        } else {
            "Close"
        }
    }

    /// Closes the window or hides it when the tray can reopen it.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context receiving native window commands.
    pub(super) fn close_window(&mut self, ctx: &egui::Context) {
        log::debug!("Close requested through the app");
        self.finish_native_shutdown(ctx, NativeShutdown::Close(self.tray_availability()));
    }

    /// Restores Quick add's previous hidden state while the tray is usable.
    pub(super) fn return_to_tray_after_capture(&mut self, ctx: &egui::Context) {
        self.hide_window(ctx, self.tray_availability());
    }

    fn hide_window(&mut self, ctx: &egui::Context, availability: Availability) -> bool {
        log::debug!("Window hide requested (tray: {availability:?})");
        if !availability.can_hide() {
            return false;
        }
        if !self.save_task_context() {
            self.show_window(ctx);
            return true;
        }
        self.finish_inline_edit();
        if let Some(id) = self.settings_list_id {
            self.rename_list(id);
        }
        if !self.retry_dirty_lists() {
            self.show_window(ctx);
            return true;
        }
        self.drag = None;
        self.hovered_title = None;
        self.window_hidden = true;
        log::info!("Window hidden to tray");
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        true
    }

    /// Reveals and focuses the existing native window.
    pub(super) fn show_window(&mut self, ctx: &egui::Context) {
        self.restore_window(ctx);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn restore_window(&mut self, ctx: &egui::Context) {
        if self.window_hidden {
            log::info!("Window restored from tray");
        }
        self.window_hidden = false;
        self.last_pin_state = None;
        // X11 window managers may remove state hints when a window is unmapped.
        // Reapply before the queued reveal, including repeated launch/hotkey requests.
        #[cfg(any(target_os = "windows", target_os = "linux", test))]
        {
            self.taskbar_hidden = None;
            self.next_taskbar_check = Instant::now();
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    }

    /// Processes native reveal, tray, close, and minimize requests while hidden or visible.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context containing the close request and receiving window commands.
    /// - `minimized` - Current minimized state from the native window.
    pub(super) fn handle_window_actions(&mut self, ctx: &egui::Context, minimized: bool) {
        let mut actions: Vec<_> =
            std::iter::from_fn(|| self.tray.as_ref().and_then(Tray::next_action)).collect();
        actions.extend(self.take_global_shortcut_actions());
        if self
            .instance
            .as_ref()
            .is_some_and(|instance| instance.take_show_request())
        {
            actions.push(Action::Show);
        }
        #[cfg(target_os = "macos")]
        if self
            .reopen
            .as_ref()
            .is_some_and(|handler| handler.take_show_request())
        {
            actions.push(Action::Show);
        }
        let availability = self.tray_availability();
        self.handle_window_events(ctx, minimized, &actions, Some(availability));
    }

    fn handle_window_events(
        &mut self,
        ctx: &egui::Context,
        minimized: bool,
        actions: &[Action],
        availability: Option<Availability>,
    ) {
        if self.quitting {
            return;
        }
        let pending = self.pending_native_shutdown.take();
        if actions.contains(&Action::Quit) || matches!(pending, Some(NativeShutdown::Quit)) {
            self.request_native_shutdown(ctx, NativeShutdown::Quit, minimized);
            return;
        }
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        let quick_add = actions.contains(&Action::QuickAdd);
        let show = actions.contains(&Action::Show);
        if show || quick_add {
            if quick_add {
                self.request_global_quick_add(ctx);
            } else {
                self.show_window(ctx);
            }
            if show {
                self.keep_window_after_capture();
            }
            if close_requested {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            }
            return;
        }
        if close_requested {
            log::debug!("Native close requested (tray: {availability:?})");
            let close = NativeShutdown::Close(availability.unwrap_or(Availability::Unavailable));
            self.request_native_shutdown(ctx, close, minimized);
        } else if let Some(NativeShutdown::Close(previous)) = pending {
            let close = NativeShutdown::Close(availability.unwrap_or(previous));
            self.request_native_shutdown(ctx, close, minimized);
        } else if minimized && !self.window_hidden {
            self.hide_window(ctx, availability.unwrap_or(Availability::Unavailable));
        } else if self.window_hidden && availability == Some(Availability::Unavailable) {
            log::warn!("Tray became unavailable while hidden; restoring the window");
            self.restore_window(ctx);
            self.status =
                Some("The desktop tray is unavailable; the window has been restored.".into());
        }
    }

    fn request_native_shutdown(
        &mut self,
        ctx: &egui::Context,
        shutdown: NativeShutdown,
        minimized: bool,
    ) {
        self.global_quick_add_requested = None;
        // Eframe runs only logic for a minimized or occluded root viewport.
        if self.window_hidden
            || minimized
            || ctx.input(|input| input.viewport().visible() == Some(false))
        {
            self.finish_native_shutdown(ctx, shutdown);
        } else {
            self.pending_native_shutdown = Some(shutdown);
        }
    }

    /// Applies native requests after widgets have consumed the current editor input.
    pub(super) fn finish_window_actions_after_input(&mut self, ctx: &egui::Context) {
        self.finish_native_shutdown_after_input(ctx);
        if let Some(was_hidden) = self.global_quick_add_requested.take()
            && !self.quitting
        {
            self.open_global_quick_add(ctx, was_hidden);
            ctx.request_repaint();
        }
    }

    fn finish_native_shutdown_after_input(&mut self, ctx: &egui::Context) {
        let Some(shutdown) = self.pending_native_shutdown else {
            return;
        };
        if !self.keyboard.deferred_events.is_empty() {
            cancel_native_close(ctx);
            return;
        }
        self.pending_native_shutdown = None;
        if !self.quitting {
            self.finish_native_shutdown(ctx, shutdown);
        }
    }

    fn finish_native_shutdown(&mut self, ctx: &egui::Context, shutdown: NativeShutdown) {
        if !self.finish_pending_changes() {
            log::info!("Close or Quit canceled because changes need attention");
            cancel_native_close(ctx);
            self.show_window(ctx);
            return;
        }
        if let NativeShutdown::Close(availability) = shutdown
            && availability != Availability::Unavailable
        {
            self.hide_window(ctx, availability);
            cancel_native_close(ctx);
            return;
        }
        self.quitting = true;
        if let Some(instance) = &self.instance {
            instance.stop_reveal_requests();
        }
        log::info!("Application exit accepted");
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Finishes accepted changes while retaining failed capture drafts before native shutdown.
    ///
    /// # Returns
    /// Whether every accepted capture, list, and preference change is saved.
    pub(super) fn finish_pending_changes(&mut self) -> bool {
        if self.reopen_failed_quick_capture() {
            return false;
        }
        if !self.save_task_context() {
            return false;
        }
        self.drag = None;
        self.finish_inline_edit();
        if let Some(id) = self.settings_list_id {
            self.rename_list(id);
        }
        if let Some((list_id, task_id, _)) = self.delete_animation.take() {
            self.remove_task(list_id, task_id);
        }
        if let Some((list_id, _)) = self.clear_animation.take() {
            self.archive_completed(list_id);
        }
        self.retry_dirty_lists()
    }
}

fn cancel_native_close(ctx: &egui::Context) {
    if ctx.input(|input| input.viewport().close_requested()) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use crate::model::Task;

    #[test]
    fn tray_quit_retains_failed_capture_until_its_retry_succeeds() {
        let (ctx, mut app) = test_app();
        app.open_quick_capture(None);
        let draw = |app: &mut MinimalistApp, events| {
            crate::app::tests::run_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(520.0, 700.0),
                    )),
                    time: Some(ctx.input(|input| input.time) + 0.3),
                    events,
                    ..Default::default()
                },
                |_| app.overlay_ui(&ctx, app.palette()),
            );
        };
        let enter = || {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        };
        draw(&mut app, vec![]);
        draw(
            &mut app,
            vec![egui::Event::Text("Retained tray capture".into())],
        );
        std::fs::rename(app.workspace.join("lists"), app.workspace.join("offline")).unwrap();
        std::fs::write(app.workspace.join("lists"), "unavailable").unwrap();
        draw(&mut app, enter());
        assert!(app.quick_capture.is_some() && app.status.is_some());
        app.window_hidden = true;
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events = vec![egui::ViewportEvent::Close];
        let output = ctx.run_logic(&input, |ctx| {
            app.handle_window_events(ctx, false, &[Action::Quit], None);
        });
        assert!(!app.quitting && !app.window_hidden);
        assert!(app.quick_capture.is_some());
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(commands.contains(&ViewportCommand::CancelClose));
        assert!(!commands.contains(&ViewportCommand::Close));
        std::fs::remove_file(app.workspace.join("lists")).unwrap();
        std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists")).unwrap();
        draw(&mut app, vec![]);
        draw(&mut app, enter());
        assert!(app.quick_capture.is_none());
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .occluded = Some(true);
        let output = ctx.run_logic(&input, |ctx| {
            app.handle_window_events(
                ctx,
                false,
                &[Action::QuickAdd, Action::Quit, Action::Show],
                None,
            );
        });
        assert!(app.quitting);
        assert!(app.global_quick_add_requested.is_none());
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(commands.contains(&ViewportCommand::Close));
        assert!(!commands.contains(&ViewportCommand::CancelClose));
        assert!(!commands.contains(&ViewportCommand::Focus));
        let saved = crate::storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks.len(), 1);
        assert_eq!(saved.lists[0].data.tasks[0].text, "Retained tray capture");
        std::fs::remove_dir_all(app.workspace).unwrap();
    }

    #[test]
    fn duplicate_launch_reveals_despite_stale_minimize_and_close_input() {
        for close_requested in [false, true] {
            let (ctx, mut app) = test_app();
            let state = app.workspace.join("instance-state");
            let crate::instance::LaunchOutcome::Primary(guard) =
                crate::instance::claim(&app.workspace, &state, false).unwrap()
            else {
                panic!("expected a new test instance");
            };
            app.set_instance_guard(guard, &ctx);
            app.window_hidden = true;
            assert!(matches!(
                crate::instance::claim(&app.workspace, &state, false).unwrap(),
                crate::instance::LaunchOutcome::Existing
            ));
            let mut input = egui::RawInput::default();
            if close_requested {
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .events = vec![egui::ViewportEvent::Close];
            }

            let output = ctx.run_logic(&input, |ctx| app.handle_window_actions(ctx, true));
            assert!(!app.window_hidden);
            let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
            assert!(commands.contains(&ViewportCommand::Visible(true)));
            assert!(commands.contains(&ViewportCommand::Minimized(false)));
            assert!(!commands.contains(&ViewportCommand::Visible(false)));
            if close_requested {
                assert!(commands.contains(&ViewportCommand::CancelClose));
            }
            app.instance = None;
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn show_and_quick_add_override_native_close_and_minimize_in_either_order() {
        for actions in [
            &[Action::QuickAdd][..],
            &[Action::Show, Action::QuickAdd],
            &[Action::QuickAdd, Action::Show],
        ] {
            let (ctx, mut app) = test_app();
            app.window_hidden = true;
            let mut input = egui::RawInput::default();
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events = vec![egui::ViewportEvent::Close];

            let output = ctx.run_logic(&input, |ctx| {
                app.handle_window_events(ctx, true, actions, Some(Availability::Available));
            });
            assert!(!app.window_hidden);
            assert_eq!(app.global_quick_add_requested, Some(actions.len() == 1));
            let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
            assert!(commands.contains(&ViewportCommand::CancelClose));
            assert!(commands.contains(&ViewportCommand::Minimized(false)));
            assert!(!commands.contains(&ViewportCommand::Visible(false)));
            assert_eq!(
                commands
                    .iter()
                    .filter(|c| **c == ViewportCommand::Focus)
                    .count(),
                1
            );
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }

    #[test]
    fn revealing_reapplies_the_saved_window_level_after_mapping() {
        for pinned in [false, true] {
            let (ctx, mut app) = test_app();
            app.settings.always_on_top = pinned;
            app.last_pin_state = Some(pinned);
            app.last_decorations_state = Some(app.settings.window_decorations);
            app.window_hidden = true;

            let output = ctx.run_logic(&Default::default(), |ctx| {
                app.show_window(ctx);
                app.apply_window_preferences(ctx);
            });
            let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
            let mapped = commands
                .iter()
                .position(|command| *command == ViewportCommand::Visible(true))
                .unwrap();
            let level = if pinned {
                egui::viewport::WindowLevel::AlwaysOnTop
            } else {
                egui::viewport::WindowLevel::Normal
            };
            let restored = commands
                .iter()
                .position(|command| *command == ViewportCommand::WindowLevel(level))
                .unwrap();
            assert!(mapped < restored);
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn tray_quick_add_reveals_a_hidden_window_and_focuses_the_entry_field() {
        let (ctx, mut app) = test_app();
        app.window_hidden = true;
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[Action::QuickAdd], None);
        });
        assert!(!app.window_hidden);
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        let mapped = commands
            .iter()
            .position(|command| *command == ViewportCommand::Visible(true))
            .unwrap();
        let focused = commands
            .iter()
            .position(|command| *command == ViewportCommand::Focus)
            .unwrap();
        assert!(mapped < focused);
        crate::app::tests::settle_keyboard(&ctx, &mut app);
        assert!(app.quick_capture.is_some());
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::new("quick-capture-text"))
        );
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    #[test]
    fn tray_quick_add_leaves_queued_paste_with_the_inline_editor() {
        use crate::app::{
            keyboard::FocusItem,
            tests::{chord, focus_widget, frame_input, keyboard_frame, run_frame, settle_keyboard},
        };

        let (ctx, mut app) = test_app();
        let list = app.lists[0].key;
        let task = Task::new("Original");
        let task_id = task.id;
        app.lists[0].data.tasks.push(task);
        assert!(app.save_list_index(0));
        focus_widget(&ctx, &mut app, FocusItem::Task(list, task_id).id());
        keyboard_frame(
            &ctx,
            &mut app,
            chord(egui::Key::Enter, egui::Modifiers::NONE),
            430.0,
        );
        settle_keyboard(&ctx, &mut app);
        assert_eq!(app.editing, Some((list, task_id)));

        let input = frame_input(
            egui::vec2(430.0, 500.0),
            ctx.input(|input| input.time) + 0.3,
            vec![egui::Event::Paste("One\nTwo".into())],
        );
        run_frame(&ctx, input, |ui| {
            // Native eframe runs logic and then UI with the same input batch.
            app.handle_window_events(ui.ctx(), false, &[Action::QuickAdd], None);
            <MinimalistApp as eframe::App>::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
        });
        settle_keyboard(&ctx, &mut app);

        let saved = crate::storage::load_workspace(&app.workspace).unwrap();
        let tasks = &saved.lists[0].data.tasks;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "OriginalOne Two");
        assert!(app.quick_capture.as_ref().unwrap().text.is_empty());
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::new("quick-capture-text"))
        );
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    #[test]
    fn native_close_and_tray_quit_save_input_from_the_last_visible_frame() {
        use crate::app::{
            keyboard::FocusItem,
            tests::{chord, focus_widget, frame_input, keyboard_frame, run_frame, settle_keyboard},
        };

        for tray_quit in [false, true] {
            for deferred in [false, true] {
                let (ctx, mut app) = test_app();
                let list = app.lists[0].key;
                let task = Task::new("Original");
                let task_id = task.id;
                app.lists[0].data.tasks.push(task);
                assert!(app.save_list_index(0));
                focus_widget(&ctx, &mut app, FocusItem::Task(list, task_id).id());
                keyboard_frame(
                    &ctx,
                    &mut app,
                    chord(egui::Key::Enter, egui::Modifiers::NONE),
                    430.0,
                );
                settle_keyboard(&ctx, &mut app);
                assert_eq!(app.editing, Some((list, task_id)));

                let mut events = vec![egui::Event::Paste("One\nTwo".into())];
                if deferred {
                    events.extend([egui::Event::Copy, egui::Event::Text("Three".into())]);
                }
                let size = egui::vec2(430.0, 500.0);
                let mut input = frame_input(size, ctx.input(|input| input.time) + 0.3, events);
                let actions: &[_] = if tray_quit {
                    &[Action::Quit]
                } else {
                    input
                        .viewports
                        .get_mut(&egui::ViewportId::ROOT)
                        .unwrap()
                        .events
                        .push(egui::ViewportEvent::Close);
                    &[]
                };
                let output = run_frame(&ctx, input, |ui| {
                    app.handle_window_events(
                        ui.ctx(),
                        false,
                        actions,
                        Some(Availability::Unavailable),
                    );
                    eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
                });
                assert_eq!(app.quitting, !deferred);
                let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
                assert_eq!(
                    commands.contains(&ViewportCommand::CancelClose),
                    deferred && !tray_quit
                );
                assert_eq!(commands.contains(&ViewportCommand::Close), !deferred);

                if deferred {
                    let input = frame_input(size, ctx.input(|input| input.time) + 0.3, vec![]);
                    let output = run_frame(&ctx, input, |ui| {
                        app.handle_window_actions(ui.ctx(), false);
                        eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
                    });
                    assert!(app.quitting);
                    assert!(
                        output.viewport_output[&egui::ViewportId::ROOT]
                            .commands
                            .contains(&ViewportCommand::Close)
                    );
                }

                let saved = crate::storage::load_workspace(&app.workspace).unwrap();
                let expected = if deferred {
                    "OriginalOne TwoThree"
                } else {
                    "OriginalOne Two"
                };
                assert_eq!(saved.lists[0].data.tasks[0].text, expected);
                std::fs::remove_dir_all(app.workspace).unwrap();
            }
        }
    }

    #[test]
    fn window_events_save_hide_restore_and_reload() {
        let (ctx, mut app) = test_app();
        let list_id = app.lists[0].key;
        let task = Task::new("Before editing");
        app.editing = Some((list_id, task.id));
        app.edit_text = "Saved before hiding".into();
        app.lists[0].data.tasks = vec![task];

        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, true, &[], Some(Availability::Available));
        });
        assert!(app.window_hidden);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&ViewportCommand::Visible(false))
        );
        assert!(app.editing.is_none());
        let mut saved = crate::storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks[0].text, "Saved before hiding");

        saved.lists[0].data.tasks[0].text = "Changed on another computer".into();
        crate::storage::save_list(&saved.lists[0]).unwrap();
        app.next_scan = Instant::now();
        app.scan_external_changes();
        assert_eq!(
            app.lists[0].data.tasks[0].text,
            "Changed on another computer"
        );

        // A window manager can discard skip-taskbar state while hidden. A
        // reveal must invalidate the cache even when its polling deadline is later.
        app.taskbar_hidden = Some(true);
        app.next_taskbar_check = Instant::now() + SCAN_INTERVAL;

        // A tray Show wins over stale minimize and close input in the same tick.
        let mut close_input = egui::RawInput::default();
        close_input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events = vec![egui::ViewportEvent::Close];
        let output = ctx.run_logic(&close_input, |ctx| {
            app.handle_window_events(ctx, true, &[Action::Show], None);
        });
        assert!(!app.window_hidden);
        assert_eq!(app.taskbar_hidden, None);
        assert!(app.next_taskbar_check <= Instant::now());
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        for command in [
            ViewportCommand::Visible(true),
            ViewportCommand::Minimized(false),
            ViewportCommand::Focus,
            ViewportCommand::CancelClose,
        ] {
            assert!(commands.contains(&command));
        }
        assert!(!commands.contains(&ViewportCommand::Visible(false)));

        let _ = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, true, &[], Some(Availability::Available));
        });
        app.next_scan = Instant::now() + SCAN_INTERVAL;
        let _ = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[], None);
        });
        assert!(app.window_hidden);
        app.next_scan = Instant::now();
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[], Some(Availability::Unavailable));
        });
        assert!(!app.window_hidden);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&ViewportCommand::Visible(true))
        );

        // No tray is an ordinary fallback: minimize stays visible to the taskbar,
        // and native close is not canceled.
        app.status = None;
        app.setup_tray(&ctx, false);
        assert!(app.status.is_none());
        assert_eq!(app.tray_availability(), Availability::Unavailable);
        let output = ctx.run_logic(&Default::default(), |ctx| app.start_hidden(ctx));
        assert!(!app.window_hidden);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&ViewportCommand::Visible(true))
        );
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, true, &[], Some(Availability::Unavailable));
        });
        assert!(!app.window_hidden);
        assert!(
            output
                .viewport_commands
                .values()
                .flatten()
                .all(|command| *command != ViewportCommand::Visible(false))
        );
        let output = ctx.run_logic(&close_input, |ctx| {
            app.handle_window_events(ctx, false, &[], Some(Availability::Unavailable));
            app.finish_native_shutdown_after_input(ctx);
        });
        assert!(app.quitting);
        assert!(
            output
                .viewport_commands
                .values()
                .flatten()
                .all(|command| *command != ViewportCommand::CancelClose)
        );
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    #[test]
    fn unchecked_or_inconclusive_tray_checks_do_not_restore_a_hidden_window() {
        for availability in [
            None,
            Some(Availability::Unknown),
            Some(Availability::Available),
        ] {
            let (ctx, mut app) = test_app();
            app.window_hidden = true;
            // The deadline may pass after handle_window_actions chooses not to probe.
            app.next_scan = Instant::now();
            let output = ctx.run_logic(&Default::default(), |ctx| {
                app.handle_window_events(ctx, false, &[], availability);
            });
            assert!(app.window_hidden);
            assert!(app.status.is_none());
            assert!(output.viewport_commands.values().flatten().all(|command| {
                !matches!(
                    command,
                    ViewportCommand::Visible(true) | ViewportCommand::Focus
                )
            }));
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn an_inconclusive_probe_does_not_hide_or_exit() {
        for close in [false, true] {
            let (ctx, mut app) = test_app();
            let mut input = egui::RawInput::default();
            if close {
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .events = vec![egui::ViewportEvent::Close];
            }
            // Only an already-created native tray can produce Unknown in the application.
            let output = ctx.run_logic(&input, |ctx| {
                app.handle_window_events(ctx, !close, &[], Some(Availability::Unknown));
                app.finish_native_shutdown_after_input(ctx);
            });
            assert!(!app.window_hidden);
            assert!(!app.quitting);
            assert!(app.status.is_none());
            let mut commands = output.viewport_commands.values().flatten();
            if close {
                assert!(commands.any(|command| *command == ViewportCommand::CancelClose));
            }
            assert!(output.viewport_commands.values().flatten().all(|command| {
                !matches!(
                    command,
                    ViewportCommand::Visible(false) | ViewportCommand::Close
                )
            }));
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn shutdown_flushes_inline_edit_and_pending_mutations_to_disk() {
        for tray_quit in [false, true] {
            let (ctx, mut app) = test_app();
            let list_id = app.lists[0].key;
            let edited = Task::new("Before editing");
            let mut completed = Task::new("Archive on quit");
            completed.completed = true;
            let deleted = Task::new("Delete on quit");
            app.editing = Some((list_id, edited.id));
            app.edit_text = "After editing".into();
            app.lists[0].data.tasks = vec![edited, completed, deleted.clone()];
            app.delete_animation = Some((list_id, deleted.id, Instant::now()));
            app.clear_animation = Some((list_id, Instant::now()));
            app.window_hidden = true;
            let output = ctx.run_logic(&Default::default(), |ctx| {
                if tray_quit {
                    app.handle_window_events(ctx, false, &[Action::Quit], None);
                } else {
                    app.close_window(ctx);
                }
            });
            assert!(app.quitting);
            assert!(
                output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Close)
            );
            eframe::App::on_exit(&mut app, None);
            let saved = crate::storage::load_workspace(&app.workspace).unwrap();
            assert_eq!(saved.lists[0].data.tasks.len(), 1);
            assert_eq!(saved.lists[0].data.tasks[0].text, "After editing");
            assert_eq!(saved.lists[0].data.archive.len(), 1);
            assert_eq!(saved.lists[0].data.archive[0].task.text, "Archive on quit");
            let mut input = egui::RawInput::default();
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events = vec![egui::ViewportEvent::Close];
            let output = ctx.run_logic(&input, |ctx| app.handle_window_actions(ctx, true));
            assert!(
                !output
                    .viewport_commands
                    .values()
                    .flatten()
                    .any(|cmd| *cmd == ViewportCommand::CancelClose)
            );
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn command_w_closes_window_above_overlays_after_saving_edits() {
        for overlay in [false, true] {
            let (ctx, mut app) = test_app();
            let list_id = app.lists[0].key;
            let edited = Task::new("Before editing");
            app.editing = Some((list_id, edited.id));
            app.edit_text = "After editing".into();
            app.lists[0].data.tasks = vec![edited];
            if overlay {
                app.open_quick_capture(None);
            }
            let output = crate::app::tests::run_app_frame(
                &ctx,
                &mut app,
                egui::RawInput {
                    events: crate::app::tests::chord(egui::Key::W, egui::Modifiers::COMMAND),
                    ..Default::default()
                },
            );
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::Close)
            );
            let saved = crate::storage::load_workspace(&app.workspace).unwrap();
            assert_eq!(saved.lists[0].data.tasks[0].text, "After editing");
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}
