//! Recovery of independently synced list files that share an identity.

use super::{
    StoredList, backups_dir, list_changed_on_disk, load_existing_workspace, load_lists, read_list,
    save_new_list,
};
use crate::model::{ArchivedTask, Task, TodoList};
use std::collections::HashMap;
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::Write,
    path::Path,
};
use uuid::Uuid;

/// Imports a duplicate as an independent list and keeps its original file in local backups.
///
/// # Arguments
/// - `root` - Workspace holding the conflicting files.
/// - `path` - Unselected file reported in the current workspace scan.
///
/// # Returns
/// The saved independent list and a summary of its differences from the original.
///
/// # Errors
/// Returns an error if the path is no longer a duplicate, the backup or copy
/// cannot be saved, or the duplicate cannot be removed without replacing newer
/// contents. Any copy edited during recovery stays in place.
pub fn recover_duplicate(root: &Path, path: &Path) -> Result<(StoredList, String), String> {
    import_duplicate(root, path)
        .map_err(|error| format!("Could not recover {}: {error}", path.display()))
}

fn import_duplicate(root: &Path, path: &Path) -> Result<(StoredList, String), String> {
    let snapshot = load_existing_workspace(root)?;
    if !snapshot
        .duplicate_paths
        .iter()
        .any(|duplicate| duplicate == path)
    {
        return Err("this file is no longer a duplicate".into());
    }
    let (source, _) = read_list(path)?;
    let Some(name) = path.file_name() else {
        return Err("the file name is missing".into());
    };
    let Some(canonical) = snapshot.lists.iter().find(|list| list.key == source.key) else {
        return Err("the file changed during recovery".into());
    };
    if list_changed_on_disk(&source)? {
        return Err("the file changed during recovery".into());
    }
    let original = source.source_bytes.borrow();
    let Some(bytes) = original.as_deref() else {
        return Err("the original file contents are unavailable".into());
    };
    backup_conflicted_bytes(root, name, bytes)
        .map_err(|error| format!("could not save a local backup: {error}"))?;
    let copy = create_independent_copy(root, &source, "recovered copy")?;
    let removed = list_changed_on_disk(&source).and_then(|changed| {
        if changed {
            return Err("the file changed during recovery".into());
        }
        fs::remove_file(path).map_err(|error| format!("could not remove the duplicate: {error}"))
    });
    if let Err(error) = removed {
        if list_changed_on_disk(&copy) == Ok(false) && fs::remove_file(&copy.path).is_ok() {
            return Err(error);
        }
        return Err(format!(
            "{error}. A recovered list was also saved to {}.",
            copy.path.display()
        ));
    }
    Ok((copy, recovery_summary(&canonical.data, &source.data)))
}

/// Saves unsaved list contents under the original filename before discarding them.
///
/// # Arguments
/// - `root` - The workspace whose local backups receive the list.
/// - `source` - The unsaved list and its original filename.
///
/// # Returns
/// Success once the backup is flushed to disk.
///
/// # Errors
/// Returns an error if the filename is missing or the contents cannot be encoded or saved.
pub fn backup_discarded_list(root: &Path, source: &StoredList) -> Result<(), String> {
    let name = source.path.file_name().ok_or_else(|| {
        "Could not back up unsaved changes: the original filename is missing.".to_owned()
    })?;
    let bytes = serde_json::to_vec_pretty(&source.data)
        .map_err(|error| format!("Could not encode unsaved changes for backup: {error}"))?;
    backup_conflicted_bytes(root, name, &bytes)
        .map_err(|error| format!("Could not back up unsaved changes: {error}"))
}

