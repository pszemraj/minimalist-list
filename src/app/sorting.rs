//! Task view sorting with a separately retained manual order on disk.

use super::MinimalistApp;
use crate::{
    model::{Task, TaskSort},
    storage::StoredList,
};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn reconcile_order(previous: &[Uuid], tasks: &[Task]) -> Vec<Uuid> {
    let live: HashSet<_> = tasks.iter().map(|task| task.id).collect();
    let known: HashSet<_> = previous.iter().copied().collect();
    let mut seen = HashSet::new();
    tasks
        .iter()
        .map(|task| task.id)
        .filter(|id| !known.contains(id))
        .chain(previous.iter().copied().filter(|id| live.contains(id)))
        .filter(|id| seen.insert(*id))
        .collect()
}

fn apply_order(tasks: &mut [Task], manual: &[Uuid], mode: TaskSort) {
    match mode {
        TaskSort::Manual => {
            let positions: HashMap<_, _> =
                manual.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            tasks.sort_by_key(|task| positions.get(&task.id).copied().unwrap_or(usize::MAX));
        }
        TaskSort::Newest => tasks.sort_by_key(|task| std::cmp::Reverse(task.created_at_unix)),
        TaskSort::Oldest => tasks.sort_by_key(|task| task.created_at_unix),
        TaskSort::Title => tasks.sort_by_cached_key(|task| task.text.to_lowercase()),
        TaskSort::IncompleteFirst => tasks.sort_by_key(|task| task.completed),
    }
}

impl MinimalistApp {
    fn task_order_key(&self, list: Uuid) -> String {
        format!("{}::{list}", self.workspace.to_string_lossy())
    }

    /// Returns the selected task view mode for this workspace and list.
    ///
    /// # Arguments
    /// - `list` - List identity in the current workspace.
    ///
    /// # Returns
    /// The saved mode, defaulting to Manual.
    pub(super) fn task_sort_mode(&self, list: Uuid) -> TaskSort {
        self.settings
            .task_sort
            .get(&self.task_order_key(list))
            .copied()
            .unwrap_or_default()
    }

    /// Captures authoritative manual sequences immediately after reading list JSON.
    pub(super) fn capture_loaded_task_orders(&mut self) {
        self.capture_loaded_task_orders_except(&[]);
    }

    /// Captures fresh JSON orders without replacing snapshots of cached lists.
    ///
    /// # Arguments
    /// - `retained_paths` - Unreadable files whose already sorted cached lists survived a scan.
    pub(super) fn capture_loaded_task_orders_except(
        &mut self,
        retained_paths: &[std::path::PathBuf],
    ) {
        self.prune_list_preferences();
        self.manual_task_order
            .retain(|id, _| self.lists.iter().any(|list| list.key == *id));
        for list in &self.lists {
            if retained_paths.contains(&list.path) {
                continue;
            }
            self.manual_task_order.insert(
                list.key,
                list.data.tasks.iter().map(|task| task.id).collect(),
            );
        }
    }

    /// Removes preferences for lists no longer present in the current workspace.
    pub(super) fn prune_list_preferences(&mut self) {
        let workspace = self.workspace.to_string_lossy();
        let prefix = format!("{workspace}::");
        let live: HashSet<_> = self.lists.iter().map(|list| list.key).collect();
        self.settings.task_sort.retain(|key, _| {
            key.strip_prefix(&prefix)
                .is_none_or(|id| id.parse::<Uuid>().is_ok_and(|id| live.contains(&id)))
        });
        if let Some(order) = self.settings.list_order.get_mut(workspace.as_ref()) {
            order.retain(|id| live.contains(id));
        }
    }

    /// Reapplies selected view modes after loading, saving, or adopting candidates.
    ///
    /// # Panics
    /// Panics if an internally enumerated list index becomes invalid.
    pub(super) fn apply_task_sorts(&mut self) {
        for index in 0..self.lists.len() {
            let list = self.lists[index].key;
            let previous = self
                .manual_task_order
                .get(&list)
                .cloned()
                .unwrap_or_else(|| {
                    self.lists[index]
                        .data
                        .tasks
                        .iter()
                        .map(|task| task.id)
                        .collect()
                });
            let manual = reconcile_order(&previous, &self.lists[index].data.tasks);
            let mode = self.task_sort_mode(list);
            apply_order(&mut self.lists[index].data.tasks, &manual, TaskSort::Manual);
            apply_order(&mut self.lists[index].data.tasks, &manual, mode);
            self.manual_task_order.insert(list, manual);
        }
    }

