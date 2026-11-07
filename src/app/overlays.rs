//! Quick-capture, cross-list search, and keyboard-reference overlays.

use super::{
    MinimalistApp, Screen,
    keyboard::{FocusItem, shortcut_groups, take_key},
    paste::new_tasks,
    text_input_shortcuts,
    theme::Palette,
};
use crate::hotkey::GlobalAction;
use crate::model::{Subtask, Task};
use crate::storage::StoredList;
use eframe::egui;
use egui::{Align, Button, ComboBox, CornerRadius, Id, Key, Layout, Modal, ScrollArea, TextEdit};
use std::time::Instant;
use uuid::Uuid;

/// Remembers the control underneath the keyboard-reference modal.
pub(super) struct ShortcutsState {
    return_focus: Option<Id>,
    request_focus: bool,
}

/// Holds the draft and destination selected in the quick-capture overlay.
pub(super) struct QuickCaptureState {
    pub(super) text: String,
    pub(super) selected_list_id: Option<Uuid>,
    request_focus: bool,
    pub(super) failure: Option<String>,
    /// Parsed tasks waiting for a retry after a failed paste save.
    pub(super) pending_batch: Vec<String>,
    /// Returns to the tray when a capture that revealed the window finishes.
    return_to_tray: bool,
}

impl QuickCaptureState {
    fn has_input(&self) -> bool {
        !self.pending_batch.is_empty() || !self.text.trim().is_empty()
    }
}

/// Holds the current global-search query and keyboard selection.
pub(super) struct FindState {
    query: String,
    selected: usize,
    request_focus: bool,
    result_ids: Vec<Id>,
    scroll_pending: bool,
}

#[derive(Clone)]
/// Identifies a search result that should be revealed and briefly highlighted.
pub(super) struct Spotlight {
    pub(super) list_id: Uuid,
    pub(super) task_id: Uuid,
    pub(super) subtask_id: Option<Uuid>,
    pub(super) archived: bool,
    pub(super) started: Instant,
    pub(super) scroll_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitState {
    Active,
    Completed,
    Archived,
}

impl HitState {
    const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "done",
            Self::Archived => "history",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchHit {
    list_id: Uuid,
    task_id: Uuid,
    subtask_id: Option<Uuid>,
    list_title: String,
    task_text: String,
    subtask_text: Option<String>,
    state: HitState,
    completed: bool,
    /// The first notes line holding a term absent from the title.
    context_snippet: Option<String>,
}

impl SearchHit {
    fn state_label(&self) -> String {
        if self.state == HitState::Archived {
            format!(
                "history · {}",
                if self.completed { "done" } else { "active" }
            )
        } else {
            self.state.label().into()
        }
    }
}

impl MinimalistApp {
    /// Reopens an accepted capture that still needs its explicit save retry.
    ///
    /// # Returns
    /// Whether an active or suspended failed capture remains unsaved.
    pub(super) fn reopen_failed_quick_capture(&mut self) -> bool {
        let failed = self
            .quick_capture
            .as_ref()
            .or(self.capture_draft.as_ref())
            .is_some_and(|draft| draft.failure.is_some());
        if failed {
            self.open_quick_capture(None);
        }
        failed
    }

    /// Opens the keyboard reference without discarding a capture or search draft.
    pub(super) fn open_shortcuts(&mut self) {
        self.cancel_overview_drag();
        if self.shortcuts.is_some() {
            return;
        }
        let return_focus = self.navigation_return_focus();
        self.drag = None;
        self.finish_inline_edit();
        self.keyboard.pending = None;
        self.shortcuts = Some(ShortcutsState {
            return_focus,
            request_focus: true,
        });
    }

    /// Saves the underlying editor and focus before opening capture or search.
    fn prepare_capture_or_search(&mut self) {
        self.cancel_overview_drag();
        self.drag = None;
        if self.quick_capture.is_none() && self.find.is_none() {
            self.keyboard.overlay_return = self.navigation_return_focus();
        }
        self.keyboard.pending = None;
        self.finish_inline_edit();
    }

