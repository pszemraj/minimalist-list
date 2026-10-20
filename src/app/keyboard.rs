//! Keyboard commands and transient focus for list and task navigation.

use super::{MinimalistApp, Screen, list::RowAction, theme::Palette};
use eframe::egui::{
    self, Event, FocusDirection, Id, Key, KeyboardShortcut, Modifiers, Response, Stroke,
};
use uuid::Uuid;

#[path = "keyboard_levels.rs"]
mod levels;

#[path = "keyboard_events.rs"]
mod events;

/// Opens the in-app shortcut reference.
pub(super) const SHOW_SHORTCUTS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Slash);
/// Opens Settings from a main screen.
pub(super) const OPEN_SETTINGS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
/// Opens Quick add.
pub(super) const QUICK_ADD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
/// Opens Find.
pub(super) const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
/// Closes the window like its close button, hiding it to the tray when available.
pub(super) const CLOSE_WINDOW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
/// Opens list creation, applying Settings edits first.
pub(super) const NEW_LIST: KeyboardShortcut = KeyboardShortcut::new(
    Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    },
    Key::N,
);
/// Cycles lists with physical Control; macOS reserves Command+Tab for apps.
pub(super) const NEXT_LIST: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
/// Cycles lists backward with physical Control.
pub(super) const PREVIOUS_LIST: KeyboardShortcut = KeyboardShortcut::new(
    Modifiers {
        shift: true,
        ..Modifiers::CTRL
    },
    Key::Tab,
);
/// Returns from a list to Lists using the native Back shortcut.
#[cfg(not(target_os = "macos"))]
pub(super) const BACK_TO_LISTS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::ALT, Key::ArrowLeft);
/// Returns from a list to Lists using the native Back shortcut.
#[cfg(target_os = "macos")]
pub(super) const BACK_TO_LISTS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::OpenBracket);
/// Returns to Lists outside editors, including an empty task entry.
pub(super) const BACKSPACE_TO_LISTS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::NONE, Key::Backspace);
/// Moves completed tasks to History.
pub(super) const CLEAR_COMPLETED: KeyboardShortcut = KeyboardShortcut::new(
    Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    },
    Key::Backspace,
);
/// Deletes a focused task using forward Delete.
pub(super) const DELETE_TASK: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
/// Moves a focused task up one position.
pub(super) const MOVE_TASK_UP: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::ALT, Key::ArrowUp);
/// Moves a focused task down one position.
pub(super) const MOVE_TASK_DOWN: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::ALT, Key::ArrowDown);

#[derive(Clone, Copy, PartialEq, Eq)]
/// A logical item whose keyboard focus survives movement and file reloads.
pub(super) enum FocusItem {
    Card(Uuid),
    Task(Uuid, Uuid),
    Subtask(Uuid, Uuid, Uuid),
    Archived(Uuid, Uuid),
}

impl FocusItem {
    /// Returns the stable identity of the item's primary keyboard control.
    ///
    /// # Returns
    ///
    /// The widget ID derived from the item's UUIDs.
    pub(super) fn id(self) -> Id {
        match self {
            Self::Card(list) => Id::new(("list-card", list)),
            Self::Task(list, task) => Id::new(("task-focus", list, task)),
            Self::Subtask(list, task, subtask) => Id::new(("subtask-focus", list, task, subtask)),
            Self::Archived(list, task) => Id::new(("restore-task", list, task)),
        }
    }
}

#[derive(Default)]
/// Pending focus requests, navigation return targets, and the last focused item.
pub(super) struct KeyboardState {
    pub(super) pending: Option<Id>,
    requested: Option<Id>,
    missing_request: Option<(Id, u8)>,
    pub(super) last_focus: Option<Id>,
    pub(super) settings_return: Option<Id>,
    pub(super) overlay_return: Option<Id>,
    pub(super) overview_list: Option<Uuid>,
    pub(super) settings_rows: Vec<Vec<Id>>,
    pub(super) settings_sections: Vec<usize>,
    pub(super) history_toggle: Option<Id>,
    pub(super) creation_choices: Vec<Id>,
    anchor: Option<(Id, FocusItem)>,
    deferred_events: Vec<Event>,
}

