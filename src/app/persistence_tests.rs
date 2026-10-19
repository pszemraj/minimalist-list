//! Failure recovery, destination-bound drafts, and preference concurrency journeys.

use super::{
    tests::{run_frame, test_app},
    *,
};
use crate::model::{Accent, Task};
use std::fs;

fn unavailable_lists(app: &MinimalistApp) {
    fs::rename(
        app.workspace.join("lists"),
        app.workspace.join("lists-offline"),
    )
    .unwrap();
    fs::write(app.workspace.join("lists"), "temporarily unavailable").unwrap();
}

fn restore_lists(app: &MinimalistApp) {
    fs::remove_file(app.workspace.join("lists")).unwrap();
    fs::rename(
        app.workspace.join("lists-offline"),
        app.workspace.join("lists"),
    )
    .unwrap();
}

#[test]
fn failed_creation_and_inline_edit_retry_exact_content_before_restart() {
    let (ctx, mut app) = test_app();
    let id = app.lists[0].key;
    unavailable_lists(&app);
    app.new_task = "Recover this task".into();
    let palette = app.palette();
    run_frame(&ctx, Default::default(), |ui| app.list_ui(ui, id, palette));
    run_frame(
        &ctx,
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| app.list_ui(ui, id, palette),
    );
    assert_eq!(app.lists[0].data.tasks.len(), 1);
    assert!(app.persistence.dirty_lists.contains(&id));
    let task = app.lists[0].data.tasks[0].id;
    app.editing = Some((id, task));
    app.edit_text = "Recover the edited title".into();
    app.finish_inline_edit();
    app.next_scan = Instant::now();
    app.scan_external_changes();
    assert_eq!(app.lists[0].data.tasks[0].text, "Recover the edited title");
    restore_lists(&app);
    assert!(app.retry_dirty_lists());
    assert!(app.retry_dirty_lists());
    let saved = storage::load_workspace(&app.workspace).unwrap();
    assert_eq!(saved.lists[0].data.tasks.len(), 1);
    assert_eq!(
        saved.lists[0].data.tasks[0].text,
        "Recover the edited title"
    );
    fs::remove_dir_all(app.workspace).unwrap();
}

#[test]
fn drafts_return_to_original_list_workspace_and_collapsed_checklist() {
    let (_, mut app) = test_app();
    let first = app.lists[0].key;
    let other = storage::create_list(&app.workspace, "Other", Accent::Mint).unwrap();
    let second = other.key;
    app.lists.push(other);
    app.new_task = "First list draft".into();
    app.open_list(second);
    assert!(app.new_task.is_empty());
    app.new_task = "Second list draft".into();
    app.open_list(first);
    assert_eq!(app.new_task, "First list draft");
    let task = Task::new("Parent");
    let parent = task.id;
    app.lists[0].data.tasks.push(task);
    app.switch_subtask_panel(Some((first, parent)));
    app.new_subtask = "Checklist draft".into();
    app.switch_subtask_panel(None);
    app.switch_subtask_panel(Some((first, parent)));
    assert_eq!(app.new_subtask, "Checklist draft");
    let original = app.workspace.clone();
    let next = original.join("another-workspace");
    app.switch_workspace(next.to_str().unwrap());
    assert!(app.new_task.is_empty());
    app.new_task = "Another workspace draft".into();
    app.switch_workspace(original.to_str().unwrap());
    app.open_list(first);
    assert_eq!(app.new_task, "First list draft");
    app.open_list(second);
    assert_eq!(app.new_task, "Second list draft");
    fs::remove_dir_all(original).unwrap();
}

