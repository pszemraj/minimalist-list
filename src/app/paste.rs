//! Multi-line paste into the task, checklist, and Quick add entry fields.

use super::{MinimalistApp, Screen};
use crate::model::Task;
use eframe::egui::{self, Event, Id};
use uuid::Uuid;

/// Chooses where a multi-line paste creates its items.
enum PasteTarget {
    /// Tasks for the open list.
    Tasks(Uuid),
    /// Checklist items for one task of the open list.
    Subtasks(Uuid, Uuid),
}

/// Removes one leading list marker when whitespace or the line's end follows it.
fn after_marker(rest: Option<&str>) -> Option<&str> {
    rest.filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        .map(str::trim_start)
}

/// Removes one bullet or number such as `- `, `* `, `• `, `3. `, or `3) `.
fn strip_bullet(line: &str) -> &str {
    let numbered = line.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = if numbered.len() < line.len() {
        numbered.strip_prefix(['.', ')'])
    } else {
        line.strip_prefix(['-', '*', '•'])
    };
    after_marker(rest).unwrap_or(line)
}

/// Removes one Markdown checkbox such as `[ ] ` or `[x] `.
fn strip_checkbox(line: &str) -> &str {
    after_marker(
        ["[ ]", "[x]", "[X]"]
            .iter()
            .find_map(|mark| line.strip_prefix(mark)),
    )
    .unwrap_or(line)
}

/// Splits pasted text into one trimmed item per nonempty line.
///
/// # Arguments
/// - `text` - Clipboard text with `\n`, `\r\n`, or `\r` line breaks.
///
/// # Returns
/// The lines in order, each without one leading bullet, number, or checkbox.
pub(super) fn pasted_items(text: &str) -> Vec<String> {
    text.split(['\r', '\n'])
        .map(|line| strip_checkbox(strip_bullet(line.trim())).trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Creates tasks that share one creation time, so date sorts keep their pasted order.
///
/// # Arguments
/// - `texts` - Task titles in top-to-bottom order.
///
/// # Returns
/// One active task per title.
pub(super) fn new_tasks(texts: &[String]) -> Vec<Task> {
    let mut tasks: Vec<Task> = texts.iter().map(Task::new).collect();
    if let Some(created) = tasks.first().map(|task| task.created_at_unix) {
        for task in &mut tasks {
            task.created_at_unix = created;
        }
    }
    tasks
}

fn take_pasted_batch(ctx: &egui::Context) -> Vec<String> {
    ctx.input_mut(|input| {
        let mut items = Vec::new();
        input.events.retain(|event| {
            let Event::Paste(text) = event else {
                return true;
            };
            let lines = pasted_items(text);
            if lines.len() < 2 {
                return true;
            }
            items.extend(lines);
            false
        });
        items
    })
}

impl MinimalistApp {
    /// Returns the list entry field that would receive a paste in this frame.
    fn paste_target(&self, ctx: &egui::Context) -> Option<PasteTarget> {
        let focused = ctx.memory(|memory| memory.focused())?;
        let Screen::List(list) = self.screen else {
            return None;
        };
        if self.has_modal() || self.persistence.workspace_failure.is_some() {
            return None;
        }
        if focused == Id::new(("new-task", list)) {
            return Some(PasteTarget::Tasks(list));
        }
        self.subtasks_for
            .filter(|&(parent_list, task)| {
                parent_list == list && focused == Id::new(("new-subtask", list, task))
            })
            .map(|(list, task)| PasteTarget::Subtasks(list, task))
    }

    /// Creates one item per line for a multi-line paste into a focused list entry.
    ///
    /// Only pastes of at least two nonempty lines are taken from this frame's input;
    /// anything else stays for the text field's ordinary paste. The field's draft text
    /// is unchanged, and the whole paste is saved once. Run it after shortcut routing,
    /// when focus is what the text fields will see, and before they run.
    ///
    /// # Arguments
    /// - `ctx` - Context containing this frame's input events.
    pub(super) fn take_multiline_paste(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Paste(_)))
        }) {
            return;
        }
        let Some(target) = self.paste_target(ctx) else {
            return;
        };
        let items = take_pasted_batch(ctx);
        if items.is_empty() {
            return;
        }
        match target {
            PasteTarget::Tasks(list) => {
                if self.add_tasks(list, &items) == Some(true) {
                    self.acknowledge(format!("Added {} tasks", items.len()));
                }
            }
            PasteTarget::Subtasks(list, task) => {
                if self.add_subtasks(list, task, &items) == Some(true) {
                    self.acknowledge(format!("Added {} subtasks", items.len()));
                }
            }
        }
    }

    /// Saves a multiline Quick add paste, leaving other input for its text widget.
    ///
    /// # Arguments
    /// - `ctx` - Context containing this frame's input events and focus.
    ///
    /// # Returns
    /// Whether a pasted batch was saved. The overlay checks whether to close after editing.
    pub(super) fn take_capture_paste(&mut self, ctx: &egui::Context) -> bool {
        let Some(capture) = &self.quick_capture else {
            return false;
        };
        // An unavailable destination or pending batch leaves the paste to the text widget.
        let Some(list) = capture.selected_list_id.filter(|&list| {
            ctx.memory(|memory| memory.has_focus(Id::new("quick-capture-text")))
                && capture.pending_batch.is_empty()
                && self.list_index(list).is_some()
                && !self.has_save_conflict(list)
        }) else {
            return false;
        };
        let items = take_pasted_batch(ctx);
        if items.is_empty() {
            return false;
        }
        let failure = capture.failure.clone();
        match self.save_capture(list, &items, failure.as_deref()) {
            Ok(()) => true,
            Err(error) => {
                if let Some(capture) = &mut self.quick_capture {
                    capture.pending_batch = items;
                    capture.failure = Some(error);
                }
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "paste_tests.rs"]
mod tests;
