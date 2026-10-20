//! Workspace overview and list-card interactions.

use super::{
    MinimalistApp,
    keyboard::{FocusItem, NEW_LIST, shortcut, take_key},
    text_input_shortcuts,
    theme::Palette,
};
use crate::model::{Accent, ListSort, TitleOverflow};
use crate::storage;
use eframe::egui;
use egui::{
    Button, Color32, CornerRadius, CursorIcon, Frame, Id, Margin, Rect, ScrollArea, Sense, Stroke,
    Vec2, text::LayoutJob,
};
use uuid::Uuid;

#[path = "overview_order.rs"]
mod order;
pub(super) use order::OverviewDrag;

impl MinimalistApp {
    /// Renders the responsive list overview and list-creation controls.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the overview.
    /// - `palette` - Colors used to render list cards.
    ///
    /// # Panics
    ///
    /// Panics if the fixed card-column count is zero.
    pub(super) fn overview_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        self.header(ui, palette, "Lists", None);
        ui.scope(|ui| {
            ui.menu_button(
                self.rich(
                    format!("Sort lists · {}", self.list_sort().label()),
                    13.0,
                    palette.muted,
                ),
                |ui| {
                    for mode in ListSort::ALL {
                        if ui
                            .selectable_label(self.list_sort() == mode, mode.label())
                            .clicked()
                        {
                            self.set_list_sort(mode);
                            ui.close();
                        }
                    }
                },
            );
            if self.list_sort() != ListSort::Manual {
                ui.add(
                    egui::Label::new(self.rich(
                        "Choose Manual order to drag tiles.",
                        12.0,
                        palette.muted,
                    ))
                    .wrap(),
                );
            }
        });
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
                if self.overview_drag.is_some()
                    && ui.input(|input| {
                        input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }))
                    })
                {
                    self.cancel_overview_drag();
                }
                if self.creating_list {
                    self.creation_keyboard(ui.ctx());
                } else {
                    self.overview_keyboard(ui.ctx(), columns);
                }
                self.keyboard.creation_choices.clear();

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

                let mut card_rects = Vec::new();
                let mut open_card = None;
                let mut open_options = None;
                for row in cards.chunks(columns) {
                    ui.horizontal(|ui| {
                        for (id, title, accent, active, completed, preview) in row {
                            let (response, settings) = self.list_card(
                                ui, *id, title, *accent, *active, *completed, preview, card_width,
                            );
                            card_rects.push((*id, response.rect, settings.rect));
                            if settings.clicked() {
                                open_options = Some((*id, settings.id));
                            } else if response.clicked() {
                                open_card = Some(*id);
                            }
                        }
                    });
                    ui.add_space(12.0);
                }
                if !self.finish_overview_drag(ui, &card_rects, palette) {
                    if let Some((id, response_id)) = open_options {
                        self.open_settings(Some(id));
                        self.keyboard.settings_return = Some(response_id);
                    } else if let Some(id) = open_card {
                        take_key(ui.ctx(), egui::Key::Enter, egui::Modifiers::NONE);
                        take_key(ui.ctx(), egui::Key::Space, egui::Modifiers::NONE);
                        self.open_list(id);
                    }
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
                            if !self.creating_list {
                                ui.disable();
                            }
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
                                            .selected(selected)
                                            .fill(if selected { p.raised } else { p.surface })
                                            .stroke(Stroke::new(
                                                if selected { 2.0 } else { 0.0 },
                                                p.accent,
                                            ))
                                            .corner_radius(12.0),
                                    );
                                    self.keyboard.creation_choices.push(response.id);
                                    if response.clicked() {
                                        self.new_list_accent = accent;
                                        // Selecting a color does not also submit the form.
                                        take_key(ui.ctx(), egui::Key::Enter, egui::Modifiers::NONE);
                                        take_key(ui.ctx(), egui::Key::Space, egui::Modifiers::NONE);
                                        ui.memory_mut(|memory| {
                                            memory.request_focus(Id::new("new-list-title"))
                                        });
                                    }
                                }
                            });
                            ui.add(
                                egui::Label::new(self.rich(
                                    "Down: colors · Up: name · Enter creates · Esc cancels",
                                    12.0,
                                    palette.muted,
                                ))
                                .wrap(),
                            );
                            let (submit, cancel) = text_input_shortcuts(ui, &response);
                            let (create_clicked, cancel_clicked) = ui
                                .horizontal_wrapped(|ui| {
                                    let create = ui.add_enabled(
                                        !self.new_list_title.trim().is_empty(),
                                        Button::new(self.rich("Create list", 15.0, palette.accent)),
                                    );
                                    let cancel = ui.add(Button::new(self.rich(
                                        "Cancel",
                                        15.0,
                                        palette.muted,
                                    )));
                                    (create.clicked(), cancel.clicked())
                                })
                                .inner;
                            if submit || create_clicked {
                                self.create_list();
                                if self.creating_list {
                                    response.request_focus();
                                }
                            } else if cancel || cancel_clicked {
                                self.new_list_title.clear();
                                self.creating_list = false;
                            }
                        });
                } else if ui
                    .push_id("new-list", |ui| {
                        ui.scope(|ui| {
                            ui.add(
                                Button::new(self.rich("+ New list", 18.0, palette.accent))
                                    .frame(false),
                            )
                        })
                        .inner
                    })
                    .inner
                    .on_hover_text(format!("New list ({})", shortcut(NEW_LIST)))
                    .clicked()
                {
                    self.creating_list = true;
                    self.request_list_focus = true;
                }
                self.keyboard_focus_ui(ui, palette);
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn list_card(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        title: &str,
        accent: Accent,
        active: usize,
        completed: usize,
        preview: &[String],
        width: f32,
    ) -> (egui::Response, egui::Response) {
        let card_palette = Palette::from_accent(accent);
        let (slot_rect, _) = ui.allocate_exact_size(Vec2::new(width, 166.0), Sense::hover());
        let response = ui.interact(
            slot_rect,
            FocusItem::Card(id).id(),
            if self.list_sort() == ListSort::Manual {
                Sense::click_and_drag()
            } else {
                Sense::click()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Open list: {title}"),
            )
        });
        self.track_focus(&response, FocusItem::Card(id));
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
                if response.has_focus() { 2.0 } else { 1.0 },
                card_palette
                    .accent
                    .linear_multiply(if response.has_focus() {
                        1.0
                    } else {
                        0.14 + hover * 0.46
                    }),
            ),
            egui::StrokeKind::Inside,
        );

        let content_rect = card_rect.shrink2(Vec2::new(18.0, 16.0));
        let painter = ui
            .painter()
            .with_clip_rect(content_rect.intersect(ui.clip_rect()));
        // Reserve the settings target and footer before laying out arbitrary user text.
        let mut title_job = LayoutJob::simple(
            title.to_owned(),
            self.font_id(22.0),
            card_palette.text,
            (content_rect.width() - 24.0).max(1.0),
        );
        title_job.wrap.max_rows = 2;
        title_job.wrap.break_anywhere = true;
        let title_galley = painter.layout_job(title_job);
        let mut y = content_rect.top() + title_galley.size().y + 8.0;
        painter.galley(content_rect.min, title_galley, card_palette.text);
        let footer = painter.layout_no_wrap(
            format!("{active} active / {completed} done"),
            self.font_id(13.0),
            card_palette.accent,
        );
        let footer_y = content_rect.bottom() - footer.size().y;
        let empty = if completed > 0 { "All done" } else { "Empty" };
        let lines: Vec<String> = if preview.is_empty() {
            vec![empty.to_owned()]
        } else {
            preview.iter().map(|line| format!("— {line}")).collect()
        };
        for line in lines {
            let mut job = LayoutJob::simple(
                line,
                self.font_id(15.0),
                card_palette.muted,
                content_rect.width(),
            );
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            let galley = painter.layout_job(job);
            if y + galley.size().y > footer_y - 6.0 {
                break;
            }
            let height = galley.size().y;
            painter.galley(
                egui::pos2(content_rect.left(), y),
                galley,
                card_palette.muted,
            );
            y += height + 3.0;
        }
        painter.galley(
            egui::pos2(content_rect.left(), footer_y),
            footer,
            card_palette.accent,
        );
        let settings_rect = Rect::from_center_size(
            slot_rect.right_top() + Vec2::new(-18.0, 18.0),
            Vec2::splat(30.0),
        );
        // A separate child does not advance the grid's cursor or move its hit
        // region with the card's purely visual hover animation.
        let mut controls = ui.new_child(
            egui::UiBuilder::new()
                .id(Id::new(("list-card-settings", id)))
                .max_rect(settings_rect),
        );
        let settings = controls
            .put(
                settings_rect,
                Button::new(self.rich(
                    "...",
                    14.0,
                    card_palette.muted.linear_multiply(0.24 + hover * 0.76),
                ))
                .frame(false),
            )
            .on_hover_text(format!("List options: {title}"));
        settings.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("List options: {title}"),
            )
        });
        self.track_focus(&settings, FocusItem::Card(id));
        self.card_hover_info(ui, &response, &settings, id, title);

        (response, settings)
    }

    fn card_hover_info(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        settings: &egui::Response,
        id: Uuid,
        title: &str,
    ) {
        let hovering = self.settings.title_overflow == TitleOverflow::Wrap
            && response.hovered()
            && !settings.hovered()
            && self.overview_drag.is_none()
            && !ui.input(|input| input.pointer.any_down());
        let mut delayed = false;
        if hovering {
            let now = ui.input(|input| input.time);
            let started = match self.overview_hover {
                Some((hovered, started)) if hovered == id => started,
                _ => {
                    self.overview_hover = Some((id, now));
                    now
                }
            };
            delayed = now - started >= 2.0;
            if !delayed {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        (2.0 - (now - started)).max(0.01),
                    ));
            }
        } else if self
            .overview_hover
            .is_some_and(|(hovered, _)| hovered == id)
        {
            self.overview_hover = None;
        }
        if delayed || (response.has_focus() && self.overview_drag.is_none()) {
            egui::Tooltip::for_widget(response).show(|ui| {
                ui.add(egui::Label::new(self.rich(title, 18.0, self.palette().text)).wrap());
                if let Some(index) = self.list_index(id) {
                    let data = &self.lists[index].data;
                    ui.label(self.rich(
                        format!(
                            "{} active · {} done · {} archived",
                            data.active_count(),
                            data.completed_count(),
                            data.archive.len()
                        ),
                        13.0,
                        self.palette().muted,
                    ));
                }
            });
        }
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
                storage::sort_lists(&mut self.lists);
                self.apply_list_order();
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

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;
