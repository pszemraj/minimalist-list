//! Quick-capture, cross-list search, and keyboard-reference overlays.

use super::{
    MinimalistApp,
    keyboard::{FocusItem, shortcut_groups, take_key},
    text_input_shortcuts,
    theme::Palette,
};
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
    /// Opens the keyboard reference without discarding a capture or search draft.
    pub(super) fn open_shortcuts(&mut self) {
        if self.shortcuts.is_some() {
            return;
        }
        let return_focus = self.navigation_return_focus();
        self.finish_inline_edit();
        self.keyboard.pending = None;
        self.shortcuts = Some(ShortcutsState {
            return_focus,
            request_focus: true,
        });
    }

    /// Opens quick capture, preferring the hinted list when no saved destination applies.
    pub(super) fn open_quick_capture(&mut self, hinted_list_id: Option<Uuid>) {
        if self.quick_capture.is_none() && self.find.is_none() {
            self.keyboard.overlay_return = self.navigation_return_focus();
        }
        self.keyboard.pending = None;
        self.finish_inline_edit();
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
        if self.quick_capture.is_none() && self.find.is_none() {
            self.keyboard.overlay_return = self.navigation_return_focus();
        }
        self.keyboard.pending = None;
        self.finish_inline_edit();
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
                    for (heading, shortcuts) in shortcut_groups() {
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
        if state.request_focus {
            ctx.memory_mut(|memory| memory.request_focus(Id::new("quick-capture-text")));
        }
        let modal = Modal::new(Id::new("quick-capture-modal")).show(ctx, |ui| {
            let available = ctx.content_rect().size() - egui::vec2(32.0, 32.0);
            ui.set_width(410.0_f32.min(available.x));
            ScrollArea::vertical()
                .max_height(available.y - 16.0)
                .show(ui, |ui| {
                    ui.label(self.rich("Quick add", 22.0, palette.text));
                    ui.add_space(8.0);
                    let response = ui.add(
                        TextEdit::singleline(&mut state.text)
                            .id(Id::new("quick-capture-text"))
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

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(self.rich("Enter adds · Esc cancels", 12.0, palette.muted));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            submit |= ui
                                .add(
                                    Button::new(self.rich("Add", 14.0, palette.accent))
                                        .frame(false),
                                )
                                .clicked();
                        });
                    });
                    submit |= text_input_shortcuts(ui, &response).0;
                    self.keyboard_focus_ui(ui, palette);
                });
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
                self.restore_overlay_focus();
                return;
            }
        }

        if !modal.should_close() {
            self.quick_capture = Some(state);
        } else {
            self.restore_overlay_focus();
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
                    ui.label(self.rich("Find anything", 22.0, palette.text));
                    ui.add_space(8.0);
                    let previous_query = state.query.clone();
                    let response = ui.add(
                        TextEdit::singleline(&mut state.query)
                            .id(Id::new("find-query"))
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
                        let down = take_key(ctx, Key::ArrowDown, egui::Modifiers::NONE);
                        let up = take_key(ctx, Key::ArrowUp, egui::Modifiers::NONE);
                        if down {
                            state.selected = (state.selected + 1) % hits.len();
                        } else if up {
                            state.selected = (state.selected + hits.len() - 1) % hits.len();
                        }
                        if text_input_shortcuts(ui, &response).0 {
                            activated = hits.get(state.selected).cloned();
                        }
                    }

                    ui.add_space(8.0);
                    if state.query.trim().is_empty() {
                        ui.label(self.rich("Type part of a task or subtask.", 13.0, palette.muted));
                    } else if hits.is_empty() {
                        ui.label(self.rich("No matches.", 14.0, palette.muted));
                    } else {
                        ScrollArea::vertical()
                            .id_salt("find-results")
                            .max_height((available.y - 150.0).max(40.0))
                            .show(ui, |ui| {
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
                    ui.label(self.rich(
                        "Up/Down selects · Enter opens · Esc closes",
                        12.0,
                        palette.muted,
                    ));
                    self.keyboard_focus_ui(ui, palette);
                });
        });

        if let Some(hit) = activated {
            self.activate_search_hit(hit);
        } else if !modal.should_close() {
            self.find = Some(state);
        } else {
            self.restore_overlay_focus();
        }
    }

    fn activate_search_hit(&mut self, hit: SearchHit) {
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

    #[test]
    fn shortcut_reference_keyboard_scrolls_within_a_narrow_viewport() {
        let (ctx, mut app) = super::super::tests::test_app();
        app.open_shortcuts();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 420.0));
        let mut draw = |key: Option<Key>| {
            let events = key
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .into_iter()
                .collect();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    time: Some(ctx.input(|input| input.time) + 0.3),
                    events,
                    ..Default::default()
                },
                |_| {
                    app.handle_app_shortcuts(&ctx);
                    app.overlay_ui(&ctx, app.palette());
                    app.finish_keyboard_frame(&ctx);
                },
            );
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|clipped| {
                    if let egui::epaint::Shape::Text(text) = clipped.shape
                        && clipped
                            .clip_rect
                            .intersects(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    {
                        Some((text.galley.text().to_owned(), text.pos))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        for _ in 0..3 {
            draw(None);
        }
        let top = draw(None);
        assert!(top.iter().any(|(text, _)| text == "General"));
        assert!(
            !top.iter()
                .any(|(text, _)| text == "Open the selected result")
        );
        assert!(top.iter().all(|(_, pos)| viewport.contains(*pos)));
        draw(Some(Key::PageDown));
        for _ in 0..3 {
            draw(None);
        }
        assert!(!draw(None).iter().any(|(text, _)| text == "General"));
        draw(Some(Key::End));
        for _ in 0..3 {
            draw(None);
        }
        assert!(
            draw(None)
                .iter()
                .any(|(text, _)| text == "Open the selected result")
        );
        draw(Some(Key::Home));
        for _ in 0..3 {
            draw(None);
        }
        assert!(draw(None).iter().any(|(text, _)| text == "General"));
        std::fs::remove_dir_all(&app.workspace).unwrap();
    }

    fn stored_list(title: &str) -> StoredList {
        let data = TodoList::new(title, Accent::Mint);
        StoredList {
            key: data.id,
            path: PathBuf::from(format!("{title}.json")),
            data,
        }
    }

    #[test]
    fn find_grows_after_typing_and_keeps_results_and_help_visible() {
        for size in [egui::vec2(520.0, 760.0), egui::vec2(320.0, 280.0)] {
            let (ctx, mut app) = super::super::tests::test_app();
            app.settings.font_size = 21.0;
            app.lists[0].data.tasks = ["Project first", "Project second", "Project third"]
                .into_iter()
                .map(Task::new)
                .collect();
            app.open_find();
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut draw = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        time: Some(ctx.input(|input| input.time) + 0.3),
                        events,
                        ..Default::default()
                    },
                    |_| app.overlay_ui(&ctx, app.palette()),
                );
                output.textures_delta.clear();
                output
                    .shapes
                    .into_iter()
                    .filter_map(|clipped| {
                        let egui::epaint::Shape::Text(text) = clipped.shape else {
                            return None;
                        };
                        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        (clipped.clip_rect.contains_rect(rect) && viewport.contains_rect(rect))
                            .then(|| text.galley.text().to_owned())
                    })
                    .collect::<Vec<_>>()
            };
            for _ in 0..3 {
                draw(Vec::new());
            }
            draw(vec![egui::Event::Text("project".into())]);
            for _ in 0..3 {
                draw(Vec::new());
            }
            let visible = draw(Vec::new());
            assert!(visible.iter().any(|text| text == "Find anything"));
            assert!(
                visible
                    .iter()
                    .any(|text| text == "Up/Down selects · Enter opens · Esc closes"),
                "Find help must remain visible after results appear at {size:?}: {visible:?}"
            );
            if size.y > 500.0 {
                for title in ["Project first", "Project second", "Project third"] {
                    assert!(visible.iter().any(|text| text.starts_with(title)));
                }
            }
            std::fs::remove_dir_all(&app.workspace).unwrap();
        }
    }

    #[test]
    fn quick_add_long_list_name_keeps_entry_and_add_inside_small_viewport() {
        let (ctx, mut app) = super::super::tests::test_app();
        app.lists[0].data.title = "A very long list name describing preparations for an international workshop and all its participating research groups".to_owned();
        app.open_quick_capture(Some(app.lists[0].key));
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 280.0));
        let palette = app.palette();
        for frame in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |_| app.overlay_ui(&ctx, palette),
            );
            output.textures_delta.clear();
            if frame < 2 {
                continue;
            }
            let entry = ctx.read_response(Id::new("quick-capture-text")).unwrap();
            assert!(viewport.contains_rect(entry.rect));
            let add = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == "Add" => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .expect("Add must remain visible");
            assert!(viewport.contains_rect(add));
        }
        std::fs::remove_dir_all(&app.workspace).unwrap();
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
