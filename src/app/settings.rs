//! In-app workspace, appearance, and list settings.

use super::{MinimalistApp, Screen, text_input_shortcuts, theme::Palette};
use crate::model::{Accent, FontChoice, TitleOverflow};
use crate::storage;
use eframe::egui;
use egui::{Align, Button, CursorIcon, Id, Label, Layout, ScrollArea, Sense, Stroke};
use rfd::FileDialog;
use uuid::Uuid;

impl MinimalistApp {
    /// Renders global settings and, when selected, controls for a specific list.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the settings screen.
    /// - `palette` - Colors used to render the settings controls.
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.keyboard.settings_rows.clear();
        self.keyboard.settings_sections.clear();
        ui.scope(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                    .clicked()
                {
                    self.close_settings();
                }
                let title_response = ui
                    .add(
                        Label::new(self.rich(
                            if self.settings_list_id.is_some() {
                                "List options"
                            } else {
                                "Settings"
                            },
                            28.0,
                            palette.text,
                        ))
                        .sense(Sense::click_and_drag()),
                    )
                    .on_hover_cursor(CursorIcon::Grab);
                if title_response.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.settings.window_decorations
                        && ui
                            .add(Button::new(self.rich("x", 18.0, palette.muted)).frame(false))
                            .on_hover_text(self.close_hint())
                            .clicked()
                    {
                        self.close_window(ui.ctx());
                    }
                });
            });
            for hint in [
                "Up/Down: rows · Left/Right: choose or adjust",
                if cfg!(target_os = "macos") {
                    "Fn+Up/Down: sections · Esc: back"
                } else {
                    "Page Up/Down: sections · Esc: back"
                },
            ] {
                ui.add(Label::new(self.rich(hint, 12.0, palette.muted)).wrap());
            }
        });
        ui.add_space(18.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.settings_contents(ui, palette);
                self.keyboard_focus_ui(ui, palette);
            });
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let target = self.settings_list_id.or(match self.return_screen {
            Some(Screen::List(id)) => Some(id),
            _ => None,
        });
        let list_mode = self.settings_list_id.is_some();
        if let Some(list) = target {
            self.keyboard
                .settings_sections
                .push(self.keyboard.settings_rows.len());
            let label = if list_mode {
                "App settings"
            } else {
                "Current list options"
            };
            let switch = ui
                .scope(|ui| {
                    ui.add(Button::new(self.rich(label, 14.0, palette.accent)).frame(false))
                })
                .inner;
            self.keyboard.settings_rows.push(vec![switch.id]);
            if switch.clicked() {
                if list_mode {
                    self.rename_list(list);
                    self.settings_list_id = None;
                    self.keyboard.pending = Some(Id::new("settings-workspace"));
                } else if let Some(index) = self.list_index(list) {
                    self.settings_list_id = Some(list);
                    self.rename_input = self.lists[index].data.title.clone();
                    self.rename_baseline.clone_from(&self.rename_input);
                    self.keyboard.pending = Some(Id::new(("settings-list-name", list)));
                }
                self.delete_armed = false;
                self.clear_history_armed = false;
            }
            ui.add_space(8.0);
        }
        if let Some(list) = self.settings_list_id {
            self.list_settings_contents(ui, palette, list);
        } else {
            self.app_settings_contents(ui, palette);
        }
    }

    fn app_settings_contents(&mut self, ui: &mut egui::Ui, palette: Palette) {
        ui.scope(|ui| {
            ui.label(self.rich("App settings", 20.0, palette.text));
            ui.label(self.rich(
                concat!("Version ", env!("CARGO_PKG_VERSION")),
                12.0,
                palette.muted,
            ));
            ui.add(
                Label::new(self.rich(
                    "Workspace, window, appearance, and shortcuts apply to every list.",
                    12.0,
                    palette.muted,
                ))
                .wrap(),
            );
        });
        ui.add_space(8.0);
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        let workspace_font = self.font_id(15.0);
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Workspace", 18.0, palette.text));
            ui.add(
                egui::Label::new(self.rich(
                    self.workspace.display().to_string(),
                    12.0,
                    palette.accent,
                ))
                .wrap(),
            );
            ui.label(self.rich(
                "One readable JSON file per list. Any shared folder can sync it.",
                13.0,
                palette.muted,
            ));
            ui.add_space(5.0);
            let workspace_response = Self::flat_text_input(
                ui,
                Id::new("settings-workspace"),
                &mut self.workspace_input,
                "Workspace folder",
                workspace_font,
                palette,
            );
            self.keyboard
                .settings_rows
                .push(vec![workspace_response.id]);
            ui.horizontal_wrapped(|ui| {
                let choose_folder = ui.add(
                    Button::new(self.rich("Choose folder", 15.0, palette.accent)).frame(false),
                );
                let use_folder = ui.add(
                    Button::new(self.rich("Use typed path", 15.0, palette.muted)).frame(false),
                );
                let open_folder = ui.button("Open workspace folder");
                let open_backups = ui.button("Open backups folder");
                self.keyboard.settings_rows.push(vec![
                    choose_folder.id,
                    use_folder.id,
                    open_folder.id,
                    open_backups.id,
                ]);
                if open_folder.clicked() {
                    self.open_workspace_folder();
                }
                if open_backups.clicked() {
                    self.open_backups_folder();
                }
                ui.label(self.rich("Enter applies", 12.0, palette.muted));
                let (submit, _) = text_input_shortcuts(ui, &workspace_response);
                // Enter submits path edits. Explicitly using the current folder
                // can still persist a launch-only workspace selection.
                if choose_folder.clicked() {
                    if let Some(path) = FileDialog::new()
                        .set_title("Choose Minimalist List workspace")
                        .set_directory(&self.workspace)
                        .pick_folder()
                    {
                        self.switch_workspace(&path.to_string_lossy());
                    }
                } else if use_folder.clicked()
                    || (submit && self.workspace_input != self.settings.workspace_path)
                {
                    // An unchanged field holds the current literal path.
                    let path = if self.workspace_input == self.settings.workspace_path {
                        self.workspace_input.clone()
                    } else {
                        storage::parse_workspace_input(&self.workspace_input)
                    };
                    self.switch_workspace(&path);
                } else if submit {
                    workspace_response.request_focus();
                }
            });
        });

        ui.add_space(12.0);
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        let toggle_font = self.font_id(15.0);
        let mut window_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Window", 18.0, palette.text));
            ui.add_space(4.0);
            let pin = Self::toggle_row(
                ui,
                Id::new("settings-always-on-top"),
                "Always on top",
                &mut self.settings.always_on_top,
                toggle_font.clone(),
                palette,
            );
            self.keyboard.settings_rows.push(vec![pin.id]);
            window_changed |= pin.changed();
            let decorations = Self::toggle_row(
                ui,
                Id::new("settings-window-decorations"),
                "Show window title bar",
                &mut self.settings.window_decorations,
                toggle_font,
                palette,
            );
            self.keyboard.settings_rows.push(vec![decorations.id]);
            window_changed |= decorations.changed();
            ui.add_space(2.0);
            ui.label(self.rich(
                "With the title bar hidden, drag the list title to move the window and use x to close it.",
                12.0,
                palette.muted,
            ));
            if self.tray.is_some() {
                let hint = if cfg!(target_os = "macos") {
                    "Close hides the window. Use the menu-bar icon to Show or Quit."
                } else if cfg!(target_os = "linux") {
                    "Close, Ctrl+W, or minimize hides the window to the tray. Some desktops, including GNOME, disable minimize for windows hidden from the taskbar. Use the tray menu to Show or Quit."
                } else {
                    "Close or minimize hides the window. Use the tray menu to Show or Quit."
                };
                ui.label(self.rich(hint, 12.0, palette.muted));
            }
        });
        if window_changed {
            self.last_pin_state = None;
            self.last_decorations_state = None;
            self.save_settings();
        }

        ui.add_space(12.0);
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        let mut appearance_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Appearance", 18.0, palette.text));
            ui.spacing_mut().slider_width = ui.available_width();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let mut choices = Vec::new();
                for (choice, label) in [(FontChoice::Sans, "Sans"), (FontChoice::Mono, "Mono")] {
                    let selected = self.settings.font == choice;
                    let response = ui.add(
                        Button::new(self.rich(label, 14.0, palette.text))
                            .selected(selected)
                            .fill(if selected {
                                palette.surface.lerp_to_gamma(palette.accent, 0.34)
                            } else {
                                palette.raised
                            })
                            .stroke(Stroke::new(
                                if selected { 1.5 } else { 0.0 },
                                palette.accent,
                            ))
                            .corner_radius(12.0),
                    );
                    choices.push(response.id);
                    if response.clicked() && !selected {
                        self.settings.font = choice;
                        appearance_changed = true;
                    }
                }
                let selected = self.settings.bold_text;
                let bold = ui.add(
                    Button::new(self.rich("Bold", 14.0, palette.text))
                        .selected(selected)
                        .fill(if selected {
                            palette.surface.lerp_to_gamma(palette.accent, 0.34)
                        } else {
                            palette.raised
                        })
                        .stroke(Stroke::new(
                            if selected { 1.5 } else { 0.0 },
                            palette.accent,
                        ))
                        .corner_radius(12.0),
                );
                choices.push(bold.id);
                self.keyboard.settings_rows.push(choices);
                if bold.clicked() {
                    self.settings.bold_text = !self.settings.bold_text;
                    appearance_changed = true;
                }
            });

            ui.add_space(10.0);
            let text_size_label = ui
                .horizontal(|ui| {
                    let label = ui.label(self.rich("Text size", 14.0, palette.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(self.rich(
                            format!("{:.0}", self.settings.font_size),
                            13.0,
                            palette.accent,
                        ));
                    });
                    label
                })
                .inner;
            let text_size = ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.font_size, 14.0..=30.0)
                        .step_by(1.0)
                        .clamping(egui::SliderClamping::Edits)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .labelled_by(text_size_label.id);
            self.keyboard.settings_rows.push(vec![text_size.id]);
            text_size.widget_info(|| {
                egui::WidgetInfo::slider(
                    ui.is_enabled(),
                    self.settings.font_size as f64,
                    "Text size",
                )
            });
            appearance_changed |= text_size.changed();

            ui.add_space(6.0);
            let row_spacing_label = ui
                .horizontal(|ui| {
                    let label = ui.label(self.rich("Row spacing", 14.0, palette.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(self.rich(
                            format!("{:.0}", self.settings.row_padding),
                            13.0,
                            palette.accent,
                        ));
                    });
                    label
                })
                .inner;
            let row_spacing = ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.row_padding, 4.0..=24.0)
                        .step_by(1.0)
                        .clamping(egui::SliderClamping::Edits)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .labelled_by(row_spacing_label.id);
            self.keyboard.settings_rows.push(vec![row_spacing.id]);
            row_spacing.widget_info(|| {
                egui::WidgetInfo::slider(
                    ui.is_enabled(),
                    self.settings.row_padding as f64,
                    "Row spacing",
                )
            });
            appearance_changed |= row_spacing.changed();

            ui.add_space(10.0);
            ui.label(self.rich("Long titles", 14.0, palette.text));
            ui.horizontal_wrapped(|ui| {
                let scroll = ui.selectable_value(
                    &mut self.settings.title_overflow,
                    TitleOverflow::Scroll,
                    "Scroll",
                );
                let wrap = ui.selectable_value(
                    &mut self.settings.title_overflow,
                    TitleOverflow::Wrap,
                    "Wrap",
                );
                self.keyboard.settings_rows.push(vec![scroll.id, wrap.id]);
                appearance_changed |= scroll.changed() || wrap.changed();
            });
            ui.label(self.rich(
                match self.settings.title_overflow {
                    TitleOverflow::Scroll => "One line. Hover to scroll long titles.",
                    TitleOverflow::Wrap => "Full titles wrap onto additional lines.",
                },
                12.0,
                palette.muted,
            ));

            ui.add_space(10.0);
            let opacity_label = ui
                .horizontal(|ui| {
                    let label = ui.label(self.rich("Background opacity", 14.0, palette.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(self.rich(
                            format!("{:.0}%", self.settings.background_opacity * 100.0),
                            13.0,
                            palette.accent,
                        ));
                    });
                    label
                })
                .inner;
            let mut opacity_percent = self.settings.background_opacity * 100.0;
            let opacity = ui
                .add(
                    egui::Slider::new(&mut opacity_percent, 20.0..=100.0)
                        .step_by(1.0)
                        .clamping(egui::SliderClamping::Edits)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .labelled_by(opacity_label.id);
            self.keyboard.settings_rows.push(vec![opacity.id]);
            opacity.widget_info(|| {
                egui::WidgetInfo::slider(
                    ui.is_enabled(),
                    opacity_percent as f64,
                    "Background opacity",
                )
            });
            if opacity.changed() {
                self.settings.background_opacity = opacity_percent / 100.0;
            }
            appearance_changed |= opacity.changed();
            ui.add_space(10.0);
            let preview_font = self.font_id(15.0);
            let previews = Self::toggle_row(
                ui,
                Id::new("settings-show-subtasks"),
                "Show subtasks below tasks",
                &mut self.settings.show_subtasks,
                preview_font,
                palette,
            );
            self.keyboard.settings_rows.push(vec![previews.id]);
            appearance_changed |= previews.changed();
        });
        if appearance_changed {
            self.schedule_settings_save(ui.ctx());
        }
        ui.add_space(12.0);
        self.global_shortcut_settings_ui(ui, palette);
    }

    fn list_settings_contents(&mut self, ui: &mut egui::Ui, palette: Palette, list_id: Uuid) {
        self.keyboard
            .settings_sections
            .push(self.keyboard.settings_rows.len());
        ui.add_space(12.0);
        self.task_sort_menu(ui, list_id);
        let rename_font = self.font_id(16.0);
        let list_name = self
            .list_index(list_id)
            .map(|index| self.lists[index].data.title.clone())
            .unwrap_or_default();
        Self::surface_card(ui, palette, |ui| {
            ui.add(
                Label::new(self.rich(format!("List options: {list_name}"), 18.0, palette.text))
                    .wrap(),
            );
            ui.add(
                Label::new(self.rich(
                    "Name and accent affect only this list.",
                    12.0,
                    palette.muted,
                ))
                .wrap(),
            );
            ui.add_space(5.0);
            let rename_response = Self::flat_text_input(
                ui,
                Id::new(("settings-list-name", list_id)),
                &mut self.rename_input,
                "List name",
                rename_font,
                palette,
            );
            self.keyboard.settings_rows.push(vec![rename_response.id]);
            let (submit, _) = text_input_shortcuts(ui, &rename_response);
            if rename_response.lost_focus() || submit {
                self.rename_list(list_id);
            }
            ui.add_space(5.0);
            ui.horizontal_wrapped(|ui| {
                let mut choices = Vec::new();
                for accent in Accent::ALL {
                    let selected = self
                        .list_index(list_id)
                        .is_some_and(|index| self.lists[index].data.accent == accent);
                    let response = ui.add(self.accent_choice_button(accent, selected));
                    choices.push(response.id);
                    if response.clicked() {
                        self.set_accent(list_id, accent);
                    }
                }
                self.keyboard.settings_rows.push(choices);
            });
            ui.add_space(10.0);
            self.clear_history_controls(ui, palette, list_id);
            if self.delete_armed
                && let Some(index) = self.list_index(list_id)
            {
                let data = &self.lists[index].data;
                ui.add(
                    Label::new(self.rich(
                        format!("Delete “{}”?", data.title),
                        15.0,
                        palette.danger,
                    ))
                    .wrap(),
                );
                let children = data
                    .tasks
                    .iter()
                    .chain(data.archive.iter().map(|item| &item.task))
                    .map(|task| task.subtasks.len())
                    .sum::<usize>();
                ui.add(Label::new(self.rich(format!("{} active tasks, {} completed tasks, {} archived tasks, and {children} subtasks will be removed. A copy is kept in local backups.", data.active_count(), data.completed_count(), data.archive.len()), 13.0, palette.text)).wrap());
            }
            let text = if self.delete_armed {
                "Confirm delete list"
            } else {
                "Delete list"
            };
            // Confirmation inserts explanatory text above this control; its
            // identity must survive that layout change while a key is held.
            let delete = ui
                .scope_builder(
                    egui::UiBuilder::new().id(Id::new(("delete-list", list_id))),
                    |ui| ui.add(Button::new(self.rich(text, 14.0, palette.danger)).frame(false)),
                )
                .inner;
            self.keyboard.settings_rows.push(vec![delete.id]);
            if delete.clicked() {
                if self.delete_armed {
                    self.delete_list(list_id);
                } else {
                    self.delete_armed = true;
                    self.clear_history_armed = false;
                }
            }
            if self.delete_armed {
                let cancel = ui.add(
                    Button::new(self.rich("Cancel deletion", 14.0, palette.muted)).frame(false),
                );
                self.keyboard.settings_rows.push(vec![cancel.id]);
                if cancel.clicked() {
                    self.delete_armed = false;
                }
            }
        });
    }

    /// Shows Clear History above Delete list with its own two-step confirmation.
    ///
    /// Nothing is shown while the list has no archived tasks.
    fn clear_history_controls(&mut self, ui: &mut egui::Ui, palette: Palette, list_id: Uuid) {
        let archived = self
            .list_index(list_id)
            .map_or(0, |index| self.lists[index].data.archive.len());
        if archived == 0 {
            self.clear_history_armed = false;
            return;
        }
        if self.clear_history_armed {
            ui.add(
                Label::new(self.rich(
                    format!("Remove all {archived} archived tasks from History?"),
                    15.0,
                    palette.danger,
                ))
                .wrap(),
            );
        }
        let text = if self.clear_history_armed {
            "Confirm clear History"
        } else {
            "Clear History"
        };
        // The label above appears on arming; the control keeps its identity while a key is held.
        let clear = ui
            .scope_builder(
                egui::UiBuilder::new().id(Id::new(("clear-history", list_id))),
                |ui| ui.add(Button::new(self.rich(text, 14.0, palette.danger)).frame(false)),
            )
            .inner;
        self.keyboard.settings_rows.push(vec![clear.id]);
        if clear.clicked() {
            if self.clear_history_armed {
                self.clear_history_armed = false;
                let focused = clear.has_focus();
                self.clear_history(list_id);
                // The focused button leaves with the History it cleared; keep arrow keys working.
                if focused
                    && self
                        .list_index(list_id)
                        .is_some_and(|index| self.lists[index].data.archive.is_empty())
                {
                    self.keyboard.pending = Some(Id::new(("settings-list-name", list_id)));
                }
            } else {
                self.clear_history_armed = true;
                self.delete_armed = false;
            }
        }
        if self.clear_history_armed {
            let cancel = ui.add(Button::new(self.rich("Cancel", 14.0, palette.muted)).frame(false));
            self.keyboard.settings_rows.push(vec![cancel.id]);
            if cancel.clicked() {
                self.clear_history_armed = false;
            }
        }
    }

    /// Saves a nonempty changed list name when its field is submitted or left.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn rename_list(&mut self, list_id: Uuid) {
        let title = self.rename_input.trim();
        // Only apply a local draft; external reloads may have changed the title
        // since Settings opened or this field was last submitted.
        if title.is_empty() || title == self.rename_baseline.trim() {
            return;
        }
        self.rename_baseline.clone_from(&self.rename_input);
        if let Some(index) = self.list_index(list_id)
            && self.lists[index].data.title != title
        {
            self.lists[index].data.title = title.to_owned();
            self.persistence.pending_filenames.insert(list_id);
            self.save_list_index(index);
            self.apply_list_order();
        }
    }

    fn set_accent(&mut self, list_id: Uuid, accent: Accent) {
        if let Some(index) = self.list_index(list_id)
            && self.lists[index].data.accent != accent
        {
            self.lists[index].data.accent = accent;
            self.save_list_index(index);
        }
    }

    fn delete_list(&mut self, list_id: Uuid) {
        if self.lists.len() <= 1 {
            self.status = Some("At least one list must remain.".to_owned());
            return;
        }
        if let Some(index) = self.list_index(list_id) {
            let neighbors = self.focus_neighbors();
            let list = self.lists.remove(index);
            match storage::delete_list(&self.workspace, &list) {
                Ok(()) => {
                    self.forget_save_conflict(list_id);
                    self.persistence.dirty_lists.remove(&list_id);
                    self.persistence.pending_filenames.remove(&list_id);
                    storage::remove_file_from_fingerprint(&mut self.fingerprint, &list.path);
                    self.prune_list_preferences();
                    self.manual_task_order.remove(&list_id);
                    self.settings.last_list_id = self.lists.first().map(|item| item.key);
                    self.save_settings();
                    self.go_to_overview();
                    self.return_screen = None;
                    self.settings_list_id = None;
                    self.repair_focus(&neighbors);
                }
                Err(error) => {
                    self.status = Some(error);
                    self.lists.insert(index, list);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
