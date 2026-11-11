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
    /// - `ctx` - Native context to wake on reopen or quit requests.
    pub fn set_instance_guard(
        &mut self,
        guard: crate::instance::InstanceGuard,
        ctx: &egui::Context,
    ) {
        guard.set_repaint_context(ctx.clone());
        self.instance = Some(guard);
        #[cfg(target_os = "macos")]
        match crate::macos::AppDelegateHandler::install(ctx) {
            Ok(handler) => self.macos_delegate = Some(handler),
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
        if !self.finish_open_editors() || !self.retry_dirty_lists() {
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
        if let Some(handler) = &self.macos_delegate {
            if self.quitting {
                handler.reply_to_quit(true);
            } else {
                actions.extend(handler.window_actions());
            }
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
        if self.keyboard.deferred_events.is_empty()
            && let Some(was_hidden) = self.global_quick_add_requested.take()
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

    /// Saves pending changes, then hides for a Close with a usable tray or exits.
    ///
    /// # Arguments
    /// - `ctx` - Context receiving native window commands.
    /// - `shutdown` - Close or Quit; a failed save keeps the window open.
    pub(super) fn finish_native_shutdown(&mut self, ctx: &egui::Context, shutdown: NativeShutdown) {
        let saved = self.finish_pending_changes();
        #[cfg(target_os = "macos")]
        let system_quit = matches!(shutdown, NativeShutdown::Quit)
            && self
                .macos_delegate
                .as_ref()
                .is_some_and(|handler| handler.reply_to_quit(saved));
        if !saved {
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
        #[cfg(target_os = "macos")]
        if system_quit {
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Finishes accepted changes while retaining failed capture drafts before native shutdown.
    ///
    /// # Returns
    /// Whether every accepted capture, list, and preference change is saved.
    pub(super) fn finish_pending_changes(&mut self) -> bool {
        if self.reopen_failed_quick_capture() || !self.finish_open_editors() {
            return false;
        }
        self.drag = None;
        if let Some((list_id, task_id, _)) = self.delete_animation.take() {
            self.remove_task(list_id, task_id);
        }
        if let Some((list_id, _)) = self.clear_animation.take() {
            self.archive_completed(list_id);
        }
        self.retry_dirty_lists()
    }

    /// Saves notes and commits inline edits and the current list name.
    ///
    /// # Returns
    /// False if the notes editor still needs attention.
    fn finish_open_editors(&mut self) -> bool {
        if !self.save_task_context() {
            return false;
        }
        self.finish_inline_edit();
        if let Some(id) = self.settings_list_id {
            self.rename_list(id);
        }
        true
    }
}

fn cancel_native_close(ctx: &egui::Context) {
    if ctx.input(|input| input.viewport().close_requested()) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
