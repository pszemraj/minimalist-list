//! Shared-list format compatibility and identity parsing independent of task contents.

use crate::model::FORMAT_VERSION;
use serde::Deserialize;
use std::{fs, path::Path};
use uuid::Uuid;

#[derive(Deserialize)]
struct ListFormat {
    format_version: u32,
}

/// Rejects formats whose structure or behavior this application does not support.
///
/// # Returns
/// `Ok(())` for the supported list format.
///
/// # Errors
/// Returns an actionable error for any version other than the current list format.
pub(super) fn check_version(version: u32) -> Result<(), String> {
    if version == FORMAT_VERSION {
        Ok(())
    } else {
        Err(format!(
            "unsupported list format {version}; this app supports {FORMAT_VERSION}. Update the app before editing this file."
        ))
    }
}

/// Checks the format header before attempting to interpret version-specific contents.
///
/// # Arguments
/// - `bytes` - Complete JSON file contents.
/// - `path` - File named in an error message.
///
/// # Returns
/// `Ok(())` when the header declares a supported format.
///
/// # Errors
/// Returns a path-specific error when the header is malformed or unsupported.
pub(super) fn check_list_format(bytes: &[u8], path: &Path) -> Result<(), String> {
    let header: ListFormat = serde_json::from_slice(bytes)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?;
    check_version(header.format_version)
        .map_err(|error| format!("ignored {}: {error}", path.display()))
}

/// Checks the current file before renaming or removing a potentially stale cached list.
///
/// # Returns
/// `Ok(())` when the file declares a supported format.
///
/// # Errors
/// Returns an error when the file cannot be read or has an unsupported format.
pub(super) fn check_file_format(path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    check_list_format(&bytes, path)
}

/// Returns an authoritative filename UUID and its optional descriptive title prefix.
///
/// # Returns
/// The list UUID and optional title prefix, or `None` for an unrecognized filename.
pub(super) fn filename_identity(path: &Path) -> Option<(Uuid, Option<&str>)> {
    let stem = path.file_stem()?.to_str()?;
    if let Ok(id) = Uuid::parse_str(stem) {
        return Some((id, None));
    }
    let (name, id) = stem.rsplit_once("--")?;
    if name.is_empty() {
        return None;
    }
    Some((Uuid::parse_str(id).ok()?, Some(name)))
}

/// Reads identity without interpreting task fields from a potentially newer format.
///
/// # Returns
/// The filename UUID when recognized, otherwise the JSON object's `id`.
///
/// # Errors
/// Returns an error when the file is unreadable, is not a JSON object, or has no list identity.
pub(super) fn read_identity(path: &Path) -> Result<Uuid, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    filename_identity(path)
        .map(|(id, _)| id)
        .or_else(|| fields.get("id")?.as_str()?.parse().ok())
        .ok_or_else(|| format!("{} has no list identity", path.display()))
}

#[cfg(test)]
#[path = "storage_format_tests.rs"]
mod tests;
