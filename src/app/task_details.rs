//! Measured checklist and read-only History views.

use super::{
    MinimalistApp,
    keyboard::{FocusItem, mark_own_focus_frame},
    text_input_shortcuts,
    theme::Palette,
};
use crate::model::Task;
use eframe::egui::{
    self, Align, Button, CornerRadius, Frame, Id, Layout, Margin, Sense, Stroke, TextEdit, Vec2,
};
use std::collections::HashSet;
use uuid::Uuid;

#[path = "task_operations.rs"]
mod operations;

#[path = "task_controls.rs"]
mod controls;

#[derive(Default)]
/// Session-only editors, recovery content, and expanded History entries.
pub(super) struct TaskDetailsState {
    pub(super) editor: Option<SubtaskEdit>,
    deleted: Option<DeletedItem>,
    expanded_history: HashSet<(Uuid, Uuid)>,
    pub(super) move_task: Option<(Uuid, Uuid, Uuid)>,
    pub(super) menu_task: Option<(Uuid, Uuid, bool)>,
    pub(super) menu_popup: Option<Id>,
    collapsed_previews: HashSet<(Uuid, Uuid)>,
}

/// A checklist text edit anchored to stable identities.
pub(super) struct SubtaskEdit {
    pub(super) list: Uuid,
    pub(super) task: Uuid,
    pub(super) subtask: Uuid,
    pub(super) text: String,
    request_focus: bool,
}

enum DeletedItem {
    Task {
        list: Uuid,
        position: usize,
        task: Task,
    },
    Subtask {
        list: Uuid,
        task: Uuid,
        position: usize,
        subtask: crate::model::Subtask,
    },
}

