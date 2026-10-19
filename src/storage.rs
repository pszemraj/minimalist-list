//! Settings and folder-backed list persistence.

use crate::model::{Accent, FontChoice, ListSort, TaskSort, TitleOverflow, TodoList};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

const APP_DIR: &str = "minimalist-list";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
/// User preferences persisted outside the task workspace.
pub struct Settings {
    pub workspace_path: String,
    pub always_on_top: bool,
    pub window_decorations: bool,
    pub font: FontChoice,
    pub font_size: f32,
    pub bold_text: bool,
    pub row_padding: f32,
    pub show_subtasks: bool,
    pub title_overflow: TitleOverflow,
    pub background_opacity: f32,
    pub last_list_id: Option<Uuid>,
    pub last_capture_list_id: Option<Uuid>,
    #[serde(default)]
    pub list_order: BTreeMap<String, Vec<Uuid>>,
    pub list_sort: BTreeMap<String, ListSort>,
    pub task_sort: BTreeMap<String, TaskSort>,
    pub manual_task_order: BTreeMap<String, Vec<Uuid>>,
    pub global_shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace_path: default_workspace_dir().to_string_lossy().into_owned(),
            always_on_top: false,
            window_decorations: true,
            font: FontChoice::Sans,
            font_size: 19.0,
            bold_text: false,
            row_padding: 6.0,
            show_subtasks: false,
            title_overflow: TitleOverflow::Scroll,
            background_opacity: 0.85,
            last_list_id: None,
            last_capture_list_id: None,
            list_order: BTreeMap::new(),
            list_sort: BTreeMap::new(),
            task_sort: BTreeMap::new(),
            manual_task_order: BTreeMap::new(),
            global_shortcut: "CommandOrControl+Alt+Shift+KeyL".into(),
        }
    }
}

