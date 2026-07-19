//! Quick-capture and cross-list search overlays.

use super::{MinimalistApp, Screen, theme::Palette};
use crate::model::{Subtask, Task};
use crate::storage::StoredList;
use eframe::egui;
use egui::{Align, Button, ComboBox, CornerRadius, Id, Key, Layout, Modal, ScrollArea, TextEdit};
use std::time::Instant;
use uuid::Uuid;

/// Holds the draft and destination selected in the quick-capture overlay.
pub(super) struct QuickCaptureState {
    text: String,
    selected_list_id: Option<Uuid>,
    request_focus: bool,
}

/// Holds the current global-search query and keyboard selection.
pub(super) struct FindState {
    query: String,
    selected: usize,
    request_focus: bool,
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
}

impl MinimalistApp {
    /// Opens quick capture or global search when their app-scoped shortcuts fire.
    pub(super) fn handle_app_shortcuts(&mut self, ctx: &egui::Context) {
        let (quick_capture, find) = ctx.input(|input| {
            (
                input.modifiers.command && input.key_pressed(Key::N),
                input.modifiers.command && input.key_pressed(Key::F),
            )
        });
        if quick_capture {
            let current_list = match self.screen {
                Screen::List(id) => Some(id),
                Screen::Overview | Screen::Settings => None,
            };
            self.open_quick_capture(current_list);
        } else if find {
            self.open_find();
        }
    }

    /// Opens quick capture, preferring the hinted list when no saved destination applies.
    pub(super) fn open_quick_capture(&mut self, hinted_list_id: Option<Uuid>) {
        self.find = None;
        if let Some(state) = &mut self.quick_capture {
            state.request_focus = true;
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
        });
    }

    /// Opens global search and requests focus for its query field.
    pub(super) fn open_find(&mut self) {
        self.quick_capture = None;
        if let Some(state) = &mut self.find {
            state.request_focus = true;
            return;
        }
        self.find = Some(FindState {
            query: String::new(),
            selected: 0,
            request_focus: true,
        });
    }

    /// Renders whichever modal capture or search overlay is active.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Egui context used to display the modal.
    /// - `palette` - Colors used to render the active overlay.
    pub(super) fn overlay_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        if self.quick_capture.is_some() {
            self.quick_capture_ui(ctx, palette);
        } else if self.find.is_some() {
            self.find_ui(ctx, palette);
        }
    }

    fn quick_capture_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        let Some(mut state) = self.quick_capture.take() else {
            return;
        };
        let choices = self
            .lists
            .iter()
            .map(|list| (list.key, list.data.title.clone()))
            .collect::<Vec<_>>();
        if state
            .selected_list_id
            .is_none_or(|id| !choices.iter().any(|(candidate, _)| *candidate == id))
        {
            state.selected_list_id = capture_destination(
                &self.lists,
                self.settings.last_capture_list_id,
                None,
                self.settings.last_list_id,
            );
        }

        let mut submit = false;
        let modal = Modal::new(Id::new("quick-capture-modal")).show(ctx, |ui| {
            ui.set_width(410.0);
            ui.label(self.rich("Quick add", 22.0, palette.text));
            ui.add_space(8.0);
            let response = ui.add(
                TextEdit::singleline(&mut state.text)
                    .font(self.font_id(self.settings.font_size.min(22.0)))
                    .hint_text("What needs doing?")
                    .desired_width(f32::INFINITY),
            );
            if state.request_focus {
                response.request_focus();
                state.request_focus = false;
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
                    .selected_text(self.rich(selected_title, 14.0, palette.text))
                    .show_ui(ui, |ui| {
                        for (id, title) in &choices {
                            ui.selectable_value(
                                &mut state.selected_list_id,
                                Some(*id),
                                self.rich(title, 14.0, palette.text),
                            );
                        }
                    });
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(self.rich("Enter adds · Esc cancels", 12.0, palette.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    submit |= ui
                        .add(Button::new(self.rich("Add", 14.0, palette.accent)).frame(false))
                        .clicked();
                });
            });
            submit |= response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        });

        if submit {
            let text = state.text.trim();
            if !text.is_empty()
                && let Some(list_id) = state.selected_list_id
                && let Some(index) = self.list_index(list_id)
            {
                let task = Task::new(text);
                self.newest_task = Some((task.id, Instant::now()));
                self.lists[index].data.tasks.insert(0, task);
                self.save_list_index(index);
                self.settings.last_capture_list_id = Some(list_id);
                self.save_settings();
                return;
            }
        }

        if !modal.should_close() {
            self.quick_capture = Some(state);
        }
    }

    fn find_ui(&mut self, ctx: &egui::Context, palette: Palette) {
        let Some(mut state) = self.find.take() else {
            return;
        };
        let mut activated = None;
        let modal = Modal::new(Id::new("find-modal")).show(ctx, |ui| {
            ui.set_width(500.0);
            ui.label(self.rich("Find anything", 22.0, palette.text));
            ui.add_space(8.0);
            let previous_query = state.query.clone();
            let response = ui.add(
                TextEdit::singleline(&mut state.query)
                    .font(self.font_id(18.0))
                    .hint_text("Search tasks and history")
                    .desired_width(f32::INFINITY),
            );
            if state.request_focus {
                response.request_focus();
                state.request_focus = false;
            }
            if state.query != previous_query {
                state.selected = 0;
            }

            let hits = search_hits(&self.lists, &state.query);
            if hits.is_empty() {
                state.selected = 0;
            } else {
                state.selected = state.selected.min(hits.len() - 1);
                let (down, up) = ui.input(|input| {
                    (
                        input.key_pressed(Key::ArrowDown),
                        input.key_pressed(Key::ArrowUp),
                    )
                });
                if down {
                    state.selected = (state.selected + 1) % hits.len();
                } else if up {
                    state.selected = (state.selected + hits.len() - 1) % hits.len();
                }
                if response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    activated = hits.get(state.selected).cloned();
                }
            }

            ui.add_space(8.0);
            if state.query.trim().is_empty() {
                ui.label(self.rich("Type part of a task or subtask.", 13.0, palette.muted));
            } else if hits.is_empty() {
                ui.label(self.rich("No matches.", 14.0, palette.muted));
            } else {
                ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (index, hit) in hits.iter().enumerate() {
                        let selected = state.selected == index;
                        let label = if let Some(subtask) = &hit.subtask_text {
                            format!(
                                "{subtask}\n↳ {} · {} · {}",
                                hit.task_text,
                                hit.list_title,
                                hit.state.label()
                            )
                        } else {
                            format!(
                                "{}\n{} · {}",
                                hit.task_text,
                                hit.list_title,
                                hit.state.label()
                            )
                        };
                        let result = ui.add_sized(
                            [ui.available_width(), 58.0],
                            Button::new(self.rich(label, 14.0, palette.text))
                                .fill(if selected {
                                    palette.raised
                                } else {
                                    palette.surface
                                })
                                .corner_radius(CornerRadius::same(10)),
                        );
                        if selected {
                            result.scroll_to_me(Some(Align::Center));
                        }
                        if result.clicked() {
                            activated = Some(hit.clone());
                        }
                    }
                });
            }

            ui.add_space(6.0);
            ui.label(self.rich("↑↓ selects · Enter opens · Esc closes", 12.0, palette.muted));
        });

        if let Some(hit) = activated {
            self.activate_search_hit(hit);
        } else if !modal.should_close() {
            self.find = Some(state);
        }
    }

    fn activate_search_hit(&mut self, hit: SearchHit) {
        self.open_list(hit.list_id);
        self.request_task_focus = false;
        self.history_open = hit.state == HitState::Archived;
        self.subtasks_for = (hit.state != HitState::Archived)
            .then_some(hit.subtask_id)
            .flatten()
            .map(|_| (hit.list_id, hit.task_id));
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
    [last_capture, hinted, last_opened]
        .into_iter()
        .flatten()
        .find(|id| lists.iter().any(|list| list.key == *id))
        .or_else(|| lists.first().map(|list| list.key))
}

