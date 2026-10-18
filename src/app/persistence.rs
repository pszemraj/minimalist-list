//! Save results, retryable in-memory edits, and visible persistence status.

use super::{MinimalistApp, theme::Palette};
use crate::storage::{self, Settings};
use eframe::egui::{self, Button, Color32, CornerRadius, Frame, Margin};
use std::{collections::HashSet, time::Instant};
use uuid::Uuid;

/// Tracks retryable saves and the last local preference snapshot.
pub(super) struct PersistenceState {
    pub(super) dirty_lists: HashSet<Uuid>,
    pub(super) workspace_failure: Option<String>,
    baseline: Settings,
    pub(super) settings_dirty: bool,
    pub(super) notice: Option<(String, Instant)>,
}

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
            workspace_failure,
            baseline,
            settings_dirty: false,
            notice: None,
        }
    }
}

impl MinimalistApp {
    /// Shows a short acknowledgement after a successful task operation.
    pub(super) fn acknowledge(&mut self, message: String) {
        self.persistence.notice = Some((message, Instant::now()));
    }

    /// Saves locally changed preferences without replacing newer unrelated values.
    pub(super) fn save_settings(&mut self) {
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
                self.status = Some(error);
            }
        }
    }

    /// Saves one list and retains failed content for explicit retry.
    ///
    /// # Returns
    /// Whether the list contents reached disk.
    ///
    /// # Panics
    /// Panics if the caller supplies an invalid loaded-list index.
    pub(super) fn save_list_index(&mut self, index: usize) -> bool {
        let list = &self.lists[index];
        let id = list.key;
        match storage::save_list(list) {
            Ok(()) => {
                self.persistence.dirty_lists.remove(&id);
                if let Err(error) =
                    storage::update_fingerprint_for_file(&mut self.fingerprint, &list.path)
                {
                    self.status = Some(error);
                }
                true
            }
            Err(error) => {
                self.persistence.dirty_lists.insert(id);
                self.status = Some(format!("Changes are unsaved: {error}"));
                false
            }
        }
    }

    /// Retries retained list and preference writes before leaving the workspace.
    ///
    /// # Returns
    /// Whether all retained writes succeeded.
    pub(super) fn retry_dirty_lists(&mut self) -> bool {
        let live: HashSet<_> = self.lists.iter().map(|list| list.key).collect();
        self.persistence.dirty_lists.retain(|id| live.contains(id));
        let ids: Vec<_> = self.persistence.dirty_lists.iter().copied().collect();
        for id in ids {
            if let Some(index) = self.list_index(id) {
                self.save_list_index(index);
            }
        }
        if self.persistence.settings_dirty {
            self.save_settings();
        }
        let saved = self.persistence.dirty_lists.is_empty() && !self.persistence.settings_dirty;
        if saved {
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
        self.switch_workspace(&selected);
        self.persisted_workspace_path = persisted;
        self.save_settings();
    }

    /// Renders visible save and workspace recovery actions above the content.
    ///
    /// # Arguments
    /// - `ui` - Reserved status region above the main view.
    /// - `palette` - Current text colors.
    pub(super) fn status_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let dirty = !self.persistence.dirty_lists.is_empty() || self.persistence.settings_dirty;
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
            .map(|path| format!("Workspace unavailable: {path}. Using a temporary fallback."))
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
                egui::ScrollArea::vertical()
                    .id_salt("status-message")
                    .max_height(54.0)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(self.rich(message, 13.0, palette.text)).wrap());
                    });
                if !success_only {
                    ui.horizontal(|ui| {
                        if dirty && ui.button("Retry save").clicked() {
                            self.retry_dirty_lists();
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
