//! Settings and folder-backed list persistence.

use crate::model::{Accent, FontChoice, TitleOverflow, TodoList};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
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
    pub title_overflow: TitleOverflow,
    pub background_opacity: f32,
    pub last_list_id: Option<Uuid>,
    pub last_capture_list_id: Option<Uuid>,
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
            title_overflow: TitleOverflow::Scroll,
            background_opacity: 0.85,
            last_list_id: None,
            last_capture_list_id: None,
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
    let path = settings_path();
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
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
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    write_json_atomic(&settings_path(), settings)
}

/// Expands, creates, and canonicalizes a workspace directory.
///
/// # Returns
///
/// The canonical absolute workspace path.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved or its list directory cannot be created.
pub fn normalize_workspace_path(raw: &str) -> Result<PathBuf, String> {
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
        let list = create_list(root, "Tasks", Accent::Mint)?;
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
            Ok((stored, canonical_name)) => candidates.push((stored, canonical_name)),
            Err(error) => {
                warnings.push(error);
                failed_paths.push(path);
            }
        }
    }

    candidates.sort_by(|(left, left_canonical), (right, right_canonical)| {
        right_canonical
            .cmp(left_canonical)
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

fn read_list(path: &Path) -> Result<(StoredList, bool), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut data = serde_json::from_slice::<TodoList>(&bytes)
        .map_err(|error| format!("ignored {}: {error}", path.display()))?;

    let file_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Uuid::parse_str(stem).ok());
    let canonical_name = file_id.is_some();
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
    let path = root.join("lists").join(format!("{}.json", data.id));
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
/// The application's directory beneath the platform data root.
pub fn default_workspace_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let root = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData/Local"));
    #[cfg(target_os = "macos")]
    let root = home_dir().join("Library/Application Support");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = xdg_dir("XDG_DATA_HOME", ".local/share");
    root.join(APP_DIR)
}

fn settings_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    let root = env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData/Roaming"));
    #[cfg(target_os = "macos")]
    let root = home_dir().join("Library/Application Support");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = xdg_dir("XDG_CONFIG_HOME", ".config");
    root.join(APP_DIR).join("settings.json")
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

        #[cfg(target_os = "windows")]
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
        }

        fs::rename(&temp, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(fallback))
}

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"));
    #[cfg(not(target_os = "windows"))]
    let home = env::var_os("HOME");
    home.map(PathBuf::from)
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
mod tests {
    use super::*;

    #[test]
    fn old_settings_keep_typography_and_default_new_preferences() {
        let settings: Settings =
            serde_json::from_str(r#"{"font_size":24.0,"row_padding":12.0}"#).unwrap();
        assert_eq!(settings.font_size, 24.0);
        assert_eq!(settings.row_padding, 12.0);
        assert_eq!(settings.title_overflow, TitleOverflow::Scroll);
        assert_eq!(settings.background_opacity, 0.85);
        let mut saved = settings;
        saved.title_overflow = TitleOverflow::Wrap;
        saved.background_opacity = 0.2;
        let reloaded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(reloaded.title_overflow, TitleOverflow::Wrap);
        assert_eq!(reloaded.background_opacity, 0.2);
    }

    fn test_root() -> PathBuf {
        env::temp_dir().join(format!("minimalist-list-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn sort_lists_orders_by_creation_then_identity() {
        let stored = |title, created_at_unix, key| {
            let mut data = TodoList::new(title, Accent::Mint);
            data.id = key;
            data.created_at_unix = created_at_unix;
            StoredList {
                key,
                path: PathBuf::new(),
                data,
            }
        };
        let first_id = Uuid::from_u128(1);
        let second_id = Uuid::from_u128(2);
        let later_id = Uuid::from_u128(3);
        let mut lists = vec![
            stored("Later", 20, later_id),
            stored("Second", 10, second_id),
            stored("First", 10, first_id),
        ];

        sort_lists(&mut lists);

        let actual = lists.iter().map(|list| list.key).collect::<Vec<_>>();
        assert_eq!(actual, vec![first_id, second_id, later_id]);
    }

    #[test]
    fn stores_and_reloads_multiple_lists() {
        let root = test_root();
        let root =
            normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
        let first = create_list(&root, "First", Accent::Mint).expect("create first list");
        let second = create_list(&root, "Second", Accent::Sky).expect("create second list");

        assert_ne!(first.path, second.path);
        assert!(first.path.exists());
        assert!(second.path.exists());
        assert_eq!(first.path.parent(), second.path.parent());

        let mut expected = vec![
            (
                first.data.created_at_unix,
                first.key,
                first.data.title.clone(),
            ),
            (
                second.data.created_at_unix,
                second.key,
                second.data.title.clone(),
            ),
        ];
        expected.sort_by_key(|(created_at, id, _)| (*created_at, *id));
        let snapshot = load_workspace(&root).expect("reload workspace");
        let actual = snapshot
            .lists
            .iter()
            .map(|list| (list.data.created_at_unix, list.key, list.data.title.clone()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn updating_one_file_keeps_other_changes_detectable() {
        let root = test_root();
        let root =
            normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
        let mut first = create_list(&root, "First", Accent::Mint).expect("create first list");
        let mut second = create_list(&root, "Second", Accent::Sky).expect("create second list");
        let mut known = workspace_fingerprint(&root).expect("fingerprint workspace");

        second.data.title = "Second changed on another machine".to_owned();
        save_list(&second).expect("save external change");
        first.data.title = "First changed locally".to_owned();
        save_list(&first).expect("save local change");
        update_fingerprint_for_file(&mut known, &first.path).expect("record local write");

        let current = workspace_fingerprint(&root).expect("fingerprint current workspace");
        let second_name = second
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("second filename");
        assert_ne!(known.0.get(second_name), current.0.get(second_name));

        fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn canonical_file_wins_over_sync_conflict_copy() {
        let root = test_root();
        let root =
            normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
        let canonical = create_list(&root, "Canonical", Accent::Mint).expect("create list");
        let conflict_path = root
            .join("lists")
            .join(format!("{} (conflicted copy).json", canonical.key));
        let mut conflict = canonical.clone();
        conflict.path = conflict_path;
        conflict.data.title = "Conflict".to_owned();
        save_list(&conflict).expect("write conflict copy");

        let snapshot = load_workspace(&root).expect("load workspace");
        assert_eq!(snapshot.lists.len(), 1);
        assert_eq!(snapshot.lists[0].data.title, "Canonical");
        assert_eq!(snapshot.warnings.len(), 1);
        assert!(conflict.path.exists());

        fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn atomic_write_leaves_no_temporary_file() {
        let root = test_root();
        let root =
            normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
        let mut list = create_list(&root, "Tasks", Accent::Mint).expect("create list");
        list.data.title = "Changed".to_owned();
        save_list(&list).expect("save list");

        let entries = fs::read_dir(root.join("lists"))
            .expect("read lists directory")
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path(), list.path);

        fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn legacy_focus_settings_are_ignored() {
        let settings = serde_json::from_str::<Settings>(
            r#"{
                "focus_fullscreen": true,
                "focus_minutes": 25
            }"#,
        )
        .expect("parse legacy settings");

        assert!(settings.last_capture_list_id.is_none());
    }
}
