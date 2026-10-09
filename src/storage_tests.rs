//! Regression tests for storage and platform-path resolution.

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
    let reloaded: Settings = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(reloaded.title_overflow, TitleOverflow::Wrap);
    assert_eq!(reloaded.background_opacity, 0.2);
}

fn test_root() -> PathBuf {
    env::temp_dir().join(format!("minimalist-list-test-{}", Uuid::new_v4()))
}

#[test]
fn native_paths_reuse_legacy_locations_independently() {
    let root = test_root();
    for (platform, name) in [
        (PathPlatform::Macos, "macos"),
        (PathPlatform::Windows, "windows"),
    ] {
        for overrides in [false, true] {
            let case = root.join(format!("{name}-{overrides}"));
            let home = case.join("home");
            let profile = case.join("profile");
            let mut environment = BTreeMap::from([
                ("HOME", home.clone().into_os_string()),
                ("USERPROFILE", profile.clone().into_os_string()),
            ]);
            let (native_data, native_config) = match platform {
                PathPlatform::Macos => {
                    let native = home.join("Library/Application Support");
                    (native.clone(), native)
                }
                PathPlatform::Windows => (
                    profile.join("AppData/Local"),
                    profile.join("AppData/Roaming"),
                ),
                PathPlatform::Xdg => unreachable!(),
            };
            let (native_data, native_config, legacy_data, legacy_config) = if overrides {
                let data = case.join("native-data");
                let config = case.join("native-config");
                let xdg_data = case.join("xdg-data");
                let xdg_config = case.join("xdg-config");
                environment.extend([
                    ("LOCALAPPDATA", data.clone().into_os_string()),
                    ("APPDATA", config.clone().into_os_string()),
                    ("XDG_DATA_HOME", xdg_data.clone().into_os_string()),
                    ("XDG_CONFIG_HOME", xdg_config.clone().into_os_string()),
                ]);
                match platform {
                    PathPlatform::Windows => (data, config, xdg_data, xdg_config),
                    _ => (native_data, native_config, xdg_data, xdg_config),
                }
            } else {
                (
                    native_data,
                    native_config,
                    home.join(".local/share"),
                    home.join(".config"),
                )
            };
            let lookup = |name: &str| environment.get(name).cloned();
            let preferred_workspace = native_data.join(APP_DIR);
            let preferred_settings = native_config.join(APP_DIR).join("settings.json");
            let legacy_workspace = legacy_data.join(APP_DIR);
            let legacy_settings = legacy_config.join(APP_DIR).join("settings.json");

            assert_eq!(
                default_workspace_dir_from(platform, &lookup),
                preferred_workspace
            );
            assert_eq!(settings_path_from(platform, &lookup), preferred_settings);
            assert!(!preferred_workspace.exists());

            let legacy_list =
                create_list(&legacy_workspace, "Existing tasks", Accent::Mint).unwrap();
            let settings = Settings {
                workspace_path: home.join("chosen-workspace").to_string_lossy().into_owned(),
                font_size: 24.0,
                ..Settings::default()
            };
            write_json_atomic(&legacy_settings, &settings).unwrap();
            let selected_workspace = default_workspace_dir_from(platform, &lookup);
            assert_eq!(selected_workspace, legacy_workspace);
            assert_eq!(
                load_workspace(&selected_workspace).unwrap().lists[0]
                    .data
                    .title,
                "Existing tasks"
            );
            let selected_settings = settings_path_from(platform, &lookup);
            let reloaded: Settings =
                serde_json::from_slice(&fs::read(&selected_settings).unwrap()).unwrap();
            assert_eq!(selected_settings, legacy_settings);
            assert_eq!(reloaded.workspace_path, settings.workspace_path);
            assert_eq!(reloaded.font_size, 24.0);
            assert!(legacy_list.path.exists());
            assert!(!preferred_workspace.exists());

            fs::create_dir_all(&preferred_workspace).unwrap();
            assert_eq!(
                default_workspace_dir_from(platform, &lookup),
                preferred_workspace
            );
            // Creating the native workspace does not supersede legacy settings.
            assert_eq!(settings_path_from(platform, &lookup), legacy_settings);
            let mut changed = reloaded;
            changed.font_size = 28.0;
            write_json_atomic(&settings_path_from(platform, &lookup), &changed).unwrap();
            let reloaded: Settings =
                serde_json::from_slice(&fs::read(&legacy_settings).unwrap()).unwrap();
            assert_eq!(reloaded.font_size, 28.0);
            assert!(!preferred_settings.exists());
            write_json_atomic(&preferred_settings, &Settings::default()).unwrap();
            assert_eq!(settings_path_from(platform, &lookup), preferred_settings);
            // Settings precedence does not make an absent native workspace win.
            if matches!(platform, PathPlatform::Windows) {
                fs::remove_dir_all(&preferred_workspace).unwrap();
                assert_eq!(
                    default_workspace_dir_from(platform, &lookup),
                    legacy_workspace
                );
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_paths_keep_xdg_overrides_and_ignore_empty_values() {
    let root = test_root();
    let home = root.join("home");
    let profile = root.join("profile");
    for xdg_override in [None, Some(PathBuf::new()), Some(root.join("override"))] {
        let mut environment = BTreeMap::from([
            ("HOME", home.clone().into_os_string()),
            ("USERPROFILE", profile.clone().into_os_string()),
        ]);
        if let Some(ref override_root) = xdg_override {
            let (data, config) = if override_root.as_os_str().is_empty() {
                (PathBuf::new(), PathBuf::new())
            } else {
                (override_root.join("data"), override_root.join("config"))
            };
            environment.extend([
                ("XDG_DATA_HOME", data.into_os_string()),
                ("XDG_CONFIG_HOME", config.into_os_string()),
            ]);
        }
        let lookup = |name: &str| environment.get(name).cloned();
        let (data, config) = match xdg_override {
            Some(path) if !path.as_os_str().is_empty() => (path.join("data"), path.join("config")),
            _ => (home.join(".local/share"), home.join(".config")),
        };
        let workspace = data.join(APP_DIR);
        let settings = config.join(APP_DIR).join("settings.json");
        assert_eq!(
            default_workspace_dir_from(PathPlatform::Xdg, &lookup),
            workspace
        );
        assert_eq!(settings_path_from(PathPlatform::Xdg, &lookup), settings);
        fs::create_dir_all(&workspace).unwrap();
        write_json_atomic(&settings, &Settings::default()).unwrap();
        for platform in [
            PathPlatform::Xdg,
            PathPlatform::Macos,
            PathPlatform::Windows,
        ] {
            assert_eq!(default_workspace_dir_from(platform, &lookup), workspace);
            assert_eq!(settings_path_from(platform, &lookup), settings);
        }
    }
    fs::remove_dir_all(&home).unwrap();
    for environment in [
        BTreeMap::from([("HOME", home.clone().into_os_string())]),
        BTreeMap::from([("USERPROFILE", profile.into_os_string())]),
        BTreeMap::new(),
    ] {
        let lookup = |name: &str| environment.get(name).cloned();
        let native_home = environment
            .get("USERPROFILE")
            .or_else(|| environment.get("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(home_dir_from(PathPlatform::Windows, &lookup), native_home);
        let legacy_home = environment
            .get("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(
            xdg_dir_from(&lookup, "XDG_DATA_HOME", ".local/share"),
            legacy_home.join(".local/share")
        );
        assert_eq!(
            xdg_dir_from(&lookup, "XDG_CONFIG_HOME", ".config"),
            legacy_home.join(".config")
        );
        assert_eq!(
            default_workspace_dir_from(PathPlatform::Windows, &lookup),
            native_home.join("AppData/Local").join(APP_DIR)
        );
        assert_eq!(
            settings_path_from(PathPlatform::Windows, &lookup),
            native_home
                .join("AppData/Roaming")
                .join(APP_DIR)
                .join("settings.json")
        );
    }
    let environment = BTreeMap::from([
        ("HOME", home.clone().into_os_string()),
        ("USERPROFILE", OsString::new()),
        ("LOCALAPPDATA", OsString::new()),
        ("APPDATA", OsString::new()),
    ]);
    let lookup = |name: &str| environment.get(name).cloned();
    assert_eq!(home_dir_from(PathPlatform::Windows, &lookup), home);
    assert_eq!(
        default_workspace_dir_from(PathPlatform::Windows, &lookup),
        home.join("AppData/Local").join(APP_DIR)
    );
    assert_eq!(
        settings_path_from(PathPlatform::Windows, &lookup),
        home.join("AppData/Roaming")
            .join(APP_DIR)
            .join("settings.json")
    );
    let environment = BTreeMap::from([
        ("HOME", OsString::new()),
        ("USERPROFILE", OsString::new()),
        ("LOCALAPPDATA", OsString::new()),
        ("APPDATA", OsString::new()),
        ("XDG_DATA_HOME", OsString::new()),
        ("XDG_CONFIG_HOME", OsString::new()),
    ]);
    let lookup = |name: &str| environment.get(name).cloned();
    for platform in [
        PathPlatform::Windows,
        PathPlatform::Macos,
        PathPlatform::Xdg,
    ] {
        assert_eq!(home_dir_from(platform, &lookup), PathBuf::from("."));
    }
    assert_eq!(
        default_workspace_dir_from(PathPlatform::Windows, &lookup),
        PathBuf::from("./AppData/Local").join(APP_DIR)
    );
    assert_eq!(
        settings_path_from(PathPlatform::Windows, &lookup),
        PathBuf::from("./AppData/Roaming")
            .join(APP_DIR)
            .join("settings.json")
    );
    assert_eq!(
        xdg_dir_from(&lookup, "XDG_DATA_HOME", ".local/share"),
        PathBuf::from("./.local/share")
    );
    assert_eq!(
        xdg_dir_from(&lookup, "XDG_CONFIG_HOME", ".config"),
        PathBuf::from("./.config")
    );
    fs::remove_dir_all(root).unwrap();
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
    let root = normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
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
    let root = normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
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
    let root = normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
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
    let root = normalize_workspace_path(&root.to_string_lossy()).expect("create test workspace");
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
