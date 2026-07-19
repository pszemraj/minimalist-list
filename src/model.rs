use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
pub enum FontChoice {
    #[default]
    Sans,
    Mono,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|task| !task.completed).count()
    }

    pub fn completed_count(&self) -> usize {
        self.tasks.iter().filter(|task| task.completed).count()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

    pub fn toggle(&mut self) {
        self.completed = !self.completed;
        self.completed_at_unix = self.completed.then(now_unix_seconds);
    }

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
pub struct Subtask {
    pub id: Uuid,
    pub text: String,
    pub completed: bool,
}

impl Subtask {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            completed: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedTask {
    pub task: Task,
    pub archived_at_unix: u64,
}

impl ArchivedTask {
    pub fn new(task: Task) -> Self {
        Self {
            task,
            archived_at_unix: now_unix_seconds(),
        }
    }
}

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
