//! Session-only drafts scoped to their originating workspace and object.

use super::{
    MinimalistApp, Screen,
    overlays::{FindState, QuickCaptureState},
};
use std::{collections::HashMap, path::PathBuf};
use uuid::Uuid;

#[derive(Default)]
/// Keeps unfinished session text attached to its workspace and object.
pub(super) struct Drafts {
    tasks: HashMap<(PathBuf, Uuid), String>,
    children: HashMap<(PathBuf, Uuid, Uuid), String>,
    captures: HashMap<PathBuf, QuickCaptureState>,
    searches: HashMap<PathBuf, FindState>,
}

impl MinimalistApp {
    /// Saves capture and search state before changing workspaces.
    pub(super) fn stash_overlay_drafts(&mut self) {
        if let Some(draft) = self.quick_capture.take().or(self.capture_draft.take()) {
            self.drafts.captures.insert(self.workspace.clone(), draft);
        }
        if let Some(draft) = self.find.take().or(self.find_draft.take()) {
            self.drafts.searches.insert(self.workspace.clone(), draft);
        }
    }
    /// Restores the current workspace's capture and search session.
    pub(super) fn restore_overlay_drafts(&mut self) {
        self.capture_draft = self.drafts.captures.remove(&self.workspace);
        self.find_draft = self.drafts.searches.remove(&self.workspace);
    }

    /// Moves an unfinished checklist entry alongside its parent task.
    ///
    /// # Arguments
    /// - `source` - Original list identity.
    /// - `destination` - Target list identity.
    /// - `task` - Parent task identity.
    pub(super) fn move_subtask_draft(&mut self, source: Uuid, destination: Uuid, task: Uuid) {
        if self.subtasks_for == Some((source, task)) {
            self.switch_subtask_panel(None);
        }
        if let Some(draft) = self
            .drafts
            .children
            .remove(&(self.workspace.clone(), source, task))
        {
            self.drafts
                .children
                .insert((self.workspace.clone(), destination, task), draft);
        }
    }

    /// Retains the visible list entry and checklist draft before navigation.
    pub(super) fn stash_input_drafts(&mut self) {
        if let Screen::List(id) = self.screen {
            self.drafts
                .tasks
                .insert((self.workspace.clone(), id), self.new_task.clone());
        }
        self.switch_subtask_panel(None);
    }

    /// Restores task entry for the selected list in the current workspace.
    pub(super) fn restore_task_draft(&mut self, list: Uuid) {
        self.new_task = self
            .drafts
            .tasks
            .remove(&(self.workspace.clone(), list))
            .unwrap_or_default();
    }

    /// Exchanges checklist panels while retaining each unfinished entry.
    pub(super) fn switch_subtask_panel(&mut self, next: Option<(Uuid, Uuid)>) {
        if self.subtasks_for == next {
            return;
        }
        self.finish_subtask_edit();
        if let Some((list, task)) = self.subtasks_for {
            self.drafts.children.insert(
                (self.workspace.clone(), list, task),
                std::mem::take(&mut self.new_subtask),
            );
        }
        self.subtasks_for = next;
        self.new_subtask = next
            .and_then(|(list, task)| {
                self.drafts
                    .children
                    .remove(&(self.workspace.clone(), list, task))
            })
            .unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn switching_workspace_through_settings_restores_the_originating_entry_draft() {
        let (_, mut app) = super::super::tests::test_app();
        let original = app.workspace.clone();
        let list = app.lists[0].key;
        app.new_task = "Keep this original list draft".into();
        app.open_settings(None);
        app.switch_workspace(original.join("next-workspace").to_str().unwrap());
        app.new_task = "Keep the next workspace draft too".into();
        app.open_settings(None);
        app.switch_workspace(original.to_str().unwrap());
        app.open_list(list);
        assert_eq!(app.new_task, "Keep this original list draft");
        app.open_settings(None);
        app.switch_workspace(original.join("next-workspace").to_str().unwrap());
        assert_eq!(app.new_task, "Keep the next workspace draft too");
        std::fs::remove_dir_all(original).unwrap();
    }
}
