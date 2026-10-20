//! Tray hiding, restoration, and explicit application shutdown.

use super::{MinimalistApp, SCAN_INTERVAL};
use crate::tray::{Action, Tray};
use eframe::egui::{self, ViewportCommand};
use std::time::Instant;

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
            return;
        }
        match Tray::new(ctx, supports_hiding) {
            Ok(tray) => self.tray = Some(tray),
            Err(error) => {
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
        self.next_taskbar_check = Instant::now() + SCAN_INTERVAL;
        let hidden = self.tray.as_ref().is_some_and(Tray::available);
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
        let available = self.tray.as_ref().is_some_and(Tray::available);
        if !self.hide_window(ctx, available) {
            self.show_window(ctx);
        }
    }

    /// Describes the close control's current behavior.
    ///
    /// # Returns
    ///
    /// The tooltip for hiding to the tray or closing without a tray.
    pub(super) fn close_hint(&self) -> &'static str {
        if self.tray.is_some() {
            "Hide to tray"
        } else {
            "Close"
        }
    }

    /// Hides the window when a tray is available, otherwise closes it.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context receiving native window commands.
    pub(super) fn close_window(&mut self, ctx: &egui::Context) {
        if !self.finish_pending_changes() {
            self.show_window(ctx);
            return;
        }
        let available = self.tray.as_ref().is_some_and(Tray::available);
        if !self.hide_window(ctx, available) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn hide_window(&mut self, ctx: &egui::Context, tray_available: bool) -> bool {
        if !tray_available {
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
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        true
    }

    /// Reveals and focuses the existing native window.
    pub(super) fn show_window(&mut self, ctx: &egui::Context) {
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
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
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
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        // Show and Quit take precedence; idle visible ticks need no D-Bus probe.
        let check_host = actions.is_empty()
            && !self.quitting
            && (close_requested
                || (minimized && !self.window_hidden)
                || (self.window_hidden && Instant::now() >= self.next_scan));
        let tray_available = check_host && self.tray.as_ref().is_some_and(Tray::available);
        self.handle_window_events(ctx, minimized, &actions, tray_available);
    }

    fn handle_window_events(
        &mut self,
        ctx: &egui::Context,
        minimized: bool,
        actions: &[Action],
        tray_available: bool,
    ) {
        let mut shown = false;
        for action in actions {
            match action {
                Action::Show => {
                    self.show_window(ctx);
                    shown = true;
                }
                Action::Quit => {
                    self.quit(ctx);
                    return;
                }
            }
        }
        if self.quitting {
            return;
        }
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if shown {
            if close_requested {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            }
            return;
        }
        if close_requested {
            if !self.finish_pending_changes() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.show_window(ctx);
                return;
            }
            if self.hide_window(ctx, tray_available) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            }
        } else if minimized && !self.window_hidden {
            self.hide_window(ctx, tray_available);
        } else if self.window_hidden && Instant::now() >= self.next_scan && !tray_available {
            self.show_window(ctx);
            self.status =
                Some("The desktop tray is unavailable; the window has been restored.".into());
            self.next_scan = Instant::now() + SCAN_INTERVAL;
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        if !self.finish_pending_changes() {
            self.show_window(ctx);
            return;
        }
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Saves an active inline edit before navigating, hiding, or quitting.
    pub(super) fn finish_inline_edit(&mut self) {
        self.finish_subtask_edit();
        if let Some((list_id, task_id)) = self.editing {
            self.commit_task_edit(list_id, task_id, &self.edit_text.clone());
        }
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
        let output = ctx.run_logic(&Default::default(), |ctx| app.quit(ctx));
        assert!(!app.quitting && !app.window_hidden);
        assert!(app.quick_capture.is_some());
        assert!(
            output
                .viewport_commands
                .values()
                .flatten()
                .all(|command| *command != ViewportCommand::Close)
        );
        std::fs::remove_file(app.workspace.join("lists")).unwrap();
        std::fs::rename(app.workspace.join("offline"), app.workspace.join("lists")).unwrap();
        draw(&mut app, vec![]);
        draw(&mut app, enter());
        assert!(app.quick_capture.is_none());
        let output = ctx.run_logic(&Default::default(), |ctx| app.quit(ctx));
        assert!(app.quitting);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Close)
        );
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
    fn window_events_save_hide_restore_and_reload() {
        let (ctx, mut app) = test_app();
        let list_id = app.lists[0].key;
        let task = Task::new("Before editing");
        app.editing = Some((list_id, task.id));
        app.edit_text = "Saved before hiding".into();
        app.lists[0].data.tasks = vec![task];

        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, true, &[], true);
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
            app.handle_window_events(ctx, true, &[Action::Show], false);
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
            app.handle_window_events(ctx, true, &[], true);
        });
        app.next_scan = Instant::now() + SCAN_INTERVAL;
        let _ = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[], false);
        });
        assert!(app.window_hidden);
        app.next_scan = Instant::now();
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, false, &[], false);
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
        let output = ctx.run_logic(&Default::default(), |ctx| app.start_hidden(ctx));
        assert!(!app.window_hidden);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&ViewportCommand::Visible(true))
        );
        let output = ctx.run_logic(&Default::default(), |ctx| {
            app.handle_window_events(ctx, true, &[], false);
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
            app.handle_window_events(ctx, false, &[], false);
        });
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
                    app.quit(ctx);
                } else {
                    app.close_window(ctx);
                }
            });
            assert_eq!(app.quitting, tray_quit);
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
