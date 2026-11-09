//! Global shortcut settings and opening the workspace in the system file manager.

use super::{MinimalistApp, text_input_shortcuts, theme::Palette};
use crate::hotkey::GlobalAction;
use crate::tray::Action;
use eframe::egui::{self, Button, Id, Label};
use std::path::Path;
use std::process::Command;

impl MinimalistApp {
    /// Registers the configured global shortcuts after the native event loop starts.
    pub fn setup_global_shortcut(&mut self, ctx: &egui::Context) {
        let enabled: Vec<_> = GlobalAction::ALL
            .into_iter()
            .filter(|action| !action.configured(&self.settings).trim().is_empty())
            .collect();
        if enabled.is_empty() {
            return;
        }
        match crate::hotkey::GlobalShortcut::new(ctx) {
            Ok(shortcut) => {
                self.global_shortcut = Some(shortcut);
                self.refresh_global_shortcuts();
            }
            Err(error) => {
                let error = format!("Global shortcut unavailable: {error}");
                for action in enabled {
                    self.report_shortcut_error(action, error.clone());
                }
            }
        }
    }

    /// Shows a shortcut failure that a later successful apply can clear.
    fn report_shortcut_error(&mut self, action: GlobalAction, error: String) {
        self.shortcut_errors[action] = Some(error.clone());
        self.status = Some(error);
    }

    /// Applies preferences that have not been attempted by the native manager.
    fn refresh_global_shortcuts(&mut self) {
        let Some(shortcut) = self.global_shortcut.as_mut() else {
            return;
        };
        let mut failures = Vec::new();
        for action in GlobalAction::ALL {
            let configured = action.configured(&self.settings);
            if shortcut.attempted.get(action).as_deref() != Some(configured) {
                *shortcut.attempted.get_mut(action) = Some(configured.to_owned());
                if let Err(error) = shortcut.configure(action, configured) {
                    failures.push((action, error));
                }
            }
        }
        for (action, error) in failures {
            self.report_shortcut_error(action, error);
        }
    }

