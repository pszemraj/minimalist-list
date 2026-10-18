//! Parent checklist controls and independent child previews below task rows.

use super::super::{MinimalistApp, keyboard::FocusItem, theme::Palette};
use crate::model::Task;
use eframe::egui::{
    self, Align2, Button, CornerRadius, Frame, Id, Margin, Rect, Sense, Stroke, Vec2,
};
use uuid::Uuid;

impl MinimalistApp {
    fn checklist_control_width(&self, ui: &egui::Ui, task: &Task) -> f32 {
        task.progress().map_or(36.0, |(done, total)| {
            ui.painter()
                .layout_no_wrap(
                    format!("{done}/{total}"),
                    self.font_id(17.0),
                    egui::Color32::WHITE,
                )
                .size()
                .x
                + 26.0
        })
    }

    /// Returns measured action space, including a complete checklist count.
    ///
    /// # Arguments
    /// - `ui` - Row UI supplying fonts.
    /// - `task` - Task whose optional context and progress need space.
    ///
    /// # Returns
    /// Reserved width for the completion control and all right-side actions.
    pub(in crate::app) fn task_controls_width(&self, ui: &egui::Ui, task: &Task) -> f32 {
        114.0
            + self.checklist_control_width(ui, task)
            + if task.context.is_empty() { 0.0 } else { 24.0 }
    }

    fn preview_is_visible(&self, list: Uuid, task: &Task) -> bool {
        self.settings.show_subtasks
            && !task.subtasks.is_empty()
            && !self.details.collapsed_previews.contains(&(list, task.id))
    }

    /// Toggles this parent's checklist without affecting any other visible previews.
    ///
    /// # Arguments
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    pub(in crate::app) fn toggle_task_subtasks(&mut self, list: Uuid, task: Uuid) {
        let has_children = self
            .list_index(list)
            .and_then(|index| self.lists.get(index))
            .is_some_and(|list| {
                list.data
                    .tasks
                    .iter()
                    .any(|item| item.id == task && !item.subtasks.is_empty())
            });
        if self.subtasks_for == Some((list, task)) {
            self.switch_subtask_panel(None);
            if self.settings.show_subtasks && has_children {
                self.details.collapsed_previews.insert((list, task));
            }
        } else if self.settings.show_subtasks && has_children {
            if !self.details.collapsed_previews.remove(&(list, task)) {
                self.details.collapsed_previews.insert((list, task));
            }
        } else {
            self.switch_subtask_panel(Some((list, task)));
        }
    }

