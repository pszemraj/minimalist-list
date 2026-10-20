//! Save results, retryable in-memory edits, and visible persistence status.

use super::{MinimalistApp, theme::Palette};
use crate::storage::{self, Settings, StoredList};
use eframe::egui::{self, Button, Color32, CornerRadius, Frame, Margin};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};
use uuid::Uuid;

/// Tracks retryable saves and the last local preference snapshot.
pub(super) struct PersistenceState {
    pub(super) dirty_lists: HashSet<Uuid>,
    conflicted_candidates: HashMap<Uuid, StoredList>,
    pub(super) pending_filenames: HashSet<Uuid>,
    pub(super) workspace_failure: Option<String>,
    baseline: Settings,
    last_retry_error: Option<String>,
    pub(super) settings_dirty: bool,
    settings_save_after: Option<Instant>,
    pub(super) notice: Option<(String, Instant)>,
}

#[cfg(test)]
#[path = "filename_retry_tests.rs"]
mod filename_retry_tests;

impl PersistenceState {
    /// Initializes save coordination from the startup preferences.
    ///
    /// # Arguments
    /// - `baseline` - Preferences read at startup.
    /// - `workspace_failure` - Selected path that needs recovery.
    ///
    /// # Returns
    /// Empty pending writes and the startup preference snapshot.
    pub(super) fn new(baseline: Settings, workspace_failure: Option<String>) -> Self {
        Self {
            dirty_lists: HashSet::new(),
            conflicted_candidates: HashMap::new(),
            pending_filenames: HashSet::new(),
            workspace_failure,
            baseline,
            last_retry_error: None,
            settings_dirty: false,
            settings_save_after: None,
            notice: None,
        }
    }
}

impl MinimalistApp {
    /// Shows a short acknowledgement after a successful task operation.
    pub(super) fn acknowledge(&mut self, message: String) {
        self.persistence.notice = Some((message, Instant::now()));
    }

    /// Coalesces continuous appearance edits into one preference write after a pause.
    pub(super) fn schedule_settings_save(&mut self, ctx: &egui::Context) {
        let delay = std::time::Duration::from_millis(250);
        self.persistence.settings_save_after = Some(Instant::now() + delay);
        ctx.request_repaint_after(delay);
    }

    /// Flushes a scheduled preference update once its quiet period has elapsed.
    pub(super) fn flush_deferred_settings(&mut self, ctx: &egui::Context) {
        if let Some(deadline) = self.persistence.settings_save_after {
            if Instant::now() >= deadline {
                self.save_settings();
            } else {
                ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
            }
        }
    }

