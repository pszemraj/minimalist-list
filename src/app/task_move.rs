//! Moving a task between lists: the destination dialog, the two-list write, and Move Undo.

use super::{MinimalistApp, MoveDialog, UndoItem};
use crate::{
    app::{
        Screen,
        keyboard::{FocusItem, take_key},
    },
    storage::StoredList,
};
use eframe::egui::{self, ComboBox, Id, Key, Modifiers, ScrollArea};
use uuid::Uuid;

/// A list's manual order and conflict snapshot, kept in case a write is abandoned.
struct WriteSnapshot {
    order: Option<Vec<Uuid>>,
    conflict: Option<StoredList>,
}

impl MinimalistApp {
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
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn undo_move(
        &mut self,
        source: Uuid,
        position: usize,
        task: Uuid,
        destination: Uuid,
    ) {
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
        } else if matches!(self.screen, Screen::List(active) if active == destination) {
            self.repair_focus_from(item, &neighbors);
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
            self.finish_inline_edit();
            self.drag = None;
            self.revealed = None;
            self.keyboard.pending = None;
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
    ///
    /// # Arguments
    ///
    /// - `source` - List that holds the task.
    /// - `task` - Task identity.
    /// - `destination` - List that receives the task, at its first Manual position.
    ///
    /// # Returns
    ///
    /// `true` once both lists are saved with the task in its new list.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn move_task_to_list(
        &mut self,
        source: Uuid,
        task: Uuid,
        destination: Uuid,
    ) -> bool {
        if source == destination {
            return false;
        }
        self.finish_inline_edit();
        let Some(from) = self.list_index(source) else {
            return false;
        };
        let Some(to) = self.list_index(destination) else {
            self.status = Some("That list is no longer available. Choose another list.".into());
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
        let mut remaining = self.lists[from].clone();
        let Some(source_position) = remaining.data.tasks.iter().position(|item| item.id == task)
        else {
            return Err("The task is no longer in its list.".into());
        };
        let moved = remaining.data.tasks.remove(source_position);
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
        if let Err(error) = self.save_list_candidate(from, remaining) {
            self.restore_write_snapshot(source_id, source_before);
            self.lists[to].data = previous_destination;
            self.restore_write_snapshot(destination_id, destination_before);
            self.save_list_index(to);
            return Err(error);
        }
        Ok(())
    }

    /// Renders the Move dialog and applies its keyboard handling.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn move_dialog_ui(&mut self, ctx: &egui::Context) {
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
            // The task left while the dialog was open, taking its return focus with it.
            self.keyboard.pending = self.default_focus();
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
                Some(1)
            } else if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                Some(others.len().saturating_sub(1))
            } else {
                None
            };
            if let Some(step) = step
                && !others.is_empty()
            {
                dialog.destination =
                    others[current.map_or(0, |index| (index + step) % others.len())];
            }
            move_now = take_key(ctx, Key::Enter, Modifiers::NONE);
        }
        let mut picker = None;
        let modal = egui::Modal::new(Id::new("move-task-modal")).show(ctx, |ui| {
            let available = ctx.content_rect().size() - egui::vec2(56.0, 56.0);
            ui.set_max_width(available.x.clamp(100.0, 360.0));
            ui.set_max_height(available.y.max(100.0));
            ui.heading("Move to list");
            ScrollArea::vertical()
                .id_salt(("move-task-title", source, task))
                .max_height((available.y - 110.0).max(40.0))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(title).wrap());
                });
            let combo = ComboBox::from_id_salt("move-destination")
                .width(ui.available_width())
                .wrap_mode(egui::TextWrapMode::Truncate)
                .selected_text(
                    self.list_index(dialog.destination)
                        .map(|index| self.lists[index].data.title.as_str())
                        .unwrap_or("Choose list"),
                )
                .show_ui(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
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
