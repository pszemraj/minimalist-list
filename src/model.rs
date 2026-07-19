//! Serializable list, task, subtask, and archive data models.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Current on-disk list format written by the application.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Color accent assigned to a list.
pub enum Accent {
    #[default]
    Mint,
    Charcoal,
    Crimson,
    Sand,
    Sky,
    Lavender,
}

impl Accent {
    pub const ALL: [Self; 6] = [
        Self::Mint,
        Self::Charcoal,
        Self::Crimson,
        Self::Sand,
        Self::Sky,
        Self::Lavender,
    ];

    /// Returns the user-facing name of the accent.
    ///
    /// # Returns
    ///
    /// A static display label for this accent.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mint => "Mint",
            Self::Charcoal => "Charcoal",
            Self::Crimson => "Crimson",
            Self::Sand => "Sand",
            Self::Sky => "Sky",
            Self::Lavender => "Lavender",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Font family available for application text.
pub enum FontChoice {
    #[default]
    Sans,
    Mono,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A persisted task list with its live tasks and archive.
pub struct TodoList {
    pub format_version: u32,
    pub id: Uuid,
    pub title: String,
    pub created_at_unix: u64,
    pub accent: Accent,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub archive: Vec<ArchivedTask>,
}

impl TodoList {
    /// Creates an empty list with a new stable identity.
    ///
    /// # Arguments
    ///
    /// - `title` - User-facing list title.
    /// - `accent` - Initial list accent.
    ///
    /// # Returns
    ///
    /// A versioned list with no tasks or archived entries.
    pub fn new(title: impl Into<String>, accent: Accent) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            id: Uuid::new_v4(),
            title: title.into(),
            created_at_unix: now_unix_seconds(),
            accent,
            tasks: Vec::new(),
            archive: Vec::new(),
        }
    }

    /// Counts tasks that have not been completed.
    ///
    /// # Returns
    ///
    /// The number of active tasks in the live task collection.
    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|task| !task.completed).count()
    }

    /// Counts completed tasks that have not yet been archived.
    ///
    /// # Returns
    ///
    /// The number of completed tasks in the live task collection.
    pub fn completed_count(&self) -> usize {
        self.tasks.iter().filter(|task| task.completed).count()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A top-level task and its optional checklist.
pub struct Task {
    pub id: Uuid,
    pub text: String,
    pub completed: bool,
    pub created_at_unix: u64,
    pub completed_at_unix: Option<u64>,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
}

impl Task {
    /// Creates an active task with a new identity and creation timestamp.
    ///
    /// # Returns
    ///
    /// A task with no completion timestamp or subtasks.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            completed: false,
            created_at_unix: now_unix_seconds(),
            completed_at_unix: None,
            subtasks: Vec::new(),
        }
    }

    /// Flips completion state and updates the completion timestamp.
    pub fn toggle(&mut self) {
        self.completed = !self.completed;
        self.completed_at_unix = self.completed.then(now_unix_seconds);
    }

    /// Returns completed and total subtask counts when a checklist exists.
    ///
    /// # Returns
    ///
    /// `Some((completed, total))` for a checklist, otherwise `None`.
    pub fn progress(&self) -> Option<(usize, usize)> {
        if self.subtasks.is_empty() {
            return None;
        }
        Some((
            self.subtasks.iter().filter(|item| item.completed).count(),
            self.subtasks.len(),
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// One checklist item attached to a task.
pub struct Subtask {
    pub id: Uuid,
    pub text: String,
    pub completed: bool,
}

impl Subtask {
    /// Creates an incomplete subtask with a new identity.
    ///
    /// # Returns
    ///
    /// A new incomplete subtask containing the supplied text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            completed: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A completed task retained with the time it entered history.
pub struct ArchivedTask {
    pub task: Task,
    pub archived_at_unix: u64,
}

impl ArchivedTask {
    /// Wraps a task for archival and records the current time.
    ///
    /// # Returns
    ///
    /// An archive entry containing the task and current timestamp.
    pub fn new(task: Task) -> Self {
        Self {
            task,
            archived_at_unix: now_unix_seconds(),
        }
    }
}

/// Returns the current Unix timestamp in whole seconds.
///
/// # Returns
///
/// Seconds since the Unix epoch, or zero when the system clock predates it.
pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