#[derive(Clone, Debug)]
/// A list paired with the file that stores it.
pub struct StoredList {
    pub key: Uuid,
    pub path: PathBuf,
    pub data: TodoList,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// File metadata used to detect workspace changes between scans.
pub struct FileStamp {
    pub modified_ns: u128,
    pub len: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Metadata snapshot keyed by each list file name.
pub struct WorkspaceFingerprint(pub BTreeMap<String, FileStamp>);

/// Lists, change metadata, and non-fatal issues found while loading a workspace.
pub struct WorkspaceSnapshot {
    pub lists: Vec<StoredList>,
    pub fingerprint: WorkspaceFingerprint,
    pub warnings: Vec<String>,
    pub failed_paths: Vec<PathBuf>,
}

/// Loads persisted settings, or defaults when no settings file exists.
///
/// # Returns
///
/// Persisted settings, or defaults when the settings file is absent.
///
/// # Errors
///
/// Returns an error when the settings file cannot be read or parsed.
pub fn load_settings() -> Result<Settings, String> {
    load_settings_from(&settings_path())
}

fn load_settings_from(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

/// Persists application settings atomically.
///
/// # Returns
///
/// `Ok(())` after the settings file is replaced.
///
/// # Errors
///
/// Returns an error when settings cannot be serialized, written, or replaced.
#[cfg(test)]
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    write_json_atomic(&settings_path(), settings)
}

/// Saves only preferences changed since this instance last loaded or saved them.
/// Malformed stored settings are replaced with the current preferences.
///
/// # Arguments
/// - `desired` - Current preferences, including local edits.
/// - `baseline` - Preferences last observed by this instance.
///
/// # Returns
/// The merged preferences saved to disk.
///
/// # Errors
/// Returns an error if existing preferences cannot be read or the result cannot be saved.
pub fn save_settings_changes(desired: &Settings, baseline: &Settings) -> Result<Settings, String> {
    save_settings_changes_at(&settings_path(), desired, baseline)
}

fn save_settings_changes_at(
    path: &Path,
    desired: &Settings,
    baseline: &Settings,
) -> Result<Settings, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    // Lock a stable sibling: replacing settings.json also replaces its file identity.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|error| format!("could not open preferences lock: {error}"))?;
    lock.lock()
        .map_err(|error| format!("could not lock preferences: {error}"))?;
    if !path.exists() {
        write_json_atomic(path, desired)?;
        return Ok(desired.clone());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let current: Settings = match serde_json::from_slice(&bytes) {
        Ok(settings) => settings,
        Err(_) => {
            write_json_atomic(path, desired)?;
            return Ok(desired.clone());
        }
    };
    let mut current = serde_json::to_value(current).map_err(|error| error.to_string())?;
    let current_map = current
        .as_object_mut()
        .ok_or("invalid stored preferences")?;
    let baseline = serde_json::to_value(baseline).map_err(|error| error.to_string())?;
    let desired = serde_json::to_value(desired).map_err(|error| error.to_string())?;
    for (key, value) in desired.as_object().ok_or("invalid preferences")? {
        if matches!(
            key.as_str(),
            "list_order" | "list_sort" | "task_sort" | "manual_task_order"
        ) {
            let current_order = current_map
                .entry(key.clone())
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .ok_or("invalid stored list order")?;
            let old_order = baseline.get(key).and_then(serde_json::Value::as_object);
            let new_order = value.as_object().ok_or("invalid list order")?;
            for (workspace, order) in new_order {
                if old_order.and_then(|old| old.get(workspace)) != Some(order) {
                    current_order.insert(workspace.clone(), order.clone());
                }
            }
            if let Some(old_order) = old_order {
                for workspace in old_order
                    .keys()
                    .filter(|path| !new_order.contains_key(*path))
                {
                    current_order.remove(workspace);
                }
            }
            continue;
        }
        if baseline.get(key) != Some(value) {
            current_map.insert(key.clone(), value.clone());
        }
    }
    let merged: Settings = serde_json::from_value(current).map_err(|error| error.to_string())?;
    write_json_atomic(path, &merged)?;
    Ok(merged)
}

/// Returns the local file used by eframe for native window geometry.
///
/// # Returns
/// A window state file beside the local settings file.
pub fn window_state_path() -> PathBuf {
    settings_path().with_file_name("window.ron")
}

/// Removes surrounding quotes or Unix shell escapes from a typed workspace path.
///
/// # Returns
///
/// The literal path text to pass to workspace normalization.
pub fn parse_workspace_input(raw: &str) -> String {
    parse_workspace_input_from(raw, cfg!(target_os = "windows"))
}

fn parse_workspace_input_from(raw: &str, windows: bool) -> String {
    let raw = raw.trim();
    for quote in ['\'', '"'] {
        if let Some(path) = raw
            .strip_prefix(quote)
            .and_then(|path| path.strip_suffix(quote))
        {
            return path.to_owned();
        }
    }
    if windows {
        return raw.to_owned();
    }
    // Terminal drops and shell completion escape spaces and punctuation;
    // a backslash before a letter or digit remains part of the name.
    let mut path = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(character) = chars.next() {
        match chars.peek() {
            Some(&next) if character == '\\' && (next == ' ' || next.is_ascii_punctuation()) => {
                path.push(next);
                chars.next();
            }
            _ => path.push(character),
        }
    }
    path
}

/// Expands, creates, and canonicalizes a literal workspace directory.
///
/// # Returns
///
/// The canonical absolute workspace path.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved or its list directory cannot be created.
pub fn normalize_workspace_path(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("workspace path is empty".into());
    }
    let path = expand_tilde(raw.trim());
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(path)
    };
    fs::create_dir_all(absolute.join("lists"))
        .map_err(|error| format!("could not create {}: {error}", absolute.display()))?;
    absolute
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", absolute.display()))
}

