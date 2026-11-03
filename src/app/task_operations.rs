//! Checklist edits, task recovery, moving, and archival mutations.

use super::{MinimalistApp, MoveDialog, SubtaskEdit, UndoItem};
use crate::{
    app::{
        CLEAR_ANIMATION_SECONDS, DELETE_ANIMATION_SECONDS, Screen,
        keyboard::{FocusItem, take_key},
    },
    model::{ArchivedTask, Subtask, Task},
    storage::StoredList,
};
use eframe::egui::{self, ComboBox, Id, Key, Modifiers};
use std::time::Instant;
use uuid::Uuid;

/// A list's manual order and conflict snapshot, kept in case a write is abandoned.
struct WriteSnapshot {
    order: Option<Vec<Uuid>>,
    conflict: Option<StoredList>,
}

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

    /// Returns a moved task to its original Manual position in the source list.
    ///
    /// Undo stays available, with a status message, when either list or the task is gone.
    ///
    /// # Arguments
    ///
    /// - `source` - List the task came from.
    /// - `position` - Its original Manual position there.
    /// - `task` - Task identity.
    /// - `destination` - List the task was moved to.
    fn undo_move(&mut self, source: Uuid, position: usize, task: Uuid, destination: Uuid) {
        let undo = UndoItem::Moved {
            source,
            position,
            task,
            destination,
        };
        let (Some(from), Some(to)) = (self.list_index(destination), self.list_index(source)) else {
            self.details.undo = Some(undo);
            self.status = Some("Undo needs both lists from the move to still exist.".into());
            return;
        };
        if !self.lists[from]
            .data
            .tasks
            .iter()
            .any(|item| item.id == task)
        {
            self.details.undo = Some(undo);
            self.status = Some("The moved task is no longer in its new list.".into());
            return;
        }
        if self.lists[to].data.tasks.iter().any(|item| item.id == task)
            || self.lists[to]
                .data
                .archive
                .iter()
                .any(|item| item.task.id == task)
        {
            self.details.undo = Some(undo);
            self.status = Some("The original list already contains this task.".into());
            return;
        }
        let item = FocusItem::Task(destination, task);
        let neighbors = self.focus_siblings(item);
        if let Err(error) = self.transfer_task(from, to, task, position) {
            self.details.undo = Some(undo);
            self.status = Some(format!(
                "Undo did not finish; the task remains in its new list. {error}"
            ));
            return;
        }
        self.move_subtask_draft(destination, source, task);
        if matches!(self.screen, Screen::List(active) if active == source) {
            self.keyboard.pending = Some(FocusItem::Task(source, task).id());
        } else {
            self.repair_focus_from(item, &neighbors);
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

    /// Opens the task's destination picker from its action menu or keyboard shortcut.
    ///
    /// The picker starts on the last destination used this session when that list
    /// still exists, and otherwise on the first other list.
    ///
    /// # Arguments
    ///
    /// - `list` - Source list identity.
    /// - `task` - Task to move.
    pub(in crate::app) fn begin_move_task(&mut self, list: Uuid, task: Uuid) {
        self.finish_inline_edit();
        self.drag = None;
        self.revealed = None;
        self.keyboard.pending = None;
        let remembered = self
            .details
            .last_move_destination
            .filter(|id| *id != list && self.list_index(*id).is_some());
        if let Some(destination) = remembered.or_else(|| {
            self.lists
                .iter()
                .find(|item| item.key != list)
                .map(|item| item.key)
        }) {
            self.details.move_task = Some(MoveDialog {
                source: list,
                task,
                destination,
                picker: None,
                request_focus: true,
            });
        } else {
            self.status = Some("Create another list before moving a task.".into());
        }
    }

    /// Moves a task between ordinary list files, leaving both lists unchanged on failure.
    fn move_task_to_list(&mut self, source: Uuid, task: Uuid, destination: Uuid) -> bool {
        if source == destination {
            return false;
        }
        self.finish_inline_edit();
        let (Some(from), Some(to)) = (self.list_index(source), self.list_index(destination)) else {
            return false;
        };
        if !self.lists[from]
            .data
            .tasks
            .iter()
            .any(|item| item.id == task)
        {
            return false;
        }
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
        let manual_position = self.manual_task_position(source, task).unwrap_or(0);
        let moved = FocusItem::Task(source, task);
        let neighbors = self.focus_siblings(moved);
        if let Err(error) = self.transfer_task(from, to, task, 0) {
            self.status = Some(format!(
                "Move did not finish; the task remains in its original list. {error}"
            ));
            return false;
        }
        self.move_subtask_draft(source, destination, task);
        self.details.undo = Some(UndoItem::Moved {
            source,
            position: manual_position,
            task,
            destination,
        });
        self.details.last_move_destination = Some(destination);
        self.repair_focus_from(moved, &neighbors);
        true
    }

    /// Captures state that a candidate write changes even when it fails.
    fn write_snapshot(&self, list: Uuid) -> WriteSnapshot {
        WriteSnapshot {
            order: self.manual_task_order.get(&list).cloned(),
            conflict: self.save_conflict_snapshot(list),
        }
    }

    /// Restores the state captured by [`Self::write_snapshot`].
    fn restore_write_snapshot(&mut self, list: Uuid, snapshot: WriteSnapshot) {
        match snapshot.order {
            Some(order) => {
                self.manual_task_order.insert(list, order);
            }
            None => {
                self.manual_task_order.remove(&list);
            }
        }
        self.restore_save_conflict(list, snapshot.conflict);
    }

    /// Writes a task into one list and then out of another, restoring both on failure.
    ///
    /// The destination is written first. A failed write adopts nothing, and an
    /// unchanged list is neither marked dirty nor left holding a conflict snapshot,
    /// so retries and external-change scans continue normally.
    ///
    /// # Arguments
    ///
    /// - `from` - Loaded index of the list that holds the task.
    /// - `to` - Loaded index of the list that receives it.
    /// - `task` - Task identity.
    /// - `position` - Manual position in the destination, clamped to its length.
    ///
    /// # Returns
    ///
    /// `Ok(())` once both lists are saved with the task in its new list.
    ///
    /// # Errors
    ///
    /// Returns the storage error of the failed write. If removing the task from the
    /// source fails after the destination was written, the destination is saved again
    /// without the task and stays dirty only if that write also fails.
    ///
    /// # Panics
    ///
    /// Panics if either index is invalid.
    pub(in crate::app) fn transfer_task(
        &mut self,
        from: usize,
        to: usize,
        task: Uuid,
        position: usize,
    ) -> Result<(), String> {
        let (source_id, destination_id) = (self.lists[from].key, self.lists[to].key);
        let Some(moved) = self.lists[from]
            .data
            .tasks
            .iter()
            .find(|item| item.id == task)
            .cloned()
        else {
            return Err("The task is no longer in its list.".into());
        };
        let source_before = self.write_snapshot(source_id);
        let destination_before = self.write_snapshot(destination_id);
        let previous_destination = self.lists[to].data.clone();
        let mut candidate = self.lists[to].clone();
        let position = position.min(candidate.data.tasks.len());
        candidate.data.tasks.insert(position, moved);
        self.restore_manual_task_position(destination_id, task, position);
        if let Err(error) = self.save_list_candidate(to, candidate) {
            self.restore_write_snapshot(destination_id, destination_before);
            return Err(error);
        }
        let mut remaining = self.lists[from].clone();
        remaining.data.tasks.retain(|item| item.id != task);
        if let Err(error) = self.save_list_candidate(from, remaining) {
            self.restore_write_snapshot(source_id, source_before);
            self.lists[to].data = previous_destination;
            self.restore_write_snapshot(destination_id, destination_before);
            self.save_list_index(to);
            return Err(error);
        }
        Ok(())
    }

    /// Renders one-step Undo for deletions and moves, and the destination picker.
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
        let Some(mut dialog) = self.details.move_task.take() else {
            return;
        };
        let (source, task) = (dialog.source, dialog.task);
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
            return;
        };
        let mut move_now = false;
        let mut cancel = false;
        // Arrows and Enter act on the focused picker only while its list is closed.
        if dialog.picker.is_some_and(|id| {
            ctx.memory(|memory| memory.has_focus(id)) && !ComboBox::is_open(ctx, id)
        }) {
            let others: Vec<_> = self
                .lists
                .iter()
                .map(|list| list.key)
                .filter(|key| *key != source)
                .collect();
            let current = others.iter().position(|key| *key == dialog.destination);
            let step = if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
                1
            } else if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                others.len().saturating_sub(1)
            } else {
                0
            };
            if step > 0 && !others.is_empty() {
                dialog.destination =
                    others[current.map_or(0, |index| (index + step) % others.len())];
            }
            move_now = take_key(ctx, Key::Enter, Modifiers::NONE);
        }
        let mut picker = None;
        let modal = egui::Modal::new(Id::new("move-task-modal")).show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 56.0).clamp(100.0, 360.0));
            ui.heading("Move to list");
            ui.add(egui::Label::new(title).wrap());
            let combo = ComboBox::from_id_salt("move-destination")
                .selected_text(
                    self.list_index(dialog.destination)
                        .map(|index| self.lists[index].data.title.as_str())
                        .unwrap_or("Choose list"),
                )
                .show_ui(ui, |ui| {
                    for list in &self.lists {
                        if list.key != source
                            && ui
                                .selectable_value(
                                    &mut dialog.destination,
                                    list.key,
                                    &list.data.title,
                                )
                                .clicked()
                        {
                            // The choosing key must not also activate the picker or Move.
                            take_key(ctx, Key::Enter, Modifiers::NONE);
                            take_key(ctx, Key::Space, Modifiers::NONE);
                            dialog.request_focus = true;
                            ui.close();
                        }
                    }
                });
            picker = Some(combo.response.id);
            if dialog.request_focus {
                combo.response.request_focus();
                dialog.request_focus = false;
            }
            ui.horizontal(|ui| {
                move_now |= ui.button("Move").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        dialog.picker = picker;
        if cancel || modal.should_close() {
            self.keyboard.pending = Some(FocusItem::Task(source, task).id());
        } else if !move_now || !self.move_task_to_list(source, task, dialog.destination) {
            self.details.move_task = Some(dialog);
        }
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
