//! Measured checklist and read-only History views.

use super::{
    MinimalistApp,
    context::relative_age,
    keyboard::{FocusItem, mark_own_focus_frame},
    text_input_shortcuts,
    theme::Palette,
};
use crate::model::{ArchivedTask, Task, now_unix_seconds};
use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, Frame, Id, Label, Layout, Margin, Rect, Sense,
    Stroke, TextEdit, UiBuilder, Vec2,
};
use std::collections::HashSet;
use uuid::Uuid;

#[path = "task_operations.rs"]
mod operations;

#[path = "task_move.rs"]
mod moving;

#[path = "task_controls.rs"]
mod controls;

#[derive(Default)]
/// Session-only editors, recovery content, and expanded History entries.
pub(super) struct TaskDetailsState {
    pub(super) editor: Option<SubtaskEdit>,
    undo: Option<UndoItem>,
    expanded_history: HashSet<(Uuid, Uuid)>,
    pub(super) move_task: Option<MoveDialog>,
    last_move_destination: Option<Uuid>,
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

/// The destination choice for one task while its Move dialog is open.
pub(super) struct MoveDialog {
    pub(super) source: Uuid,
    pub(super) task: Uuid,
    pub(super) destination: Uuid,
    picker: Option<Id>,
    request_focus: bool,
}

/// The latest deletion or move, retained for one-step Undo.
enum UndoItem {
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
    Archived {
        list: Uuid,
        position: usize,
        item: ArchivedTask,
    },
    Moved {
        source: Uuid,
        /// The task's original Manual position in `source`.
        position: usize,
        task: Uuid,
        destination: Uuid,
    },
}

/// Returns the identity of the title control that expands one History row.
///
/// # Arguments
///
/// - `list` - Parent list identity.
/// - `task` - Archived task identity.
///
/// # Returns
///
/// The stable widget identity of the row's title control.
pub(in crate::app) fn history_title_id(list: Uuid, task: Uuid) -> Id {
    Id::new(("history-details", list, task))
}

/// Per-frame values shared by every row of one History pass.
struct HistoryFrame {
    /// Archive length when the pass began; a different length ends the pass.
    count: usize,
    now: u64,
    age_height: f32,
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

