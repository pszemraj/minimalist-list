//! Tray hiding, restoration, and explicit application shutdown.

use super::{MinimalistApp, SCAN_INTERVAL};
use crate::tray::{Action, Tray};
use eframe::egui::{self, ViewportCommand};
use std::time::Instant;

impl MinimalistApp {
    /// Installs the native tray after the window's event loop has started.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Native application's egui context.
    /// - `supports_hiding` - Whether the selected window backend supports hide and restore.
    pub fn setup_tray(&mut self, ctx: &egui::Context, supports_hiding: bool) {
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
        if !self.hide_window(ctx) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn hide_window(&mut self, ctx: &egui::Context) -> bool {
        if !self.tray.as_ref().is_some_and(Tray::available) {
            return false;
        }
        self.drag = None;
        self.hovered_title = None;
        self.window_hidden = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        true
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.window_hidden = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Processes tray events and native close or minimize requests while hidden or visible.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context containing the close request and receiving window commands.
    /// - `minimized` - Current minimized state from the native window.
    pub(super) fn handle_window_actions(&mut self, ctx: &egui::Context, minimized: bool) {
        let mut shown = false;
        while let Some(action) = self.tray.as_ref().and_then(Tray::next_action) {
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
            if self.hide_window(ctx) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            }
        } else if minimized && !self.window_hidden {
            self.hide_window(ctx);
        } else if self.window_hidden
            && Instant::now() >= self.next_scan
            && !self.tray.as_ref().is_some_and(Tray::available)
        {
            self.show_window(ctx);
            self.status =
                Some("The desktop tray is unavailable; the window has been restored.".into());
            self.next_scan = Instant::now() + SCAN_INTERVAL;
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.finish_pending_changes();
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn finish_pending_changes(&mut self) {
        self.drag = None;
        if let Some((list_id, task_id)) = self.editing {
            self.commit_task_edit(list_id, task_id, &self.edit_text.clone());
        }
        if let Some((list_id, task_id, _)) = self.delete_animation.take() {
            self.remove_task(list_id, task_id);
        }
        if let Some((list_id, _)) = self.clear_animation.take() {
            self.archive_completed(list_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use crate::model::Task;

    #[test]
    fn quit_flushes_inline_edit_and_pending_mutations_to_disk() {
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
        let output = ctx.run_logic(&Default::default(), |ctx| app.quit(ctx));
        assert!(app.quitting);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT].contains(&ViewportCommand::Close)
        );
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
