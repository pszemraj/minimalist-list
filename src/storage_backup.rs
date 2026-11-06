//! Local list snapshots and recoverable list deletion, kept beside the settings file.

use super::{StoredList, format, list_file_name, settings_path};
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
    snapshot_list_with(workspace, list, false)
}

/// Keeps the list's observed file contents even when a recent snapshot exists.
///
/// Used before an action the app cannot undo, such as clearing History.
///
/// # Arguments
///
/// - `workspace` - Workspace that owns the list.
/// - `list` - List whose last loaded or saved file contents are copied.
///
/// # Returns
///
/// `Ok(())` after a snapshot is written, or when the list has no observed file contents.
///
/// # Errors
///
/// Returns an error when the backup folder cannot be written.
pub fn snapshot_list_now(workspace: &Path, list: &StoredList) -> Result<(), String> {
    snapshot_list_with(workspace, list, true)
}

fn snapshot_list_with(workspace: &Path, list: &StoredList, force: bool) -> Result<(), String> {
    let Some(bytes) = list.source_bytes.borrow().clone() else {
        return Ok(());
    };
    let directory = backups_dir(workspace)
        .join("snapshots")
        .join(list.key.to_string());
    snapshot_into(&directory, &bytes, unix_now(), force)
        .map_err(|error| format!("could not back up {}: {error}", list.path.display()))
}

struct Snapshot {
    time: u64,
    sequence: u64,
    path: PathBuf,
}

/// Lists timestamped snapshots, including earlier backups with no sequence suffix.
fn snapshot_files(directory: &Path) -> std::io::Result<Vec<Snapshot>> {
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(directory)?.flatten() {
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
            continue;
        };
        let (time, sequence) = stem.split_once('-').unwrap_or((stem, "0"));
        if time.bytes().all(|byte| byte.is_ascii_digit())
            && sequence.bytes().all(|byte| byte.is_ascii_digit())
            && let (Ok(time), Ok(sequence)) = (time.parse(), sequence.parse())
            && entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            snapshots.push(Snapshot {
                time,
                sequence,
                path: entry.path(),
            });
        }
    }
    Ok(snapshots)
}

/// Writes one timestamped snapshot unless the newest is under an hour old and
/// `force` is false, then prunes excess snapshots in creation order.
/// Other files are untouched, and an existing snapshot is never replaced.
///
/// # Arguments
///
/// - `directory` - Per-list snapshot folder, created when absent.
/// - `bytes` - Previous list file contents.
/// - `now` - Current Unix seconds used in the snapshot name.
/// - `force` - Whether to write even when the newest snapshot is under an hour old.
///
/// # Returns
///
/// `Ok(())` after a snapshot is written or skipped as too recent.
///
/// # Errors
///
/// Returns an error when the folder cannot be read or the snapshot cannot be written.
pub(super) fn snapshot_into(
    directory: &Path,
    bytes: &[u8],
    now: u64,
    force: bool,
) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let existing = snapshot_files(directory)?;
    let newest = existing
        .iter()
        .max_by_key(|snapshot| (snapshot.sequence, snapshot.time));
    // A newest snapshot dated in the future must not suppress backups.
    if !force
        && newest.is_some_and(|snapshot| {
            snapshot.time <= now && now - snapshot.time < SNAPSHOT_INTERVAL_SECS
        })
    {
        return Ok(());
    }
    // Sequence numbers preserve creation order across a wall-clock correction.
    let next_sequence = |sequence: u64| {
        sequence
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("snapshot sequence exhausted"))
    };
    let mut sequence = next_sequence(newest.map_or(0, |snapshot| snapshot.sequence))?;
    let (destination, temporary) = loop {
        let name = format!("{now}-{sequence}.json");
        let destination = directory.join(&name);
        if fs::symlink_metadata(&destination).is_err() {
            break (destination, directory.join(format!(".{name}.tmp")));
        }
        sequence = next_sequence(sequence)?;
    };
    fs::write(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Ok(mut snapshots) = snapshot_files(directory) {
        snapshots.sort_unstable_by_key(|snapshot| (snapshot.sequence, snapshot.time));
        for snapshot in snapshots
            .iter()
            .take(snapshots.len().saturating_sub(SNAPSHOT_LIMIT))
        {
            let _ = fs::remove_file(&snapshot.path);
        }
    }
    Ok(())
}

/// Removes a list from its workspace, keeping its file in local backups.
///
/// # Arguments
///
/// - `workspace` - Workspace that owns the list.
/// - `list` - List whose contents are kept in `backups/<key>/deleted/<unix>-<filename>`.
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
    format::check_version(list.data.format_version)?;
    if let Some(directory) = list.path.parent() {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path != list.path
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && format::read_identity(&path).is_ok_and(|id| id == list.key)
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
    // A timestamp prefix can change how the workspace reader interprets a source name.
    let name = list_file_name(&list.data.title, list.key);
    let source_type = fs::symlink_metadata(&list.path).map_err(fail)?.file_type();
    if !source_type.is_file() && !source_type.is_symlink() {
        return Err(fail(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the list path is no longer a file",
        )));
    }
    format::check_file_format(&list.path)?;
    let directory = backups_dir(workspace).join("deleted");
    fs::create_dir_all(&directory).map_err(fail)?;
    // Never replace an earlier deleted copy that shares the same second and name.
    let mut stamp = unix_now();
    let kept = loop {
        let candidate = directory.join(format!("{stamp}-{name}"));
        if fs::symlink_metadata(&candidate).is_err() {
            break candidate;
        }
        stamp += 1;
    };
    if source_type.is_file() && fs::rename(&list.path, &kept).is_ok() {
        return Ok(());
    }
    // Copy the target of a symlink, or copy across volumes, before removing the workspace entry.
    let moved = fs::copy(&list.path, &kept).and_then(|_| fs::remove_file(&list.path));
    if let Err(error) = moved {
        let _ = fs::remove_file(&kept);
        return Err(fail(error));
    }
    Ok(())
}
