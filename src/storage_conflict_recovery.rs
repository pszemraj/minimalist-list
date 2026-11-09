//! Recovery of independently synced list files that share an identity.

use super::{
    StoredList, backups_dir, list_changed_on_disk, load_existing_workspace, load_lists, read_list,
    save_new_list,
};
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
/// The saved independent list after the duplicate leaves the workspace.
///
/// # Errors
/// Returns an error if the path is no longer a duplicate, the backup or copy
/// cannot be saved, or the duplicate cannot be removed without replacing newer
/// contents. Any copy edited during recovery stays in place.
pub fn recover_duplicate(root: &Path, path: &Path) -> Result<StoredList, String> {
    import_duplicate(root, path)
        .map_err(|error| format!("Could not recover {}: {error}", path.display()))
}

fn import_duplicate(root: &Path, path: &Path) -> Result<StoredList, String> {
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
    if !snapshot.lists.iter().any(|list| list.key == source.key) || list_changed_on_disk(&source)? {
        return Err("the file changed during recovery".into());
    }
    let original = source.source_bytes.borrow();
    let Some(bytes) = original.as_deref() else {
        return Err("the original file contents are unavailable".into());
    };
    keep_original(root, name, bytes)
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
    Ok(copy)
}

/// Saves the observed contents under the original filename before changing the workspace.
fn keep_original(root: &Path, name: &OsStr, bytes: &[u8]) -> std::io::Result<()> {
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
