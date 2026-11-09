//! Shared-list format compatibility and identity parsing independent of task contents.

use crate::model::FORMAT_VERSION;
use serde::{Deserialize, de::IgnoredAny};
use std::{collections::BTreeMap, fs, path::Path};
use uuid::Uuid;

const NUMBER_MARKER: &str = "$serde_json::private::Number";

#[derive(Deserialize)]
struct ListFormat {
    format_version: u32,
}

#[derive(Deserialize)]
struct EmbeddedIdentity {
    id: Uuid,
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

/// Checks the format header and rejects metadata that would be rewritten ambiguously.
///
/// # Arguments
/// - `bytes` - Complete JSON file contents.
/// - `path` - File named in an error message.
///
/// # Returns
/// `Ok(())` when the header declares a supported format.
///
/// # Errors
/// Returns a path-specific error when the format or metadata cannot round-trip safely.
pub(super) fn check_list_format(bytes: &[u8], path: &Path) -> Result<(), String> {
    let header: ListFormat = serde_json::from_slice(bytes)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?;
    check_version(header.format_version)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?;
    if contains_number_marker(bytes)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?
    {
        return Err(format!(
            "ignored {}: reserved JSON key {NUMBER_MARKER} cannot be saved safely",
            path.display()
        ));
    }
    Ok(())
}

fn contains_number_marker(bytes: &[u8]) -> serde_json::Result<bool> {
    // A literal or escaped dollar is required for the key.
    if !bytes.contains(&b'$') && !bytes.windows(6).any(|bytes| bytes == b"\\u0024") {
        return Ok(false);
    }
    // The header read already checked JSON structure. Decode each string so a
    // marker in a key is distinguished from one in a value or escaped text.
    let mut remaining = bytes;
    while let Some(quote) = remaining.iter().position(|&byte| byte == b'"') {
        let mut strings =
            serde_json::Deserializer::from_slice(&remaining[quote..]).into_iter::<String>();
        let Some(string) = strings.next().transpose()? else {
            break;
        };
        remaining = &remaining[quote + strings.byte_offset()..];
        if string == NUMBER_MARKER && remaining.trim_ascii_start().starts_with(b":") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Checks the current file's format and identity before a cached list can rename or remove it.
///
/// # Arguments
/// - `path` - Current list file.
/// - `expected` - Identity of the cached list.
///
/// # Returns
/// `Ok(())` when the file declares a supported format and belongs to this list.
///
/// # Errors
/// Returns an error for an unreadable file, unsupported format, or changed identity.
pub(super) fn check_file_for_mutation(path: &Path, expected: Uuid) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    check_list_format(&bytes, path)?;
    if identity_from_bytes(&bytes, path)? != expected {
        return Err(format!(
            "{} belongs to a different list. Reload before changing it.",
            path.display()
        ));
    }
    Ok(())
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
    identity_from_bytes(&bytes, path)
}

fn identity_from_bytes(bytes: &[u8], path: &Path) -> Result<Uuid, String> {
    if !bytes.trim_ascii_start().starts_with(b"{") {
        return Err(format!("{} has no list identity", path.display()));
    }
    if let Some((id, _)) = filename_identity(path) {
        serde_json::from_slice::<BTreeMap<String, IgnoredAny>>(bytes)
            .map_err(|error| error.to_string())?;
        return Ok(id);
    }
    serde_json::from_slice::<EmbeddedIdentity>(bytes)
        .map(|identity| identity.id)
        .map_err(|error| {
            format!(
                "could not read list identity in {}: {error}",
                path.display()
            )
        })
}

#[cfg(test)]
#[path = "storage_format_tests.rs"]
mod tests;
