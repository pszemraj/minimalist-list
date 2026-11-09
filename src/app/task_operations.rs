//! Checklist edits, task recovery, and archival mutations.

use super::{MinimalistApp, PastedBatch, SubtaskEdit, UndoItem};
use crate::{
    app::{CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, Screen, keyboard::FocusItem},
    model::{ArchivedTask, Subtask, Task},
    storage,
};
use eframe::egui::{self, Id};
use std::{collections::HashSet, time::Instant};
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
        let parent_index = self.lists[index]
            .data
            .tasks
            .iter()
            .position(|item| item.id == task)?;
        let subtasks: Vec<_> = texts.iter().map(Subtask::new).collect();
        if subtasks.len() > 1 {
            self.details.undo = Some(UndoItem::Pasted {
                list,
                batch: PastedBatch::Subtasks {
                    parent: task,
                    items: subtasks
                        .iter()
                        .map(|child| (child.id, child.clone()))
                        .collect(),
                },
            });
        }
        self.lists[index].data.tasks[parent_index]
            .subtasks
            .extend(subtasks);
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

    /// Saves the active inline title or checklist edit before leaving its editor.
    pub(in crate::app) fn finish_inline_edit(&mut self) {
        self.finish_subtask_edit();
        if let Some((list_id, task_id)) = self.editing {
            self.commit_task_edit(list_id, task_id, &self.edit_text.clone());
        }
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

    /// Tracks a task paste for Undo, including the contents before any later edit.
    ///
    /// # Arguments
    /// - `list` - Destination list.
    /// - `tasks` - The tasks added in this operation.
    pub(in crate::app) fn record_task_paste_undo(&mut self, list: Uuid, tasks: &[Task]) {
        if tasks.len() > 1 {
            self.details.undo = Some(UndoItem::Pasted {
                list,
                batch: PastedBatch::Tasks(
                    tasks.iter().map(|task| (task.id, task.clone())).collect(),
                ),
            });
        }
    }

    /// Identifies an empty entry where Ctrl+Z can undo a paste without undoing draft text.
    ///
    /// # Returns
    /// Whether the matching entry owns focus and has no text input to undo.
    pub(in crate::app) fn can_undo_paste_in_entry(&self, ctx: &egui::Context) -> bool {
        let Some(UndoItem::Pasted { list, batch }) = &self.details.undo else {
            return false;
        };
        let (field, draft) = match batch.parent() {
            Some(task) => (Id::new(("new-subtask", list, task)), &self.new_subtask),
            None => (Id::new(("new-task", list)), &self.new_task),
        };
        draft.is_empty()
            && ctx.memory(|memory| memory.has_focus(field))
            && !ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Ime(_)
                    )
                })
            })
    }

    /// Reverses the latest deletion, move, or paste in one step.
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
            Some(UndoItem::Archived {
                list,
                position,
                item,
            }) => self.undo_archived_deletion(list, position, item),
            Some(UndoItem::Pasted { list, batch }) => self.undo_paste(list, batch),
            None => {}
        }
    }

    fn undo_paste(&mut self, list: Uuid, batch: PastedBatch) {
        self.finish_inline_edit();
        let neighbors = self.focus_neighbors();
        let Some(index) = self.list_index(list) else {
            self.details.undo = Some(UndoItem::Pasted { list, batch });
            self.status = Some("The pasted items' list is unavailable.".into());
            return;
        };
        let removed = match &batch {
            PastedBatch::Tasks(items) => {
                let protected: HashSet<_> = items
                    .keys()
                    .copied()
                    .filter(|&task| self.has_subtask_draft(list, task))
                    .collect();
                let tasks = &mut self.lists[index].data.tasks;
                let before = tasks.len();
                tasks.retain(|task| {
                    protected.contains(&task.id) || items.get(&task.id) != Some(task)
                });
                before - tasks.len()
            }
            PastedBatch::Subtasks { parent, items } => {
                let Some(task) = self.lists[index]
                    .data
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == *parent)
                else {
                    self.details.undo = Some(UndoItem::Pasted { list, batch });
                    self.status = Some("The pasted items' parent task is not active.".into());
                    return;
                };
                let before = task.subtasks.len();
                task.subtasks
                    .retain(|child| items.get(&child.id) != Some(child));
                before - task.subtasks.len()
            }
        };
        if removed == 0 {
            self.acknowledge("No unchanged pasted items to remove.".into());
            return;
        }
        self.drag = None;
        if self.save_list_index(index) {
            self.acknowledge(if removed == batch.len() {
                "Paste undone".into()
            } else {
                "Unchanged pasted items removed.".into()
            });
        }
        self.repair_focus(&neighbors);
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
        if self.task_id_exists(task.id) {
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

    /// Reports whether any list holds an active or archived task with this identity.
    fn task_id_exists(&self, id: Uuid) -> bool {
        self.lists.iter().any(|item| {
            item.data.tasks.iter().any(|existing| existing.id == id)
                || item
                    .data
                    .archive
                    .iter()
                    .any(|existing| existing.task.id == id)
        })
    }

    /// Reinserts a permanently deleted History entry at its original position and time.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    fn undo_archived_deletion(&mut self, list: Uuid, position: usize, item: ArchivedTask) {
        let Some(index) = self.list_index(list) else {
            self.details.undo = Some(UndoItem::Archived {
                list,
                position,
                item,
            });
            return;
        };
        if self.task_id_exists(item.task.id) {
            // Keep Undo available so a stale or different duplicate can be resolved.
            self.details.undo = Some(UndoItem::Archived {
                list,
                position,
                item,
            });
            self.status = Some("The deleted task already exists in the workspace.".into());
            return;
        }
        let id = item.task.id;
        let archive = &mut self.lists[index].data.archive;
        archive.insert(position.min(archive.len()), item);
        if matches!(self.screen, Screen::List(active) if active == list) {
            self.history_open = true;
        }
        self.finish_undo(list, index, FocusItem::Archived(list, id));
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

    /// Permanently removes one History entry, retaining it with its time for Undo.
    ///
    /// # Arguments
    ///
    /// - `list` - Parent list identity.
    /// - `task` - Archived task identity.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn delete_archived(&mut self, list: Uuid, task: Uuid) {
        let neighbors = self.focus_neighbors();
        if let Some(index) = self.list_index(list)
            && let Some(position) = self.lists[index]
                .data
                .archive
                .iter()
                .position(|item| item.task.id == task)
        {
            let item = self.lists[index].data.archive.remove(position);
            self.details.undo = Some(UndoItem::Archived {
                list,
                position,
                item,
            });
            self.details.expanded_history.remove(&(list, task));
            if self.lists[index].data.archive.is_empty() {
                self.history_open = false;
            }
            self.save_list_index(index);
            self.repair_focus(&neighbors);
        }
    }

    /// Empties a list's History without touching its active tasks.
    ///
    /// Pending edits must reach disk before taking the forced snapshot, so it
    /// includes every entry being cleared. Either failure leaves History intact.
    ///
    /// # Arguments
    ///
    /// - `list` - List whose archived tasks are removed.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(in crate::app) fn clear_history(&mut self, list: Uuid) {
        if let Some(index) = self.list_index(list) {
            if self.persistence.dirty_lists.contains(&list) && !self.save_list_index(index) {
                return;
            }
            if let Err(error) = storage::snapshot_list_now(&self.workspace, &self.lists[index]) {
                self.status = Some(format!(
                    "History was kept because no backup could be written: {error}"
                ));
                return;
            }
            self.lists[index].data.archive.clear();
            self.details
                .expanded_history
                .retain(|(parent, _)| *parent != list);
            self.history_open = false;
            self.save_list_index(index);
            self.clear_resolved_save_error();
        }
    }

    /// Renders one-step Undo for task changes, then the Move dialog.
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
            Some(UndoItem::Pasted { batch, .. }) => Some(format!(
                "Pasted {} {}",
                batch.len(),
                if batch.parent().is_some() {
                    "subtasks"
                } else {
                    "tasks"
                },
            )),
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