    /// Opens quick capture, preferring the hinted list when no saved destination applies.
    pub(super) fn open_quick_capture(&mut self, hinted_list_id: Option<Uuid>) {
        self.prepare_capture_or_search();
        if self.find.is_some() {
            self.find_draft = self.find.take();
        }
        if let Some(state) = &mut self.quick_capture {
            state.request_focus = true;
            return;
        }
        if let Some(mut draft) = self.capture_draft.take() {
            draft.request_focus = true;
            if !draft.has_input() {
                draft.selected_list_id = capture_destination(
                    &self.lists,
                    self.settings.last_capture_list_id,
                    hinted_list_id,
                    self.settings.last_list_id,
                );
            }
            self.quick_capture = Some(draft);
            return;
        }
        self.quick_capture = Some(QuickCaptureState {
            text: String::new(),
            selected_list_id: capture_destination(
                &self.lists,
                self.settings.last_capture_list_id,
                hinted_list_id,
                self.settings.last_list_id,
            ),
            request_focus: true,
            failure: None,
            pending_batch: Vec::new(),
            return_to_tray: false,
        });
    }

    /// Opens quick capture for the current list and focuses its text field.
    pub(super) fn open_quick_add_here(&mut self, ctx: &egui::Context) {
        let current = match self.screen {
            Screen::List(id) => Some(id),
            _ => None,
        };
        self.open_quick_capture(current);
        self.focus_now(ctx, Id::new("quick-capture-text"));
    }

    /// Reveals the window now and queues Quick add until the current widgets process their input.
    ///
    /// # Arguments
    /// - `ctx` - Context receiving the native window commands.
    pub(super) fn request_global_quick_add(&mut self, ctx: &egui::Context) {
        *self.global_quick_add_requested.get_or_insert(false) |= self.window_hidden;
        self.show_window(ctx);
    }

    /// Opens quick capture for a request from outside the window, above other transient layers.
    ///
    /// Dismissing the keyboard reference or Move keeps its return focus for Quick add. Open
    /// menus are dismissed. An already open Quick add keeps its draft and regains focus.
    ///
    /// # Arguments
    /// - `ctx` - Context used to focus the capture field.
    /// - `was_hidden` - Whether this request revealed the window from the tray.
    pub(super) fn open_global_quick_add(&mut self, ctx: &egui::Context, was_hidden: bool) {
        if let Some(reference) = self.shortcuts.take() {
            self.keyboard.pending = reference.return_focus;
        }
        if let Some(dialog) = self.details.move_task.take() {
            let source_exists = self.lists.iter().any(|list| {
                list.key == dialog.source
                    && list.data.tasks.iter().any(|task| task.id == dialog.task)
            });
            self.keyboard.pending = if source_exists {
                Some(FocusItem::Task(dialog.source, dialog.task).id())
            } else {
                self.default_focus()
            };
        }
        egui::Popup::close_all(ctx);
        self.open_quick_add_here(ctx);
        if let Some(state) = &mut self.quick_capture {
            state.return_to_tray |= was_hidden;
        }
    }

    /// Opens global search and requests focus for its query field.
    pub(super) fn open_find(&mut self) {
        self.prepare_capture_or_search();
        if self.quick_capture.is_some() {
            self.capture_draft = self.quick_capture.take();
        }
        if let Some(state) = &mut self.find {
            state.request_focus = true;
            return;
        }
        if let Some(mut draft) = self.find_draft.take() {
            draft.request_focus = true;
            draft.scroll_pending = true;
            self.find = Some(draft);
            return;
        }
        self.find = Some(FindState {
            query: String::new(),
            selected: 0,
            request_focus: true,
            result_ids: Vec::new(),
            scroll_pending: true,
        });
    }

