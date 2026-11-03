//! Focus requests, logical anchors, sibling repair, and focus drawing for keyboard navigation.

use super::{FocusItem, MinimalistApp, OWN_FOCUS_FRAME, Screen};
use crate::app::theme::Palette;
use eframe::egui::{self, Id, Response, Stroke};
use uuid::Uuid;

impl MinimalistApp {
    /// Requests focus for a control immediately and queues it for retry.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context whose memory receives the focus request.
    /// - `id` - Control to focus.
    pub(in crate::app) fn focus_now(&mut self, ctx: &egui::Context, id: Id) {
        self.keyboard.pending = Some(id);
        self.keyboard.requested = Some(id);
        ctx.memory_mut(|memory| memory.request_focus(id));
    }

    /// Records the logical item owning a focused control for removal/reload recovery.
    ///
    /// # Arguments
    ///
    /// - `response` - Rendered control whose focus is checked.
    /// - `item` - Logical item owning that control.
    pub(in crate::app) fn track_focus(&mut self, response: &Response, item: FocusItem) {
        if response.has_focus() {
            self.keyboard.anchor = Some((response.id, item));
            if let FocusItem::Card(list) = item {
                self.keyboard.overview_list = Some(list);
            }
        }
    }

    /// Reports whether keyboard focus currently rests on a control of the logical item.
    ///
    /// # Arguments
    ///
    /// - `item` - Logical item whose controls are checked.
    ///
    /// # Returns
    ///
    /// Whether the focused control is tracked as belonging to the item.
    pub(in crate::app) fn has_focus_within(&self, item: FocusItem) -> bool {
        self.keyboard.anchor.is_some_and(|(_, owner)| owner == item)
    }

    /// Resolves the task that owns the focused control, including its editors and child controls.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context holding the focused control.
    ///
    /// # Returns
    ///
    /// The owning list, task, and archived state, when focus belongs to a task.
    pub(in crate::app) fn focused_task(&self, ctx: &egui::Context) -> Option<(Uuid, Uuid, bool)> {
        if let Some(editor) = &self.task_context {
            return Some((editor.list, editor.task, editor.archived));
        }
        if egui::Popup::is_any_open(ctx) {
            return self
                .details
                .menu_popup
                .filter(|popup| egui::Popup::is_id_open(ctx, *popup))
                .and(self.details.menu_task);
        }
        let focused = ctx.memory(|memory| memory.focused());
        if let Some((list, task)) = self.editing
            && focused == Some(Id::new(("edit-task", list, task)))
        {
            return Some((list, task, false));
        }
        if let Some(editor) = &self.details.editor
            && focused
                == Some(Id::new((
                    "edit-subtask",
                    editor.list,
                    editor.task,
                    editor.subtask,
                )))
        {
            return Some((editor.list, editor.task, false));
        }
        match self
            .keyboard
            .anchor
            .filter(|(id, _)| Some(*id) == focused)
            .map(|(_, owner)| owner)
        {
            Some(FocusItem::Task(list, task) | FocusItem::Subtask(list, task, _)) => {
                Some((list, task, false))
            }
            Some(FocusItem::Archived(list, task)) => Some((list, task, true)),
            _ => None,
        }
    }

    /// Captures siblings before an operation that may remove the focused item.
    ///
    /// # Returns
    ///
    /// The focused item's siblings in their current display order.
    pub(in crate::app) fn focus_neighbors(&self) -> Vec<FocusItem> {
        self.keyboard
            .anchor
            .map(|(_, item)| self.focus_siblings(item))
            .unwrap_or_default()
    }

