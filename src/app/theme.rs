//! Accent-derived colors and shared rendering helpers.

use super::MinimalistApp;
use super::keyboard::{BACK_TO_LISTS, FIND, OPEN_SETTINGS, QUICK_ADD, shortcut};
use crate::model::{Accent, FontChoice};
use eframe::egui;
use egui::{
    Align, Button, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Label, Layout,
    Margin, Pos2, Rect, RichText, Sense, Stroke, TextEdit, Vec2,
};
use uuid::Uuid;

const DEFAULT_SIZE_RECIPROCAL: f32 = 1.0 / 19.0;

/// Registers embedded bold faces while retaining egui's fallback glyph coverage.
///
/// # Panics
///
/// Panics if egui's default font definitions omit a standard font family.
pub(super) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes, base) in [
        (
            "sans-bold",
            include_bytes!("../../assets/fonts/Ubuntu-Bold.ttf").as_slice(),
            FontFamily::Proportional,
        ),
        (
            "mono-bold",
            include_bytes!("../../assets/fonts/Hack-Bold.ttf").as_slice(),
            FontFamily::Monospace,
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        let mut fallback = fonts.families[&base].clone();
        fallback.insert(0, name.to_owned());
        fonts
            .families
            .insert(FontFamily::Name(name.into()), fallback);
    }
    ctx.set_fonts(fonts);
}

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
            surface: Color32::from_rgb(surface.0, surface.1, surface.2),
            raised: Color32::from_rgb(raised.0, raised.1, raised.2),
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
        let family = match (self.settings.font, self.settings.bold_text) {
            (FontChoice::Sans, false) => FontFamily::Proportional,
            (FontChoice::Mono, false) => FontFamily::Monospace,
            (FontChoice::Sans, true) => FontFamily::Name("sans-bold".into()),
            (FontChoice::Mono, true) => FontFamily::Name("mono-bold".into()),
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
        RichText::new(text).font(self.font_id(size)).color(color)
    }

    /// Scales content text relative to its size at the default text-size setting.
    ///
    /// # Returns
    ///
    /// A font identifier scaled by the configured content size.
    pub(super) fn content_font_id(&self, base_size: f32) -> FontId {
        self.font_id(base_size * self.settings.font_size * DEFAULT_SIZE_RECIPROCAL)
    }

    /// Styles task content with the configured font, weight, and relative size.
    ///
    /// # Arguments
    ///
    /// - `text` - Content to display.
    /// - `base_size` - Size in points at the default content-size setting.
    /// - `color` - Text color.
    ///
    /// # Returns
    ///
    /// Rich text ready to render task content in an egui widget.
    pub(super) fn content_rich(
        &self,
        text: impl Into<String>,
        base_size: f32,
        color: Color32,
    ) -> RichText {
        RichText::new(text)
            .font(self.content_font_id(base_size))
            .color(color)
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
    /// The toggle response, including its keyboard identity and changed state.
    pub(super) fn toggle_row(
        ui: &mut egui::Ui,
        id: Id,
        label: &str,
        value: &mut bool,
        font: FontId,
        palette: Palette,
    ) -> egui::Response {
        let label_width = (ui.available_width() - 58.0).max(1.0);
        let galley = ui
            .painter()
            .layout(label.to_owned(), font, palette.text, label_width);
        let height = (galley.size().y + 12.0).max(36.0);
        let mut response =
            ui.add_sized([ui.available_width(), height], Button::new("").frame(false));
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
        ui.painter().galley(
            Pos2::new(
                response.rect.left() + 4.0,
                response.rect.center().y - galley.size().y * 0.5,
            ),
            galley,
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

        response
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

    /// Backs a measured text/control group while leaving surrounding gutters transparent.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the group.
    /// - `palette` - Opaque backing surface color.
    /// - `needs_backing` - Whether the window background can show the desktop.
    /// - `add_contents` - Closure rendering the grouped content.
    ///
    /// # Returns
    ///
    /// The group's measured response and the closure's result.
    pub(super) fn readable_region<R>(
        ui: &mut egui::Ui,
        palette: Palette,
        needs_backing: bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let background = ui.painter().add(egui::Shape::Noop);
        let shown = ui.scope(add_contents);
        if needs_backing {
            ui.painter().set(
                background,
                egui::Shape::rect_filled(
                    shown.response.rect.expand2(Vec2::new(4.0, 3.0)),
                    8.0,
                    palette.surface,
                ),
            );
        }
        shown
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
        Self::readable_region(ui, palette, self.settings.background_opacity < 1.0, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = Vec2::new(4.0, 4.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                if list_id.is_some()
                    && ui
                        .add(Button::new(self.rich("<", 24.0, palette.text)).frame(false))
                        .on_hover_text(format!("Back to Lists ({})", shortcut(BACK_TO_LISTS)))
                        .clicked()
                {
                    self.go_to_overview();
                }

                let pin = if self.settings.always_on_top {
                    "pinned"
                } else {
                    "pin"
                };
                // Controls precede the title in accessibility order so its width uses their actual layout.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.settings.window_decorations
                        && ui
                            .add(Button::new(self.rich("x", 18.0, palette.muted)).frame(false))
                            .on_hover_text(self.close_hint())
                            .clicked()
                    {
                        self.close_window(ui.ctx());
                    }
                    let settings = ui
                        .add(Button::new(self.rich("...", 18.0, palette.muted)).frame(false))
                        .on_hover_text(if list_id.is_some() {
                            "List options".to_owned()
                        } else {
                            format!("Settings ({})", shortcut(OPEN_SETTINGS))
                        });
                    settings.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            if list_id.is_some() {
                                "List options"
                            } else {
                                "Settings"
                            },
                        )
                    });
                    if settings.clicked() {
                        self.open_settings(list_id);
                        // Pointer openers belong to this frame, before last_focus is refreshed.
                        self.keyboard.settings_return = Some(settings.id);
                    }
                    let find = ui
                        .add(Button::new(self.rich("find", 14.0, palette.muted)).frame(false))
                        .on_hover_text(format!("Find tasks ({})", shortcut(FIND)));
                    if find.clicked() {
                        self.open_find();
                        self.keyboard.overlay_return = Some(find.id);
                    }
                    let add = ui
                        .add(Button::new(self.rich("add", 14.0, palette.accent)).frame(false))
                        .on_hover_text(format!("Quick add ({})", shortcut(QUICK_ADD)));
                    if add.clicked() {
                        self.open_quick_capture(list_id);
                        self.keyboard.overlay_return = Some(add.id);
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

                    let title_response = ui
                        .allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), 36.0),
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
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translucent_header_text_has_opaque_measured_backing_and_clear_gutters() {
        let (ctx, mut app) = super::super::tests::test_app();
        app.settings.background_opacity = 0.2;
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(320.0, 520.0));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |ui| {
                let mut content =
                    ui.new_child(egui::UiBuilder::new().max_rect(viewport.shrink(16.0)));
                app.header(&mut content, app.palette(), "Lists", None);
            },
        );
        output.textures_delta.clear();
        for (index, shape) in output.shapes.iter().enumerate() {
            if let egui::epaint::Shape::Text(text) = &shape.shape {
                let text_rect = Rect::from_min_size(text.pos, text.galley.size());
                assert!(output.shapes[..index].iter().any(|shape| matches!(&shape.shape,
                    egui::epaint::Shape::Rect(rect) if rect.fill.a() == 255 && rect.rect.contains_rect(text_rect)
                )), "No opaque backing under {}", text.galley.text());
            }
        }
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::epaint::Shape::Rect(rect) if rect.fill.a() == 255 && rect.rect.contains(viewport.center())
        )));
        std::fs::remove_dir_all(app.workspace).unwrap();
    }

    #[test]
    fn bold_faces_render_distinct_glyphs_and_relative_content_scales() {
        let (ctx, mut app) = super::super::tests::test_app();
        for choice in [FontChoice::Sans, FontChoice::Mono] {
            app.settings.font = choice;
            super::super::tests::run_frame(&ctx, Default::default(), |ui| {
                app.settings.bold_text = false;
                let regular = ui.painter().layout_no_wrap(
                    "Readable task".into(),
                    app.font_id(19.0),
                    Color32::WHITE,
                );
                app.settings.bold_text = true;
                let bold = ui.painter().layout_no_wrap(
                    "Readable task".into(),
                    app.font_id(19.0),
                    Color32::WHITE,
                );
                assert_ne!(regular.rows[0].visuals.mesh, bold.rows[0].visuals.mesh);
                app.settings.font_size = 14.0;
                let small = ui.painter().layout_no_wrap(
                    "Task child".into(),
                    app.content_font_id(15.0),
                    Color32::WHITE,
                );
                app.settings.font_size = 30.0;
                let large = ui.painter().layout_no_wrap(
                    "Task child".into(),
                    app.content_font_id(15.0),
                    Color32::WHITE,
                );
                assert!(large.size().y > small.size().y * 1.9);
            });
        }
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    #[test]
    fn toggle_label_wraps_before_reserved_switch_at_narrow_width() {
        for choice in [FontChoice::Sans, FontChoice::Mono] {
            let (ctx, mut app) = super::super::tests::test_app();
            app.settings.font = choice;
            for size in [15.0, 30.0] {
                let mut bounds = Rect::NOTHING;
                let mut value = false;
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    ui.set_width(256.0);
                    let response = MinimalistApp::toggle_row(
                        ui,
                        Id::new("test-toggle"),
                        "Show window title bar",
                        &mut value,
                        app.font_id(size),
                        app.palette(),
                    );
                    bounds = response.rect;
                });
                let label = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text)
                            if text.galley.text() == "Show window title bar" =>
                        {
                            Some(Rect::from_min_size(text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(label.right() <= bounds.right() - 54.0);
                assert!(bounds.contains_rect(label));
                output.textures_delta.clear();
            }
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn small_header_keeps_title_clear_of_controls_with_both_fonts_and_pin_states() {
        let title = "A long list title describing the upcoming research workshop";
        for width in [280.0, 320.0] {
            for font in [FontChoice::Sans, FontChoice::Mono] {
                for pinned in [false, true] {
                    for decorations in [false, true] {
                        let (ctx, mut app) = super::super::tests::test_app();
                        app.settings.font = font;
                        app.settings.always_on_top = pinned;
                        app.settings.window_decorations = decorations;
                        let palette = app.palette();
                        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 280.0));
                        let frame = |app: &mut MinimalistApp, events| {
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(viewport),
                                    events,
                                    ..Default::default()
                                },
                                |ui| {
                                    let mut header = ui.new_child(
                                        egui::UiBuilder::new().max_rect(viewport.shrink(16.0)),
                                    );
                                    app.header(&mut header, palette, title, Some(app.lists[0].key));
                                },
                            );
                            output.textures_delta.clear();
                            output
                        };
                        let output = frame(&mut app, Vec::new());
                        let text_rect = |label: &str| {
                            output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::epaint::Shape::Text(text)
                                        if text.galley.text() == label =>
                                    {
                                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                                    }
                                    _ => None,
                                })
                                .unwrap()
                        };
                        let title_rect = text_rect(title);
                        let mut controls =
                            vec!["...", "find", "add", if pinned { "pinned" } else { "pin" }];
                        if !decorations {
                            controls.push("x");
                        }
                        for control in &controls {
                            let rect = text_rect(control);
                            assert!(viewport.contains_rect(rect));
                            assert!(
                                title_rect.right() < rect.left(),
                                "width={width}, {font:?}, pinned={pinned}, decorations={decorations}: {title_rect:?}, {rect:?}"
                            );
                        }
                        let back_rect = text_rect("<");
                        assert!(viewport.contains_rect(back_rect));
                        assert!(back_rect.right() < title_rect.left());
                        controls.push("<");
                        let mut focused_controls = std::collections::HashSet::new();
                        for _ in 0..controls.len() + 2 {
                            frame(
                                &mut app,
                                [true, false]
                                    .into_iter()
                                    .map(|pressed| egui::Event::Key {
                                        key: egui::Key::Tab,
                                        physical_key: None,
                                        pressed,
                                        repeat: false,
                                        modifiers: egui::Modifiers::default(),
                                    })
                                    .collect(),
                            );
                            frame(&mut app, Vec::new());
                            if let Some(response) = ctx
                                .memory(|memory| memory.focused())
                                .and_then(|id| ctx.read_response(id))
                            {
                                assert!(viewport.contains_rect(response.rect));
                                for control in &controls {
                                    if response.rect.contains(text_rect(control).center()) {
                                        focused_controls.insert(*control);
                                    }
                                }
                            }
                        }
                        assert_eq!(focused_controls.len(), controls.len());

                        let position = text_rect("find").center();
                        for pressed in [true, false] {
                            frame(
                                &mut app,
                                vec![
                                    egui::Event::PointerMoved(position),
                                    egui::Event::PointerButton {
                                        pos: position,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: egui::Modifiers::default(),
                                    },
                                ],
                            );
                        }
                        assert!(app.find.is_some());
                        std::fs::remove_dir_all(&app.workspace).unwrap();
                    }
                }
            }
        }
    }
}
