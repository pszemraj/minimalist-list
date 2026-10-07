//! Accent-derived colors and shared rendering helpers.

use super::MinimalistApp;
use crate::model::{Accent, FontChoice};
use eframe::egui;
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Label,
    Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Vec2,
};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq)]
/// Complete set of colors used to render one accent theme.
pub(super) struct Palette {
    pub(super) background: Color32,
    pub(super) surface: Color32,
    pub(super) raised: Color32,
    pub(super) accent: Color32,
    pub(super) text: Color32,
    pub(super) muted: Color32,
    pub(super) danger: Color32,
}

impl Palette {
    /// Builds the dark palette associated with an accent choice.
    ///
    /// # Returns
    ///
    /// The complete palette associated with `accent`.
    pub(super) fn from_accent(accent: Accent) -> Self {
        match accent {
            Accent::Mint => Self::new((22, 38, 35), (31, 53, 48), (41, 67, 60), (137, 220, 187)),
            Accent::Charcoal => {
                Self::new((27, 29, 33), (39, 42, 47), (52, 56, 62), (200, 205, 215))
            }
            Accent::Crimson => Self::new((45, 24, 30), (65, 33, 41), (82, 43, 52), (235, 142, 159)),
            Accent::Sand => Self::new((43, 38, 28), (61, 53, 37), (78, 67, 46), (228, 194, 127)),
            Accent::Sky => Self::new((24, 35, 47), (34, 50, 67), (44, 64, 85), (137, 193, 235)),
            Accent::Lavender => {
                Self::new((35, 28, 46), (50, 40, 64), (65, 52, 82), (187, 158, 233))
            }
        }
    }

    fn new(
        background: (u8, u8, u8),
        surface: (u8, u8, u8),
        raised: (u8, u8, u8),
        accent: (u8, u8, u8),
    ) -> Self {
        Self {
            background: Color32::from_rgb(background.0, background.1, background.2),
            surface: Color32::from_rgb(surface.0, surface.1, surface.2).linear_multiply(0.12),
            raised: Color32::from_rgb(raised.0, raised.1, raised.2).linear_multiply(0.18),
            accent: Color32::from_rgb(accent.0, accent.1, accent.2),
            text: Color32::from_rgb(242, 244, 243),
            muted: Color32::from_rgb(158, 171, 166),
            danger: Color32::from_rgb(240, 116, 132),
        }
    }

    /// Interpolates every palette color toward another palette.
    ///
    /// # Arguments
    ///
    /// - `other` - Target palette.
    /// - `amount` - Blend amount, clamped from zero to one.
    ///
    /// # Returns
    ///
    /// A palette containing the interpolated colors.
    pub(super) fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            background: self.background.lerp_to_gamma(other.background, amount),
            surface: self.surface.lerp_to_gamma(other.surface, amount),
            raised: self.raised.lerp_to_gamma(other.raised, amount),
            accent: self.accent.lerp_to_gamma(other.accent, amount),
            text: self.text.lerp_to_gamma(other.text, amount),
            muted: self.muted.lerp_to_gamma(other.muted, amount),
            danger: self.danger.lerp_to_gamma(other.danger, amount),
        }
    }

    /// Applies a shared opacity multiplier to every palette color.
    ///
    /// # Returns
    ///
    /// A palette whose colors use the clamped opacity.
    pub(super) fn fade(self, opacity: f32) -> Self {
        let opacity = opacity.clamp(0.0, 1.0);
        Self {
            background: self.background.linear_multiply(opacity),
            surface: self.surface.linear_multiply(opacity),
            raised: self.raised.linear_multiply(opacity),
            accent: self.accent.linear_multiply(opacity),
            text: self.text.linear_multiply(opacity),
            muted: self.muted.linear_multiply(opacity),
            danger: self.danger.linear_multiply(opacity),
        }
    }
}

impl MinimalistApp {
    /// Creates a font identifier using the configured font family.
    ///
    /// # Returns
    ///
    /// A font identifier with the requested size and configured family.
    pub(super) fn font_id(&self, size: f32) -> FontId {
        let family = match self.settings.font {
            FontChoice::Sans => FontFamily::Proportional,
            FontChoice::Mono => FontFamily::Monospace,
        };
        FontId::new(size, family)
    }

    /// Styles text with the configured family, weight, size, and supplied color.
    ///
    /// # Arguments
    ///
    /// - `text` - Content to display.
    /// - `size` - Font size in points.
    /// - `color` - Text color.
    ///
    /// # Returns
    ///
    /// Rich text ready to pass to an egui widget.
    pub(super) fn rich(&self, text: impl Into<String>, size: f32, color: Color32) -> RichText {
        let mut rich = RichText::new(text).font(self.font_id(size)).color(color);
        if self.settings.bold_text {
            rich = rich.strong();
        }
        rich
    }