/// Loads all readable list files and a fingerprint from a workspace.
///
/// # Returns
///
/// A snapshot containing readable lists, file metadata, and non-fatal warnings.
///
/// # Errors
///
/// Returns an error when the workspace list directory cannot be created or read.
pub fn load_workspace(root: &Path) -> Result<WorkspaceSnapshot, String> {
    let lists_dir = root.join("lists");
    fs::create_dir_all(&lists_dir)
        .map_err(|error| format!("could not create {}: {error}", lists_dir.display()))?;

    let mut paths = fs::read_dir(&lists_dir)
        .map_err(|error| format!("could not read {}: {error}", lists_dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        })
        .collect::<Vec<_>>();
    paths.sort();

    if paths.is_empty() {
        let list = create_list(root, "Tasks", Accent::default())?;
        return Ok(WorkspaceSnapshot {
            lists: vec![list],
            fingerprint: workspace_fingerprint(root)?,
            warnings: Vec::new(),
            failed_paths: Vec::new(),
        });
    }

    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    let mut failed_paths = Vec::new();

    for path in paths {
        match read_list(&path) {
            Ok((stored, canonical_name)) => {
                // A blocked rename saves newer content at its original UUID-bearing path.
                let modified = if canonical_name != 0 {
                    fs::metadata(&path)
                        .map(|metadata| stamp_from_metadata(&metadata).modified_ns)
                        .unwrap_or_default()
                } else {
                    0
                };
                candidates.push((stored, (canonical_name != 0, modified, canonical_name)));
            }
            Err(error) => {
                warnings.push(error);
                failed_paths.push(path);
            }
        }
    }

    candidates.sort_by(|(left, left_priority), (right, right_priority)| {
        right_priority
            .cmp(left_priority)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut seen_ids = HashSet::new();
    let mut lists = Vec::new();
    for (stored, _) in candidates {
        if seen_ids.insert(stored.key) {
            lists.push(stored);
        } else {
            warnings.push(format!(
                "ignored duplicate list identity in {}; the file remains untouched on disk",
                stored.path.display()
            ));
        }
    }
    sort_lists(&mut lists);

    Ok(WorkspaceSnapshot {
        lists,
        fingerprint: workspace_fingerprint(root)?,
        warnings,
        failed_paths,
    })
}

fn read_list(path: &Path) -> Result<(StoredList, u8), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut data = serde_json::from_slice::<TodoList>(&bytes)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?;

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let legacy_id = Uuid::parse_str(stem).ok();
    let descriptive = stem.rsplit_once("--").and_then(|(name, id)| {
        (!name.is_empty())
            .then(|| Uuid::parse_str(id).ok().map(|id| (name, id)))
            .flatten()
    });
    let file_id = legacy_id.or_else(|| descriptive.map(|(_, id)| id));
    let canonical_name = match descriptive {
        Some((name, _)) if name == filename_title(&data.title) => 2,
        _ if file_id.is_some() => 1,
        _ => 0,
    };
    let key = file_id.unwrap_or(data.id);
    data.id = key;

    Ok((
        StoredList {
            key,
            path: path.to_owned(),
            data,
        },
        canonical_name,
    ))
}

/// Orders lists by creation time, using identity to stabilize same-second ties.
pub fn sort_lists(lists: &mut [StoredList]) {
    lists.sort_by_key(|entry| (entry.data.created_at_unix, entry.key));
}

/// Creates and persists a new empty list in a workspace.
///
/// # Arguments
///
/// - `root` - Workspace root containing the `lists` directory.
/// - `title` - User-facing title for the new list.
/// - `accent` - Initial list accent.
///
/// # Returns
///
/// The new list paired with its storage path.
///
/// # Errors
///
/// Returns an error when the new list cannot be serialized, written, or installed.
pub fn create_list(root: &Path, title: &str, accent: Accent) -> Result<StoredList, String> {
    let data = TodoList::new(title, accent);
    let path = root.join("lists").join(list_file_name(title, data.id));
    let stored = StoredList {
        key: data.id,
        path,
        data,
    };
    save_list(&stored)?;
    Ok(stored)
}

/// Persists a list atomically to its associated path.
///
/// # Returns
///
/// `Ok(())` after the list file is replaced.
///
/// # Errors
///
/// Returns an error when the list cannot be serialized, written, or replaced.
pub fn save_list(list: &StoredList) -> Result<(), String> {
    write_json_atomic(&list.path, &list.data)
}

fn filename_title(title: &str) -> String {
    let mut clean = String::new();
    let mut separator = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            let extra = usize::from(separator && !clean.is_empty());
            if clean.len() + extra + character.len_utf8() > 96 {
                break;
            }
            if extra > 0 {
                clean.push('-');
            }
            clean.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    if clean.is_empty() {
        "list".into()
    } else {
        clean
    }
}

fn list_file_name(title: &str, id: Uuid) -> String {
    format!("{}--{id}.json", filename_title(title))
}

/// Updates a list's filename to its clean current title and stable identity.
///
/// # Arguments
/// - `list` - A saved list whose title was explicitly changed by the user.
///
/// # Returns
/// Success after renaming, or immediately if its filename is already current.
///
/// # Errors
/// Returns an error if the destination already exists or the original cannot be renamed.
pub fn rename_list_file(list: &mut StoredList) -> Result<(), String> {
    let destination = list
        .path
        .with_file_name(list_file_name(&list.data.title, list.key));
    if destination == list.path {
        return Ok(());
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(format!(
                "could not rename list: {} already exists",
                destination.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "could not inspect {}: {error}",
                destination.display()
            ));
        }
    }
    fs::rename(&list.path, &destination).map_err(|error| {
        format!(
            "could not rename {} to {}: {error}",
            list.path.display(),
            destination.display()
        )
    })?;
    list.path = destination;
    Ok(())
}