    /// Renders the keyboard reference or the active capture/search overlay.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Egui context used to display the modal.
    /// - `palette` - Colors used to render the active overlay.
    pub(super) fn overlay_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        if self.shortcuts.is_some() {
            self.shortcuts_ui(ctx, palette);
        } else if self.quick_capture.is_some() {
            self.quick_capture_ui(ctx, palette);
        } else if self.find.is_some() {
            self.find_ui(ctx, palette);
        }
    }

    fn shortcuts_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        let Some(mut state) = self.shortcuts.take() else {
            return;
        };
        let available = ctx.content_rect().size() - egui::vec2(32.0, 32.0);
        let scroll = if take_key(ctx, Key::ArrowDown, egui::Modifiers::NONE) {
            -40.0
        } else if take_key(ctx, Key::ArrowUp, egui::Modifiers::NONE) {
            40.0
        } else if take_key(ctx, Key::PageDown, egui::Modifiers::NONE) {
            -available.y * 0.8
        } else if take_key(ctx, Key::PageUp, egui::Modifiers::NONE) {
            available.y * 0.8
        } else {
            0.0
        };
        let home = take_key(ctx, Key::Home, egui::Modifiers::NONE);
        let end = take_key(ctx, Key::End, egui::Modifiers::NONE);
        let modal = Modal::new(Id::new("keyboard-shortcuts-modal")).show(ctx, |ui| {
            ui.set_width(500.0_f32.min(available.x));
            ui.horizontal(|ui| {
                ui.label(self.rich("Keyboard shortcuts", 20.0, palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close = ui.add(Button::new(self.rich("Close", 13.0, palette.accent)));
                    if state.request_focus {
                        close.request_focus();
                        self.keyboard.pending = Some(close.id);
                        state.request_focus = false;
                    }
                    if close.clicked() {
                        ui.close();
                    }
                });
            });
            ui.label(self.rich(
                "Up/Down or Page Up/Down scroll · Esc closes",
                12.0,
                palette.muted,
            ));
            ui.separator();
            ScrollArea::vertical()
                .id_salt("keyboard-shortcuts-scroll")
                .max_height(available.y - 90.0)
                .show(ui, |ui| {
                    ui.scroll_with_delta(egui::vec2(0.0, scroll));
                    if home {
                        ui.scroll_to_cursor(Some(Align::Min));
                    }
                    let [show, quick_add] = GlobalAction::ALL
                        .map(|action| crate::hotkey::label(action.configured(&self.settings)));
                    for (heading, shortcuts) in shortcut_groups(show, quick_add) {
                        ui.add_space(10.0);
                        ui.label(self.rich(heading, 17.0, palette.accent));
                        for (action, keys) in shortcuts {
                            if ui.available_width() < 380.0 {
                                ui.label(self.rich(action, 14.0, palette.text));
                                ui.label(self.rich(keys, 13.0, palette.muted));
                            } else {
                                ui.horizontal(|ui| {
                                    ui.label(self.rich(action, 14.0, palette.text));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(self.rich(keys, 13.0, palette.muted));
                                    });
                                });
                            }
                        }
                    }
                    if end {
                        ui.scroll_to_cursor(Some(Align::Max));
                    }
                });
            self.keyboard_focus_ui(ui, palette);
        });
        if modal.should_close() {
            self.keyboard.pending = state.return_focus;
            // Clear egui's previous-pass modal barrier before the next input arrives.
            ctx.request_discard("Restore focus after closing keyboard shortcuts");
        } else {
            self.shortcuts = Some(state);
        }
    }

    fn quick_capture_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        let batch_saved = self.take_capture_paste(ctx);
        let Some(mut state) = self.quick_capture.take() else {
            return;
        };
        let previous_destination = state.selected_list_id;
        let choices = self
            .lists
            .iter()
            .map(|list| (list.key, list.data.title.clone()))
            .collect::<Vec<_>>();
        if state
            .selected_list_id
            .is_none_or(|id| !choices.iter().any(|(candidate, _)| *candidate == id))
        {
            state.selected_list_id = if !state.has_input() {
                capture_destination(
                    &self.lists,
                    self.settings.last_capture_list_id,
                    None,
                    self.settings.last_list_id,
                )
            } else {
                None
            };
        }

        let mut submit = false;
        let mut recover = false;
        let mut discard = false;
        let mut cancel = false;
        if state.request_focus {
            ctx.memory_mut(|memory| memory.request_focus(Id::new("quick-capture-text")));
        }
        let modal = Modal::new(Id::new("quick-capture-modal")).show(ctx, |ui| {
            let available = ctx.content_rect().size() - egui::vec2(32.0, 32.0);
            ui.set_width(410.0_f32.min(available.x));
            // Modal areas retain their previous size. Allow the form to grow
            // when validation adds a message instead of inheriting that height.
            ui.set_max_height(available.y - 16.0);
            ScrollArea::vertical()
                .max_height(available.y - 16.0)
                .show(ui, |ui| {
                    ui.label(self.rich("Quick add", 22.0, palette.text));
                    ui.add_space(8.0);
                    let response = ui.add(
                        TextEdit::singleline(&mut state.text)
                            .id(Id::new("quick-capture-text"))
                            .font(self.content_font_id(19.0))
                            .hint_text("What needs doing?")
                            .desired_width(f32::INFINITY),
                    );
                    if state.request_focus {
                        response.request_focus();
                        state.request_focus = false;
                    }

                    if state
                        .selected_list_id
                        .is_some_and(|id| self.has_save_conflict(id))
                    {
                        ui.horizontal_wrapped(|ui| {
                            recover = ui.button("Keep local copy and reload").clicked();
                            discard = ui.button("Discard my change and reload").clicked();
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(self.rich("List", 13.0, palette.muted));
                        let selected_title = state
                            .selected_list_id
                            .and_then(|id| {
                                choices
                                    .iter()
                                    .find(|(candidate, _)| *candidate == id)
                                    .map(|(_, title)| title.as_str())
                            })
                            .unwrap_or("Choose a list");
                        ComboBox::from_id_salt("quick-capture-list")
                            .width(ui.available_width())
                            .wrap_mode(egui::TextWrapMode::Truncate)
                            .selected_text(self.rich(selected_title, 14.0, palette.text))
                            .show_ui(ui, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                for (id, title) in &choices {
                                    if ui
                                        .selectable_value(
                                            &mut state.selected_list_id,
                                            Some(*id),
                                            self.rich(title, 14.0, palette.text),
                                        )
                                        .clicked()
                                    {
                                        take_key(ctx, Key::Enter, egui::Modifiers::NONE);
                                        take_key(ctx, Key::Space, egui::Modifiers::NONE);
                                        state.request_focus = true;
                                        ui.close();
                                    }
                                }
                            });
                    });

                    if state.selected_list_id.is_none() && state.has_input() {
                        ui.add(
                            egui::Label::new(self.rich(
                                "Choose a destination list to save this draft.",
                                13.0,
                                palette.text,
                            ))
                            .wrap(),
                        );
                    }
                    if let Some(error) = &state.failure {
                        let message = if state.pending_batch.is_empty() {
                            "Not saved. Your draft is kept.".to_owned()
                        } else {
                            format!(
                                "Not saved. {} pasted tasks are kept for retry.",
                                state.pending_batch.len()
                            )
                        };
                        ui.add(egui::Label::new(self.rich(message, 13.0, palette.text)).wrap())
                            .on_hover_text(error);
                    }
                    ui.add_space(8.0);
                    let conflicted = state
                        .selected_list_id
                        .is_some_and(|id| self.has_save_conflict(id));
                    ui.horizontal(|ui| {
                        ui.label(self.rich(
                            if conflicted {
                                "Esc cancels"
                            } else {
                                "Enter adds · Esc cancels"
                            },
                            12.0,
                            palette.muted,
                        ));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !conflicted {
                                submit |= ui
                                    .add_enabled(
                                        state.selected_list_id.is_some() && state.has_input(),
                                        Button::new(self.rich(
                                            if state.failure.is_some() {
                                                "Retry save"
                                            } else {
                                                "Add"
                                            },
                                            14.0,
                                            palette.accent,
                                        ))
                                        .frame(false),
                                    )
                                    .clicked();
                            }
                            cancel = ui.button("Cancel").clicked();
                        });
                    });
                    if !conflicted {
                        submit |= text_input_shortcuts(ui, &response).0;
                    }
                    self.keyboard_focus_ui(ui, palette);
                });
        });

        if previous_destination != state.selected_list_id {
            self.forget_abandoned_capture_conflict(previous_destination, state.failure.is_some());
        }
        if batch_saved && !state.has_input() {
            self.finish_quick_capture(ctx, state.return_to_tray);
            return;
        }
        if submit {
            state.request_focus = true;
            if state.has_input()
                && let Some(list_id) = state.selected_list_id
            {
                let retry_batch = !state.pending_batch.is_empty();
                let typed = state.text.trim().to_owned();
                let items = if retry_batch {
                    state.pending_batch.as_slice()
                } else {
                    std::slice::from_ref(&typed)
                };
                match self.save_capture(list_id, items, state.failure.as_deref()) {
                    Ok(()) => {
                        if !retry_batch || typed.is_empty() {
                            self.finish_quick_capture(ctx, state.return_to_tray);
                            return;
                        }
                        state.pending_batch.clear();
                        state.failure = None;
                    }
                    Err(error) => state.failure = Some(error),
                }
            }
        }

        if let Some(list) = state.selected_list_id.filter(|_| recover || discard) {
            let return_to_tray = state.return_to_tray;
            self.quick_capture = Some(state);
            let resolved = if recover {
                self.keep_local_copy_and_reload(list)
            } else {
                self.discard_local_change_and_reload(list)
            };
            if resolved {
                self.finish_quick_capture(ctx, return_to_tray);
            }
        } else {
            let clicked_outside = modal.backdrop_response.clicked();
            if cancel || (modal.should_close() && (!clicked_outside || !state.has_input())) {
                self.forget_abandoned_capture_conflict(
                    state.selected_list_id,
                    state.failure.is_some(),
                );
                self.finish_quick_capture(ctx, state.return_to_tray);
            } else {
                state.request_focus |= clicked_outside;
                self.quick_capture = Some(state);
            }
        }
    }

    fn forget_abandoned_capture_conflict(&mut self, destination: Option<Uuid>, failed: bool) {
        if let Some(list) = destination.filter(|_| failed)
            && !self.persistence.dirty_lists.contains(&list)
            && self
                .task_context
                .as_ref()
                .is_none_or(|editor| editor.list != list || editor.failure.is_none())
        {
            self.forget_save_conflict(list);
        }
    }

    fn finish_quick_capture(&mut self, ctx: &egui::Context, return_to_tray: bool) {
        self.restore_overlay_focus();
        if return_to_tray {
            self.return_to_tray_after_capture(ctx);
        }
        ctx.request_repaint();
    }

    /// Saves captured tasks at the top of a list, replaying once on a clean external edit.
    ///
    /// # Arguments
    /// - `list_id` - Destination list identity.
    /// - `items` - Task titles in top-to-bottom order.
    /// - `earlier_failure` - The failed save this retries, whose status message a success clears.
    ///
    /// # Returns
    /// Success after the list, last destination, and acknowledgement are updated.
    ///
    /// # Errors
    /// Returns the storage error after showing it as the status message.
    ///
    /// # Panics
    /// Panics if an internally resolved list index is invalid.
    pub(super) fn save_capture(
        &mut self,
        list_id: Uuid,
        items: &[String],
        earlier_failure: Option<&str>,
    ) -> Result<(), String> {
        let Some(index) = self.list_index(list_id) else {
            let error = "The destination list is no longer available.".to_owned();
            self.status = Some(error.clone());
            return Err(error);
        };
        let tasks = new_tasks(items);
        let newest = tasks.first().map(|task| task.id);
        if let Err(error) = self.write_capture_tasks(index, &tasks) {
            self.status = Some(error.clone());
            return Err(error);
        }
        self.record_paste_undo(list_id, None, tasks.iter().map(|task| task.id));
        self.newest_task = newest.map(|id| (id, Instant::now()));
        self.settings.last_capture_list_id = Some(list_id);
        self.save_settings();
        if !self.persistence.settings_dirty
            && self.persistence.dirty_lists.is_empty()
            && self.persistence.pending_filenames.is_empty()
            && earlier_failure.is_some_and(|error| self.status.as_deref() == Some(error))
        {
            self.status = None;
        }
        let title = &self.lists[index].data.title;
        self.acknowledge(match items {
            [_] => format!("Added to {title}"),
            _ => format!("Added {} tasks to {title}", items.len()),
        });
        Ok(())
    }

    fn find_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        let Some(mut state) = self.find.take() else {
            return;
        };
        let mut activated = None;
        let modal = Modal::new(Id::new("find-modal")).show(ctx, |ui| {
            let available = ctx.content_rect().size() - egui::vec2(32.0, 32.0);
            ui.set_width(500.0_f32.min(available.x));
            ui.set_max_height(available.y - 16.0);
            ScrollArea::vertical()
                .max_height(available.y - 16.0)
                .show(ui, |ui| {
                    ui.label(self.rich("Find tasks", 22.0, palette.text));
                    ui.add_space(8.0);
                    let previous_query = state.query.clone();
                    let response = ui.add(
                        TextEdit::singleline(&mut state.query)
                            .id(Id::new("find-query"))
                            .font(self.content_font_id(18.0))
                            .hint_text("Search tasks and history")
                            .desired_width(f32::INFINITY),
                    );
                    if state.request_focus {
                        response.request_focus();
                        state.request_focus = false;
                    }
                    if state.query != previous_query {
                        state.selected = 0;
                        state.scroll_pending = true;
                    }

                    let hits = search_hits(&self.lists, &state.query);
                    let focused = ctx.memory(|memory| memory.focused());
                    let focused_result =
                        state.result_ids.iter().position(|id| Some(*id) == focused);
                    let mut move_result_focus = false;
                    let submit_query = text_input_shortcuts(ui, &response).0;
                    if hits.is_empty() {
                        if submit_query {
                            response.request_focus();
                        }
                        state.selected = 0;
                    } else {
                        if let Some(index) = focused_result {
                            state.scroll_pending |= state.selected != index;
                            state.selected = index;
                        }
                        state.selected = state.selected.min(hits.len() - 1);
                        let down = take_key(ctx, Key::ArrowDown, egui::Modifiers::NONE);
                        let up = take_key(ctx, Key::ArrowUp, egui::Modifiers::NONE);
                        if down {
                            state.selected = (state.selected + 1) % hits.len();
                        } else if up {
                            state.selected = (state.selected + hits.len() - 1) % hits.len();
                        }
                        state.scroll_pending |= down || up;
                        move_result_focus = focused_result.is_some() && (down || up);
                        if submit_query
                            || (focused_result.is_some()
                                && take_key(ctx, Key::Enter, egui::Modifiers::NONE))
                        {
                            activated = hits.get(state.selected).cloned();
                        }
                    }
                    state.result_ids.clear();

                    ui.add_space(8.0);
                    if state.query.trim().is_empty() {
                        ui.label(self.rich(
                            "Type part of a task, note, or subtask.",
                            13.0,
                            palette.muted,
                        ));
                    } else if hits.is_empty() {
                        ui.label(self.rich("No matches.", 14.0, palette.muted));
                    } else {
                        ScrollArea::vertical()
                            .id_salt("find-results")
                            .max_height((available.y - 150.0).max(40.0))
                            .show(ui, |ui| {
                                for (index, hit) in hits.iter().enumerate() {
                                    let selected = state.selected == index;
                                    let title = hit.subtask_text.as_ref().unwrap_or(&hit.task_text);
                                    let metadata = if hit.subtask_id.is_some() {
                                        format!(
                                            "↳ {} · {} · {}",
                                            hit.task_text,
                                            hit.list_title,
                                            hit.state_label()
                                        )
                                    } else {
                                        format!("{} · {}", hit.list_title, hit.state_label())
                                    };
                                    let mut label = egui::text::LayoutJob::default();
                                    label.append(
                                        title,
                                        0.0,
                                        egui::TextFormat {
                                            font_id: self.content_font_id(15.0),
                                            color: palette.text,
                                            ..Default::default()
                                        },
                                    );
                                    label.append(
                                        &format!("\n{metadata}"),
                                        0.0,
                                        egui::TextFormat {
                                            font_id: self.content_font_id(12.0),
                                            color: palette.muted,
                                            ..Default::default()
                                        },
                                    );
                                    if let Some(snippet) = &hit.context_snippet {
                                        label.append(
                                            &format!("\nin notes: {snippet}"),
                                            0.0,
                                            egui::TextFormat {
                                                font_id: self.content_font_id(12.0),
                                                color: palette.muted,
                                                ..Default::default()
                                            },
                                        );
                                    }
                                    label.wrap.max_width = (ui.available_width() - 24.0).max(1.0);
                                    label.wrap.max_rows =
                                        3 + usize::from(hit.context_snippet.is_some());
                                    label.wrap.break_anywhere = true;
                                    let height = ui
                                        .fonts_mut(|fonts| fonts.layout_job(label.clone()))
                                        .size()
                                        .y
                                        + 16.0;
                                    let result = ui.add_sized(
                                        [ui.available_width(), height.max(48.0)],
                                        Button::new(())
                                            .left_text(label)
                                            .wrap()
                                            .stroke(egui::Stroke::new(
                                                if selected { 2.0 } else { 0.0 },
                                                palette.accent,
                                            ))
                                            .fill(if selected {
                                                palette.raised
                                            } else {
                                                palette.surface
                                            })
                                            .corner_radius(CornerRadius::same(10)),
                                    );
                                    let result = result.on_hover_text(title);
                                    state.result_ids.push(result.id);
                                    if selected && move_result_focus {
                                        result.request_focus();
                                    }
                                    if selected && state.scroll_pending {
                                        result.scroll_to_me(Some(Align::Center));
                                        state.scroll_pending = false;
                                    }
                                    if result.clicked() {
                                        state.selected = index;
                                        activated = Some(hit.clone());
                                    }
                                }
                            });
                    }

                    ui.add_space(6.0);
                    ui.label(self.rich(
                        "Up/Down selects · Enter opens · Esc closes",
                        12.0,
                        palette.muted,
                    ));
                    self.keyboard_focus_ui(ui, palette);
                });
        });

        if let Some(hit) = activated {
            self.find_draft = Some(state);
            self.activate_search_hit(hit);
        } else if !modal.should_close() {
            self.find = Some(state);
        } else {
            self.find_draft = Some(state);
            self.restore_overlay_focus();
        }
    }

    fn activate_search_hit(&mut self, hit: SearchHit) {
        if !self.save_task_context() {
            return;
        }
        self.open_list(hit.list_id);
        self.request_task_focus = false;
        self.keyboard.overlay_return = None;
        self.keyboard.pending = Some(if hit.state == HitState::Archived {
            FocusItem::Archived(hit.list_id, hit.task_id).id()
        } else if let Some(subtask) = hit.subtask_id {
            FocusItem::Subtask(hit.list_id, hit.task_id, subtask).id()
        } else {
            FocusItem::Task(hit.list_id, hit.task_id).id()
        });
        self.history_open = hit.state == HitState::Archived;
        if hit.state == HitState::Archived {
            self.expand_archived(hit.list_id, hit.task_id);
        }
        let panel = (hit.state != HitState::Archived)
            .then_some(hit.subtask_id)
            .flatten()
            .map(|_| (hit.list_id, hit.task_id));
        self.switch_subtask_panel(panel);
        self.spotlight = Some(Spotlight {
            list_id: hit.list_id,
            task_id: hit.task_id,
            subtask_id: hit.subtask_id,
            archived: hit.state == HitState::Archived,
            started: Instant::now(),
            scroll_pending: true,
        });
    }
}

