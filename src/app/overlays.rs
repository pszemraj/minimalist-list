//! Cross-list search, keyboard reference, and overlay dispatch.

use super::{
    MinimalistApp,
    keyboard::{FocusItem, shortcut_groups, take_key},
    text_input_shortcuts,
    theme::Palette,
};
use crate::hotkey::GlobalAction;
use crate::model::{Subtask, Task};
use crate::storage::StoredList;
use eframe::egui;
use egui::{Align, Button, CornerRadius, Id, Key, Layout, Modal, ScrollArea, TextEdit};
use std::time::Instant;
use uuid::Uuid;

/// Remembers the control underneath the keyboard-reference modal.
pub(super) struct ShortcutsState {
    pub(super) return_focus: Option<Id>,
    request_focus: bool,
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
    /// Checks for a modal covering the current screen.
    ///
    /// # Returns
    /// Whether a modal is open.
    pub(super) fn has_modal(&self) -> bool {
        self.quick_capture.is_some()
            || self.find.is_some()
            || self.shortcuts.is_some()
            || self.details.move_task.is_some()
            || self.task_context.is_some()
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
    pub(super) fn prepare_capture_or_search(&mut self) {
        self.cancel_overview_drag();
        self.drag = None;
        if self.quick_capture.is_none() && self.find.is_none() {
            self.keyboard.overlay_return = self.navigation_return_focus();
        }
        self.keyboard.pending = None;
        self.finish_inline_edit();
    }

    /// Opens global search and requests focus for its query field.
    pub(super) fn open_find(&mut self) {
        self.prepare_capture_or_search();
        if self.quick_capture.is_some() {
            self.keep_window_after_capture();
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
