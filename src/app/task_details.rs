use super::{CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, MinimalistApp, theme::Palette};
use crate::model::{ArchivedTask, Subtask, Task};
use eframe::egui;
use egui::{
    Align, Button, CornerRadius, Frame, Id, Key, Layout, Margin, Rect, Sense, TextEdit, Vec2,
};
use std::time::Instant;
use uuid::Uuid;

impl MinimalistApp {
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
                    ui.horizontal(|ui| {
                        let completion_progress = ui.ctx().animate_bool_with_time_and_easing(
                            Id::new(("subtask-complete", list_id, task.id, subtask.id)),
                            subtask.completed,
                            0.20,
                            egui::emath::easing::cubic_out,
                        );
                        let completion = ui.add_sized([26.0, 26.0], Button::new("").frame(false));
                        let completion = self.completion_control(
                            ui,
                            completion,
                            subtask.completed,
                            completion_progress,
                            palette,
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
                            if ui
                                .add(Button::new(self.rich("x", 14.0, palette.muted)).frame(false))
                                .clicked()
                            {
                                self.delete_subtask(list_id, task.id, subtask.id);
                            }
                        });
                    });
                }
                let subtask_response = ui.add(
                    TextEdit::singleline(&mut self.new_subtask)
                        .hint_text("Add subtask")
                        .desired_width(f32::INFINITY),
                );
                let submit = ui.input(|input| input.key_pressed(Key::Enter))
                    && (subtask_response.has_focus() || subtask_response.lost_focus());
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

    pub(super) fn begin_clear_completed(&mut self, list_id: Uuid) {
        if self.clear_animation.is_none() {
            self.revealed = None;
            self.clear_animation = Some((list_id, Instant::now()));
        }
    }

    pub(super) fn begin_delete_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if self.delete_animation.is_none() {
            self.revealed = None;
            self.subtasks_for = None;
            self.delete_animation = Some((list_id, task_id, Instant::now()));
        }
    }

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

    pub(super) fn remove_task(&mut self, list_id: Uuid, task_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
            self.lists[index]
                .data
                .tasks
                .retain(|task| task.id != task_id);
            self.save_list_index(index);
        }
    }

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

    pub(super) fn archive_completed(&mut self, list_id: Uuid) {
        if let Some(index) = self.list_index(list_id) {
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
                    ui.horizontal(|ui| {
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