fn capture_destination(
    lists: &[StoredList],
    last_capture: Option<Uuid>,
    hinted: Option<Uuid>,
    last_opened: Option<Uuid>,
) -> Option<Uuid> {
    [hinted, last_capture, last_opened]
        .into_iter()
        .flatten()
        .find(|id| lists.iter().any(|list| list.key == *id))
        .or_else(|| lists.first().map(|list| list.key))
}

fn search_hits(lists: &[StoredList], query: &str) -> Vec<SearchHit> {
    let query = query.to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for list in lists {
        for task in &list.data.tasks {
            let state = if task.completed {
                HitState::Completed
            } else {
                HitState::Active
            };
            collect_task_hits(&mut hits, list, task, state, &terms);
        }
        for archived in &list.data.archive {
            collect_task_hits(&mut hits, list, &archived.task, HitState::Archived, &terms);
        }
    }
    hits
}

/// Adds a hit when every term is in the task's title or notes, plus one per matching child.
fn collect_task_hits(
    hits: &mut Vec<SearchHit>,
    list: &StoredList,
    task: &Task,
    state: HitState,
    terms: &[&str],
) {
    let title = task.text.to_lowercase();
    let context = task.context.to_lowercase();
    if terms
        .iter()
        .all(|term| title.contains(term) || context.contains(term))
    {
        let context_only: Vec<&str> = terms
            .iter()
            .copied()
            .filter(|term| !title.contains(term))
            .collect();
        hits.push(SearchHit {
            list_id: list.key,
            task_id: task.id,
            subtask_id: None,
            list_title: list.data.title.clone(),
            task_text: task.text.clone(),
            subtask_text: None,
            state,
            completed: task.completed,
            context_snippet: context_snippet(&task.context, &context_only),
        });
    }
    for subtask in &task.subtasks {
        let text = subtask.text.to_lowercase();
        if terms.iter().all(|term| text.contains(term)) {
            hits.push(subtask_hit(list, task, subtask, state));
        }
    }
}

/// Returns the first trimmed notes line containing any term, capped at 120 characters.
fn context_snippet(context: &str, terms: &[&str]) -> Option<String> {
    let line = context
        .lines()
        .find(|line| {
            let line = line.to_lowercase();
            terms.iter().any(|term| line.contains(term))
        })?
        .trim();
    let mut snippet: String = line.chars().take(120).collect();
    if line.chars().nth(120).is_some() {
        snippet.push('…');
    }
    Some(snippet)
}

fn subtask_hit(list: &StoredList, task: &Task, subtask: &Subtask, state: HitState) -> SearchHit {
    SearchHit {
        list_id: list.key,
        task_id: task.id,
        subtask_id: Some(subtask.id),
        list_title: list.data.title.clone(),
        task_text: task.text.clone(),
        subtask_text: Some(subtask.text.clone()),
        state: if state == HitState::Archived {
            state
        } else if subtask.completed {
            HitState::Completed
        } else {
            HitState::Active
        },
        completed: subtask.completed,
        context_snippet: None,
    }
}

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
