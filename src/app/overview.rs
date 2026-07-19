use super::{MinimalistApp, theme::Palette};
use crate::model::Accent;
use crate::storage;
use eframe::egui;
use egui::{
    Align, Button, Color32, CornerRadius, CursorIcon, Frame, Id, Key, Label, Layout, Margin, Rect,
    ScrollArea, Sense, Stroke, Vec2,
};
use uuid::Uuid;

impl MinimalistApp {
    pub(super) fn overview_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.header(ui, palette, "Lists", None);
        ui.add_space(12.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let columns = if width > 760.0 {
                    3
                } else if width > 460.0 {
                    2
                } else {
                    1
                };
                let card_width = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;

                let cards: Vec<(Uuid, String, Accent, usize, usize, Vec<String>)> = self
                    .lists
                    .iter()
                    .map(|item| {
                        (
                            item.key,
                            item.data.title.clone(),
                            item.data.accent,
                            item.data.active_count(),
                            item.data.completed_count(),
                            item.data
                                .tasks
                                .iter()
                                .filter(|task| !task.completed)
                                .take(3)
                                .map(|task| task.text.clone())
                                .collect(),
                        )
                    })
                    .collect();

                for row in cards.chunks(columns) {
                    ui.horizontal(|ui| {
                        for (id, title, accent, active, completed, preview) in row {
                            let (response, settings_clicked) = self.list_card(
                                ui, *id, title, *accent, *active, *completed, preview, card_width,
                            );
                            if settings_clicked {
                                self.open_settings(Some(*id));
                            } else if response.clicked() {
                                self.open_list(*id);
                            }
                        }
                    });
                    ui.add_space(12.0);
                }

                let create_progress = ui.ctx().animate_bool_with_time_and_easing(
                    Id::new("new-list-panel"),
                    self.creating_list,
                    0.20,
                    egui::emath::easing::cubic_out,
                );
                if create_progress > 0.001 {
                    ui.add_space(8.0 * create_progress);
                    let input_font = self.font_id(18.0);
                    Frame::NONE
                        .fill(
                            palette
                                .surface
                                .lerp_to_gamma(palette.raised, (1.0 - create_progress) * 0.18),
                        )
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.set_opacity(create_progress);
                            ui.add_space((1.0 - create_progress) * 8.0);
                            let response = Self::flat_text_input(
                                ui,
                                Id::new("new-list-title"),
                                &mut self.new_list_title,
                                "List name",
                                input_font,
                                palette,
                            );
                            if self.request_list_focus {
                                response.request_focus();
                                self.request_list_focus = false;
                            }
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                for accent in Accent::ALL {
                                    let p = Palette::from_accent(accent);
                                    let selected = accent == self.new_list_accent;
                                    let response = ui.add(
                                        Button::new(self.rich(accent.label(), 13.0, p.text))
                                            .fill(if selected { p.raised } else { p.surface })
                                            .stroke(Stroke::new(
                                                if selected { 2.0 } else { 0.0 },
                                                p.accent,
                                            ))
                                            .corner_radius(12.0),
                                    );
                                    if response.clicked() {
                                        self.new_list_accent = accent;
                                    }
                                }
                            });
                            ui.label(self.rich("Enter creates · Esc cancels", 12.0, palette.muted));
                            let (submit, cancel) = ui.input(|input| {
                                (
                                    input.key_pressed(Key::Enter)
                                        && (response.has_focus() || response.lost_focus()),
                                    input.key_pressed(Key::Escape),
                                )
                            });
                            if submit {
                                self.create_list();
                            } else if cancel {
                                self.new_list_title.clear();
                                self.creating_list = false;
                            }
                        });
                } else if ui
                    .add(Button::new(self.rich("+ New list", 18.0, palette.accent)).frame(false))
                    .clicked()
                {
                    self.creating_list = true;
                    self.request_list_focus = true;
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn list_card(
        &self,
        ui: &mut egui::Ui,
        id: Uuid,
        title: &str,
        accent: Accent,
        active: usize,
        completed: usize,
        preview: &[String],
        width: f32,
    ) -> (egui::Response, bool) {
        let card_palette = Palette::from_accent(accent);
        let (slot_rect, response) = ui.allocate_exact_size(Vec2::new(width, 166.0), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let hover = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("list-card-hover", id)),
            response.hovered(),
            0.18,
            egui::emath::easing::cubic_out,
        );
        let pressed = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("list-card-pressed", id)),
            response.is_pointer_button_down_on(),
            0.08,
            egui::emath::easing::quadratic_out,
        );
        let lift = hover * 3.5 - pressed * 1.5;
        let card_rect = slot_rect
            .shrink2(Vec2::new(2.0 + hover, 3.0 + hover))
            .translate(Vec2::new(0.0, -lift));

        let shadow = egui::epaint::Shadow {
            offset: [0, 5],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha((44.0 * hover) as u8),
        };
        ui.painter()
            .add(shadow.as_shape(card_rect, CornerRadius::same(18)));
        ui.painter().rect(
            card_rect,
            18.0,
            card_palette
                .surface
                .lerp_to_gamma(card_palette.raised, hover * 0.48),
            Stroke::new(
                1.0,
                card_palette.accent.linear_multiply(0.14 + hover * 0.46),
            ),
            egui::StrokeKind::Inside,
        );

        let content_rect = card_rect.shrink2(Vec2::new(18.0, 16.0));
        let mut card_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("list-card-content", id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        card_ui.set_opacity(0.88 + hover * 0.12);
        card_ui.add(Label::new(self.rich(title, 22.0, card_palette.text)).wrap());
        card_ui.add_space(8.0);
        for line in preview {
            card_ui.add(
                Label::new(self.rich(format!("— {line}"), 15.0, card_palette.muted)).truncate(),
            );
        }
        if preview.is_empty() {
            card_ui.label(self.rich("Empty", 15.0, card_palette.muted));
        }
        card_ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.label(self.rich(
                format!("{active} active / {completed} done"),
                13.0,
                card_palette.accent,
            ));
        });

        let settings_rect = Rect::from_center_size(
            card_rect.right_top() + Vec2::new(-18.0, 18.0),
            Vec2::splat(30.0),
        );
        let settings = ui
            .put(
                settings_rect,
                Button::new(self.rich(
                    "...",
                    14.0,
                    card_palette.muted.linear_multiply(0.24 + hover * 0.76),
                ))
                .frame(false),
            )
            .on_hover_text("List settings");

        (response, settings.clicked())
    }

    fn create_list(&mut self) {
        let title = self.new_list_title.trim();
        if title.is_empty() {
            return;
        }
        match storage::create_list(&self.workspace, title, self.new_list_accent) {
            Ok(list) => {
                let id = list.key;
                let path = list.path.clone();
                self.lists.push(list);
                self.new_list_title.clear();
                self.creating_list = false;
                if let Err(error) =
                    storage::update_fingerprint_for_file(&mut self.fingerprint, &path)
                {
                    self.status = Some(error);
                }
                self.open_list(id);
            }
            Err(error) => self.status = Some(error),
        }
    }
}
