//! Atomic file replacement and cleanup of abandoned sibling files.

use super::{StoredList, stamp_from_metadata};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    time::{Duration, SystemTime},
};
use uuid::Uuid;

/// Serializes JSON and atomically replaces its destination.
///
/// # Arguments
/// - `path` - Destination file.
/// - `value` - JSON-compatible contents.
///
/// # Returns
/// Success after the flushed temporary file replaces the destination.
///
/// # Errors
/// Returns an error when directory creation, serialization, writing, or replacement fails.
pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_json_atomic_checked(path, value, || Ok(())).map(|_| ())
}

/// Writes JSON after checking the destination before staging and before replacement.
///
/// # Arguments
/// - `path` - Destination file.
/// - `value` - JSON-compatible contents.
/// - `before_replace` - Guard evaluated before staging and again immediately before replacement.
///
/// # Returns
/// Metadata and bytes belonging to the flushed temporary file that was installed.
///
/// # Errors
/// Returns an error if the guard rejects the write or persistence fails.
pub(super) fn write_json_atomic_checked<T: Serialize>(
    path: &Path,
    value: &T,
    before_replace: impl Fn() -> Result<(), String>,
) -> Result<(fs::Metadata, Vec<u8>), String> {
    before_replace()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data.json");
    let temp = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    let write_result = (|| {
        let mut file = File::create(&temp)
            .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
        file.lock()
            .map_err(|error| format!("could not lock {}: {error}", temp.display()))?;
        file.write_all(&bytes)
            .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not flush {}: {error}", temp.display()))?;
        let saved_metadata = file
            .metadata()
            .map_err(|error| format!("could not inspect {}: {error}", temp.display()))?;
        before_replace()?;
        fs::rename(&temp, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
        Ok((saved_metadata, bytes))
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

/// Removes only recognized, unlocked temporary files older than a day.
/// The age threshold also protects a recent write arriving from another device.
pub(super) fn cleanup_stale_temps(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str().and_then(|name| name.strip_prefix('.')) else {
            continue;
        };
        let Some((_, suffix)) = name.rsplit_once(".json.tmp-") else {
            continue;
        };
        let Some((pid, id)) = suffix.split_once('-') else {
            continue;
        };
        if pid.parse::<u32>().is_err() || Uuid::parse_str(id).is_err() {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file()
            || metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_none_or(|age| age < Duration::from_secs(24 * 60 * 60))
        {
            continue;
        }
        if let Ok(file) = File::options().read(true).write(true).open(&path)
            && file.try_lock().is_ok()
        {
            let _ = fs::remove_file(path);
        }
    }
}

/// Persists a list atomically to its associated path.
///
/// # Returns
///
/// `Ok(())` after the list file is replaced.
///
/// # Errors
///
/// Returns an error when the list changed on disk since loading or saving it,
/// or when it cannot be serialized, written, or replaced.
pub fn save_list(list: &StoredList) -> Result<(), String> {
    let (metadata, bytes) = write_json_atomic_checked(&list.path, &list.data, || {
        if list_changed_on_disk(list)? {
            return Err(format!(
                "{} changed on disk. Its newer contents were kept; your changes remain unsaved.",
                list.path.display()
            ));
        }
        Ok(())
    })?;
    list.stamp.set(Some(stamp_from_metadata(&metadata)));
    *list.source_bytes.borrow_mut() = Some(bytes.into());
    Ok(())
}

/// Compares the file with its last loaded or saved bytes on a metadata mismatch.
///
/// # Returns
/// Whether its contents differ from the observed version, or its presence changed.
///
/// # Errors
/// Returns an error when the file metadata or required contents cannot be read.
pub fn list_changed_on_disk(list: &StoredList) -> Result<bool, String> {
    let current = match fs::metadata(&list.path) {
        Ok(metadata) => Some(stamp_from_metadata(&metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "could not inspect {}: {error}",
                list.path.display()
            ));
        }
    };
    if current == list.stamp.get() {
        return Ok(false);
    }
    if current.is_some()
        && list.stamp.get().is_some()
        && let Some(original) = list.source_bytes.borrow().as_ref()
    {
        let bytes = fs::read(&list.path)
            .map_err(|error| format!("could not read {}: {error}", list.path.display()))?;
        return Ok(bytes.as_slice() != original.as_ref());
    }
    Ok(true)
}