/// Saves list contents under their original filename in a unique local backup directory.
fn backup_conflicted_bytes(root: &Path, name: &OsStr, bytes: &[u8]) -> std::io::Result<()> {
    let conflicts = backups_dir(root).join("conflicted");
    fs::create_dir_all(&conflicts)?;
    let directory = conflicts.join(Uuid::new_v4().to_string());
    fs::create_dir(&directory)?;
    let write = (|| {
        let mut file = File::create_new(directory.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if write.is_err() {
        let _ = fs::remove_dir_all(directory);
    }
    write
}

/// Saves local contents as a separate list before reloading an external edit.
///
/// # Arguments
/// - `root` - Destination workspace.
/// - `source` - Local list contents to preserve.
///
/// # Returns
/// A new list with independent identities and the original list if it is still readable.
/// # Errors
/// Returns an error when the existing lists folder cannot be read or the copy cannot be saved.
pub fn create_local_copy(
    root: &Path,
    source: &StoredList,
) -> Result<(StoredList, Option<StoredList>), String> {
    // The original may have been deleted, but an absent lists folder may be offline.
    let original = load_lists(root, false)?
        .lists
        .into_iter()
        .find(|list| list.key == source.key);
    let copy = create_independent_copy(root, source, "local copy")?;
    Ok((copy, original))
}

fn create_independent_copy(
    root: &Path,
    source: &StoredList,
    label: &str,
) -> Result<StoredList, String> {
    let mut data = source.data.clone();
    data.id = Uuid::new_v4();
    data.title = format!("{} ({label})", data.title);
    for task in data
        .tasks
        .iter_mut()
        .chain(data.archive.iter_mut().map(|entry| &mut entry.task))
    {
        task.id = Uuid::new_v4();
        for child in &mut task.subtasks {
            child.id = Uuid::new_v4();
        }
    }
    save_new_list(root, data)
}

/// Summarizes task changes and list-level differences before a recovery copy receives new IDs.
///
/// # Arguments
/// - `original` - The canonical list as read from disk.
/// - `copy` - The incoming or unsaved list with its original identities.
///
/// # Returns
/// A short description of the differences, with an example task per category.
pub fn recovery_summary(original: &TodoList, copy: &TodoList) -> String {
    let original_tasks: HashMap<_, _> = task_versions(original)
        .map(|entry| (entry.0.id, entry))
        .collect();
    let copy_tasks: HashMap<_, _> = task_versions(copy)
        .map(|entry| (entry.0.id, entry))
        .collect();
    let mut changes = [
        summarize_tasks(
            "Only in copy",
            task_versions(copy)
                .map(|(task, _)| task)
                .filter(|task| !original_tasks.contains_key(&task.id)),
        ),
        summarize_tasks(
            "Changed",
            task_versions(copy).filter_map(|entry| {
                original_tasks
                    .get(&entry.0.id)
                    .is_some_and(|previous| *previous != entry)
                    .then_some(entry.0)
            }),
        ),
        summarize_tasks(
            "Only in original",
            task_versions(original)
                .map(|(task, _)| task)
                .filter(|task| !copy_tasks.contains_key(&task.id)),
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let common_order = |list: &TodoList, other: &HashMap<Uuid, (&Task, Option<&ArchivedTask>)>| {
        list.tasks
            .iter()
            .map(|task| task.id)
            .filter(|id| {
                other
                    .get(id)
                    .is_some_and(|(_, archived)| archived.is_none())
            })
            .collect::<Vec<_>>()
    };
    if original.title != copy.title
        || original.accent != copy.accent
        || original.extra != copy.extra
        || common_order(original, &copy_tasks) != common_order(copy, &original_tasks)
    {
        changes.push("List details or Manual order differ".into());
    }
    if changes.is_empty() {
        return "No differences in task contents, list details, or Manual order.".into();
    }
    changes.join(". ") + "."
}

fn task_versions(list: &TodoList) -> impl Iterator<Item = (&Task, Option<&ArchivedTask>)> {
    list.tasks
        .iter()
        .map(|task| (task, None))
        .chain(list.archive.iter().map(|entry| (&entry.task, Some(entry))))
}

fn summarize_tasks<'a>(label: &str, mut tasks: impl Iterator<Item = &'a Task>) -> Option<String> {
    let first = tasks.next()?;
    let mut characters = first.text.trim().chars();
    let mut title: String = characters.by_ref().take(32).collect();
    if characters.next().is_some() {
        title.push_str("...");
    }
    Some(format!("{label} ({}) · {title}", 1 + tasks.count()))
}
