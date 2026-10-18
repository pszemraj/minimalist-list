//! Global reveal shortcut settings and opening the workspace in the system file manager.

use super::{MinimalistApp, text_input_shortcuts, theme::Palette};
use eframe::egui::{self, Button, Id, Label};
use std::path::Path;
use std::process::Command;

impl MinimalistApp {
    /// Registers the configured global shortcut after the native event loop starts.
    pub fn setup_global_shortcut(&mut self, ctx: &egui::Context) {
        if self.settings.global_shortcut.trim().is_empty() {
            return;
        }
        match crate::hotkey::GlobalShortcut::new(ctx) {
            Ok(shortcut) => {
                self.global_shortcut = Some(shortcut);
                self.poll_global_shortcut(ctx);
            }
            Err(error) => self.status = Some(format!("Global shortcut unavailable: {error}")),
        }
    }

    /// Applies changed preferences and reveals the existing window on a global press.
    pub(super) fn poll_global_shortcut(&mut self, ctx: &egui::Context) {
        let Some(shortcut) = self.global_shortcut.as_mut() else {
            return;
        };
        if shortcut.attempted.as_deref() != Some(&self.settings.global_shortcut) {
            shortcut.attempted = Some(self.settings.global_shortcut.clone());
            if let Err(error) = shortcut.configure(&self.settings.global_shortcut) {
                self.status = Some(error);
            }
        }
        if shortcut.take_show_request() {
            self.show_window(ctx);
        }
    }

    /// Shows the current workspace in Finder, Explorer, or the desktop file manager.
    pub(super) fn open_workspace_folder(&mut self) {
        match folder_command(&self.workspace).spawn() {
            Ok(_) => self.acknowledge("Opened workspace folder".into()),
            Err(error) => self.status = Some(format!("Could not open workspace folder: {error}")),
        }
    }

    /// Renders an editable, explicitly applied global reveal shortcut.
    ///
    /// # Arguments
    /// - `ui` - Destination settings panel.
    /// - `palette` - Current control colors.
    pub(super) fn global_shortcut_settings_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        let font = self.font_id(14.0);
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Global shortcut", 18.0, palette.text));
            ui.add(Label::new(self.rich("Show this window from another app. It always reveals the window; pressing it again keeps it visible.", 12.0, palette.muted)).wrap());
            ui.add(
                Label::new(self.rich(
                    format!(
                        "Configured: {}",
                        crate::hotkey::label(&self.settings.global_shortcut)
                    ),
                    13.0,
                    palette.accent,
                ))
                .wrap(),
            );
            let field = Self::flat_text_input(
                ui,
                Id::new("settings-global-shortcut"),
                &mut self.global_shortcut_input,
                "Ctrl+Alt+Shift+L",
                font,
                palette,
            );
            field.widget_info(|| {
                egui::WidgetInfo::text_edit(
                    true,
                    &self.global_shortcut_input,
                    &self.global_shortcut_input,
                    "Global reveal shortcut",
                )
            });
            self.keyboard.settings_rows.push(vec![field.id]);
            ui.add(
                Label::new(self.rich(
                    if cfg!(target_os = "macos") {
                        "Example: Cmd+Option+Shift+L. Use Ctrl or Cmd with another modifier."
                    } else {
                        "Example: Ctrl+Alt+Shift+L. Use Ctrl with another modifier."
                    },
                    12.0,
                    palette.muted,
                ))
                .wrap(),
            );
            let (submit, _) = text_input_shortcuts(ui, &field);
            ui.horizontal_wrapped(|ui| {
                let apply = ui.button("Apply shortcut");
                let disable = ui.add_enabled(
                    !self.settings.global_shortcut.is_empty(),
                    Button::new("Disable shortcut"),
                );
                self.keyboard.settings_rows.push(vec![apply.id, disable.id]);
                if disable.clicked() {
                    self.global_shortcut_input.clear();
                }
                if apply.clicked() || disable.clicked() || submit {
                    self.apply_global_shortcut(ui.ctx());
                    field.request_focus();
                }
            });
        });
    }

    fn apply_global_shortcut(&mut self, ctx: &egui::Context) {
        let requested = self.global_shortcut_input.trim().to_owned();
        if let Err(error) = crate::hotkey::parse(&requested) {
            self.status = Some(error);
            return;
        }
        if self.global_shortcut.is_none() && !requested.is_empty() {
            match crate::hotkey::GlobalShortcut::new(ctx) {
                Ok(shortcut) => self.global_shortcut = Some(shortcut),
                Err(error) => {
                    self.status = Some(error);
                    return;
                }
            }
        }
        if let Some(shortcut) = self.global_shortcut.as_mut() {
            if let Err(error) = shortcut.configure(&requested) {
                self.status = Some(error);
                return;
            }
            shortcut.attempted = Some(requested.clone());
        }
        self.settings.global_shortcut = requested;
        self.save_settings();
        if !self.persistence.settings_dirty {
            self.acknowledge(format!(
                "Global shortcut: {}",
                crate::hotkey::label(&self.settings.global_shortcut)
            ));
        }
    }
}

fn folder_command(path: &Path) -> Command {
    let mut command = Command::new(if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer.exe"
    } else {
        "xdg-open"
    });
    command.arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_open_passes_one_literal_path_without_a_shell() {
        let path = Path::new("/tmp/List ' ; $(touch no-file) workspace");
        let command = folder_command(path);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![path.as_os_str()]
        );
        assert_ne!(command.get_program(), "sh");
    }

    #[test]
    fn invalid_shortcut_keeps_the_previous_preference_and_never_initializes_native_registration() {
        let (ctx, mut app) = crate::app::tests::test_app();
        let previous = app.settings.global_shortcut.clone();
        app.global_shortcut_input = "KeyQ".into();
        app.apply_global_shortcut(&ctx);
        assert_eq!(app.settings.global_shortcut, previous);
        assert!(app.global_shortcut.is_none());
        assert!(app.status.as_ref().unwrap().contains("modifier"));
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }
}