fn search_hits(lists: &[StoredList], query: &str) -> Vec<SearchHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
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
            collect_task_hits(&mut hits, list, task, state, &query);
        }
        for archived in &list.data.archive {
            collect_task_hits(&mut hits, list, &archived.task, HitState::Archived, &query);
        }
    }
    hits
}

fn collect_task_hits(
    hits: &mut Vec<SearchHit>,
    list: &StoredList,
    task: &Task,
    state: HitState,
    query: &str,
) {
    if task.text.to_lowercase().contains(query) {
        hits.push(SearchHit {
            list_id: list.key,
            task_id: task.id,
            subtask_id: None,
            list_title: list.data.title.clone(),
            task_text: task.text.clone(),
            subtask_text: None,
            state,
        });
    }
    for subtask in &task.subtasks {
        if subtask.text.to_lowercase().contains(query) {
            hits.push(subtask_hit(list, task, subtask, state));
        }
    }
}

fn subtask_hit(list: &StoredList, task: &Task, subtask: &Subtask, state: HitState) -> SearchHit {
    SearchHit {
        list_id: list.key,
        task_id: task.id,
        subtask_id: Some(subtask.id),
        list_title: list.data.title.clone(),
        task_text: task.text.clone(),
        subtask_text: Some(subtask.text.clone()),
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Accent, ArchivedTask, Subtask, TodoList};
    use std::path::PathBuf;

    fn stored_list(title: &str) -> StoredList {
        let data = TodoList::new(title, Accent::Mint);
        StoredList {
            key: data.id,
            path: PathBuf::from(format!("{title}.json")),
            data,
        }
    }

    #[test]
    fn searches_active_completed_archived_and_subtask_text() {
        let mut list = stored_list("Work");
        list.data.tasks.push(Task::new("Active needle"));

        let mut completed = Task::new("Completed needle");
        completed.toggle();
        list.data.tasks.push(completed);

        let mut parent = Task::new("Parent");
        parent.subtasks.push(Subtask::new("Subtask needle"));
        list.data.tasks.push(parent);

        let archived = Task::new("Archived needle");
        list.data.archive.push(ArchivedTask::new(archived));

        let hits = search_hits(&[list], "NEEDLE");
        assert_eq!(hits.len(), 4);
        assert_eq!(hits[0].state, HitState::Active);
        assert_eq!(hits[1].state, HitState::Completed);
        assert!(hits[2].subtask_id.is_some());
        assert_eq!(hits[3].state, HitState::Archived);
    }

    #[test]
    fn empty_search_has_no_results() {
        assert!(search_hits(&[stored_list("Tasks")], "  ").is_empty());
    }

    #[test]
    fn capture_destination_prefers_last_successful_valid_list() {
        let first = stored_list("First");
        let second = stored_list("Second");
        let first_id = first.key;
        let second_id = second.key;
        let missing = Uuid::new_v4();
        let lists = vec![first, second];

        assert_eq!(
            capture_destination(&lists, Some(second_id), Some(first_id), None),
            Some(second_id)
        );
        assert_eq!(
            capture_destination(&lists, Some(missing), Some(second_id), Some(first_id)),
            Some(second_id)
        );
    }
}
