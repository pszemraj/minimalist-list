//! Checklist edits, task recovery, moving, and archival mutations.

use super::{DeletedItem, MinimalistApp, SubtaskEdit};
use crate::{
    app::{CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, keyboard::FocusItem},
    model::{ArchivedTask, Subtask},
};
use eframe::egui::{self, Id};
use std::time::Instant;
use uuid::Uuid;

impl MinimalistApp {
    /// Adds the current checklist draft to its originating parent.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn add_subtask(&mut self, list: Uuid, task: Uuid) {
        let text = self.new_subtask.trim();
        if text.is_empty() {
            return;
        }
        if let Some(index) = self.list_index(list)
            && let Some(parent) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|item| item.id == task)
        {
            parent.subtasks.push(Subtask::new(text));
            self.new_subtask.clear();
            self.save_list_index(index);
        }
    }

    /// Toggles a checklist completion state and saves its parent list.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    /// - `child` - Checklist item identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn toggle_subtask(&mut self, list: Uuid, task: Uuid, child: Uuid) {
        if let Some(index) = self.list_index(list)
            && let Some(subtask) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|item| item.id == task)
                .and_then(|parent| parent.subtasks.iter_mut().find(|item| item.id == child))
        {
            subtask.completed = !subtask.completed;
            self.save_list_index(index);
        }
    }

    /// Starts a checklist edit after saving the outgoing title or checklist edit.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    /// - `child` - Checklist item identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn begin_subtask_edit(&mut self, list: Uuid, task: Uuid, child: Uuid) {
        self.finish_inline_edit();
        let Some(subtask) = self
            .list_index(list)
            .and_then(|index| {
                self.lists[index]
                    .data
                    .tasks
                    .iter()
                    .find(|item| item.id == task)
            })
            .and_then(|parent| parent.subtasks.iter().find(|item| item.id == child))
        else {
            return;
        };
        self.details.editor = Some(SubtaskEdit {
            list,
            task,
            subtask: child,
            text: subtask.text.clone(),
            request_focus: true,
        });
    }

    /// Applies an outgoing checklist edit without changing its identity or state.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn finish_subtask_edit(&mut self) {
        let Some(edit) = self.details.editor.take() else {
            return;
        };
        let text = edit.text.trim();
        if let Some(index) = self.list_index(edit.list)
            && let Some(child) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == edit.task)
                .and_then(|task| {
                    task.subtasks
                        .iter_mut()
                        .find(|child| child.id == edit.subtask)
                })
            && !text.is_empty()
            && child.text != text
        {
            child.text = text.to_owned();
            self.save_list_index(index);
        }
    }

    /// Cancels the checklist draft and restores focus to the original child.
    pub(in crate::app) fn cancel_subtask_edit(&mut self) {
        if let Some(edit) = self.details.editor.take() {
            self.keyboard.pending =
                Some(FocusItem::Subtask(edit.list, edit.task, edit.subtask).id());
        }
    }

    /// Removes a checklist item while retaining its original position for Undo.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    /// - `child` - Checklist item identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn delete_subtask(&mut self, list: Uuid, task: Uuid, child: Uuid) {
        self.finish_subtask_edit();
        let neighbors = self.focus_neighbors();
        if let Some(index) = self.list_index(list)
            && let Some(parent) = self.lists[index]
                .data
                .tasks
                .iter_mut()
                .find(|item| item.id == task)
            && let Some(position) = parent.subtasks.iter().position(|item| item.id == child)
        {
            let subtask = parent.subtasks.remove(position);
            self.details.deleted = Some(DeletedItem::Subtask {
                list,
                task,
                position,
                subtask,
            });
            self.save_list_index(index);
            self.repair_focus(&neighbors);
        }
    }

    /// Starts archiving after committing the completed task's current editor.
    pub(in crate::app) fn begin_clear_completed(&mut self, list: Uuid) {
        self.finish_inline_edit();
        if self.clear_animation.is_none()
            && self
                .lists
                .iter()
                .any(|item| item.key == list && item.data.completed_count() > 0)
        {
            self.revealed = None;
            self.clear_animation = Some((list, Instant::now()));
        }
    }

    /// Starts deletion after committing editor content for Undo restoration.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Task to remove.
    pub(in crate::app) fn begin_delete_task(&mut self, list: Uuid, task: Uuid) {
        self.finish_inline_edit();
        if self.delete_animation.is_none() {
            self.revealed = None;
            self.switch_subtask_panel(None);
            self.delete_animation = Some((list, task, Instant::now()));
        }
    }

    /// Returns the eased removal animation progress for the given task.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Task whose animation is queried.
    ///
    /// # Returns
    ///
    /// Eased animation progress in the range zero to one.
    ///
    /// # Panics
    ///
    /// Panics if the configured animation duration is zero.
    pub(in crate::app) fn delete_progress(&self, list: Uuid, task: Uuid) -> f32 {
        self.delete_animation
            .map_or(0.0, |(active_list, active_task, started)| {
                if active_list != list || active_task != task {
                    0.0
                } else {
                    egui::emath::easing::cubic_in_out(
                        (started.elapsed().as_secs_f32() / DELETE_ANIMATION_SECONDS)
                            .clamp(0.0, 1.0),
                    )
                }
            })
    }

    /// Removes a task while retaining its complete content and original position for Undo.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Task to remove.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn remove_task(&mut self, list: Uuid, task: Uuid) {
        self.finish_inline_edit();
        let neighbors = self.focus_neighbors();
        if let Some(index) = self.list_index(list)
            && let Some(position) = self.lists[index]
                .data
                .tasks
                .iter()
                .position(|item| item.id == task)
        {
            if self.drag.as_ref().is_some_and(|drag| drag.list_id == list) {
                self.drag = None;
            }
            let manual_position = self.manual_task_position(list, task).unwrap_or(position);
            let task = self.lists[index].data.tasks.remove(position);
            self.details.deleted = Some(DeletedItem::Task {
                list,
                position: manual_position,
                task,
            });
            self.save_list_index(index);
            self.repair_focus(&neighbors);
        }
    }

    /// Restores the latest deletion at its original position, preserving all content.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn undo_delete(&mut self) {
        let Some(deleted) = self.details.deleted.take() else {
            return;
        };
        let list = match &deleted {
            DeletedItem::Task { list, .. } | DeletedItem::Subtask { list, .. } => *list,
        };
        let Some(index) = self.list_index(list) else {
            self.details.deleted = Some(deleted);
            return;
        };
        let mut restored_task_position = None;
        let focus = match deleted {
            DeletedItem::Task { position, task, .. } => {
                let focus = FocusItem::Task(list, task.id);
                if self.lists.iter().any(|item| {
                    item.data
                        .tasks
                        .iter()
                        .any(|existing| existing.id == task.id)
                        || item
                            .data
                            .archive
                            .iter()
                            .any(|existing| existing.task.id == task.id)
                }) {
                    self.status = Some("The deleted task already exists in the workspace.".into());
                    return;
                }
                let position = position.min(self.lists[index].data.tasks.len());
                restored_task_position = Some((task.id, position));
                self.lists[index].data.tasks.insert(position, task);
                focus
            }
            DeletedItem::Subtask {
                task,
                position,
                subtask,
                ..
            } => {
                let archived = self.lists[index]
                    .data
                    .archive
                    .iter()
                    .any(|item| item.task.id == task);
                let data = &mut self.lists[index].data;
                let Some(parent) = data
                    .tasks
                    .iter_mut()
                    .chain(data.archive.iter_mut().map(|item| &mut item.task))
                    .find(|item| item.id == task)
                else {
                    self.details.deleted = Some(DeletedItem::Subtask {
                        list,
                        task,
                        position,
                        subtask,
                    });
                    self.status =
                        Some("Restore the parent task before restoring this subtask.".into());
                    return;
                };
                let focus = if archived {
                    FocusItem::Archived(list, task)
                } else {
                    FocusItem::Subtask(list, task, subtask.id)
                };
                if !parent.subtasks.iter().any(|item| item.id == subtask.id) {
                    parent
                        .subtasks
                        .insert(position.min(parent.subtasks.len()), subtask);
                }
                if matches!(self.screen, crate::app::Screen::List(active) if active == list) {
                    if archived {
                        self.history_open = true;
                        self.expand_archived(list, task);
                    } else {
                        self.switch_subtask_panel(Some((list, task)));
                    }
                }
                focus
            }
        };
        if let Some((task, position)) = restored_task_position {
            self.restore_manual_task_position(list, task, position);
        }
        self.save_list_index(index);
        self.apply_task_sorts();
        if matches!(self.screen, crate::app::Screen::List(active) if active == list) {
            self.keyboard.pending = Some(focus.id());
        }
    }

    /// Returns the eased archive animation progress for completed rows.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `completed` - Whether this task participates in archival.
    ///
    /// # Returns
    ///
    /// Eased animation progress in the range zero to one.
    ///
    /// # Panics
    ///
    /// Panics if the configured animation duration is zero.
    pub(in crate::app) fn clear_progress(&self, list: Uuid, completed: bool) -> f32 {
        if !completed {
            return 0.0;
        }
        self.clear_animation.map_or(0.0, |(active, started)| {
            if active != list {
                0.0
            } else {
                egui::emath::easing::cubic_in_out(
                    (started.elapsed().as_secs_f32() / CLEAR_ANIMATION_SECONDS).clamp(0.0, 1.0),
                )
            }
        })
    }

    /// Archives completed tasks after committing editors and clearing stale panel state.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn archive_completed(&mut self, list: Uuid) {
        self.finish_inline_edit();
        let neighbors = self.focus_neighbors();
        if let Some(index) = self.list_index(list) {
            if self.lists[index].data.completed_count() == 0 {
                return;
            }
            if self.drag.as_ref().is_some_and(|drag| drag.list_id == list) {
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
            if self.subtasks_for.is_some_and(|(parent_list, task)| {
                parent_list == list && completed.iter().any(|item| item.task.id == task)
            }) {
                self.switch_subtask_panel(None);
            }
            self.lists[index].data.archive.splice(0..0, completed);
            self.save_list_index(index);
            self.repair_focus(&neighbors);
        }
    }

    /// Restores an archived task as incomplete without changing its checklist.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Archived task identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn restore_archived(&mut self, list: Uuid, task: Uuid) {
        if let Some(index) = self.list_index(list)
            && let Some(position) = self.lists[index]
                .data
                .archive
                .iter()
                .position(|item| item.task.id == task)
        {
            let mut restored = self.lists[index].data.archive.remove(position).task;
            restored.completed = false;
            restored.completed_at_unix = None;
            let position = self.lists[index].data.tasks.len();
            self.lists[index].data.tasks.push(restored);
            self.restore_manual_task_position(list, task, position);
            if self.lists[index].data.archive.is_empty() {
                self.history_open = false;
            }
            self.details.expanded_history.remove(&(list, task));
            self.save_list_index(index);
            self.keyboard.pending = Some(FocusItem::Task(list, task).id());
        }
    }

    /// Opens the task's destination picker from its existing action menu.
    ///
    /// # Arguments
    ///
    /// - `list` - Source list identity.
    /// - `task` - Task to move.
    pub(in crate::app) fn begin_move_task(&mut self, list: Uuid, task: Uuid) {
        self.finish_inline_edit();
        self.drag = None;
        self.revealed = None;
        if let Some(destination) = self
            .lists
            .iter()
            .find(|item| item.key != list)
            .map(|item| item.key)
        {
            self.details.move_task = Some((list, task, destination));
        } else {
            self.status = Some("Create another list before moving a task.".into());
        }
    }

    /// Moves a task between ordinary list files, rolling back both lists on failure.
    fn move_task_to_list(&mut self, source: Uuid, task: Uuid, destination: Uuid) -> bool {
        if source == destination {
            return false;
        }
        self.finish_inline_edit();
        let (Some(from), Some(to)) = (self.list_index(source), self.list_index(destination)) else {
            return false;
        };
        let Some(position) = self.lists[from]
            .data
            .tasks
            .iter()
            .position(|item| item.id == task)
        else {
            return false;
        };
        if self.lists[to].data.tasks.iter().any(|item| item.id == task)
            || self.lists[to]
                .data
                .archive
                .iter()
                .any(|item| item.task.id == task)
        {
            self.status = Some(
                "The destination already contains this task. Resolve the duplicate before moving."
                    .into(),
            );
            return false;
        }
        let neighbors = self.focus_neighbors();
        let manual_position = self.manual_task_position(source, task).unwrap_or(position);
        let original_source = self.lists[from].data.clone();
        let original_destination = self.lists[to].data.clone();
        let moved = self.lists[from].data.tasks[position].clone();
        self.lists[to].data.tasks.insert(0, moved);
        let destination_saved = self.save_list_index(to);
        if destination_saved
            && let Some(position) = self.lists[from]
                .data
                .tasks
                .iter()
                .position(|item| item.id == task)
        {
            self.lists[from].data.tasks.remove(position);
            if self.save_list_index(from) {
                self.move_subtask_draft(source, destination, task);
                if let Some(DeletedItem::Subtask {
                    list, task: parent, ..
                }) = &mut self.details.deleted
                    && *list == source
                    && *parent == task
                {
                    *list = destination;
                }
                self.repair_focus(&neighbors);
                self.acknowledge(format!("Moved to {}", self.lists[to].data.title));
                return true;
            }
        }
        self.lists[from].data = original_source;
        self.lists[to].data = original_destination;
        self.restore_manual_task_position(source, task, manual_position);
        let failure = self.status.clone();
        self.save_list_index(from);
        self.save_list_index(to);
        self.status = Some(format!(
            "Move did not finish; the task remains in its original list. {}",
            failure.unwrap_or_default()
        ));
        false
    }

    /// Renders one-step deletion recovery and the destination picker.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn task_operations_ui(&mut self, ctx: &egui::Context) {
        if self.details.deleted.is_some() {
            egui::Area::new(Id::new("delete-undo"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Deleted");
                            if ui.button("Undo").clicked() {
                                self.undo_delete();
                            }
                            if ui.button("Dismiss").clicked() {
                                self.details.deleted = None;
                            }
                        });
                    });
                });
        }
        let Some((source, task, mut destination)) = self.details.move_task else {
            return;
        };
        let mut move_now = false;
        let mut cancel = false;
        let title = self
            .list_index(source)
            .and_then(|index| {
                self.lists[index]
                    .data
                    .tasks
                    .iter()
                    .find(|item| item.id == task)
            })
            .map(|item| item.text.clone());
        let Some(title) = title else {
            self.details.move_task = None;
            return;
        };
        let modal = egui::Modal::new(Id::new("move-task-modal")).show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 56.0).clamp(100.0, 360.0));
            ui.heading("Move to list");
            ui.add(egui::Label::new(title).wrap());
            egui::ComboBox::from_id_salt("move-destination")
                .selected_text(
                    self.list_index(destination)
                        .map(|index| self.lists[index].data.title.as_str())
                        .unwrap_or("Choose list"),
                )
                .show_ui(ui, |ui| {
                    for list in &self.lists {
                        if list.key != source {
                            ui.selectable_value(&mut destination, list.key, &list.data.title);
                        }
                    }
                });
            ui.horizontal(|ui| {
                move_now = ui.button("Move").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        self.details.move_task = Some((source, task, destination));
        if cancel
            || modal.should_close()
            || (move_now && self.move_task_to_list(source, task, destination))
        {
            self.details.move_task = None;
        }
    }
}

#[cfg(test)]
#[path = "task_operations_tests.rs"]
mod tests;
