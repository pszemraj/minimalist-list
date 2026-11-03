//! Checklist edits, task recovery, and archival mutations.

use super::{MinimalistApp, SubtaskEdit, UndoItem};
use crate::{
    app::{CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, Screen, keyboard::FocusItem},
    model::{ArchivedTask, Subtask, Task},
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
        let text = self.new_subtask.trim().to_owned();
        if !text.is_empty() && self.add_subtasks(list, task, &[text]).is_some() {
            self.new_subtask.clear();
        }
    }

    /// Appends checklist items to a parent task in one save, keeping their given order.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Parent task identity.
    /// - `texts` - Item titles in top-to-bottom order.
    ///
    /// # Returns
    ///
    /// `None` if the parent is missing; otherwise whether the list reached disk.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn add_subtasks(
        &mut self,
        list: Uuid,
        task: Uuid,
        texts: &[String],
    ) -> Option<bool> {
        let index = self.list_index(list)?;
        let parent = self.lists[index]
            .data
            .tasks
            .iter_mut()
            .find(|item| item.id == task)?;
        parent.subtasks.extend(texts.iter().map(Subtask::new));
        Some(self.save_list_index(index))
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
            self.details.undo = Some(UndoItem::Subtask {
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
            self.details.undo = Some(UndoItem::Task {
                list,
                position: manual_position,
                task,
            });
            self.save_list_index(index);
            self.repair_focus(&neighbors);
        }
    }

    /// Reverses the latest deletion or move in one step, preserving all content.
    pub(in crate::app) fn undo_last(&mut self) {
        match self.details.undo.take() {
            Some(UndoItem::Task {
                list,
                position,
                task,
            }) => self.undo_task_deletion(list, position, task),
            Some(UndoItem::Subtask {
                list,
                task,
                position,
                subtask,
            }) => self.undo_subtask_deletion(list, task, position, subtask),
            Some(UndoItem::Moved {
                source,
                position,
                task,
                destination,
            }) => self.undo_move(source, position, task, destination),
            None => {}
        }
    }

    /// Reinserts a deleted task at its original Manual position.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    fn undo_task_deletion(&mut self, list: Uuid, position: usize, task: Task) {
        let Some(index) = self.list_index(list) else {
            self.details.undo = Some(UndoItem::Task {
                list,
                position,
                task,
            });
            return;
        };
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
            // Keep Undo available so a stale or different duplicate can be resolved.
            self.details.undo = Some(UndoItem::Task {
                list,
                position,
                task,
            });
            self.status = Some("The deleted task already exists in the workspace.".into());
            return;
        }
        let id = task.id;
        let position = position.min(self.lists[index].data.tasks.len());
        self.lists[index].data.tasks.insert(position, task);
        self.restore_manual_task_position(list, id, position);
        self.finish_undo(list, index, FocusItem::Task(list, id));
    }

    /// Reinserts a deleted checklist item at its original position.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    fn undo_subtask_deletion(&mut self, list: Uuid, task: Uuid, position: usize, subtask: Subtask) {
        let Some(index) = self.list_index(list) else {
            self.details.undo = Some(UndoItem::Subtask {
                list,
                task,
                position,
                subtask,
            });
            return;
        };
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
            self.details.undo = Some(UndoItem::Subtask {
                list,
                task,
                position,
                subtask,
            });
            self.status = Some("Restore the parent task before restoring this subtask.".into());
            return;
        };
        let focus = if archived {
            FocusItem::Archived(list, task)
        } else {
            FocusItem::Subtask(list, task, subtask.id)
        };
        if parent.subtasks.iter().any(|item| item.id == subtask.id) {
            // Keep Undo available so a stale or different duplicate can be resolved.
            self.details.undo = Some(UndoItem::Subtask {
                list,
                task,
                position,
                subtask,
            });
            self.status = Some("The deleted subtask already exists in its task.".into());
            return;
        }
        parent
            .subtasks
            .insert(position.min(parent.subtasks.len()), subtask);
        if matches!(self.screen, Screen::List(active) if active == list) {
            if archived {
                self.history_open = true;
                self.expand_archived(list, task);
            } else {
                self.switch_subtask_panel(Some((list, task)));
            }
        }
        self.finish_undo(list, index, focus);
    }

    /// Saves a restored deletion and focuses it when its list is open.
    ///
    /// # Panics
    ///
    /// Panics if the caller supplies an invalid loaded-list index.
    fn finish_undo(&mut self, list: Uuid, index: usize, focus: FocusItem) {
        self.save_list_index(index);
        self.apply_task_sorts();
        if matches!(self.screen, Screen::List(active) if active == list) {
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

    /// Renders one-step Undo for deletions and moves, then the Move dialog.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn task_operations_ui(&mut self, ctx: &egui::Context) {
        let toast = match &self.details.undo {
            Some(UndoItem::Moved { destination, .. }) => Some(
                self.list_index(*destination)
                    .map_or("Moved".to_owned(), |index| {
                        format!("Moved to {}", clipped(&self.lists[index].data.title, 40))
                    }),
            ),
            Some(_) => Some("Deleted".to_owned()),
            None => None,
        };
        if let Some(message) = toast {
            egui::Area::new(Id::new("delete-undo"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(message);
                            if ui.button("Undo").clicked() {
                                self.undo_last();
                            }
                            if ui.button("Dismiss").clicked() {
                                self.details.undo = None;
                            }
                        });
                    });
                });
        }
        self.move_dialog_ui(ctx);
    }
}

/// Shortens a list title for the Undo notice.
fn clipped(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars().take(limit - 1).chain(['…']).collect()
}

#[cfg(test)]
#[path = "task_operations_tests.rs"]
mod tests;