    /// Renders measured History rows and their independent Restore actions.
    ///
    /// Every row reserves its measured height, but only rows that are visible, focused, expanded,
    /// highlighted, or showing their menu build contents, so long histories stay cheap.
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
        let count = self.lists[index].data.archive.len();
        ui.add_space(8.0 * progress);
        ui.scope(|ui| {
            ui.set_opacity(progress);
            Frame::NONE
                .fill(palette.surface)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.label(self.rich("History", 18.0, palette.text));
                    let age_height = ui
                        .painter()
                        .layout_no_wrap(
                            "Archived".to_owned(),
                            self.font_id(12.0),
                            Color32::PLACEHOLDER,
                        )
                        .size()
                        .y;
                    let frame = HistoryFrame {
                        count,
                        now: now_unix_seconds(),
                        age_height,
                    };
                    for position in 0..count {
                        if !self.history_row(ui, list, position, &frame, palette) {
                            break;
                        }
                    }
                });
        });
    }

    /// Reserves one History row and builds its contents when they can be seen or used.
    ///
    /// Skipped rows keep their Restore control registered, so keyboard navigation and
    /// scrolling reach them.
    ///
    /// # Returns
    ///
    /// Whether the next row may be rendered; false once the archive changed during the pass.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    fn history_row(
        &mut self,
        ui: &mut egui::Ui,
        list: Uuid,
        position: usize,
        frame: &HistoryFrame,
        palette: Palette,
    ) -> bool {
        let Some(index) = self.list_index(list) else {
            return false;
        };
        let archive = &self.lists[index].data.archive;
        if archive.len() != frame.count {
            return false;
        }
        let id = archive[position].task.id;
        let mut expanded = self.details.expanded_history.contains(&(list, id));
        if self.spotlight.as_ref().is_some_and(|spotlight| {
            spotlight.archived
                && spotlight.list_id == list
                && spotlight.task_id == id
                && spotlight.subtask_id.is_some()
        }) {
            self.expand_archived(list, id);
            expanded = true;
        }
        let marker = if expanded { "▾" } else { "▸" };
        let spacing = ui.spacing().item_spacing;
        let width = (ui.available_width() - 110.0 - 2.0 * spacing.x).max(1.0);
        let galley = ui.painter().layout(
            format!(
                "{marker} {}",
                self.lists[index].data.archive[position].task.text
            ),
            self.content_font_id(15.0),
            Color32::PLACEHOLDER,
            width,
        );
        let height = (galley.size().y + spacing.y + frame.age_height).max(28.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let title_rect = Rect::from_min_size(rect.min, Vec2::new(width, height));
        let copy_rect = Rect::from_min_size(
            egui::pos2(title_rect.right() + spacing.x, rect.top()),
            Vec2::new(26.0, 28.0),
        );
        let restore_rect = Rect::from_min_size(
            egui::pos2(copy_rect.right() + spacing.x, rect.top()),
            Vec2::new(76.0, 28.0),
        );
        let restore_item = FocusItem::Archived(list, id);
        let needs_contents = ui.is_rect_visible(rect)
            || expanded
            || self.has_focus_within(restore_item)
            || self.details.menu_task == Some((list, id, true))
            || self.spotlight.as_ref().is_some_and(|spotlight| {
                spotlight.archived && spotlight.list_id == list && spotlight.task_id == id
            });
        if !needs_contents {
            let response = Self::history_restore_button(
                ui,
                restore_rect,
                restore_item.id(),
                &self.lists[index].data.archive[position].task.text,
            );
            self.track_focus(&response, restore_item);
            // A key that focuses an unbuilt row and presses Enter in one frame still activates it.
            if response.clicked() {
                self.restore_archived(list, id);
            }
            return true;
        }
        let item = self.lists[index].data.archive[position].clone();
        let task = &item.task;
        let mut title_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("history-row", list, id))
                .max_rect(title_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        let title = title_ui.add(
            Label::new(self.content_rich(format!("{marker} {}", task.text), 15.0, palette.muted))
                .wrap(),
        );
        let response = title_ui.interact(title.rect, history_title_id(list, id), Sense::click());
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
        self.track_focus(&response, restore_item);
        self.task_actions_menu(&response, list, id, true);
        if response.clicked() {
            if expanded {
                self.details.expanded_history.remove(&(list, id));
            } else {
                self.expand_archived(list, id);
            }
        }
        if response.has_focus() {
            title_ui.painter().rect_stroke(
                response.rect,
                3.0,
                Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        title_ui.add(
            Label::new(self.rich(
                relative_age("Archived", item.archived_at_unix, frame.now),
                12.0,
                palette.muted,
            ))
            .truncate(),
        );
        self.task_copy_control(ui, copy_rect, (list, id, true), palette);
        let response =
            Self::history_restore_button(ui, restore_rect, restore_item.id(), &task.text);
        ui.painter().text(
            restore_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Restore",
            self.font_id(13.0),
            palette.accent,
        );
        self.track_focus(&response, restore_item);
        if response.clicked() {
            self.restore_archived(list, id);
        }
        let row = Rect::from_min_max(rect.min, egui::pos2(restore_rect.right(), rect.bottom()));
        self.detail_spotlight(ui, row, (list, id, None, true), palette);
        if expanded {
            ui.indent(("history-children", list, id), |ui| {
                if !task.context.is_empty() && ui.button("View context").clicked() {
                    self.open_task_context(list, id, true);
                }
                if task.subtasks.is_empty() {
                    ui.label(self.rich("No subtasks", 13.0, palette.muted));
                }
                for child in &task.subtasks {
                    let row = self.child_preview_row(ui, child, palette);
                    self.detail_spotlight(ui, row, (list, id, Some(child.id), true), palette);
                }
            });
        }
        true
    }

    /// Registers the same Restore button for visible and skipped History rows.
    fn history_restore_button(ui: &egui::Ui, rect: Rect, id: Id, title: &str) -> egui::Response {
        let response = ui.interact(rect, id, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Restore: {title}"))
        });
        response
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

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
