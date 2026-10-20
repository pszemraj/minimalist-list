//! Explicit recovery keeps local edits in an independent list before reloading sync changes.

use crate::app::MinimalistApp;
use crate::{
    model::Task,
    storage::{self, StoredList},
};
use uuid::Uuid;

impl MinimalistApp {
    /// Remembers the exact candidate rejected because its associated file changed.
    ///
    /// # Arguments
    /// - `candidate` - Local contents that were not written.
    pub(in crate::app) fn record_save_conflict(&mut self, candidate: &StoredList) {
        if storage::list_changed_on_disk(candidate).unwrap_or(false) {
            self.persistence
                .conflicted_candidates
                .insert(candidate.key, candidate.clone());
        }
    }

    /// Returns whether this list has a retained candidate needing conflict recovery.
    ///
    /// # Returns
    /// Whether its local edits can be preserved with the recovery action.
    pub(in crate::app) fn has_save_conflict(&self, list: Uuid) -> bool {
        self.persistence.conflicted_candidates.contains_key(&list)
    }

    /// Forgets conflict snapshots after a workspace change or successful external reload.
    pub(in crate::app) fn clear_save_conflicts(&mut self) {
        self.persistence.conflicted_candidates.clear();
    }

    /// Forgets a resolved or explicitly canceled list's conflict snapshot.
    pub(in crate::app) fn forget_save_conflict(&mut self, list: Uuid) {
        self.persistence.conflicted_candidates.remove(&list);
    }

    /// Saves a local copy before adopting the independently changed original list.
    ///
    /// # Arguments
    /// - `list` - Conflicted original list identity.
    ///
    /// # Returns
    /// Whether the local copy was saved and this conflict was resolved.
    ///
    /// # Panics
    /// Panics if an internally resolved loaded-list index is invalid.
    pub(in crate::app) fn keep_local_copy_and_reload(&mut self, list: Uuid) -> bool {
        let Some(mut candidate) = self.persistence.conflicted_candidates.get(&list).cloned() else {
            return false;
        };
        if let Some(index) = self.list_index(list) {
            candidate.data = self.lists[index].data.clone();
        }
        if let Some((editing_list, task_id)) = self.editing
            && editing_list == list
            && let Some(task) = candidate
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            && !self.edit_text.trim().is_empty()
        {
            task.text = self.edit_text.trim().to_owned();
        }
        if let Some(editor) = &self.details.editor
            && editor.list == list
            && let Some(task) = candidate
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == editor.task)
            && let Some(child) = task
                .subtasks
                .iter_mut()
                .find(|child| child.id == editor.subtask)
            && !editor.text.trim().is_empty()
        {
            child.text = editor.text.trim().to_owned();
        }
        if let Some(editor) = &self.task_context
            && editor.list == list
            && !editor.archived
            && let Some(task) = candidate
                .data
                .tasks
                .iter_mut()
                .find(|task| task.id == editor.task)
        {
            task.context.clone_from(&editor.text);
        }
        for draft in self
            .quick_capture
            .iter()
            .chain(self.capture_draft.iter())
            .filter(|draft| draft.selected_list_id == Some(list))
        {
            let text = draft.text.trim();
            if !text.is_empty() {
                candidate.data.tasks.insert(0, Task::new(text));
            }
        }
        if self.settings_list_id == Some(list)
            && self.rename_input.trim() != self.rename_baseline.trim()
            && !self.rename_input.trim().is_empty()
        {
            candidate.data.title = self.rename_input.trim().to_owned();
        }
        self.prepare_list_for_save(&mut candidate);
        let result = storage::load_workspace(&self.workspace).and_then(|snapshot| {
            storage::create_local_copy(&self.workspace, &candidate).map(|copy| (snapshot, copy))
        });
        let (snapshot, copy) = match result {
            Ok(saved) => saved,
            Err(error) => {
                self.status = Some(error);
                return false;
            }
        };
        let copy_title = copy.data.title.clone();
        self.lists.retain(|stored| stored.key != list);
        self.manual_task_order.remove(&list);
        if let Some(original) = snapshot.lists.into_iter().find(|stored| stored.key == list) {
            self.manual_task_order.insert(
                list,
                original.data.tasks.iter().map(|task| task.id).collect(),
            );
            self.lists.push(original);
        }
        self.lists.push(copy);
        if self
            .editing
            .is_some_and(|(editing_list, _)| editing_list == list)
        {
            self.editing = None;
        }
        if self
            .details
            .editor
            .as_ref()
            .is_some_and(|editor| editor.list == list)
        {
            self.details.editor = None;
        }
        self.persistence.dirty_lists.remove(&list);
        self.persistence.pending_filenames.remove(&list);
        self.persistence.conflicted_candidates.remove(&list);
        if self
            .task_context
            .as_ref()
            .is_some_and(|editor| editor.list == list)
        {
            self.task_context = None;
        }
        if self
            .quick_capture
            .as_ref()
            .is_some_and(|draft| draft.selected_list_id == Some(list))
        {
            self.quick_capture = None;
        }
        if self
            .capture_draft
            .as_ref()
            .is_some_and(|draft| draft.selected_list_id == Some(list))
        {
            self.capture_draft = None;
        }
        if self.settings_list_id == Some(list) {
            self.rename_input.clone_from(&self.rename_baseline);
        }
        if self.list_index(list).is_none()
            && matches!(self.screen, crate::app::Screen::List(id) if id == list)
        {
            self.go_to_overview();
        }
        self.apply_list_order();
        self.apply_task_sorts();
        self.next_scan = std::time::Instant::now();
        self.status = Some(format!(
            "Local changes kept in “{copy_title}”; the original file was kept on disk."
        ));
        true
    }
}