impl MinimalistApp {
    /// Renders checklist rows at their measured heights with reserved action space.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination panel.
    /// - `list_id` - Parent list identity.
    /// - `task` - Task whose children are displayed.
    /// - `palette` - Panel colors.
    /// - `progress` - Reveal opacity from zero to one.
    pub(super) fn subtasks_panel(
        &mut self,
        ui: &mut egui::Ui,
        list_id: Uuid,
        task: &Task,
        palette: Palette,
        progress: f32,
    ) {
        ui.add_space(4.0 * progress);
        ui.scope(|ui| {
            ui.set_opacity(progress);
            Frame::NONE
                .fill(palette.raised)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    for subtask in &task.subtasks {
                        let row = self.subtask_row(ui, list_id, task.id, subtask, palette);
                        self.detail_spotlight(
                            ui,
                            row,
                            (list_id, task.id, Some(subtask.id), false),
                            palette,
                        );
                    }
                    let input_font = self.content_font_id(15.0);
                    let response = ui.add(
                        TextEdit::singleline(&mut self.new_subtask)
                            .id(Id::new(("new-subtask", list_id, task.id)))
                            .font(input_font)
                            .hint_text("Add subtask")
                            .desired_width(f32::INFINITY),
                    );
                    // egui's text frame already outlines these fields when focused.
                    mark_own_focus_frame(ui.ctx(), response.id);
                    let (submit, _) = text_input_shortcuts(ui, &response);
                    self.track_focus(&response, FocusItem::Task(list_id, task.id));
                    if submit {
                        self.add_subtask(list_id, task.id);
                        response.request_focus();
                    }
                });
        });
    }

    fn subtask_row(
        &mut self,
        ui: &mut egui::Ui,
        list: Uuid,
        task: Uuid,
        subtask: &crate::model::Subtask,
        palette: Palette,
    ) -> egui::Rect {
        ui.push_id(("subtask-row", list, task, subtask.id), |ui| {
            let width = ui.available_width();
            let spacing = ui.spacing().item_spacing.x;
            let text_width = (width - 56.0 - 2.0 * spacing).max(1.0);
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                let response = ui.interact(
                    rect,
                    FocusItem::Subtask(list, task, subtask.id).id(),
                    Sense::click(),
                );
                let response = self.completion_control(
                    ui,
                    response,
                    subtask.completed,
                    f32::from(subtask.completed),
                    palette,
                    &subtask.text,
                );
                self.track_focus(&response, FocusItem::Subtask(list, task, subtask.id));
                if response.clicked() {
                    self.toggle_subtask(list, task, subtask.id);
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(text_width, 28.0),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        let is_editing = self.details.editor.as_ref().is_some_and(|edit| {
                            edit.list == list && edit.task == task && edit.subtask == subtask.id
                        });
                        let font = self.content_font_id(15.0);
                        if is_editing {
                            let edit = self.details.editor.as_mut().unwrap();
                            let response = ui.add(
                                TextEdit::multiline(&mut edit.text)
                                    .id(Id::new(("edit-subtask", list, task, subtask.id)))
                                    .font(font)
                                    .desired_width(text_width)
                                    .desired_rows(1)
                                    .return_key(None),
                            );
                            mark_own_focus_frame(ui.ctx(), response.id);
                            if edit.request_focus {
                                response.request_focus();
                                edit.request_focus = false;
                            }
                            self.track_focus(&response, FocusItem::Subtask(list, task, subtask.id));
                            let (submit, cancel) = text_input_shortcuts(ui, &response);
                            if cancel {
                                self.cancel_subtask_edit();
                            } else if submit || response.lost_focus() {
                                self.finish_subtask_edit();
                                if submit {
                                    self.keyboard.pending =
                                        Some(FocusItem::Subtask(list, task, subtask.id).id());
                                }
                            }
                        } else {
                            let color = if subtask.completed {
                                palette.muted
                            } else {
                                palette.text
                            };
                            let label = ui.add(
                                egui::Label::new(self.content_rich(&subtask.text, 15.0, color))
                                    .wrap(),
                            );
                            let response = ui.interact(
                                label.rect,
                                Id::new(("edit-subtask-button", list, task, subtask.id)),
                                Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    format!("Edit subtask: {}", subtask.text),
                                )
                            });
                            self.track_focus(&response, FocusItem::Subtask(list, task, subtask.id));
                            if response.has_focus() {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    3.0,
                                    Stroke::new(1.0, palette.accent),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if response.clicked() {
                                self.begin_subtask_edit(list, task, subtask.id);
                            }
                        }
                    },
                );
                let delete = ui
                    .add_sized(
                        [28.0, 28.0],
                        Button::new(self.rich("×", 23.0, palette.muted)).frame(false),
                    )
                    .on_hover_text("Delete subtask");
                self.track_focus(&delete, FocusItem::Subtask(list, task, subtask.id));
                delete.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Delete subtask: {}", subtask.text),
                    )
                });
                if delete.has_focus() {
                    ui.painter().rect_stroke(
                        delete.rect,
                        4.0,
                        Stroke::new(1.0, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if delete.clicked() {
                    self.delete_subtask(list, task, subtask.id);
                }
            })
            .response
            .rect
        })
        .inner
    }

    /// Expands an archived task for inspection without changing its completion state.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Archived task identity.
    pub(super) fn expand_archived(&mut self, list: Uuid, task: Uuid) {
        self.details.expanded_history.insert((list, task));
    }

    /// Renders naturally sized History rows and their independent Restore actions.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination panel.
    /// - `list` - Parent list identity.
    /// - `palette` - Panel colors.
    /// - `progress` - Reveal opacity from zero to one.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn history_ui(
        &mut self,
        ui: &mut egui::Ui,
        list: Uuid,
        palette: Palette,
        progress: f32,
    ) {
        let Some(index) = self.list_index(list) else {
            return;
        };
        let archived = self.lists[index].data.archive.clone();
        ui.add_space(8.0 * progress);
        ui.scope(|ui| {
            ui.set_opacity(progress);
            Frame::NONE
                .fill(palette.surface)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(self.rich("History", 18.0, palette.text));
                    for item in archived {
                        let task = &item.task;
                        let mut expanded = self.details.expanded_history.contains(&(list, task.id));
                        if self.spotlight.as_ref().is_some_and(|spotlight| {
                            spotlight.archived
                                && spotlight.list_id == list
                                && spotlight.task_id == task.id
                                && spotlight.subtask_id.is_some()
                        }) {
                            self.expand_archived(list, task.id);
                            expanded = true;
                        }
                        let width =
                            (ui.available_width() - 110.0 - 2.0 * ui.spacing().item_spacing.x)
                                .max(1.0);
                        let row = ui
                            .horizontal_top(|ui| {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(width, 28.0),
                                    Layout::top_down(Align::LEFT),
                                    |ui| {
                                        let title = ui.add(
                                            egui::Label::new(self.content_rich(
                                                format!(
                                                    "{} {}",
                                                    if expanded { "▾" } else { "▸" },
                                                    task.text
                                                ),
                                                15.0,
                                                palette.muted,
                                            ))
                                            .wrap(),
                                        );
                                        let response = ui.interact(
                                            title.rect,
                                            Id::new(("history-details", list, task.id)),
                                            Sense::click(),
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                true,
                                                format!(
                                                    "{} archived task: {}",
                                                    if expanded { "Collapse" } else { "Inspect" },
                                                    task.text
                                                ),
                                            )
                                        });
                                        self.track_focus(
                                            &response,
                                            FocusItem::Archived(list, task.id),
                                        );
                                        self.task_actions_menu(&response, list, task.id, true);
                                        if response.clicked() {
                                            if expanded {
                                                self.details
                                                    .expanded_history
                                                    .remove(&(list, task.id));
                                            } else {
                                                self.expand_archived(list, task.id);
                                            }
                                        }
                                        if response.has_focus() {
                                            ui.painter().rect_stroke(
                                                response.rect,
                                                3.0,
                                                Stroke::new(1.0, palette.accent),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                    },
                                );
                                let (copy_rect, _) =
                                    ui.allocate_exact_size(Vec2::new(26.0, 28.0), Sense::hover());
                                self.task_copy_control(
                                    ui,
                                    copy_rect,
                                    (list, task.id, true),
                                    palette,
                                );
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::new(76.0, 28.0), Sense::hover());
                                let response = ui.interact(
                                    rect,
                                    FocusItem::Archived(list, task.id).id(),
                                    Sense::click(),
                                );
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Restore",
                                    self.font_id(13.0),
                                    palette.accent,
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        format!("Restore: {}", task.text),
                                    )
                                });
                                self.track_focus(&response, FocusItem::Archived(list, task.id));
                                if response.clicked() {
                                    self.restore_archived(list, task.id);
                                }
                            })
                            .response
                            .rect;
                        self.detail_spotlight(ui, row, (list, task.id, None, true), palette);
                        if expanded {
                            ui.indent(("history-children", list, task.id), |ui| {
                                if !task.context.is_empty() && ui.button("View context").clicked() {
                                    self.open_task_context(list, task.id, true);
                                }
                                if task.subtasks.is_empty() {
                                    ui.label(self.rich("No subtasks", 13.0, palette.muted));
                                }
                                for child in &task.subtasks {
                                    let row = self.child_preview_row(ui, child, palette);
                                    self.detail_spotlight(
                                        ui,
                                        row,
                                        (list, task.id, Some(child.id), true),
                                        palette,
                                    );
                                }
                            });
                        }
                    }
                });
        });
    }

    fn detail_spotlight(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        target: (Uuid, Uuid, Option<Uuid>, bool),
        palette: Palette,
    ) {
        let (list, task, child, archived) = target;
        if let Some(spotlight) = self.spotlight.as_mut().filter(|spotlight| {
            spotlight.archived == archived
                && spotlight.list_id == list
                && spotlight.task_id == task
                && spotlight.subtask_id == child
        }) {
            if spotlight.scroll_pending {
                ui.scroll_to_rect(rect, Some(Align::Center));
                spotlight.scroll_pending = false;
            }
            let opacity = (1.0 - spotlight.started.elapsed().as_secs_f32() / 1.4).clamp(0.0, 1.0);
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                8.0,
                Stroke::new(1.5, palette.accent.linear_multiply(opacity)),
                egui::StrokeKind::Inside,
            );
        }
    }
}
