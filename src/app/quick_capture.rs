//! Quick add drafts, window behavior, and saving captured tasks.

use super::{
    MinimalistApp, Screen,
    keyboard::{FocusItem, take_key},
    paste::new_tasks,
    text_input_shortcuts,
    theme::Palette,
};
use crate::storage::StoredList;
use eframe::egui;
use egui::{Align, Button, ComboBox, Id, Key, Layout, Modal, ScrollArea, TextEdit};
use std::time::Instant;
use uuid::Uuid;

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

    /// Keeps the window visible when Show or Find takes over from Quick add.
    pub(super) fn keep_window_after_capture(&mut self) {
        self.global_quick_add_requested = self.global_quick_add_requested.map(|_| false);
        for capture in self.quick_capture.iter_mut().chain(&mut self.capture_draft) {
            capture.return_to_tray = false;
        }
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

    /// Renders the active Quick add form and handles submission or dismissal.
    ///
    /// # Arguments
    /// - `ctx` - Context for modal input and focus.
    /// - `palette` - Colors for the capture form.
    pub(super) fn quick_capture_ui(&mut self, ctx: &egui::Context, palette: Palette) {
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

                    if !self.persistence.waiting_for_list_folder
                        && state
                            .selected_list_id
                            .is_some_and(|id| self.has_save_conflict(id))
                    {
                        ui.horizontal_wrapped(|ui| {
                            recover = ui.button("Keep local copy and reload").clicked();
                            discard = ui.button("Discard all unsaved changes").clicked();
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
                        let message = if self.persistence.waiting_for_list_folder {
                            "Waiting for the folder. Your draft is kept.".to_owned()
                        } else if state.pending_batch.is_empty() {
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
                    let conflicted = !self.persistence.waiting_for_list_folder
                        && state
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
        self.record_task_paste_undo(list_id, &tasks);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::tests::{chord, keyboard_frame, settle_keyboard, test_app},
        model::Accent,
        storage,
    };

    #[test]
    fn capture_destination_prefers_context_then_last_successful_valid_list() {
        let (_, mut app) = test_app();
        let first = app.lists[0].key;
        let second = storage::create_list(&app.workspace, "Second", Accent::Mint).unwrap();
        let second_id = second.key;
        app.lists.push(second);

        assert_eq!(
            capture_destination(&app.lists, Some(second_id), Some(first), None),
            Some(first)
        );
        assert_eq!(
            capture_destination(
                &app.lists,
                Some(Uuid::new_v4()),
                Some(second_id),
                Some(first)
            ),
            Some(second_id)
        );
        std::fs::remove_dir_all(app.workspace).unwrap();
    }

    #[test]
    fn global_capture_remembers_hidden_origin_and_can_finish_without_a_tray() {
        for (hidden, submit) in [(false, true), (true, true), (true, false)] {
            let (ctx, mut app) = test_app();
            app.window_hidden = hidden;
            let _ = ctx.run_logic(&Default::default(), |ctx| app.request_global_quick_add(ctx));
            settle_keyboard(&ctx, &mut app);
            let state = app.quick_capture.as_mut().unwrap();
            assert_eq!(state.return_to_tray, hidden);
            state.text = "Captured from another app".into();
            let key = if submit { Key::Enter } else { Key::Escape };
            keyboard_frame(&ctx, &mut app, chord(key, egui::Modifiers::NONE), 430.0);
            assert!(app.quick_capture.is_none());
            assert!(!app.window_hidden, "no tray is available");
            assert!(!app.quitting);
            assert_eq!(app.lists[0].data.tasks.len(), usize::from(submit));
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }

    #[test]
    fn explicit_show_clears_return_to_tray_for_open_and_suspended_capture() {
        for suspended in [false, true] {
            let (ctx, mut app) = test_app();
            app.open_global_quick_add(&ctx, true);
            app.quick_capture.as_mut().unwrap().text = "Retained capture".into();
            if suspended {
                app.capture_draft = app.quick_capture.take();
            }

            app.keep_window_after_capture();

            app.open_global_quick_add(&ctx, false);
            let capture = app.quick_capture.as_ref().unwrap();
            assert!(!capture.return_to_tray);
            assert_eq!(capture.text, "Retained capture");
            std::fs::remove_dir_all(app.workspace).unwrap();
        }
    }

    #[test]
    fn find_parks_tray_capture_without_hiding_after_a_later_save() {
        let (ctx, mut app) = test_app();
        app.window_hidden = true;
        let _ = ctx.run_logic(&Default::default(), |ctx| app.request_global_quick_add(ctx));
        settle_keyboard(&ctx, &mut app);
        assert!(app.quick_capture.as_ref().unwrap().return_to_tray);
        let title = "Keep the window open";
        keyboard_frame(&ctx, &mut app, vec![egui::Event::Text(title.into())], 430.0);
        keyboard_frame(
            &ctx,
            &mut app,
            chord(Key::F, egui::Modifiers::COMMAND),
            430.0,
        );
        assert!(app.find.is_some() && app.quick_capture.is_none());
        keyboard_frame(
            &ctx,
            &mut app,
            chord(Key::N, egui::Modifiers::COMMAND),
            430.0,
        );
        let capture = app.quick_capture.as_ref().unwrap();
        assert_eq!(capture.text, title);
        assert!(!capture.return_to_tray);
        settle_keyboard(&ctx, &mut app);
        keyboard_frame(
            &ctx,
            &mut app,
            chord(Key::Enter, egui::Modifiers::NONE),
            430.0,
        );

        assert!(app.quick_capture.is_none());
        assert!(!app.window_hidden);
        let saved = storage::load_workspace(&app.workspace).unwrap();
        assert_eq!(saved.lists[0].data.tasks.len(), 1);
        assert_eq!(saved.lists[0].data.tasks[0].text, title);
        std::fs::remove_dir_all(app.workspace).unwrap();
    }
}