/// Consumes an exact chord, including its platform-independent command modifier.
///
/// # Arguments
///
/// - `ctx` - Context holding the current input events.
/// - `key` - Logical key to handle.
/// - `modifiers` - Exact modifiers required by the command.
///
/// # Returns
///
/// Whether a matching key press was consumed.
pub(super) fn take_key(ctx: &egui::Context, key: Key, modifiers: Modifiers) -> bool {
    let taken = ctx.input_mut(|input| {
        let mut taken = false;
        input.events.retain(|event| {
            let matches = matches!(event, Event::Key {
                key: pressed_key, pressed: true, modifiers: pressed_modifiers, ..
            } if *pressed_key == key && pressed_modifiers.matches_exact(modifiers));
            taken |= matches;
            !matches
        });
        taken
    });
    if taken {
        // egui has already scheduled arrow navigation at the beginning of this pass.
        ctx.memory_mut(|memory| memory.move_focus(FocusDirection::None));
    }
    taken
}

/// Formats a shortcut for the native platform.
///
/// # Arguments
///
/// - `binding` - The key and modifiers used by command routing.
///
/// # Returns
///
/// A label using the binding's real modifiers and native key names.
pub(super) fn shortcut(binding: KeyboardShortcut) -> String {
    shortcut_for_platform(binding, cfg!(target_os = "macos"))
}

fn shortcut_for_platform(binding: KeyboardShortcut, mac: bool) -> String {
    let key = match binding.logical_key {
        Key::Comma => ",",
        Key::Slash => "/",
        Key::OpenBracket => "[",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Home if mac => "Fn+Left",
        Key::End if mac => "Fn+Right",
        Key::PageUp if mac => "Fn+Up",
        Key::PageDown if mac => "Fn+Down",
        Key::Backspace if mac => "Delete",
        Key::Delete if mac => "Fn+Delete",
        key => key.name(),
    };
    let modifiers = egui::ModifierNames::NAMES.format(&binding.modifiers, mac);
    if modifiers.is_empty() {
        key.to_owned()
    } else {
        format!("{modifiers}+{key}")
    }
}

/// A reference group with action descriptions and formatted key bindings.
pub(super) type ShortcutGroup = (&'static str, Vec<(&'static str, String)>);

/// Builds the reference from the same bindings used by routing and tooltips.
///
/// # Returns
///
/// Context groups containing native shortcut labels.
pub(super) fn shortcut_groups() -> Vec<ShortcutGroup> {
    let pair = |first, second| format!("{} / {}", shortcut(first), shortcut(second));
    vec![
        (
            "General",
            vec![
                ("Keyboard shortcuts", shortcut(SHOW_SHORTCUTS)),
                ("Settings (main screens)", shortcut(OPEN_SETTINGS)),
                ("Quick add", shortcut(QUICK_ADD)),
                ("Find tasks and History", shortcut(FIND)),
                ("New list (Lists, list, Settings)", shortcut(NEW_LIST)),
                (
                    "Show window (global)",
                    shortcut(KeyboardShortcut::new(
                        Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT,
                        Key::L,
                    )),
                ),
                ("Close window", shortcut(CLOSE_WINDOW)),
            ],
        ),
        (
            "Inside a list",
            vec![
                ("Back to Lists", shortcut(BACK_TO_LISTS)),
                ("Next / previous list", pair(NEXT_LIST, PREVIOUS_LIST)),
                ("Move completed tasks to History", shortcut(CLEAR_COMPLETED)),
            ],
        ),
        (
            "Task actions",
            vec![
                (
                    "Move the task up / down",
                    pair(MOVE_TASK_UP, MOVE_TASK_DOWN),
                ),
                (
                    "Undo deletion",
                    shortcut(KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
                ),
                (
                    "Copy task text",
                    shortcut(KeyboardShortcut::new(
                        Modifiers::COMMAND | Modifiers::SHIFT,
                        Key::C,
                    )),
                ),
            ],
        ),
    ]
}

fn take_complete_task_copy(ctx: &egui::Context, enabled: bool) -> bool {
    let binding = Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    };
    let state_id = Id::new("complete-copy-modifiers");
    let previous = ctx
        .data(|data| data.get_temp::<Modifiers>(state_id))
        .unwrap_or_else(|| ctx.input(|input| input.modifiers));
    // egui-winit emits Event::Copy for Command+C even with Shift, without a
    // Key::C event. Replay modifier changes in event order so releasing the
    // chord before repaint cannot turn it into an ordinary selection copy.
    let (taken, current) = ctx.input_mut(|input| {
        let mut current = previous;
        let mut taken = false;
        input.events.retain(|event| {
            match event {
                Event::ModifiersChanged(modifiers) => current = *modifiers,
                Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } => {
                    current = *modifiers;
                    if enabled && *key == Key::C && *pressed && modifiers.matches_exact(binding) {
                        taken = true;
                        return false;
                    }
                }
                Event::Copy if enabled && current.matches_exact(binding) => {
                    taken = true;
                    return false;
                }
                _ => {}
            }
            true
        });
        (taken, current)
    });
    ctx.data_mut(|data| data.insert_temp(state_id, current));
    taken
}

