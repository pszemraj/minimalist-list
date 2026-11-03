//! Local list snapshots and recoverable list deletion, kept beside the settings file.

use super::{StoredList, read_list, settings_path};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Minimum age of the newest snapshot before another one is written.
const SNAPSHOT_INTERVAL_SECS: u64 = 3600;
/// Snapshots kept for each list.
const SNAPSHOT_LIMIT: usize = 100;

/// Derives a stable local key from a normalized workspace path.
///
/// A fixed FNV-1a hash keeps names stable across Rust/toolchain updates.
///
/// # Arguments
///
/// - `workspace` - Workspace path, normally canonical.
///
/// # Returns
///
/// Sixteen lowercase hexadecimal digits.
pub fn workspace_key(workspace: &Path) -> String {
    let key = workspace
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("{key:016x}")
}

/// Returns the local folder that holds one workspace's snapshots and deleted lists.
///
/// The folder sits beside the settings file, never inside the synced workspace.
///
/// # Arguments
///
/// - `workspace` - Workspace whose backups are wanted.
///
/// # Returns
///
/// The `backups/<workspace key>` folder, which may not exist yet.
pub fn backups_dir(workspace: &Path) -> PathBuf {
    let workspace = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_owned());
    settings_path()
        .with_file_name("backups")
        .join(workspace_key(&workspace))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Keeps the bytes a save is about to replace, at most once an hour per list.
///
/// # Arguments
///
/// - `workspace` - Workspace that owns the list.
/// - `list` - List about to be saved; its observed file contents are copied.
///
/// # Returns
///
/// `Ok(())` after a snapshot is written or skipped as unnecessary.
///
/// # Errors
///
/// Returns an error when the backup folder cannot be written.
pub fn snapshot_list(workspace: &Path, list: &StoredList) -> Result<(), String> {
    let Some(bytes) = list.source_bytes.borrow().clone() else {
        return Ok(());
    };
    let directory = backups_dir(workspace)
        .join("snapshots")
        .join(list.key.to_string());
    snapshot_into(&directory, &bytes, unix_now())
        .map_err(|error| format!("could not back up {}: {error}", list.path.display()))
}

/// Lists the numeric `<unix seconds>.json` snapshot names in a folder.
fn snapshot_times(directory: &Path) -> std::io::Result<Vec<u64>> {
    let mut times = Vec::new();
    for entry in fs::read_dir(directory)?.flatten() {
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
            continue;
        };
        if !stem.is_empty()
            && stem.bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(time) = stem.parse()
            && entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            times.push(time);
        }
    }
    times.sort_unstable();
    Ok(times)
}

/// Writes one timestamped snapshot unless the newest is under an hour old, then
/// removes the oldest numeric snapshots beyond the limit. Other files are untouched.
///
/// # Arguments
///
/// - `directory` - Per-list snapshot folder, created when absent.
/// - `bytes` - Previous list file contents.
/// - `now` - Current Unix seconds, which also names the snapshot.
///
/// # Returns
///
/// `Ok(())` after a snapshot is written or skipped as too recent.
///
/// # Errors
///
/// Returns an error when the folder cannot be read or the snapshot cannot be written.
pub(super) fn snapshot_into(directory: &Path, bytes: &[u8], now: u64) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let existing = snapshot_times(directory)?;
    // A newest snapshot dated in the future must not suppress backups.
    if existing
        .last()
        .is_some_and(|&newest| newest <= now && now - newest < SNAPSHOT_INTERVAL_SECS)
    {
        return Ok(());
    }
    let temporary = directory.join(format!(".{now}.json.tmp"));
    fs::write(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, directory.join(format!("{now}.json"))) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Ok(times) = snapshot_times(directory) {
        for time in times
            .iter()
            .take(times.len().saturating_sub(SNAPSHOT_LIMIT))
        {
            let _ = fs::remove_file(directory.join(format!("{time}.json")));
        }
    }
    Ok(())
}

/// Removes a list from its workspace, keeping its file in local backups.
///
/// # Arguments
///
/// - `workspace` - Workspace that owns the list.
/// - `list` - List whose file is moved to `backups/<key>/deleted/<unix>-<filename>`.
///
/// # Returns
///
/// `Ok(())` after the file has left the workspace.
///
/// # Errors
///
/// Returns an error when another readable file has the same identity or the file
/// cannot be kept in backups and removed. The list file then stays in place.
pub fn delete_list(workspace: &Path, list: &StoredList) -> Result<(), String> {
    if let Some(directory) = list.path.parent() {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path != list.path
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && let Ok((other, _)) = read_list(&path)
                && other.key == list.key
            {
                return Err(format!(
                    "could not delete list: duplicate list identity in {}; resolve the duplicate files first",
                    path.display()
                ));
            }
        }
    }
    let fail = |error: std::io::Error| {
        format!(
            "could not delete {}: {error}; it was left in place",
            list.path.display()
        )
    };
    let name = list.path.file_name().ok_or_else(|| {
        format!(
            "could not delete {}: it has no file name",
            list.path.display()
        )
    })?;
    let directory = backups_dir(workspace).join("deleted");
    fs::create_dir_all(&directory).map_err(fail)?;
    // Never replace an earlier deleted copy that shares the same second and name.
    let mut stamp = unix_now();
    let kept = loop {
        let mut file = OsString::from(format!("{stamp}-"));
        file.push(name);
        let candidate = directory.join(file);
        if fs::symlink_metadata(&candidate).is_err() {
            break candidate;
        }
        stamp += 1;
    };
    if fs::rename(&list.path, &kept).is_ok() {
        return Ok(());
    }
    // Renaming fails across volumes; copy first so the list is never lost.
    let moved = fs::copy(&list.path, &kept).and_then(|_| fs::remove_file(&list.path));
    if let Err(error) = moved {
        let _ = fs::remove_file(&kept);
        return Err(fail(error));
    }
    Ok(())
}
