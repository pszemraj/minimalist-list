//! Directional navigation through task, checklist, History, and creation levels.

use super::{
    DELETE_TASK, FocusItem, MOVE_TASK_DOWN, MOVE_TASK_UP, MinimalistApp, RowAction, take_key,
    take_shortcut,
};
use eframe::egui::{self, Event, Id, Key, Modifiers};
use uuid::Uuid;

impl MinimalistApp {
    /// Closes History and restores its toggle, or the screen's default control.
    pub(super) fn close_history(&mut self) {
        self.history_open = false;
        self.keyboard.pending = self
            .keyboard
            .history_toggle
            .or_else(|| self.default_focus());
    }

    /// Routes arrows within a list and leaves activation to secondary controls.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context holding current input and keyboard focus.
    /// - `list` - List whose task and History levels are displayed.
    ///
    /// # Panics
    ///
    /// Panics if a resolved task or archive position is outside its nonempty list.
    pub(super) fn task_keyboard(&mut self, ctx: &egui::Context, list: Uuid) {
        let Some(index) = self.list_index(list) else {
            return;
        };
        let focused = ctx.memory(|memory| memory.focused());
        let entry = Id::new(("new-task", list));
        if focused == Some(entry) {
            if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
                let target = self.lists[index]
                    .data
                    .tasks
                    .first()
                    .map(|task| FocusItem::Task(list, task.id).id())
                    .or(self.keyboard.history_toggle)
                    .unwrap_or(entry);
                self.focus_now(ctx, target);
            }
            return;
        }
        if self.checklist_keyboard(ctx, list) || ctx.text_edit_focused() || self.editing.is_some() {
            return;
        }
        if focused.is_some() && focused == self.keyboard.history_toggle {
            if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                let target = self.lists[index]
                    .data
                    .tasks
                    .last()
                    .map_or(entry, |task| FocusItem::Task(list, task.id).id());
                self.focus_now(ctx, target);
            } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE)
                || take_key(ctx, Key::ArrowRight, Modifiers::NONE)
            {
                if let Some(task) = self.lists[index].data.archive.first() {
                    let target = FocusItem::Archived(list, task.task.id).id();
                    self.history_open = true;
                    self.focus_now(ctx, target);
                }
            } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
                self.close_history();
            }
            return;
        }
        let owner = self
            .keyboard
            .anchor
            .filter(|(id, _)| Some(*id) == focused)
            .map(|(_, item)| item);
        if let Some(position) = self.lists[index]
            .data
            .archive
            .iter()
            .position(|item| focused == Some(FocusItem::Archived(list, item.task.id).id()))
        {
            let archive = &self.lists[index].data.archive;
            let target = if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                if position == 0 {
                    self.keyboard.history_toggle.unwrap_or(entry)
                } else {
                    FocusItem::Archived(list, archive[position - 1].task.id).id()
                }
            } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
                FocusItem::Archived(list, archive[(position + 1).min(archive.len() - 1)].task.id)
                    .id()
            } else if take_key(ctx, Key::Home, Modifiers::NONE) {
                FocusItem::Archived(list, archive[0].task.id).id()
            } else if take_key(ctx, Key::End, Modifiers::NONE) {
                FocusItem::Archived(list, archive.last().unwrap().task.id).id()
            } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
                self.close_history();
                return;
            } else {
                return;
            };
            self.focus_now(ctx, target);
            return;
        }
        let tasks = &self.lists[index].data.tasks;
        let Some(position) = tasks.iter().position(|task| {
            focused == Some(FocusItem::Task(list, task.id).id())
                || owner == Some(FocusItem::Task(list, task.id))
        }) else {
            return;
        };
        let task = tasks[position].clone();
        let row = FocusItem::Task(list, task.id).id();
        if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
            let target = if position == 0 {
                entry
            } else {
                FocusItem::Task(list, tasks[position - 1].id).id()
            };
            self.focus_now(ctx, target);
        } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
            let target = if self.subtasks_for == Some((list, task.id)) {
                task.subtasks
                    .first()
                    .map_or(Id::new(("new-subtask", list, task.id)), |subtask| {
                        FocusItem::Subtask(list, task.id, subtask.id).id()
                    })
            } else {
                tasks
                    .get(position + 1)
                    .map(|task| FocusItem::Task(list, task.id).id())
                    .or(self.keyboard.history_toggle)
                    .unwrap_or(row)
            };
            self.focus_now(ctx, target);
        } else if take_key(ctx, Key::Home, Modifiers::NONE) {
            self.focus_now(ctx, FocusItem::Task(list, tasks[0].id).id());
        } else if take_key(ctx, Key::End, Modifiers::NONE) {
            self.focus_now(ctx, FocusItem::Task(list, tasks.last().unwrap().id).id());
        } else if focused == Some(row) {
            self.keyboard.anchor = Some((row, FocusItem::Task(list, task.id)));
            // Delete removes one row per press; held navigation may still repeat.
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(event,
                        Event::Key { key: Key::Delete, pressed: true, repeat: true, modifiers, .. }
                        if modifiers.matches_exact(Modifiers::NONE)
                    )
                })
            });
            if take_shortcut(ctx, MOVE_TASK_UP) {
                self.apply_row_action(RowAction::Reorder(
                    list,
                    task.id,
                    position.saturating_sub(1),
                ));
            } else if take_shortcut(ctx, MOVE_TASK_DOWN) {
                self.apply_row_action(RowAction::Reorder(
                    list,
                    task.id,
                    (position + 1).min(tasks.len() - 1),
                ));
            } else if take_key(ctx, Key::Enter, Modifiers::NONE) {
                self.apply_row_action(RowAction::BeginEdit(list, task.id, task.text));
            } else if take_key(ctx, Key::Space, Modifiers::NONE) {
                self.apply_row_action(RowAction::Toggle(list, task.id));
            } else if take_shortcut(ctx, DELETE_TASK) {
                self.apply_row_action(RowAction::Delete(list, task.id));
            } else if take_key(ctx, Key::ArrowRight, Modifiers::NONE) {
                if self.subtasks_for != Some((list, task.id)) {
                    self.new_subtask.clear();
                }
                self.subtasks_for = Some((list, task.id));
                let target = task
                    .subtasks
                    .first()
                    .map_or(Id::new(("new-subtask", list, task.id)), |subtask| {
                        FocusItem::Subtask(list, task.id, subtask.id).id()
                    });
                self.focus_now(ctx, target);
            } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
                self.subtasks_for = None;
            }
        }
    }

    fn checklist_keyboard(&mut self, ctx: &egui::Context, list: Uuid) -> bool {
        let Some((active_list, task_id)) = self.subtasks_for else {
            return false;
        };
        if active_list != list {
            return false;
        }
        let Some(task) = self.list_index(list).and_then(|index| {
            self.lists[index]
                .data
                .tasks
                .iter()
                .find(|task| task.id == task_id)
        }) else {
            return false;
        };
        let focused = ctx.memory(|memory| memory.focused());
        let entry = Id::new(("new-subtask", list, task_id));
        let owner = self
            .keyboard
            .anchor
            .filter(|(id, _)| Some(*id) == focused)
            .map(|(_, item)| item);
        let position = task.subtasks.iter().position(|subtask| {
            focused == Some(FocusItem::Subtask(list, task_id, subtask.id).id())
                || owner == Some(FocusItem::Subtask(list, task_id, subtask.id))
        });
        if focused == Some(entry) {
            if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                let target = task
                    .subtasks
                    .last()
                    .map_or(FocusItem::Task(list, task_id).id(), |subtask| {
                        FocusItem::Subtask(list, task_id, subtask.id).id()
                    });
                self.focus_now(ctx, target);
            } else {
                take_key(ctx, Key::ArrowDown, Modifiers::NONE);
            }
            return true;
        }
        let Some(position) = position else {
            return false;
        };
        let target = if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
            if position == 0 {
                FocusItem::Task(list, task_id).id()
            } else {
                FocusItem::Subtask(list, task_id, task.subtasks[position - 1].id).id()
            }
        } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
            task.subtasks.get(position + 1).map_or(entry, |subtask| {
                FocusItem::Subtask(list, task_id, subtask.id).id()
            })
        } else if take_key(ctx, Key::Home, Modifiers::NONE) {
            FocusItem::Subtask(list, task_id, task.subtasks[0].id).id()
        } else if take_key(ctx, Key::End, Modifiers::NONE) {
            entry
        } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
            self.subtasks_for = None;
            FocusItem::Task(list, task_id).id()
        } else {
            return true;
        };
        self.focus_now(ctx, target);
        true
    }

    /// Moves between the new-list name and its row of accent choices.
    ///
    /// # Panics
    ///
    /// Panics if the selected accent is missing from the application's fixed choices.
    pub(in crate::app) fn creation_keyboard(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|memory| memory.focused());
        let entry = Id::new("new-list-title");
        let choices = &self.keyboard.creation_choices;
        if choices.is_empty() {
            return;
        }
        let target = if focused == Some(entry) {
            if !take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
                return;
            }
            let selected = crate::model::Accent::ALL
                .iter()
                .position(|accent| *accent == self.new_list_accent)
                .unwrap();
            choices[selected]
        } else if let Some(index) = choices.iter().position(|id| Some(*id) == focused) {
            if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
                entry
            } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
                choices[index.saturating_sub(1)]
            } else if take_key(ctx, Key::ArrowRight, Modifiers::NONE) {
                choices[(index + 1).min(choices.len() - 1)]
            } else {
                return;
            }
        } else {
            return;
        };
        self.focus_now(ctx, target);
    }
}