fn take_shortcut(ctx: &egui::Context, binding: KeyboardShortcut) -> bool {
    take_key(ctx, binding.logical_key, binding.modifiers)
}

impl MinimalistApp {
    /// Routes app commands before rendering widgets so one key has one effect.
    pub(super) fn handle_app_shortcuts(&mut self, ctx: &egui::Context) {
        self.prepare_editor_event_order(ctx);
        self.route_app_shortcuts(ctx);
        // A shortcut can replace the screen and queue a different input during
        // this frame. Focus it before its widget consumes accompanying text.
        if let Some(target) = self.keyboard.pending
            && self.keyboard.requested != Some(target)
        {
            self.focus_now(ctx, target);
        }
    }

    fn route_app_shortcuts(&mut self, ctx: &egui::Context) {
        // Activation and dismissal act once per press, even when a widget changes
        // focus or closes an overlay. Keep text events and navigation repeats intact.
        let escape = ctx.input_mut(|input| {
            let escape = input.events.iter().any(|event| {
                matches!(event,
                    Event::Key { key: Key::Escape, pressed: true, modifiers, .. }
                    if modifiers.matches_exact(Modifiers::NONE)
                )
            });
            input.events.retain(|event| {
                !matches!(
                    event,
                    Event::Key {
                        key: Key::Enter | Key::Space | Key::Escape,
                        pressed: true,
                        repeat: true,
                        ..
                    }
                )
            });
            escape
        });
        // egui surrenders focus for Escape before this handler runs. Preserve it
        // for ignored repeats and main screens; a dismissed layer sets its return target.
        if escape
            && ctx.memory(|memory| memory.focused().is_none())
            && let Some(id) = self.keyboard.last_focus
        {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        if self.request_task_focus
            && let Screen::List(list) = self.screen
        {
            ctx.memory_mut(|memory| memory.request_focus(Id::new(("new-task", list))));
            self.request_task_focus = false;
        }
        self.keyboard.requested = self.keyboard.pending;
        if let Some(id) = self.keyboard.requested {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        // Closing saves pending edits, so it works above every overlay.
        if take_shortcut(ctx, CLOSE_WINDOW) {
            self.close_window(ctx);
            return;
        }
        let copy_target = (self.shortcuts.is_none()
            && self.quick_capture.is_none()
            && self.find.is_none()
            && self.details.move_task.is_none())
        .then(|| self.task_copy_target(ctx))
        .flatten();
        if take_complete_task_copy(ctx, copy_target.is_some())
            && let Some((list, task, archived)) = copy_target
        {
            self.copy_task(ctx, list, task, archived);
            return;
        }
        if self.details.move_task.is_some() {
            return;
        }
        if egui::Popup::is_any_open(ctx) {
            return;
        }
        // Some layouts need Shift or Alt to type the logical '/' key.
        if ctx.input_mut(|input| input.consume_shortcut(&SHOW_SHORTCUTS)) {
            self.open_shortcuts();
            return;
        }
        if self.shortcuts.is_some() {
            return;
        }
        if self.task_context.is_none() && take_shortcut(ctx, NEW_LIST) {
            if self.quick_capture.is_none() && self.find.is_none() {
                if matches!(self.screen, Screen::Settings) {
                    self.close_settings();
                }
                self.go_to_overview();
                self.creating_list = true;
                self.request_list_focus = true;
                self.keyboard.pending = None;
            }
            return;
        }
        if take_shortcut(ctx, QUICK_ADD) {
            let current = match self.screen {
                Screen::List(id) => Some(id),
                _ => None,
            };
            self.open_quick_capture(current);
            self.focus_now(ctx, Id::new("quick-capture-text"));
            return;
        }
        if take_shortcut(ctx, FIND) {
            self.open_find();
            self.focus_now(ctx, Id::new("find-query"));
            return;
        }
        if self.quick_capture.is_some()
            || self.find.is_some()
            || self.details.move_task.is_some()
            || self.task_context.is_some()
        {
            return;
        }
        if !ctx.text_edit_focused() && take_key(ctx, Key::Z, Modifiers::COMMAND) {
            self.undo_delete();
            return;
        }
        // Like '/', a logical comma may require Shift on the user's layout.
        if ctx.input_mut(|input| input.consume_shortcut(&OPEN_SETTINGS)) {
            if !matches!(self.screen, Screen::Settings) {
                self.open_settings(None);
            }
            return;
        }
        if take_key(ctx, Key::Escape, Modifiers::NONE) {
            self.keyboard_escape(ctx);
            return;
        }
        if matches!(self.screen, Screen::Settings) {
            self.settings_keyboard(ctx);
            return;
        }
        if let Screen::List(list) = self.screen {
            if take_shortcut(ctx, BACK_TO_LISTS) || self.take_backspace_to_lists(ctx, list) {
                self.go_to_overview();
            } else if take_shortcut(ctx, PREVIOUS_LIST) {
                self.cycle_list(list, true);
            } else if take_shortcut(ctx, NEXT_LIST) {
                self.cycle_list(list, false);
            } else if !ctx.text_edit_focused() && take_shortcut(ctx, CLEAR_COMPLETED) {
                self.begin_clear_completed(list);
            } else {
                self.task_keyboard(ctx, list);
            }
        }
    }

    fn take_backspace_to_lists(&self, ctx: &egui::Context, list: Uuid) -> bool {
        let empty_entry = self.new_task.is_empty()
            && ctx.memory(|memory| memory.focused()) == Some(Id::new(("new-task", list)));
        if self.editing.is_some()
            || self.subtasks_for.is_some()
            || self.revealed.is_some()
            || self.history_open
            || (ctx.text_edit_focused() && !empty_entry)
        {
            return false;
        }
        // A held deletion may empty the field, but must not then navigate away.
        let fresh_press = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Key { key: Key::Backspace, pressed: true, repeat: false, modifiers, .. }
                    if modifiers.matches_exact(Modifiers::NONE)
                )
            })
        });
        fresh_press && take_shortcut(ctx, BACKSPACE_TO_LISTS)
    }

    fn settings_keyboard(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|memory| memory.focused());
        let Some((row, column)) =
            self.keyboard
                .settings_rows
                .iter()
                .enumerate()
                .find_map(|(row, ids)| {
                    ids.iter()
                        .position(|id| Some(*id) == focused)
                        .map(|column| (row, column))
                })
        else {
            return;
        };
        let rows = &self.keyboard.settings_rows;
        let section = self
            .keyboard
            .settings_sections
            .partition_point(|start| *start <= row)
            - 1;
        let (target_row, target_column) = if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
            (row.saturating_sub(1), column)
        } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
            ((row + 1).min(rows.len() - 1), column)
        } else if take_key(ctx, Key::PageUp, Modifiers::NONE) {
            let Some(previous) = section.checked_sub(1) else {
                return;
            };
            (self.keyboard.settings_sections[previous], 0)
        } else if take_key(ctx, Key::PageDown, Modifiers::NONE) {
            let Some(next) = self.keyboard.settings_sections.get(section + 1) else {
                return;
            };
            (*next, 0)
        } else if rows[row].len() > 1 && take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
            (row, column.saturating_sub(1))
        } else if rows[row].len() > 1 && take_key(ctx, Key::ArrowRight, Modifiers::NONE) {
            (row, (column + 1).min(rows[row].len() - 1))
        } else {
            return;
        };
        let target = &rows[target_row];
        self.focus_now(ctx, target[target_column.min(target.len() - 1)]);
    }

    fn keyboard_escape(&mut self, ctx: &egui::Context) {
        if matches!(self.screen, Screen::Settings) {
            self.close_settings();
        } else if self.details.editor.is_some() {
            self.cancel_subtask_edit();
        } else if let Some((list, task)) = self.editing.take() {
            self.keyboard.pending = Some(FocusItem::Task(list, task).id());
        } else if self.creating_list {
            self.creating_list = false;
            self.new_list_title.clear();
            self.focus_overview();
        } else if self.history_open
            && (self.keyboard.history_toggle.is_some()
                && self.keyboard.history_toggle == ctx.memory(|memory| memory.focused())
                || self.keyboard.anchor.is_some_and(|(id, item)| {
                    Some(id) == ctx.memory(|memory| memory.focused())
                        && matches!(item, FocusItem::Archived(_, _))
                }))
        {
            self.close_history();
        } else if let Some((list, task)) = self.subtasks_for {
            self.switch_subtask_panel(None);
            self.keyboard.pending = Some(FocusItem::Task(list, task).id());
        } else if let Some((list, task)) = self.revealed.take() {
            self.keyboard.pending = Some(FocusItem::Task(list, task).id());
        } else if self.history_open {
            self.close_history();
        }
    }

    /// Requests the remembered overview card, or the first available card.
    pub(super) fn focus_overview(&mut self) {
        self.keyboard.pending = self.overview_focus();
    }

    fn overview_focus(&self) -> Option<Id> {
        let id = self
            .keyboard
            .overview_list
            .filter(|id| self.list_index(*id).is_some())
            .or_else(|| self.lists.first().map(|list| list.key));
        id.map(|id| FocusItem::Card(id).id())
    }

    /// Moves through cards using the overview's current column count.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context holding focus and input.
    /// - `columns` - Number of columns in the rendered card grid.
    ///
    /// # Panics
    ///
    /// Panics if the fixed column count is zero.
    pub(super) fn overview_keyboard(&mut self, ctx: &egui::Context, columns: usize) {
        let focused = ctx.memory(|memory| memory.focused());
        let Some(index) = self
            .lists
            .iter()
            .position(|list| focused == Some(FocusItem::Card(list.key).id()))
        else {
            return;
        };
        let target = if take_key(ctx, Key::ArrowUp, Modifiers::NONE) {
            if index >= columns {
                index - columns
            } else {
                index
            }
        } else if take_key(ctx, Key::ArrowDown, Modifiers::NONE) {
            if index + columns < self.lists.len() {
                index + columns
            } else {
                index
            }
        } else if take_key(ctx, Key::ArrowLeft, Modifiers::NONE) {
            if index % columns > 0 {
                index - 1
            } else {
                index
            }
        } else if take_key(ctx, Key::ArrowRight, Modifiers::NONE) {
            if index % columns + 1 < columns && index + 1 < self.lists.len() {
                index + 1
            } else {
                index
            }
        } else {
            return;
        };
        self.focus_now(ctx, FocusItem::Card(self.lists[target].key).id());
    }

    fn focus_now(&mut self, ctx: &egui::Context, id: Id) {
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
    pub(super) fn track_focus(&mut self, response: &Response, item: FocusItem) {
        if response.has_focus() {
            self.keyboard.anchor = Some((response.id, item));
            if let FocusItem::Card(list) = item {
                self.keyboard.overview_list = Some(list);
            }
        }
    }

    /// Resolves complete-task copying from every focused task control and editor.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context holding the focused control.
    ///
    /// # Returns
    ///
    /// The owning list, task, and archived state, when focus belongs to a task.
    pub(super) fn task_copy_target(&self, ctx: &egui::Context) -> Option<(Uuid, Uuid, bool)> {
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
    ///
    /// # Panics
    ///
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn focus_neighbors(&self) -> Vec<FocusItem> {
        let Some((_, item)) = self.keyboard.anchor else {
            return Vec::new();
        };
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
    pub(super) fn repair_focus(&mut self, previous: &[FocusItem]) {
        let Some((_, item)) = self.keyboard.anchor else {
            return;
        };
        let current = self.focus_neighbors();
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

    fn default_focus(&self) -> Option<Id> {
        match self.screen {
            Screen::List(list) => Some(Id::new(("new-task", list))),
            Screen::Overview => self.overview_focus(),
            Screen::Settings => Some(Id::new("settings-workspace")),
        }
    }

    /// Restores the control that opened a capture/search modal.
    pub(super) fn restore_overlay_focus(&mut self) {
        self.keyboard.pending = self.keyboard.overlay_return.take();
    }

    /// Returns a surviving control when navigation saves and removes an inline editor.
    ///
    /// # Returns
    ///
    /// The edited task or checklist row, or the last focused control.
    pub(super) fn navigation_return_focus(&self) -> Option<Id> {
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
    pub(super) fn keyboard_focus_ui(&self, ui: &mut egui::Ui, palette: Palette) {
        if let Some(response) = ui
            .ctx()
            .memory(|memory| memory.focused())
            .and_then(|id| ui.ctx().read_response(id))
            && response.layer_id == ui.layer_id()
        {
            if response.gained_focus() {
                response.scroll_to_me(None);
            }
            // The capture entry already draws its own focused frame glow.
            if matches!(self.screen, Screen::List(list) if response.id == Id::new(("new-task", list)))
            {
                return;
            }
            if self.keyboard.anchor.is_some_and(|(id, item)| {
                id == response.id && item.id() == id && !matches!(item, FocusItem::Archived(_, _))
            }) {
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
    pub(super) fn finish_keyboard_frame(&mut self, ctx: &egui::Context) {
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

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