    /// Renders a single-line input with the shared flat frame and focus treatment.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the input.
    /// - `id` - Stable widget identity.
    /// - `text` - Editable input buffer.
    /// - `hint` - Placeholder shown when the buffer is empty.
    /// - `font` - Font used for input text.
    /// - `palette` - Colors used for the frame and focus treatment.
    ///
    /// # Returns
    ///
    /// The text widget's response.
    pub(super) fn flat_text_input(
        ui: &mut egui::Ui,
        id: Id,
        text: &mut String,
        hint: &str,
        font: FontId,
        palette: Palette,
    ) -> egui::Response {
        let shown = Frame::NONE
            .fill(palette.raised)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(text)
                        .id(id)
                        .font(font)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .frame(Frame::NONE),
                )
            });
        let rect = shown.response.rect;
        let response = shown.inner;
        let focus = ui.ctx().animate_bool_with_time_and_easing(
            id.with("focus"),
            response.has_focus(),
            0.16,
            egui::emath::easing::cubic_out,
        );
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            12.0,
            Stroke::new(
                1.0 + focus,
                palette
                    .surface
                    .lerp_to_gamma(palette.accent, 0.24 + focus * 0.76),
            ),
            egui::StrokeKind::Inside,
        );
        response
    }

    /// Renders a full-width animated boolean setting and reports whether it changed.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the control.
    /// - `id` - Stable animation identity.
    /// - `label` - User-facing setting name.
    /// - `value` - Boolean preference edited by the control.
    /// - `font` - Font used for the label.
    /// - `palette` - Colors used to render the control.
    ///
    /// # Returns
    ///
    /// `true` when the value changed during this frame.
    pub(super) fn toggle_row(
        ui: &mut egui::Ui,
        id: Id,
        label: &str,
        value: &mut bool,
        font: FontId,
        palette: Palette,
    ) -> bool {
        let mut response = ui.add_sized([ui.available_width(), 36.0], Button::new("").frame(false));
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label)
        });

        let hover = ui.ctx().animate_bool_with_time_and_easing(
            id.with("hover"),
            response.hovered(),
            0.14,
            egui::emath::easing::cubic_out,
        );
        let enabled = ui.ctx().animate_bool_with_time_and_easing(
            id.with("enabled"),
            *value,
            0.18,
            egui::emath::easing::cubic_out,
        );
        if hover > 0.0 {
            ui.painter().rect_filled(
                response.rect,
                10.0,
                palette.raised.linear_multiply(hover * 0.45),
            );
        }
        ui.painter().text(
            Pos2::new(response.rect.left() + 4.0, response.rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font,
            palette.text.lerp_to_gamma(palette.accent, enabled * 0.12),
        );

        let track = Rect::from_center_size(
            Pos2::new(response.rect.right() - 23.0, response.rect.center().y),
            Vec2::new(42.0, 23.0),
        );
        ui.painter().rect(
            track,
            12.0,
            palette
                .surface
                .lerp_to_gamma(palette.accent, enabled * 0.76),
            Stroke::new(
                1.0,
                palette
                    .raised
                    .lerp_to_gamma(palette.accent, 0.20 + hover * 0.42),
            ),
            egui::StrokeKind::Inside,
        );
        let knob_x = egui::lerp((track.left() + 11.5)..=(track.right() - 11.5), enabled);
        let knob_center = Pos2::new(knob_x, track.center().y);
        ui.painter().circle_filled(
            knob_center,
            8.0 + hover * 0.4,
            palette
                .text
                .lerp_to_gamma(palette.background, enabled * 0.58),
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.shrink(1.0),
                10.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }

        response.changed()
    }

    /// Renders content inside the shared rounded surface-card frame.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the card.
    /// - `palette` - Colors used for the card surface.
    /// - `add_contents` - Closure that renders the card body.
    ///
    /// # Returns
    ///
    /// The card response and the closure's return value.
    pub(super) fn surface_card<R>(
        ui: &mut egui::Ui,
        palette: Palette,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let content_width = (ui.available_width() - 32.0).max(0.0);
        Frame::NONE
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                add_contents(ui)
            })
    }

    /// Paints the animated task-completion control over an allocated response.
    ///
    /// # Arguments
    ///
    /// - `ui` - UI whose painter draws the control.
    /// - `response` - Interaction region allocated for the control.
    /// - `completed` - Current task completion state.
    /// - `progress` - Completion animation progress from zero to one.
    /// - `palette` - Colors used to paint the control.
    /// - `text` - Task or subtask text included in its accessible name.
    ///
    /// # Returns
    ///
    /// The original response with accessibility metadata and hover text attached.
    pub(super) fn completion_control(
        &self,
        ui: &mut egui::Ui,
        response: egui::Response,
        completed: bool,
        progress: f32,
        palette: Palette,
        text: &str,
    ) -> egui::Response {
        let label = if completed {
            format!("Mark active: {text}")
        } else {
            format!("Complete: {text}")
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, completed, &label)
        });

        let center = response.rect.center();
        let radius = response.rect.width().min(response.rect.height()) * 0.22;
        let hover = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("completion-hover", response.id)),
            response.hovered(),
            0.12,
            egui::emath::easing::cubic_out,
        );
        if hover > 0.0 {
            ui.painter().circle_filled(
                center,
                radius + 4.0 + hover,
                palette.raised.linear_multiply(hover),
            );
        }
        ui.painter().circle_stroke(
            center,
            radius,
            Stroke::new(
                1.8,
                palette.accent.lerp_to_gamma(palette.text, hover * 0.18),
            ),
        );
        if progress > 0.0 {
            let scale = 0.68 + progress * 0.32;
            ui.painter().circle_filled(
                center,
                radius * scale,
                palette.accent.linear_multiply(progress),
            );
            let check = palette.background.linear_multiply(progress);
            ui.painter().line_segment(
                [center + Vec2::new(-4.5, 0.0), center + Vec2::new(-1.2, 3.2)],
                Stroke::new(1.8, check),
            );
            ui.painter().line_segment(
                [center + Vec2::new(-1.2, 3.2), center + Vec2::new(5.0, -4.0)],
                Stroke::new(1.8, check),
            );
        }
        if response.has_focus() {
            ui.painter()
                .circle_stroke(center, radius + 4.0, Stroke::new(1.5, palette.text));
        }

        response.on_hover_text(label)
    }

    /// Renders the draggable screen header and its navigation actions.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the header.
    /// - `palette` - Colors used to render header controls.
    /// - `title` - Screen title.
    /// - `list_id` - Current list identity, or `None` on the overview.
    pub(super) fn header(
        &mut self,
        ui: &mut egui::Ui,
        palette: Palette,
        title: &str,
        list_id: Option<Uuid>,
    ) {
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(4.0, 4.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            if list_id.is_some()
                && ui
                    .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                    .clicked()
            {
                self.go_to_overview();
            }

            let pin = if self.settings.always_on_top {
                "pinned"
            } else {
                "pin"
            };
            let mut controls_width = [("...", 18.0), ("find", 14.0), ("add", 14.0), (pin, 14.0)]
                .into_iter()
                .map(|(text, size)| {
                    ui.painter()
                        .layout_no_wrap(text.to_owned(), self.font_id(size), palette.text)
                        .size()
                        .x
                        + ui.spacing().item_spacing.x
                })
                .sum::<f32>();
            if !self.settings.window_decorations {
                controls_width += ui
                    .painter()
                    .layout_no_wrap("x".to_owned(), self.font_id(18.0), palette.text)
                    .size()
                    .x
                    + ui.spacing().item_spacing.x;
            }
            let title_width = (ui.available_width() - controls_width).max(24.0);
            let title_response = ui
                .allocate_ui_with_layout(
                    Vec2::new(title_width, 36.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(
                            Label::new(self.rich(title, 28.0, palette.text))
                                .truncate()
                                .sense(Sense::click_and_drag()),
                        )
                    },
                )
                .inner
                .on_hover_cursor(CursorIcon::Grab);
            if title_response.double_clicked() {
                self.go_to_overview();
            } else if title_response.drag_started() {
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
                if ui
                    .add(Button::new(self.rich("...", 18.0, palette.muted)).frame(false))
                    .on_hover_text("Settings")
                    .clicked()
                {
                    self.open_settings(list_id);
                }
                if ui
                    .add(Button::new(self.rich("find", 14.0, palette.muted)).frame(false))
                    .on_hover_text("Find anything (Ctrl+F)")
                    .clicked()
                {
                    self.open_find();
                }
                if ui
                    .add(Button::new(self.rich("add", 14.0, palette.accent)).frame(false))
                    .on_hover_text("Quick add (Ctrl+N)")
                    .clicked()
                {
                    self.open_quick_capture(list_id);
                }
                if ui
                    .add(Button::new(self.rich(pin, 14.0, palette.accent)).frame(false))
                    .on_hover_text("Always on top")
                    .clicked()
                {
                    self.settings.always_on_top = !self.settings.always_on_top;
                    self.last_pin_state = None;
                    self.save_settings();
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_header_keeps_title_clear_of_controls_with_both_fonts_and_pin_states() {
        let title = "A long list title describing the upcoming research workshop";
        for font in [FontChoice::Sans, FontChoice::Mono] {
            for pinned in [false, true] {
                for decorations in [false, true] {
                    let (ctx, mut app) = super::super::tests::test_app();
                    app.settings.font = font;
                    app.settings.always_on_top = pinned;
                    app.settings.window_decorations = decorations;
                    let palette = app.palette();
                    let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(320.0, 280.0));
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |ui| {
                            let mut header = ui
                                .new_child(egui::UiBuilder::new().max_rect(viewport.shrink(16.0)));
                            app.header(&mut header, palette, title, Some(app.lists[0].key));
                        },
                    );
                    output.textures_delta.clear();
                    let text_rect = |label: &str| {
                        output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                                }
                                _ => None,
                            })
                            .unwrap()
                    };
                    let title_rect = text_rect(title);
                    for control in ["...", "find", "add", if pinned { "pinned" } else { "pin" }] {
                        let rect = text_rect(control);
                        assert!(viewport.contains_rect(rect));
                        assert!(
                            title_rect.right() < rect.left(),
                            "{font:?}, pinned={pinned}, decorations={decorations}: {title_rect:?}, {rect:?}"
                        );
                    }
                    std::fs::remove_dir_all(&app.workspace).unwrap();
                }
            }
        }
    }
}