    /// Prepares a modified candidate for serialization in retained Manual order.
    ///
    /// # Arguments
    /// - `candidate` - Whole-list candidate, including its latest task mutations.
    pub(super) fn prepare_list_for_save(&mut self, candidate: &mut StoredList) {
        let mode = self.task_sort_mode(candidate.key);
        let manual = if mode == TaskSort::Manual {
            candidate.data.tasks.iter().map(|task| task.id).collect()
        } else {
            let previous = self
                .manual_task_order
                .get(&candidate.key)
                .cloned()
                .unwrap_or_default();
            reconcile_order(&previous, &candidate.data.tasks)
        };
        self.manual_task_order.insert(candidate.key, manual.clone());
        apply_order(&mut candidate.data.tasks, &manual, TaskSort::Manual);
    }

    /// Clones the loaded list and prepares its tasks for a Manual-order disk write.
    ///
    /// # Arguments
    /// - `index` - Loaded list index.
    ///
    /// # Returns
    /// A whole-list candidate with content unchanged and tasks in Manual order.
    ///
    /// # Panics
    /// Panics if the caller supplies an invalid loaded-list index.
    pub(super) fn list_for_save(&mut self, index: usize) -> StoredList {
        let mut candidate = self.lists[index].clone();
        self.prepare_list_for_save(&mut candidate);
        candidate
    }

    /// Finds a task's retained Manual position for deletion recovery.
    ///
    /// # Arguments
    /// - `list` - Source list identity.
    /// - `task` - Task identity.
    ///
    /// # Returns
    /// Its Manual position if the task exists.
    ///
    /// # Panics
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn manual_task_position(&self, list: Uuid, task: Uuid) -> Option<usize> {
        if self.task_sort_mode(list) != TaskSort::Manual
            && let Some(order) = self.manual_task_order.get(&list)
            && let Some(position) = order.iter().position(|id| *id == task)
        {
            return Some(position);
        }
        self.list_index(list).and_then(|index| {
            self.lists[index]
                .data
                .tasks
                .iter()
                .position(|item| item.id == task)
        })
    }

    /// Restores a known task position before preparing an Undo or History write.
    ///
    /// # Arguments
    /// - `list` - Destination list identity.
    /// - `task` - Restored task identity.
    /// - `position` - Its original or requested Manual position.
    ///
    /// # Panics
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn restore_manual_task_position(&mut self, list: Uuid, task: Uuid, position: usize) {
        let fallback = self
            .list_index(list)
            .map(|index| {
                self.lists[index]
                    .data
                    .tasks
                    .iter()
                    .map(|item| item.id)
                    .collect()
            })
            .unwrap_or_default();
        let order = self.manual_task_order.entry(list).or_insert(fallback);
        order.retain(|id| *id != task);
        order.insert(position.min(order.len()), task);
    }

    /// Renders the task view mode selector, including restoration of Manual order.
    ///
    /// # Arguments
    /// - `ui` - Destination for the menu button.
    /// - `list` - List identity.
    pub(super) fn task_sort_menu(&mut self, ui: &mut egui::Ui, list: Uuid) {
        let mode = self.task_sort_mode(list);
        let menu = ui.menu_button(format!("Sort: {}", mode.label()), |ui| {
            for order in TaskSort::ALL {
                if ui.selectable_label(mode == order, order.label()).clicked() {
                    self.sort_tasks(list, order);
                    ui.close();
                }
            }
        });
        self.keyboard.settings_rows.push(vec![menu.response.id]);
    }

    /// Changes the visible sort while preserving task files in Manual order.
    ///
    /// # Arguments
    /// - `list` - List identity.
    /// - `order` - Selected view mode.
    ///
    /// # Returns
    /// Whether visible task positions changed.
    ///
    /// # Panics
    /// Panics if an internally resolved list index becomes invalid.
    pub(super) fn sort_tasks(&mut self, list: Uuid, order: TaskSort) -> bool {
        self.finish_inline_edit();
        self.drag = None;
        self.revealed = None;
        let Some(index) = self.list_index(list) else {
            return false;
        };
        let previous: Vec<_> = self.lists[index]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .collect();
        let old_mode = self.task_sort_mode(list);
        let key = self.task_order_key(list);
        if old_mode == TaskSort::Manual {
            self.manual_task_order.insert(list, previous.clone());
        }
        self.settings.task_sort.insert(key, order);
        self.apply_task_sorts();
        if old_mode != order {
            self.save_settings();
        }
        self.lists[index]
            .data
            .tasks
            .iter()
            .map(|task| task.id)
            .ne(previous)
    }
}

#[cfg(test)]
#[path = "sorting_tests.rs"]
mod tests;