    /// Saves locally changed preferences without replacing newer unrelated values.
    pub(super) fn save_settings(&mut self) {
        self.persistence.settings_save_after = None;
        let mut desired = self.settings.clone();
        desired
            .workspace_path
            .clone_from(&self.persisted_workspace_path);
        match storage::save_settings_changes(&desired, &self.persistence.baseline) {
            Ok(saved) => {
                // A launch-only workspace override must remain launch-only.
                let runtime_workspace = self.settings.workspace_path.clone();
                self.persisted_workspace_path
                    .clone_from(&saved.workspace_path);
                self.settings = saved.clone();
                self.settings.workspace_path = runtime_workspace;
                self.persistence.baseline = saved;
                self.persistence.settings_dirty = false;
            }
            Err(error) => {
                self.persistence.settings_dirty = true;
                self.persistence.last_retry_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Saves one list and retains failed content for explicit retry.
    ///
    /// # Returns
    /// Whether the list contents reached disk; filename-only failures remain warnings.
    ///
    /// # Panics
    /// Panics if the caller supplies an invalid loaded-list index.
    pub(super) fn save_list_index(&mut self, index: usize) -> bool {
        let list = self.list_for_save(index);
        let id = list.key;
        let saved = match storage::save_list(&list) {
            Ok(()) => {
                self.lists[index].stamp.set(list.stamp.get());
                self.complete_list_save(index)
            }
            Err(error) => {
                self.record_save_conflict(&list);
                self.persistence.dirty_lists.insert(id);
                let error = format!("Changes are unsaved: {error}");
                self.persistence.last_retry_error = Some(error.clone());
                self.status = Some(error);
                false
            }
        };
        self.apply_task_sorts();
        saved
    }

    /// Writes a modal's candidate before adopting its changes in the loaded list.
    ///
    /// # Arguments
    /// - `index` - Loaded list to replace after a successful write.
    /// - `candidate` - Modified list, still independent of the loaded contents.
    ///
    /// # Returns
    /// Success after adopting the candidate and restoring the selected view sort.
    ///
    /// # Errors
    /// Returns the storage error and retains any external-change conflict snapshot.
    ///
    /// # Panics
    /// Panics if the caller supplies an invalid loaded-list index.
    pub(super) fn save_list_candidate(
        &mut self,
        index: usize,
        mut candidate: StoredList,
    ) -> Result<(), String> {
        self.prepare_list_for_save(&mut candidate);
        if let Err(error) = storage::save_list(&candidate) {
            self.record_save_conflict(&candidate);
            return Err(error);
        }
        self.lists[index] = candidate;
        self.apply_task_sorts();
        self.complete_list_save(index);
        Ok(())
    }

    /// Finishes a successful candidate write, including explicitly requested file renames.
    ///
    /// # Arguments
    /// - `index` - Loaded list whose associated file has just been saved.
    ///
    /// # Returns
    /// Whether the content is saved, even if its descriptive filename could not be updated.
    ///
    /// # Panics
    /// Panics if the caller supplies an invalid loaded-list index.
    pub(super) fn complete_list_save(&mut self, index: usize) -> bool {
        let id = self.lists[index].key;
        self.persistence.dirty_lists.remove(&id);
        self.persistence.conflicted_candidates.remove(&id);
        if self.persistence.pending_filenames.contains(&id) {
            let previous = self.lists[index].path.clone();
            match storage::rename_list_file(&mut self.lists[index]) {
                Ok(()) => {
                    self.persistence.pending_filenames.remove(&id);
                    if previous != self.lists[index].path {
                        storage::remove_file_from_fingerprint(&mut self.fingerprint, &previous);
                    }
                }
                Err(error) => {
                    self.persistence.pending_filenames.remove(&id);
                    let error = format!(
                        "List saved at its previous filename. Rename it again to retry the filename update: {error}"
                    );
                    self.status = Some(error);
                }
            }
        }
        if let Err(error) =
            storage::update_fingerprint_for_file(&mut self.fingerprint, &self.lists[index])
        {
            self.status = Some(error);
        }
        true
    }

    /// Retries retained list and preference writes before leaving the workspace.
    ///
    /// # Returns
    /// Whether all retained writes succeeded.
    pub(super) fn retry_dirty_lists(&mut self) -> bool {
        let live: HashSet<_> = self.lists.iter().map(|list| list.key).collect();
        self.persistence.dirty_lists.retain(|id| live.contains(id));
        self.persistence
            .pending_filenames
            .retain(|id| live.contains(id));
        let ids: Vec<_> = self
            .persistence
            .dirty_lists
            .union(&self.persistence.pending_filenames)
            .copied()
            .collect();
        for id in ids {
            if let Some(index) = self.list_index(id) {
                self.save_list_index(index);
            }
        }
        if self.persistence.settings_dirty || self.persistence.settings_save_after.is_some() {
            self.save_settings();
        }
        let saved = self.persistence.dirty_lists.is_empty()
            && self.persistence.pending_filenames.is_empty()
            && !self.persistence.settings_dirty;
        if saved
            && let Some(error) = self.persistence.last_retry_error.take()
            && self.status.as_ref() == Some(&error)
        {
            self.status = None;
        }
        saved
    }

    fn retry_workspace(&mut self) {
        let Some(selected) = self.persistence.workspace_failure.clone() else {
            return;
        };
        if !self.retry_dirty_lists() {
            return;
        }
        // Retry is recovery of the original selection, including a launch override;
        // unlike explicitly choosing a folder it does not rewrite that preference.
        let persisted = self.persisted_workspace_path.clone();
        self.open_workspace(&selected, false);
        self.persisted_workspace_path = persisted;
        self.save_settings();
    }

    /// Renders visible save and workspace recovery actions above the content.
    ///
    /// # Arguments
    /// - `ui` - Reserved status region above the main view.
    /// - `palette` - Current text colors.
    pub(super) fn status_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let dirty = !self.persistence.dirty_lists.is_empty()
            || !self.persistence.pending_filenames.is_empty()
            || self.persistence.settings_dirty;
        let failed_workspace = self.persistence.workspace_failure.clone();
        let notice = self
            .persistence
            .notice
            .as_ref()
            .map(|(message, _)| message.clone());
        let success_only =
            !dirty && failed_workspace.is_none() && self.status.is_none() && notice.is_some();
        let message = failed_workspace
            .as_ref()
            .map(|path| format!("Workspace unavailable: {path}. Retry or choose another folder."))
            .or_else(|| self.status.clone())
            .or(notice)
            .unwrap_or_else(|| "Changes are unsaved.".into());
        Frame::NONE
            .fill(if success_only {
                palette.surface
            } else {
                Color32::from_rgba_unmultiplied(80, 25, 35, 245)
            })
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                if failed_workspace.is_some() {
                    ui.add(egui::Label::new(self.rich(message, 13.0, palette.text)).wrap());
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("status-message")
                        .max_height(54.0)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(self.rich(message, 13.0, palette.text)).wrap());
                        });
                }
                if !success_only {
                    ui.horizontal_wrapped(|ui| {
                        if dirty && ui.button("Retry save").clicked() {
                            self.retry_dirty_lists();
                        }
                        let conflict = self
                            .persistence
                            .conflicted_candidates
                            .keys()
                            .copied()
                            .find(|id| self.persistence.dirty_lists.contains(id));
                        if let Some(id) = conflict
                            && ui.button("Keep local copy and reload").clicked()
                        {
                            self.keep_local_copy_and_reload(id);
                        }
                        if failed_workspace.is_some() && ui.button("Retry workspace").clicked() {
                            self.retry_workspace();
                        }
                        if failed_workspace.is_some() && ui.button("Choose workspace…").clicked()
                        {
                            self.open_settings(None);
                        }
                        if !dirty
                            && failed_workspace.is_none()
                            && ui.add(Button::new("Dismiss")).clicked()
                        {
                            self.status = None;
                            self.persistence.notice = None;
                        }
                    });
                }
            });
    }
}

#[cfg(test)]
#[path = "persistence_ui_tests.rs"]
mod ui_tests;

#[path = "save_conflict.rs"]
mod save_conflict;