    /// Paints a clear add mark, or an expand/collapse chevron and checklist progress.
    ///
    /// # Arguments
    /// - `ui` - Row UI.
    /// - `row` - Complete parent row bounds.
    /// - `list` - Parent list identity.
    /// - `task` - Parent task.
    /// - `palette` - Action colors.
    ///
    /// # Returns
    /// Whether the user activated this parent's checklist control.
    pub(in crate::app) fn task_checklist_control(
        &mut self,
        ui: &mut egui::Ui,
        row: Rect,
        list: Uuid,
        task: &Task,
        palette: Palette,
    ) -> bool {
        let width = self.checklist_control_width(ui, task);
        let rect = Rect::from_center_size(
            egui::pos2(row.right() - 64.0 - width * 0.5, row.center().y),
            Vec2::new(width, 32.0),
        );
        let expanded =
            self.subtasks_for == Some((list, task.id)) || self.preview_is_visible(list, task);
        let label = if task.subtasks.is_empty() {
            "Add subtasks"
        } else if expanded {
            "Collapse subtasks"
        } else {
            "Expand subtasks"
        };
        let response = ui
            .interact(
                rect,
                Id::new(("task-checklist", list, task.id)),
                Sense::click(),
            )
            .on_hover_text(format!("{label} (Right enters; Left or Esc returns)"));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("{label}: {}", task.text),
            )
        });
        self.track_focus(&response, FocusItem::Task(list, task.id));
        let stroke = Stroke::new(2.0, palette.accent);
        if let Some((done, total)) = task.progress() {
            let center = egui::pos2(rect.left() + 9.0, rect.center().y);
            let [first, middle, last] = if expanded {
                [
                    center + Vec2::new(-4.0, -2.0),
                    center + Vec2::new(0.0, 2.0),
                    center + Vec2::new(4.0, -2.0),
                ]
            } else {
                [
                    center + Vec2::new(-2.0, -4.0),
                    center + Vec2::new(2.0, 0.0),
                    center + Vec2::new(-2.0, 4.0),
                ]
            };
            ui.painter().line_segment([first, middle], stroke);
            ui.painter().line_segment([middle, last], stroke);
            ui.painter().text(
                egui::pos2(rect.left() + 20.0, rect.center().y),
                Align2::LEFT_CENTER,
                format!("{done}/{total}"),
                self.font_id(17.0),
                palette.accent,
            );
        } else {
            let center = rect.center();
            ui.painter().line_segment(
                [center - Vec2::new(8.0, 0.0), center + Vec2::new(8.0, 0.0)],
                stroke,
            );
            ui.painter().line_segment(
                [center - Vec2::new(0.0, 8.0), center + Vec2::new(0.0, 8.0)],
                stroke,
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        response.clicked()
    }

    /// Shows independent child previews, keeping only one active add/edit draft owner.
    ///
    /// # Arguments
    /// - `ui` - Parent list UI immediately below the task row.
    /// - `list` - Parent list identity.
    /// - `task` - Task whose children may be visible.
    /// - `palette` - Panel colors.
    pub(in crate::app) fn task_children_below(
        &mut self,
        ui: &mut egui::Ui,
        list: Uuid,
        task: &Task,
        palette: Palette,
    ) {
        let editing = self.subtasks_for == Some((list, task.id));
        let preview = self.preview_is_visible(list, task);
        let progress = ui.ctx().animate_bool_with_time_and_easing(
            Id::new(("subtasks-panel", list, task.id)),
            editing || preview,
            0.22,
            egui::emath::easing::cubic_out,
        );
        if progress <= 0.001 {
            return;
        }
        ui.add_enabled_ui(editing || preview, |ui| {
            if editing {
                self.subtasks_panel(ui, list, task, palette, progress);
            } else if preview {
                ui.add_space(4.0);
                Frame::NONE
                    .fill(palette.raised)
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        for child in &task.subtasks {
                            self.child_preview_row(ui, child, palette);
                        }
                        let response = ui
                            .push_id(("edit-visible-subtasks", list, task.id), |ui| {
                                ui.add(
                                    Button::new(self.rich("Edit subtasks", 15.0, palette.accent))
                                        .frame(false),
                                )
                            })
                            .inner;
                        self.track_focus(&response, FocusItem::Task(list, task.id));
                        if response.clicked() {
                            self.details.collapsed_previews.remove(&(list, task.id));
                            self.switch_subtask_panel(Some((list, task.id)));
                            self.keyboard.pending = Some(Id::new(("new-subtask", list, task.id)));
                        }
                    });
            }
        });
    }

    /// Paints a read-only checklist item with a font-independent completion marker.
    ///
    /// # Arguments
    /// - `ui` - Destination panel.
    /// - `child` - Child text and completion state.
    /// - `palette` - Text and marker colors.
    ///
    /// # Returns
    /// Measured bounds including the complete wrapped child text.
    pub(in crate::app) fn child_preview_row(
        &self,
        ui: &mut egui::Ui,
        child: &crate::model::Subtask,
        palette: Palette,
    ) -> Rect {
        ui.horizontal_top(|ui| {
            let text_width = (ui.available_width() - 24.0 - ui.spacing().item_spacing.x).max(1.0);
            let (marker, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    false,
                    child.completed,
                    &child.text,
                )
            });
            let center = marker.center();
            ui.painter()
                .circle_stroke(center, 7.0, Stroke::new(1.8, palette.accent));
            if child.completed {
                ui.painter().circle_filled(center, 7.0, palette.accent);
                ui.painter().line_segment(
                    [center + Vec2::new(-4.5, 0.0), center + Vec2::new(-1.2, 3.2)],
                    Stroke::new(1.8, palette.background),
                );
                ui.painter().line_segment(
                    [center + Vec2::new(-1.2, 3.2), center + Vec2::new(5.0, -4.0)],
                    Stroke::new(1.8, palette.background),
                );
            }
            ui.allocate_ui_with_layout(
                Vec2::new(text_width, 24.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.add(
                        egui::Label::new(self.content_rich(
                            &child.text,
                            15.0,
                            if child.completed {
                                palette.muted
                            } else {
                                palette.text
                            },
                        ))
                        .wrap(),
                    )
                },
            );
        })
        .response
        .rect
    }
}

#[cfg(test)]
#[path = "task_controls_tests.rs"]
mod tests;
