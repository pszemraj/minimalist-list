//! In-app workspace, appearance, and list settings.

use super::{MinimalistApp, theme::Palette};
use crate::model::{Accent, FontChoice};
use crate::storage;
use eframe::egui;
use egui::{Align, Button, CursorIcon, Id, Key, Label, Layout, ScrollArea, Sense, Stroke};
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
        ui.horizontal(|ui| {
            if ui
                .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                .clicked()
            {
                self.close_settings();
            }
            let title_response = ui
                .add(
                    Label::new(self.rich("Settings", 28.0, palette.text))
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
                        .on_hover_text("Close")
                        .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        ui.add_space(18.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.settings_contents(ui, palette));
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let workspace_font = self.font_id(15.0);
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Workspace", 18.0, palette.text));
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
            ui.horizontal(|ui| {
                let choose_folder = ui
                    .add(Button::new(self.rich("Choose folder", 15.0, palette.accent)).frame(false))
                    .clicked();
                let use_folder = ui
                    .add(Button::new(self.rich("Use typed path", 15.0, palette.muted)).frame(false))
                    .clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich("Enter applies", 12.0, palette.muted));
                });
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                if choose_folder {
                    if let Some(path) = FileDialog::new()
                        .set_title("Choose Minimalist List workspace")
                        .set_directory(&self.workspace)
                        .pick_folder()
                    {
                        self.workspace_input = path.to_string_lossy().into_owned();
                        self.switch_workspace();
                    }
                } else if use_folder
                    || (enter
                        && (workspace_response.has_focus() || workspace_response.lost_focus()))
                {
                    self.switch_workspace();
                }
            });
        });

        ui.add_space(12.0);
        let toggle_font = self.font_id(15.0);
        let mut window_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Window", 18.0, palette.text));
            ui.add_space(4.0);
            window_changed |= Self::toggle_row(
                ui,
                Id::new("settings-always-on-top"),
                "Always on top",
                &mut self.settings.always_on_top,
                toggle_font.clone(),
                palette,
            );
            window_changed |= Self::toggle_row(
                ui,
                Id::new("settings-window-decorations"),
                "Native window decorations",
                &mut self.settings.window_decorations,
                toggle_font,
                palette,
            );
            ui.add_space(2.0);
            ui.label(self.rich(
                "Without decorations, drag the title to move the window and use x to close it.",
                12.0,
                palette.muted,
            ));
        });
        if window_changed {
            self.last_pin_state = None;
            self.last_decorations_state = None;
            self.save_settings();
        }

        ui.add_space(12.0);
        let mut appearance_changed = false;
        Self::surface_card(ui, palette, |ui| {
            ui.label(self.rich("Appearance", 18.0, palette.text));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (choice, label) in [(FontChoice::Sans, "Sans"), (FontChoice::Mono, "Mono")] {
                    let selected = self.settings.font == choice;
                    let response = ui.add(
                        Button::new(self.rich(label, 14.0, palette.text))
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
                    if response.clicked() && !selected {
                        self.settings.font = choice;
                        appearance_changed = true;
                    }
                }
                let selected = self.settings.bold_text;
                if ui
                    .add(
                        Button::new(self.rich("Bold", 14.0, palette.text))
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
                    )
                    .clicked()
                {
                    self.settings.bold_text = !self.settings.bold_text;
                    appearance_changed = true;
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Text size", 14.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich(
                        format!("{:.0}", self.settings.font_size),
                        13.0,
                        palette.accent,
                    ));
                });
            });
            appearance_changed |= ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.font_size, 14.0..=30.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Row spacing", 14.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(self.rich(
                        format!("{:.0}", self.settings.row_padding),
                        13.0,
                        palette.accent,
                    ));
                });
            });
            appearance_changed |= ui
                .add_sized(
                    [ui.available_width(), 18.0],
                    egui::Slider::new(&mut self.settings.row_padding, 4.0..=24.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();
        });
        if appearance_changed {
            self.save_settings();
        }

        if let Some(list_id) = self.settings_list_id {
            ui.add_space(12.0);
            let rename_font = self.font_id(16.0);
            Self::surface_card(ui, palette, |ui| {
                ui.label(self.rich("Current list", 18.0, palette.text));
                ui.add_space(5.0);
                let rename_response = Self::flat_text_input(
                    ui,
                    Id::new(("settings-list-name", list_id)),
                    &mut self.rename_input,
                    "List name",
                    rename_font,
                    palette,
                );
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                if rename_response.lost_focus() || (enter && rename_response.has_focus()) {
                    self.rename_list(list_id);
                }
                ui.add_space(5.0);
                ui.horizontal_wrapped(|ui| {
                    for accent in Accent::ALL {
                        let accent_palette = Palette::from_accent(accent);
                        let selected = self
                            .list_index(list_id)
                            .is_some_and(|index| self.lists[index].data.accent == accent);
                        if ui
                            .add(
                                Button::new(self.rich(accent.label(), 13.0, accent_palette.text))
                                    .fill(if selected {
                                        accent_palette.raised
                                    } else {
                                        accent_palette.surface
                                    })
                                    .stroke(Stroke::new(
                                        if selected { 2.0 } else { 0.0 },
                                        accent_palette.accent,
                                    ))
                                    .corner_radius(12.0),
                            )
                            .clicked()
                        {
                            self.set_accent(list_id, accent);
                        }
                    }
                });
                ui.add_space(10.0);
                let text = if self.delete_armed {
                    "Confirm delete list"
                } else {
                    "Delete list"
                };
                if ui
                    .add(Button::new(self.rich(text, 14.0, palette.danger)).frame(false))
                    .clicked()
                {
                    if self.delete_armed {
                        self.delete_list(list_id);
                    } else {
                        self.delete_armed = true;
                    }
                }
            });
        }
    }

    fn rename_list(&mut self, list_id: Uuid) {
        let title = self.rename_input.trim();
        if title.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list_id)
            && self.lists[index].data.title != title
        {
            self.lists[index].data.title = title.to_owned();
            self.save_list_index(index);
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
            let list = self.lists.remove(index);
            match storage::delete_list(&list) {
                Ok(()) => {
                    storage::remove_file_from_fingerprint(&mut self.fingerprint, &list.path);
                    self.settings.last_list_id = self.lists.first().map(|item| item.key);
                    self.save_settings();
                    self.go_to_overview();
                    self.return_screen = None;
                    self.settings_list_id = None;
                }
                Err(error) => {
                    self.status = Some(error);
                    self.lists.insert(index, list);
                }
            }
        }
    }
}