/// Deletes a list's associated file.
///
/// # Returns
///
/// `Ok(())` after the file is removed.
///
/// # Errors
///
/// Returns an error when the list file cannot be removed.
pub fn delete_list(list: &StoredList) -> Result<(), String> {
    fs::remove_file(&list.path)
        .map_err(|error| format!("could not delete {}: {error}", list.path.display()))
}

/// Collects change-detection metadata for every list file in a workspace.
///
/// # Returns
///
/// A fingerprint keyed by list file name.
///
/// # Errors
///
/// Returns an error when the list directory cannot be created or read.
pub fn workspace_fingerprint(root: &Path) -> Result<WorkspaceFingerprint, String> {
    let mut map = BTreeMap::new();
    let lists_dir = root.join("lists");
    fs::create_dir_all(&lists_dir)
        .map_err(|error| format!("could not create {}: {error}", lists_dir.display()))?;
    for entry in fs::read_dir(&lists_dir)
        .map_err(|error| format!("could not read {}: {error}", lists_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("json")
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        map.insert(
            entry.file_name().to_string_lossy().into_owned(),
            stamp_from_metadata(&metadata),
        );
    }
    Ok(WorkspaceFingerprint(map))
}

/// Refreshes one list file's entry in an existing fingerprint.
///
/// # Arguments
///
/// - `fingerprint` - Snapshot to update in place.
/// - `path` - List file whose metadata should be recorded.
///
/// # Returns
///
/// `Ok(())` after the entry is refreshed.
///
/// # Errors
///
/// Returns an error when the file name or metadata cannot be read.
pub fn update_fingerprint_for_file(
    fingerprint: &mut WorkspaceFingerprint,
    path: &Path,
) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("could not identify {}", path.display()))?;
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    fingerprint
        .0
        .insert(name.to_owned(), stamp_from_metadata(&metadata));
    Ok(())
}

/// Removes one list file's entry from an existing fingerprint.
///
/// # Arguments
///
/// - `fingerprint` - Snapshot to update in place.
/// - `path` - List file whose entry should be removed.
pub fn remove_file_from_fingerprint(fingerprint: &mut WorkspaceFingerprint, path: &Path) {
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        fingerprint.0.remove(name);
    }
}

fn stamp_from_metadata(metadata: &fs::Metadata) -> FileStamp {
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    FileStamp {
        modified_ns,
        len: metadata.len(),
    }
}

/// Returns the platform-appropriate default task workspace directory.
///
/// # Returns
///
/// The application's directory beneath the platform data root, or an existing
/// legacy workspace when the native directory does not exist.
pub fn default_workspace_dir() -> PathBuf {
    default_workspace_dir_from(PathPlatform::current(), &|name| env::var_os(name))
}

#[derive(Clone, Copy)]
enum PathPlatform {
    #[cfg(any(target_os = "windows", test))]
    Windows,
    #[cfg(any(target_os = "macos", test))]
    Macos,
    #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
    Xdg,
}