    /// Collects global presses alongside the other native window actions.
    ///
    /// # Returns
    /// Pending Show and Quick add requests.
    pub(super) fn take_global_shortcut_actions(&mut self) -> impl Iterator<Item = Action> + '_ {
        self.refresh_global_shortcuts();
        [
            (GlobalAction::Show, Action::Show),
            (GlobalAction::QuickAdd, Action::QuickAdd),
        ]
        .into_iter()
        .filter_map(|(shortcut_action, window_action)| {
            self.global_shortcut
                .as_ref()
                .is_some_and(|shortcut| shortcut.take_request(shortcut_action))
                .then_some(window_action)
        })
    }

    /// Shows the current workspace in Finder, Explorer, or the desktop file manager.
    pub(super) fn open_workspace_folder(&mut self) {
        match folder_command(&self.workspace).spawn() {
            Ok(_) => self.acknowledge("Opened workspace folder".into()),
            Err(error) => self.status = Some(format!("Could not open workspace folder: {error}")),
        }
    }

    /// Shows this workspace's local backups in the system file manager, creating the folder first.
    pub(super) fn open_backups_folder(&mut self) {
        let path = crate::storage::backups_dir(&self.workspace);
        match std::fs::create_dir_all(&path).and_then(|()| folder_command(&path).spawn()) {
            Ok(_) => self.acknowledge("Opened backups folder".into()),
            Err(error) => self.status = Some(format!("Could not open backups folder: {error}")),
        }
    }

    /// Renders the editable, explicitly applied global shortcuts as one keyboard section.
    ///
    /// # Arguments
    /// - `ui` - Destination settings panel.
    /// - `palette` - Current control colors.
    pub(super) fn global_shortcut_settings_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        for action in GlobalAction::ALL {
            if action != GlobalAction::Show {
                ui.add_space(12.0);
            }
            self.global_shortcut_card(ui, palette, action);
        }
    }

    fn global_shortcut_card(&mut self, ui: &mut egui::Ui, palette: Palette, action: GlobalAction) {
        let (description, letter, field_id, field_name) = match action {
            GlobalAction::Show => (
                "Show this window from another app. It always reveals the window; pressing it again keeps it visible.",
                "L",
                "settings-global-shortcut",
                "Global reveal shortcut",
            ),
            GlobalAction::QuickAdd => (
                "Open Quick add from another app. It shows the window first, then waits for the task text.",
                "N",
                "settings-quick-add-shortcut",
                "Global quick add shortcut",
            ),
        };
        let font = self.font_id(14.0);
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich(action.title(), 18.0, palette.text));
            ui.add(Label::new(self.rich(description, 12.0, palette.muted)).wrap());
            ui.add(
                Label::new(self.rich(
                    format!(
                        "Configured: {}",
                        crate::hotkey::label(action.configured(&self.settings))
                    ),
                    13.0,
                    palette.accent,
                ))
                .wrap(),
            );
            let field = Self::flat_text_input(
                ui,
                Id::new(field_id),
                &mut self.shortcut_inputs[action],
                &format!("Ctrl+Alt+Shift+{letter}"),
                font,
                palette,
            );
            field.widget_info(|| {
                egui::WidgetInfo::text_edit(
                    true,
                    &self.shortcut_inputs[action],
                    &self.shortcut_inputs[action],
                    field_name,
                )
            });
            self.keyboard.settings_rows.push(vec![field.id]);
            ui.add(
                Label::new(self.rich(
                    if cfg!(target_os = "macos") {
                        format!(
                            "Example: Cmd+Option+Shift+{letter}. Use Ctrl or Cmd with another modifier."
                        )
                    } else {
                        format!("Example: Ctrl+Alt+Shift+{letter}. Use Ctrl with another modifier.")
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
                    !action.configured(&self.settings).is_empty(),
                    Button::new("Disable shortcut"),
                );
                self.keyboard.settings_rows.push(vec![apply.id, disable.id]);
                if disable.clicked() {
                    self.shortcut_inputs[action].clear();
                }
                if apply.clicked() || disable.clicked() || submit {
                    self.apply_global_shortcut(ui.ctx(), action);
                    field.request_focus();
                }
            });
        });
    }

    fn apply_global_shortcut(&mut self, ctx: &egui::Context, action: GlobalAction) {
        let requested = self.shortcut_inputs[action].trim().to_owned();
        if let Err(error) = crate::hotkey::parse_for(action, &requested, &self.settings) {
            self.report_shortcut_error(action, error);
            return;
        }
        if self.global_shortcut.is_none() && !requested.is_empty() {
            match crate::hotkey::GlobalShortcut::new(ctx) {
                Ok(shortcut) => self.global_shortcut = Some(shortcut),
                Err(error) => {
                    self.report_shortcut_error(action, error);
                    return;
                }
            }
        }
        if let Some(shortcut) = self.global_shortcut.as_mut() {
            if let Err(error) = shortcut.configure(action, &requested) {
                self.report_shortcut_error(action, error);
                return;
            }
            shortcut.attempted[action] = Some(requested.clone());
        }
        if let Some(error) = self.shortcut_errors[action].take()
            && self.status.as_ref() == Some(&error)
        {
            self.status = None;
        }
        *action.configured_mut(&mut self.settings) = requested;
        self.save_settings();
        if !self.persistence.settings_dirty {
            self.acknowledge(format!(
                "{}: {}",
                action.title(),
                crate::hotkey::label(action.configured(&self.settings))
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
        for action in GlobalAction::ALL {
            let (ctx, mut app) = crate::app::tests::test_app();
            let previous = action.configured(&app.settings).to_owned();
            app.shortcut_inputs[action] = "KeyQ".into();
            app.apply_global_shortcut(&ctx, action);
            assert_eq!(action.configured(&app.settings), previous);
            assert!(app.global_shortcut.is_none());
            assert!(app.status.as_ref().unwrap().contains("modifier"));
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn a_chord_used_by_the_other_action_is_rejected_before_native_registration() {
        for action in GlobalAction::ALL {
            let (ctx, mut app) = crate::app::tests::test_app();
            let previous = action.configured(&app.settings).to_owned();
            let taken = action.other().configured(&app.settings).to_owned();
            app.shortcut_inputs[action] = crate::hotkey::input_text(&taken);
            app.apply_global_shortcut(&ctx, action);
            let status = app.status.as_deref().unwrap();
            assert!(status.starts_with("Already used by the "), "{status}");
            assert!(status.contains(&action.other().title().to_lowercase()));
            assert_eq!(action.configured(&app.settings), previous);
            assert_eq!(action.other().configured(&app.settings), taken);
            assert!(app.global_shortcut.is_none());
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn quick_add_shortcut_can_be_changed_and_disabled_independently() {
        let (ctx, mut app) = crate::app::tests::test_app();
        let reveal = app.settings.global_shortcut.clone();
        app.shortcut_inputs[GlobalAction::QuickAdd].clear();
        app.apply_global_shortcut(&ctx, GlobalAction::QuickAdd);
        assert!(app.settings.quick_add_shortcut.is_empty());
        assert_eq!(app.settings.global_shortcut, reveal);
        assert!(app.global_shortcut.is_none());
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    #[test]
    fn a_successful_apply_clears_only_its_own_resolved_shortcut_error() {
        for action in GlobalAction::ALL {
            let (ctx, mut app) = crate::app::tests::test_app();
            for unrelated in [None, Some("Unrelated save error")] {
                app.shortcut_inputs[action] = "KeyQ".into();
                app.apply_global_shortcut(&ctx, action);
                assert!(app.status.as_ref().unwrap().contains("modifier"));
                if let Some(error) = unrelated {
                    app.status = Some(error.into());
                }
                // Disabling succeeds without native registration.
                app.shortcut_inputs[action].clear();
                app.apply_global_shortcut(&ctx, action);
                assert_eq!(app.status.as_deref(), unrelated);
            }
            // The other action's pending error is not resolved by this apply.
            let other = action.other();
            app.shortcut_inputs[other] = "KeyQ".into();
            app.apply_global_shortcut(&ctx, other);
            let error = app.status.clone().unwrap();
            app.shortcut_inputs[action].clear();
            app.apply_global_shortcut(&ctx, action);
            assert_eq!(app.status, Some(error));
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }
}