#[test]
fn failed_startup_does_not_load_another_instances_default_workspace() {
    let root = std::env::temp_dir().join(format!("startup-recovery-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let selected = root.join("selected");
    fs::write(&selected, "temporary obstruction").unwrap();
    let default_workspace = storage::default_workspace_dir();
    let default_list =
        storage::create_list(&default_workspace, "Already owned", Accent::Mint).unwrap();
    let default_bytes = fs::read(&default_list.path).unwrap();
    let state = storage::window_state_path();
    let state_dir = state.parent().unwrap();
    let crate::instance::LaunchOutcome::Primary(default_owner) =
        crate::instance::claim(&default_workspace, state_dir, false).unwrap()
    else {
        panic!("expected a default workspace owner");
    };
    let settings = Settings {
        workspace_path: selected.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut app = MinimalistApp::new(&ctx, settings, None, None);
    let crate::instance::LaunchOutcome::Primary(recovery_owner) =
        crate::claim_launch_instance(selected.to_str().unwrap(), state_dir, false).unwrap()
    else {
        panic!("expected a recovery workspace owner");
    };
    app.set_instance_guard(recovery_owner, &ctx);
    let recovery_empty = app.lists.is_empty();
    let recovery_workspace = app.workspace.clone();
    let recovery_failure = app.persistence.workspace_failure.is_some();
    let default_revealed_at_startup = default_owner.take_show_request();
    let after_startup = fs::read(&default_list.path).unwrap();
    app.switch_workspace(default_workspace.to_str().unwrap());
    let default_revealed_by_choice = default_owner.take_show_request();
    let choice_workspace = app.workspace.clone();
    let choice_empty = app.lists.is_empty();

    fs::remove_file(&selected).unwrap();
    let recovered = storage::create_list(&selected, "Recovered selection", Accent::Sky).unwrap();
    app.switch_workspace(selected.to_str().unwrap());
    let recovered_list = app.lists.first().map(|list| list.key);
    let recovered_failure = app.persistence.workspace_failure.is_some();
    fs::remove_file(default_list.path).unwrap();
    app.instance = None;
    drop(default_owner);
    fs::remove_dir_all(root).unwrap();
    assert!(
        recovery_empty,
        "recovery must not load unclaimed default lists"
    );
    assert_eq!(recovery_workspace, selected);
    assert!(recovery_failure);
    assert!(!default_revealed_at_startup);
    assert_eq!(after_startup, default_bytes);
    assert!(default_revealed_by_choice);
    assert_eq!(choice_workspace, selected);
    assert!(choice_empty);
    assert_eq!(recovered_list, Some(recovered.key));
    assert!(!recovered_failure);
}

#[test]
fn failed_startup_leaves_absent_default_workspace_uncreated() {
    const CHILD_ROOT: &str = "MINIMALIST_RECOVERY_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = std::path::PathBuf::from(root);
        let selected = root.join("selected");
        let default_workspace = storage::default_workspace_dir();
        assert!(!default_workspace.exists());
        let settings = Settings {
            workspace_path: selected.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let app = MinimalistApp::new(&ctx, settings, None, None);
        assert!(
            !default_workspace.exists(),
            "recovery created a default fallback"
        );
        assert!(app.lists.is_empty());
        assert_eq!(app.workspace, selected);
        return;
    }
    let root = std::env::temp_dir().join(format!("empty-recovery-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("selected"), "temporary obstruction").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::persistence_tests::failed_startup_leaves_absent_default_workspace_uncreated",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn temporary_startup_failure_preserves_selected_path_and_retry_recovers_it() {
    let root = std::env::temp_dir().join(format!("unavailable-workspace-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let selected = root.join("selected");
    fs::write(&selected, "temporary obstruction").unwrap();
    let settings = Settings {
        workspace_path: selected.to_string_lossy().into_owned(),
        ..Default::default()
    };
    storage::save_settings(&settings).unwrap();
    let ctx = egui::Context::default();
    let mut app = MinimalistApp::new(&ctx, settings.clone(), None, None);
    assert!(app.persistence.workspace_failure.is_some());
    app.go_to_overview();
    app.save_settings();
    assert_eq!(
        storage::load_settings().unwrap().workspace_path,
        settings.workspace_path
    );
    fs::remove_file(&selected).unwrap();
    let original = storage::create_list(&selected, "Recovered", Accent::Mint).unwrap();
    app.switch_workspace(selected.to_str().unwrap());
    assert_eq!(app.lists[0].key, original.key);
    assert!(app.persistence.workspace_failure.is_none());
    app.switch_workspace("   ");
    assert_eq!(app.lists[0].key, original.key);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_instance_navigation_preserves_newer_appearance_preferences() {
    let (_, mut first) = test_app();
    first.save_settings();
    let (_, mut stale) = test_app();
    stale.settings = storage::load_settings().unwrap();
    stale.persistence = persistence::PersistenceState::new(stale.settings.clone(), None);
    first.settings.bold_text = true;
    first.settings.always_on_top = true;
    first.settings.background_opacity = 0.4;
    first.save_settings();
    stale.settings.last_list_id = Some(stale.lists[0].key);
    stale.save_settings();
    let saved = storage::load_settings().unwrap();
    assert!(saved.bold_text && saved.always_on_top);
    assert_eq!(saved.background_opacity, 0.4);
    assert_eq!(saved.last_list_id, Some(stale.lists[0].key));
    fs::remove_dir_all(first.workspace).unwrap();
    fs::remove_dir_all(stale.workspace).unwrap();
}

#[test]
fn shutdown_and_workspace_switch_finish_pending_rename() {
    let (_, mut app) = test_app();
    let list = app.lists[0].key;
    app.open_settings(Some(list));
    app.rename_input = "Saved at shutdown".into();
    app.finish_pending_changes();
    assert_eq!(
        storage::load_workspace(&app.workspace).unwrap().lists[0]
            .data
            .title,
        "Saved at shutdown"
    );
    app.rename_input = "Saved before switching".into();
    let original = app.workspace.clone();
    app.switch_workspace(original.join("next").to_str().unwrap());
    assert_eq!(
        storage::load_workspace(&original).unwrap().lists[0]
            .data
            .title,
        "Saved before switching"
    );
    fs::remove_dir_all(original).unwrap();
}

#[test]
fn finishing_changes_preserves_unrelated_diagnostics_without_pending_writes() {
    for diagnostic in [
        "Global shortcut unavailable: shortcut is already registered",
        "Duplicate list files were found",
        "Could not read an external list file",
    ] {
        let (_, mut app) = test_app();
        app.status = Some(diagnostic.into());
        assert!(app.retry_dirty_lists());
        assert_eq!(app.status.as_deref(), Some(diagnostic));
        assert!(app.finish_pending_changes());
        assert_eq!(app.status.as_deref(), Some(diagnostic));
        let original = app.workspace.clone();
        app.switch_workspace(original.join("next-workspace").to_str().unwrap());
        assert_ne!(app.workspace, original);
        assert_eq!(app.status.as_deref(), Some(diagnostic));
        fs::remove_dir_all(original).unwrap();
    }
}

#[test]
fn successful_retry_clears_its_own_error_but_preserves_a_later_diagnostic() {
    for later_diagnostic in [None, Some("Global shortcut registration failed")] {
        let (_, mut app) = test_app();
        let list = app.lists[0].key;
        app.lists[0]
            .data
            .tasks
            .push(Task::new("Recover this change"));
        unavailable_lists(&app);
        assert!(!app.save_list_index(0));
        assert!(app.persistence.dirty_lists.contains(&list));
        assert!(
            app.status
                .as_deref()
                .unwrap()
                .starts_with("Changes are unsaved:")
        );
        if let Some(diagnostic) = later_diagnostic {
            app.status = Some(diagnostic.into());
        }
        restore_lists(&app);
        assert!(app.retry_dirty_lists());
        assert_eq!(app.status.as_deref(), later_diagnostic);
        assert!(app.retry_dirty_lists());
        assert_eq!(app.status.as_deref(), later_diagnostic);
        assert_eq!(
            storage::load_workspace(&app.workspace).unwrap().lists[0]
                .data
                .tasks[0]
                .text,
            "Recover this change"
        );
        fs::remove_dir_all(app.workspace).unwrap();
    }
}

#[test]
fn success_acknowledgement_expires_without_dismissing_save_errors() {
    let (ctx, mut app) = test_app();
    app.acknowledge("Added to Tasks".into());
    assert!(app.persistence.notice.is_some());
    app.status = Some("Changes are unsaved".into());
    app.persistence.notice.as_mut().unwrap().1 = Instant::now() - Duration::from_secs(4);
    app.update_motion(&ctx);
    assert!(app.persistence.notice.is_none());
    assert_eq!(app.status.as_deref(), Some("Changes are unsaved"));
    fs::remove_dir_all(app.workspace).unwrap();
}