    /// Lists an item and its siblings in their current display order.
    ///
    /// # Arguments
    ///
    /// - `item` - Logical item whose siblings are wanted.
    ///
    /// # Returns
    ///
    /// The siblings, or nothing when the item's parent no longer exists.
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid.
    pub(in crate::app) fn focus_siblings(&self, item: FocusItem) -> Vec<FocusItem> {
        match item {
            FocusItem::Card(_) => self
                .lists
                .iter()
                .map(|list| FocusItem::Card(list.key))
                .collect(),
            FocusItem::Task(list, _) => self
                .list_index(list)
                .map(|i| {
                    self.lists[i]
                        .data
                        .tasks
                        .iter()
                        .map(|task| FocusItem::Task(list, task.id))
                        .collect()
                })
                .unwrap_or_default(),
            FocusItem::Archived(list, _) => self
                .list_index(list)
                .map(|i| {
                    self.lists[i]
                        .data
                        .archive
                        .iter()
                        .map(|task| FocusItem::Archived(list, task.task.id))
                        .collect()
                })
                .unwrap_or_default(),
            FocusItem::Subtask(list, task, _) => self
                .list_index(list)
                .and_then(|i| self.lists[i].data.tasks.iter().find(|t| t.id == task))
                .map(|t| {
                    t.subtasks
                        .iter()
                        .map(|s| FocusItem::Subtask(list, task, s.id))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Restores focus to the next surviving sibling, then the previous one or entry.
    ///
    /// # Panics
    ///
    /// Panics if a resolved sibling position is outside the previous sibling list.
    pub(in crate::app) fn repair_focus(&mut self, previous: &[FocusItem]) {
        if let Some((_, item)) = self.keyboard.anchor {
            self.repair_focus_from(item, previous);
        }
    }

    /// Repairs focus for an item that left its siblings, even after a modal cleared the anchor.
    ///
    /// # Arguments
    ///
    /// - `item` - Item that held focus before the operation.
    /// - `previous` - Its siblings captured before the operation.
    ///
    /// # Panics
    ///
    /// Panics if a resolved sibling position is outside the previous sibling list.
    pub(in crate::app) fn repair_focus_from(&mut self, item: FocusItem, previous: &[FocusItem]) {
        let current = self.focus_siblings(item);
        if current.contains(&item) {
            return;
        }
        let position = previous.iter().position(|old| *old == item).unwrap_or(0);
        let next = previous
            .iter()
            .skip(position + 1)
            .chain(previous[..position].iter().rev())
            .find(|candidate| current.contains(candidate));
        self.keyboard.pending = next.map(|item| item.id()).or_else(|| self.default_focus());
    }

    /// Returns the screen's default control, such as task entry or the first card.
    ///
    /// # Returns
    ///
    /// The control ID to focus when no better target survives.
    pub(in crate::app) fn default_focus(&self) -> Option<Id> {
        match self.screen {
            Screen::List(list) => Some(Id::new(("new-task", list))),
            Screen::Overview => self.overview_focus(),
            Screen::Settings => Some(Id::new("settings-workspace")),
        }
    }

    /// Restores the control that opened a capture/search modal.
    pub(in crate::app) fn restore_overlay_focus(&mut self) {
        self.keyboard.pending = self.keyboard.overlay_return.take();
    }

    /// Returns a surviving control when navigation saves and removes an inline editor.
    ///
    /// # Returns
    ///
    /// The edited task or checklist row, or the last focused control.
    pub(in crate::app) fn navigation_return_focus(&self) -> Option<Id> {
        self.editing
            .map(|(list, task)| FocusItem::Task(list, task).id())
            .or_else(|| {
                self.details
                    .editor
                    .as_ref()
                    .map(|edit| FocusItem::Subtask(edit.list, edit.task, edit.subtask).id())
            })
            .or(self.keyboard.pending)
            .or(self.keyboard.last_focus)
            .or_else(|| self.default_focus())
    }

    /// Draws focus even for frameless controls and scrolls newly focused widgets into view.
    ///
    /// # Arguments
    ///
    /// - `ui` - UI containing the focused control.
    /// - `palette` - Colors used for the focus outline.
    pub(in crate::app) fn keyboard_focus_ui(&self, ui: &mut egui::Ui, palette: Palette) {
        if let Some(response) = ui
            .ctx()
            .memory(|memory| memory.focused())
            .and_then(|id| ui.ctx().read_response(id))
            && response.layer_id == ui.layer_id()
        {
            if response.gained_focus() {
                response.scroll_to_me(None);
            }
            if ui
                .ctx()
                .data(|data| data.get_temp::<bool>(response.id.with(OWN_FOCUS_FRAME)))
                .is_some()
            {
                return;
            }
            if self.keyboard.anchor.is_some_and(|(id, item)| {
                id == response.id
                    && ((item.id() == id && !matches!(item, FocusItem::Archived(_, _)))
                        || matches!(item, FocusItem::Task(_, task)
                            if id == Id::new(("task-text", task))))
            }) {
                return;
            }
            // Expanding an empty visible region would leave a sliver at the clip edge.
            if !response.interact_rect.is_positive() {
                return;
            }
            ui.painter()
                .with_clip_rect(response.interact_rect.expand(2.0))
                .rect_stroke(
                    response.rect.expand(1.0),
                    4.0,
                    Stroke::new(1.5, palette.accent),
                    egui::StrokeKind::Inside,
                );
        }
    }

    /// Remembers focus after rendering, discarding stale logical anchors.
    pub(in crate::app) fn finish_keyboard_frame(&mut self, ctx: &egui::Context) {
        // A delivered request is complete even if Tab moved on in the same pass.
        // Keep requests queued this pass, and retries blocked by a closing modal.
        if let Some(id) = self
            .keyboard
            .pending
            .filter(|id| Some(*id) == self.keyboard.requested)
        {
            let response = ctx.read_response(id).filter(|response| response.enabled());
            if response.is_none() {
                // Keep the retry queued without exposing an absent accessibility focus.
                ctx.memory_mut(|memory| memory.surrender_focus(id));
            }
            if response.as_ref().is_some_and(|response| {
                ctx.memory(|memory| memory.is_above_modal_layer(response.layer_id))
            }) {
                self.keyboard.pending = None;
                self.keyboard.missing_request = None;
            } else {
                let attempts = self
                    .keyboard
                    .missing_request
                    .filter(|(previous, _)| *previous == id)
                    .map_or(1, |(_, attempts)| attempts + 1);
                self.keyboard.missing_request = Some((id, attempts));
                if attempts >= 3 {
                    // Sync or an animation can remove a saved return control. Never
                    // keep stealing focus for an ID that no longer gets rendered.
                    self.keyboard.pending = None;
                    self.keyboard.missing_request = None;
                    ctx.memory_mut(|memory| memory.surrender_focus(id));
                    // Keep a deliberate click or Tab made during the return attempt.
                    if ctx.memory(|memory| memory.focused().is_none())
                        && let Some(fallback) = self
                            .default_focus()
                            .and_then(|id| ctx.read_response(id))
                            .filter(|response| response.enabled())
                    {
                        fallback.request_focus();
                    }
                } else {
                    // Retain focus across a closing modal's previous-pass barrier,
                    // so the restored field accepts the very next input.
                    if response.is_some() {
                        ctx.memory_mut(|memory| memory.request_focus(id));
                    }
                    ctx.request_repaint();
                }
            }
        } else {
            self.keyboard.missing_request = None;
        }
        self.keyboard.last_focus = ctx.memory(|memory| memory.focused());
        if self.keyboard.last_focus.is_some()
            && self
                .keyboard
                .anchor
                .is_some_and(|(id, _)| Some(id) != self.keyboard.last_focus)
        {
            self.keyboard.anchor = None;
        }
    }
}