impl PathPlatform {
    fn current() -> Self {
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "macos")]
        return Self::Macos;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        Self::Xdg
    }
}

fn default_workspace_dir_from(
    platform: PathPlatform,
    lookup: &impl Fn(&str) -> Option<OsString>,
) -> PathBuf {
    if let Some(root) = lookup("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join(APP_DIR);
    }
    let root = match platform {
        #[cfg(any(target_os = "windows", test))]
        PathPlatform::Windows => lookup("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir_from(platform, lookup).join("AppData/Local")),
        #[cfg(any(target_os = "macos", test))]
        PathPlatform::Macos => home_dir_from(platform, lookup).join("Library/Application Support"),
        #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
        PathPlatform::Xdg => xdg_dir_from(lookup, "XDG_DATA_HOME", ".local/share"),
    };
    let preferred = root.join(APP_DIR);
    match platform {
        #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
        PathPlatform::Xdg => preferred,
        #[cfg(any(target_os = "windows", target_os = "macos", test))]
        _ => prefer_existing_path(
            preferred,
            xdg_dir_from(lookup, "XDG_DATA_HOME", ".local/share").join(APP_DIR),
        ),
    }
}

#[cfg(not(test))]
fn settings_path() -> PathBuf {
    settings_path_from(PathPlatform::current(), &|name| env::var_os(name))
}

/// Gives each test thread its own settings file, away from the user's settings.
#[cfg(test)]
fn settings_path() -> PathBuf {
    thread_local! {
        static PATH: PathBuf = env::temp_dir()
            .join(format!("minimalist-list-settings-{}.json", Uuid::new_v4()));
    }
    PATH.with(Clone::clone)
}

fn settings_path_from(
    platform: PathPlatform,
    lookup: &impl Fn(&str) -> Option<OsString>,
) -> PathBuf {
    if let Some(root) = lookup("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join(APP_DIR).join("settings.json");
    }
    let root = match platform {
        #[cfg(any(target_os = "windows", test))]
        PathPlatform::Windows => lookup("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir_from(platform, lookup).join("AppData/Roaming")),
        #[cfg(any(target_os = "macos", test))]
        PathPlatform::Macos => home_dir_from(platform, lookup).join("Library/Application Support"),
        #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
        PathPlatform::Xdg => xdg_dir_from(lookup, "XDG_CONFIG_HOME", ".config"),
    };
    let preferred = root.join(APP_DIR).join("settings.json");
    match platform {
        #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
        PathPlatform::Xdg => preferred,
        #[cfg(any(target_os = "windows", target_os = "macos", test))]
        _ => prefer_existing_path(
            preferred,
            xdg_dir_from(lookup, "XDG_CONFIG_HOME", ".config")
                .join(APP_DIR)
                .join("settings.json"),
        ),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn prefer_existing_path(preferred: PathBuf, legacy: PathBuf) -> PathBuf {
    if !preferred.exists() && legacy.exists() {
        legacy
    } else {
        preferred
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
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
        file.write_all(&bytes)
            .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("could not flush {}: {error}", temp.display()))?;

        fs::rename(&temp, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

fn xdg_dir_from(
    lookup: &impl Fn(&str) -> Option<OsString>,
    variable: &str,
    fallback: &str,
) -> PathBuf {
    // Earlier versions used HOME on every platform, including Windows.
    let home = lookup("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    lookup(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(fallback))
}

fn home_dir() -> PathBuf {
    home_dir_from(PathPlatform::current(), &|name| env::var_os(name))
}

fn home_dir_from(platform: PathPlatform, lookup: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    let home = match platform {
        #[cfg(any(target_os = "windows", test))]
        PathPlatform::Windows => lookup("USERPROFILE")
            .filter(|value| !value.is_empty())
            .or_else(|| lookup("HOME")),
        #[cfg(any(target_os = "macos", test))]
        PathPlatform::Macos => lookup("HOME"),
        #[cfg(any(not(any(target_os = "windows", target_os = "macos")), test))]
        PathPlatform::Xdg => lookup("HOME"),
    };
    home.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "storage_filename_tests.rs"]
mod filename_tests;
