//! Subtask, deletion, completion-clearing, and history views.

use super::{
    CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, MinimalistApp, text_input_shortcuts,
    theme::Palette,
};
use crate::model::{ArchivedTask, Subtask, Task};
use eframe::egui;
use egui::{
    Align, Button, CornerRadius, Frame, Id, Layout, Margin, Rect, Sense, Stroke, TextEdit, Vec2,
};
use std::time::Instant;
use uuid::Uuid;

impl MinimalistApp {
    /// Renders the animated subtask editor for a task.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the panel.
    /// - `list_id` - Parent list identity.
    /// - `task` - Task whose checklist is displayed.
    /// - `palette` - Colors used to render the panel.
    /// - `progress` - Panel reveal progress from zero to one.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid during an interaction.
    pub(super) fn subtasks_panel(
        &mut self,
        ui: &mut egui::Ui,
        list_id: Uuid,
        task: &Task,
        palette: Palette,
        progress: f32,
    ) {
        let full_height = 66.0 + task.subtasks.len() as f32 * 36.0;
        let visible_height = (full_height * progress).max(1.0);
        ui.add_space(4.0 * progress);
        let (clip_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), visible_height),
            Sense::hover(),
        );
        let content_rect = Rect::from_min_size(
            clip_rect.min + Vec2::new(0.0, -(1.0 - progress) * 10.0),
            Vec2::new(clip_rect.width(), full_height),
        );
        let mut panel_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("subtasks-panel-content", list_id, task.id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        panel_ui.set_clip_rect(clip_rect.intersect(ui.clip_rect()));
        panel_ui.set_opacity(progress);
        Frame::NONE
            .fill(palette.raised)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(&mut panel_ui, |ui| {
                for subtask in &task.subtasks {
                    let row = ui
                        .scope_builder(
                            egui::UiBuilder::new().id(Id::new((
                                "subtask-row",
                                list_id,
                                task.id,
                                subtask.id,
                            ))),
                            |ui| {
                                ui.horizontal(|ui| {
                                    let completion_progress =
                                        ui.ctx().animate_bool_with_time_and_easing(
                                            Id::new((
                                                "subtask-complete",
                                                list_id,
                                                task.id,
                                                subtask.id,
                                            )),
                                            subtask.completed,
                                            0.20,
                                            egui::emath::easing::cubic_out,
                                        );
                                    let completion =
                                        ui.add_sized([26.0, 26.0], Button::new("").frame(false));
                                    let completion = self.completion_control(
                                        ui,
                                        completion,
                                        subtask.completed,
                                        completion_progress,
                                        palette,
                                        &subtask.text,
                                    );
                                    if completion.clicked() {
                                        self.toggle_subtask(list_id, task.id, subtask.id);
                                    }
                                    ui.label(
                                        self.rich(
                                            &subtask.text,
                                            15.0,
                                            palette
                                                .text
                                                .lerp_to_gamma(palette.muted, completion_progress),
                                        ),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let delete = ui.add(
                                            Button::new(self.rich("x", 14.0, palette.muted))
                                                .frame(false),
                                        );
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
                                            self.delete_subtask(list_id, task.id, subtask.id);
                                        }
                                    });
                                })
                            },
                        )
                        .inner;
                    if let Some(spotlight) = self.spotlight.as_mut().filter(|spotlight| {
                        !spotlight.archived
                            && spotlight.list_id == list_id
                            && spotlight.task_id == task.id
                            && spotlight.subtask_id == Some(subtask.id)
                    }) {
                        if spotlight.scroll_pending {
                            ui.scroll_to_rect(row.response.rect, Some(Align::Center));
                            spotlight.scroll_pending = false;
                        }
                        let opacity =
                            (1.0 - spotlight.started.elapsed().as_secs_f32() / 1.4).clamp(0.0, 1.0);
                        ui.painter().rect_stroke(
                            row.response.rect.shrink(1.0),
                            8.0,
                            Stroke::new(1.5, palette.accent.linear_multiply(opacity)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                let subtask_response = ui.add(
                    TextEdit::singleline(&mut self.new_subtask)
                        .id(Id::new(("new-subtask", list_id, task.id)))
                        .hint_text("Add subtask")
                        .desired_width(f32::INFINITY),
                );
                let (submit, _) = text_input_shortcuts(ui, &subtask_response);
                if submit {
                    self.add_subtask(list_id, task.id);
                    subtask_response.request_focus();
                }
            });
    }

    fn add_subtask(&mut self, list_id: Uuid, task_id: Uuid) {
        let text = self.new_subtask.trim();
        if text.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.subtasks.push(Subtask::new(text));
            self.new_subtask.clear();
            self.save_list_index(index);
        }
    }

    fn toggle_subtask(&mut self, list_id: Uuid, task_id: Uuid, subtask_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(subtask) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .and_then(|task| {
                    task.subtasks
                        .iter_mut()
                        .find(|subtask| subtask.id == subtask_id)
                })
        {
            subtask.completed = !subtask.completed;
            self.save_list_index(index);
        }
    }

    fn delete_subtask(&mut self, list_id: Uuid, task_id: Uuid, subtask_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(task) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.subtasks.retain(|subtask| subtask.id != subtask_id);
            self.save_list_index(index);
        }
    }

    /// Starts the animation that moves completed tasks into history.
    pub(super) fn begin_clear_completed(&mut self, list_id: Uuid) {
        if self.clear_animation.is_none() {
            self.revealed = None;
            self.clear_animation = Some((list_id, Instant::now()));
        }
    }

    /// Starts the removal animation for a task.
    ///
    /// # Arguments
    ///
    /// - `list_id` - Parent list identity.
    /// - `task_id` - Identity of the task to remove.
    pub(super) fn begin_delete_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if self.delete_animation.is_none() {
            self.revealed = None;
            self.subtasks_for = None;
            self.delete_animation = Some((list_id, task_id, Instant::now()));
        }
    }

    /// Returns the current removal animation progress for a task.
    ///
    /// # Arguments
    ///
    /// - `list_id` - Parent list identity.
    /// - `task_id` - Task whose animation is queried.
    ///
    /// # Returns
    ///
    /// Eased progress from zero to one, or zero when the task is not being removed.
    ///
    /// # Panics
    ///
    /// Panics if the configured deletion duration is zero.
    pub(super) fn delete_progress(&self, list_id: Uuid, task_id: Uuid) -> f32 {
        self.delete_animation
            .map_or(0.0, |(active_list, active_task, started)| {
                if active_list != list_id || active_task != task_id {
                    0.0
                } else {
                    egui::emath::easing::cubic_in_out(
                        (started.elapsed().as_secs_f32() / DELETE_ANIMATION_SECONDS)
                            .clamp(0.0, 1.0),
                    )
                }
            })
    }

    /// Removes a task from its list and persists the change.
    ///
    /// # Arguments
    ///
    /// - `list_id` - Parent list identity.
    /// - `task_id` - Identity of the task to remove.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn remove_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
            if self
                .drag
                .as_ref()
                .is_some_and(|drag| drag.list_id == list_id)
            {
                self.drag = None;
            }
            self.lists[index]
                .data
                .tasks
                .retain(|task| task.id != task_id);
            self.save_list_index(index);
        }
    }

    /// Returns the current clear animation progress for a completed task.
    ///
    /// # Arguments
    ///
    /// - `list_id` - Parent list identity.
    /// - `completed` - Whether the task participates in the clear animation.
    ///
    /// # Returns
    ///
    /// Eased progress from zero to one, or zero for an active task or list.
    ///
    /// # Panics
    ///
    /// Panics if the configured clear duration is zero.
    pub(super) fn clear_progress(&self, list_id: Uuid, completed: bool) -> f32 {
        if !completed {
            return 0.0;
        }
        self.clear_animation.map_or(0.0, |(active_list, started)| {
            if active_list != list_id {
                0.0
            } else {
                egui::emath::easing::cubic_in_out(
                    (started.elapsed().as_secs_f32() / CLEAR_ANIMATION_SECONDS).clamp(0.0, 1.0),
                )
            }
        })
    }

    /// Moves every completed task in a list into its archive.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn archive_completed(&mut self, list_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
            if self
                .drag
                .as_ref()
                .is_some_and(|drag| drag.list_id == list_id)
            {
                self.drag = None;
            }
            let mut active = Vec::new();
            let mut completed = Vec::new();
            for task in self.lists[index].data.tasks.drain(..) {
                if task.completed {
                    completed.push(ArchivedTask::new(task));
                } else {
                    active.push(task);
                }
            }
            self.lists[index].data.tasks = active;
            self.lists[index].data.archive.splice(0..0, completed);
            self.save_list_index(index);
        }
    }

    /// Renders the animated archive panel and its restore actions.
    ///
    /// # Arguments
    ///
    /// - `ui` - Destination UI for the archive panel.
    /// - `list_id` - List whose archive is displayed.
    /// - `palette` - Colors used to render the panel.
    /// - `progress` - Panel reveal progress from zero to one.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid during an interaction.
    pub(super) fn history_ui(
        &mut self,
        ui: &mut egui::Ui,
        list_id: Uuid,
        palette: Palette,
        progress: f32,
    ) {
        let Some(index) = self.list_index(list_id) else {
            return;
        };
        let archived = self.lists[index].data.archive.clone();
        let full_height = 62.0 + archived.len() as f32 * 34.0;
        let visible_height = (full_height * progress).max(1.0);
        ui.add_space(8.0 * progress);
        let (clip_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), visible_height),
            Sense::hover(),
        );
        let content_rect = Rect::from_min_size(
            clip_rect.min + Vec2::new(0.0, -(1.0 - progress) * 12.0),
            Vec2::new(clip_rect.width(), full_height),
        );
        let mut history_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("history-panel-content", list_id))
                .max_rect(content_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        history_ui.set_clip_rect(clip_rect.intersect(ui.clip_rect()));
        history_ui.set_opacity(progress);
        Frame::NONE
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(14))
            .show(&mut history_ui, |ui| {
                ui.label(self.rich("History", 18.0, palette.text));
                for item in archived {
                    let row = ui.horizontal(|ui| {
                        ui.label(
                            self.rich(item.task.text.clone(), 15.0, palette.muted)
                                .strikethrough(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new(self.rich("Restore", 13.0, palette.accent))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                self.restore_archived(list_id, item.task.id);
                            }
                        });
                    });
                    if let Some(spotlight) = self.spotlight.as_mut().filter(|spotlight| {
                        spotlight.archived
                            && spotlight.list_id == list_id
                            && spotlight.task_id == item.task.id
                    }) {
                        if spotlight.scroll_pending {
                            ui.scroll_to_rect(row.response.rect, Some(Align::Center));
                            spotlight.scroll_pending = false;
                        }
                        let opacity =
                            (1.0 - spotlight.started.elapsed().as_secs_f32() / 1.4).clamp(0.0, 1.0);
                        ui.painter().rect_stroke(
                            row.response.rect.shrink(1.0),
                            8.0,
                            Stroke::new(1.5, palette.accent.linear_multiply(opacity)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            });
    }

    fn restore_archived(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id)
            && let Some(position) = self.lists[index]
                .data
                .archive
                .iter()
                .position(|item| item.task.id == task_id)
        {
            let mut task = self.lists[index].data.archive.remove(position).task;
            task.completed = false;
            task.completed_at_unix = None;
            self.lists[index].data.tasks.push(task);
            self.save_list_index(index);
        }
    }
}
