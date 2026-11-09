//! Native application entry point and window configuration.

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod hotkey;
mod instance;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(target_os = "linux", test))]
mod linux_cursor;
#[cfg(any(target_os = "linux", test))]
mod linux_taskbar;
mod logging;
#[cfg(target_os = "macos")]
mod macos;
mod model;
mod storage;
mod tray;

use app::MinimalistApp;
use eframe::egui;
use egui::viewport::WindowLevel;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const HELP: &str = concat!(
    env!("CARGO_PKG_NAME"),
    " ",
    env!("CARGO_PKG_VERSION"),
    "\n",
    env!("CARGO_PKG_DESCRIPTION"),
    "\n\n",
    "Usage: minimalist-list [OPTIONS]\n\n",
    "Options:\n",
    "      --data-dir <PATH>  Use PATH as the workspace for this launch\n",
    "      --start-hidden     Start in the tray when available\n",
    "      --log              Write daily lifecycle logs to the per-user log directory\n",
    "      --debug            Write detailed daily diagnostic logs\n",
    "  -h, --help             Print help\n",
    "  -V, --version          Print version\n\n",
    "Workspace precedence: --data-dir, MINIMALIST_LIST_WORKSPACE, saved setting, platform default.\n",
);

#[derive(Debug, PartialEq, Eq)]
enum LaunchAction {
    Run {
        data_dir: Option<String>,
        start_hidden: bool,
        logging: logging::Mode,
    },
    Help,
    Version,
}

fn parse_launch_args(args: impl IntoIterator<Item = OsString>) -> Result<LaunchAction, String> {
    let mut args = args.into_iter();
    let mut data_dir = None;
    let mut start_hidden = false;
    let mut logging = logging::Mode::Off;

    while let Some(argument) = args.next() {
        let text = argument
            .to_str()
            .ok_or_else(|| "command-line arguments must be valid UTF-8".to_owned())?;
        match text {
            "-h" | "--help" => return Ok(LaunchAction::Help),
            "-V" | "--version" => return Ok(LaunchAction::Version),
            "--start-hidden" => start_hidden = true,
            "--log" => {
                if logging != logging::Mode::Debug {
                    logging = logging::Mode::Log;
                }
            }
            "--debug" => logging = logging::Mode::Debug,
            "--data-dir" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--data-dir requires a path".to_owned())?
                    .into_string()
                    .map_err(|_| "--data-dir requires a UTF-8 path".to_owned())?;
                if path.is_empty() {
                    return Err("--data-dir requires a non-empty path".to_owned());
                }
                if path.starts_with('-') {
                    return Err("--data-dir requires a path".to_owned());
                }
                data_dir = Some(path);
            }
            _ if text.starts_with("--data-dir=") => {
                let path = &text["--data-dir=".len()..];
                if path.is_empty() {
                    return Err("--data-dir requires a non-empty path".to_owned());
                }
                data_dir = Some(path.to_owned());
            }
            _ => return Err(format!("unknown argument: {text}")),
        }
    }

    Ok(LaunchAction::Run {
        data_dir,
        start_hidden,
        logging,
    })
}

fn select_workspace_override(
    data_dir: Option<String>,
    environment: Option<OsString>,
) -> Result<Option<String>, String> {
    match data_dir {
        Some(path) => Ok(Some(path)),
        None => environment
            .filter(|path| !path.is_empty())
            .map(|path| {
                path.into_string()
                    .map_err(|_| "MINIMALIST_LIST_WORKSPACE requires a UTF-8 path".to_owned())
            })
            .transpose(),
    }
}

fn claim_launch_instance(
    requested_workspace: &str,
    resolved_workspace: &Result<(PathBuf, bool), String>,
    state_dir: &Path,
    start_hidden: bool,
) -> Result<instance::LaunchOutcome, String> {
    // Keep failed requests separate so they can display their recovery window.
    let workspace = resolved_workspace.as_ref().map_or_else(
        |_| Path::new(requested_workspace),
        |(path, _)| path.as_path(),
    );
    instance::claim(workspace, state_dir, start_hidden)
}

fn main() -> eframe::Result {
    let (data_dir, start_hidden, logging) = match parse_launch_args(env::args_os().skip(1)) {
        Ok(LaunchAction::Run {
            data_dir,
            start_hidden,
            logging,
        }) => (data_dir, start_hidden, logging),
        Ok(LaunchAction::Help) => {
            print!("{HELP}");
            return Ok(());
        }
        Ok(LaunchAction::Version) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Err(error) => {
            eprintln!("error: {error}\n\nRun minimalist-list --help for usage.");
            std::process::exit(2);
        }
    };
    let logging_warning = logging::init(logging).err().map(|error| {
        let warning = format!("Could not start diagnostic logging: {error}");
        eprintln!("{warning}");
        warning
    });
    log::info!(
        "Starting Minimalist List {} on {} (start hidden: {start_hidden}, logging: {logging:?})",
        env!("CARGO_PKG_VERSION"),
        env::consts::OS
    );
    let (settings, settings_warning) = storage::load_settings();
    if settings_warning.is_some() {
        log::warn!("Local preferences could not be fully loaded");
    }
    let warning = match (settings_warning, logging_warning) {
        (Some(settings), Some(logging)) => Some(format!("{settings}\n{logging}")),
        (settings, logging) => settings.or(logging),
    };
    let workspace_override =
        select_workspace_override(data_dir, env::var_os("MINIMALIST_LIST_WORKSPACE"))
            .map_err(|error| eframe::Error::AppCreation(error.into()))?;

    let requested_workspace = workspace_override
        .as_deref()
        .unwrap_or(&settings.workspace_path);
    let workspace =
        storage::resolve_launch_workspace(requested_workspace, workspace_override.is_some());
    let state_path = storage::window_state_path();
    let state_dir = state_path.parent().unwrap_or_else(|| Path::new("."));
    let instance =
        match claim_launch_instance(requested_workspace, &workspace, state_dir, start_hidden) {
            Ok(instance::LaunchOutcome::Existing) => {
                log::info!("Existing workspace instance notified; this launch is exiting");
                return Ok(());
            }
            Ok(instance::LaunchOutcome::Primary(instance)) => instance,
            Err(error) => {
                log::error!("Could not claim workspace instance: {error}");
                return Err(eframe::Error::AppCreation(error.into()));
            }
        };
    let level = if settings.always_on_top {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    };
    let options = eframe::NativeOptions {
        persistence_path: Some(storage::window_state_path()),
        viewport: egui::ViewportBuilder::default()
            .with_app_id("minimalist-list")
            .with_icon(tray::window_icon_data())
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([320.0, 280.0])
            .with_transparent(true)
            .with_taskbar(false)
            .with_visible(!start_hidden)
            // A minimized macOS window stays in the Dock after hiding; close hides instead.
            .with_minimize_button(!cfg!(target_os = "macos"))
            .with_decorations(settings.window_decorations)
            .with_window_level(level),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    // Winit cannot hide or restore native Wayland windows. Prefer X11 only for a usable tray.
    #[cfg(target_os = "linux")]
    let prefer_x11 = x11rb::connect(None).is_ok() && tray::linux_host_available();
    #[cfg(target_os = "linux")]
    let options = {
        let mut options = options;
        if prefer_x11 {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            }));
        }
        options
    };
    #[cfg(target_os = "macos")]
    let options = {
        let mut options = options;
        options.event_loop_builder = Some(Box::new(move |builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory);
            builder.with_activate_ignoring_other_apps(!start_hidden);
        }));
        options
    };
    #[cfg(target_os = "linux")]
    let run_native = linux::run_native;
    #[cfg(not(target_os = "linux"))]
    let run_native = eframe::run_native;
    let result = run_native(
        "Minimalist List",
        options,
        Box::new(move |cc| {
            let mut app = MinimalistApp::from_launch(
                &cc.egui_ctx,
                settings,
                workspace_override,
                workspace,
                warning,
            );
            app.set_instance_guard(instance, &cc.egui_ctx);
            #[cfg(target_os = "linux")]
            let supports_hiding = {
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                cc.winit_window().is_some_and(|window| {
                    window
                        .window_handle()
                        .is_ok_and(|handle| matches!(handle.as_raw(), RawWindowHandle::Xlib(_)))
                })
            };
            #[cfg(not(target_os = "linux"))]
            let supports_hiding = true;
            app.setup_tray(&cc.egui_ctx, supports_hiding);
            #[cfg(target_os = "windows")]
            if let Some(window) = cc.winit_window() {
                tray::set_window_taskbar_icon(window);
            }
            app.setup_global_shortcut(&cc.egui_ctx);
            if start_hidden {
                app.start_hidden(&cc.egui_ctx);
            }
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            app.update_taskbar_presence(cc.winit_window().map(AsRef::as_ref));
            Ok(Box::new(app))
        }),
    );
    match &result {
        Ok(()) => log::info!("Native event loop exited normally"),
        Err(error) => log::error!("Native event loop failed: {error}"),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_requested_workspace_keeps_its_own_launch_identity() {
        let root = std::env::temp_dir().join(format!("startup-claim-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let blocked = root.join("requested-workspace");
        std::fs::write(&blocked, "temporarily unavailable").unwrap();
        let default_workspace = storage::default_workspace_dir();
        // Claim its identity in the fixture state directory without opening its data.
        let default_resolution = Ok((default_workspace.clone(), true));
        let blocked = blocked.to_str().unwrap();
        let default_workspace = default_workspace.to_str().unwrap();
        let blocked_resolution = storage::resolve_launch_workspace(blocked, true);
        assert!(blocked_resolution.is_err());
        let instance::LaunchOutcome::Primary(default_owner) =
            claim_launch_instance(default_workspace, &default_resolution, &root, false).unwrap()
        else {
            panic!("expected a default workspace owner");
        };

        let instance::LaunchOutcome::Primary(recovery_owner) =
            claim_launch_instance(blocked, &blocked_resolution, &root, false).unwrap()
        else {
            panic!("the failed request must reach its recovery UI");
        };
        assert!(!default_owner.take_show_request());
        assert!(matches!(
            claim_launch_instance(blocked, &blocked_resolution, &root, false).unwrap(),
            instance::LaunchOutcome::Existing
        ));
        assert!(recovery_owner.take_show_request());
        assert!(!default_owner.take_show_request());

        assert!(matches!(
            claim_launch_instance(default_workspace, &default_resolution, &root, false).unwrap(),
            instance::LaunchOutcome::Existing
        ));
        assert!(default_owner.take_show_request());
        assert!(!recovery_owner.take_show_request());

        // If the folder comes back before the window is constructed, startup
        // must still wait for ordinary workspace recovery to load existing lists.
        std::fs::remove_file(blocked).unwrap();
        let lists = Path::new(blocked).join("lists");
        std::fs::create_dir_all(&lists).unwrap();
        let ctx = egui::Context::default();
        let mut app = MinimalistApp::from_launch(
            &ctx,
            storage::Settings::default(),
            Some(blocked.to_owned()),
            blocked_resolution,
            None,
        );
        app.set_instance_guard(recovery_owner, &ctx);
        assert_eq!(std::fs::read_dir(lists).unwrap().count(), 0);
        drop(app);
        drop(default_owner);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn startup_loads_the_workspace_claimed_before_an_alias_changes() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("startup-alias-{}", uuid::Uuid::new_v4()));
        let first = root.join("first");
        let second = root.join("second");
        let alias = root.join("alias");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        symlink(&first, &alias).unwrap();
        let requested = alias.to_str().unwrap();
        let resolution = storage::resolve_launch_workspace(requested, true);
        let instance::LaunchOutcome::Primary(owner) =
            claim_launch_instance(requested, &resolution, &root, false).unwrap()
        else {
            panic!("expected to claim the first workspace");
        };
        assert!(owner.owns_workspace(&first));
        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();

        let ctx = egui::Context::default();
        let mut app = MinimalistApp::from_launch(
            &ctx,
            storage::Settings::default(),
            Some(requested.to_owned()),
            resolution,
            None,
        );
        app.set_instance_guard(owner, &ctx);
        assert_eq!(fs::read_dir(first.join("lists")).unwrap().count(), 1);
        assert!(!second.join("lists").exists());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_data_dir_argument() {
        let action = parse_launch_args([OsString::from("--data-dir"), OsString::from("/cli")]);
        assert_eq!(
            action,
            Ok(LaunchAction::Run {
                data_dir: Some("/cli".to_owned()),
                start_hidden: false,
                logging: logging::Mode::Off,
            })
        );
    }

    #[test]
    fn data_dir_does_not_consume_the_next_option() {
        for option in ["--debug", "-V", "--data-dir=/other"] {
            let args = [OsString::from("--data-dir"), OsString::from(option)];
            assert_eq!(
                parse_launch_args(args),
                Err("--data-dir requires a path".to_owned())
            );
        }
        assert!(matches!(
            parse_launch_args([OsString::from("--data-dir=--debug")]),
            Ok(LaunchAction::Run { data_dir: Some(path), .. }) if path == "--debug"
        ));
    }

    #[test]
    fn hidden_start_keeps_workspace_arguments_and_normal_launch_stays_visible() {
        assert_eq!(
            parse_launch_args([
                OsString::from("--start-hidden"),
                OsString::from("--data-dir=/workspace with spaces"),
            ]),
            Ok(LaunchAction::Run {
                data_dir: Some("/workspace with spaces".into()),
                start_hidden: true,
                logging: logging::Mode::Off,
            })
        );
        assert_eq!(
            parse_launch_args([]),
            Ok(LaunchAction::Run {
                data_dir: None,
                start_hidden: false,
                logging: logging::Mode::Off,
            })
        );
    }

    #[test]
    fn logging_flags_select_detail_level_without_changing_launch_options() {
        for (flags, expected) in [
            (&["--log"][..], logging::Mode::Log),
            (&["--log", "--debug"][..], logging::Mode::Debug),
            (&["--debug", "--log"][..], logging::Mode::Debug),
        ] {
            let args = flags
                .iter()
                .copied()
                .chain(["--start-hidden", "--data-dir=workspace"])
                .map(OsString::from);
            assert_eq!(
                parse_launch_args(args),
                Ok(LaunchAction::Run {
                    data_dir: Some("workspace".into()),
                    start_hidden: true,
                    logging: expected,
                }),
            );
        }
    }

    #[test]
    fn cli_data_dir_precedes_environment() {
        let selected =
            select_workspace_override(Some("/cli".to_owned()), Some(OsString::from("/env")))
                .unwrap();
        assert_eq!(selected.as_deref(), Some("/cli"));
    }

    #[test]
    fn environment_precedes_saved_settings_when_cli_is_absent() {
        let selected = select_workspace_override(None, Some(OsString::from("/env"))).unwrap();
        assert_eq!(selected.as_deref(), Some("/env"));
    }

    #[test]
    fn literal_launch_options_preserve_surrounding_spaces() {
        let path = "  workspace  ";
        for args in [
            vec![OsString::from("--data-dir"), OsString::from(path)],
            vec![OsString::from(format!("--data-dir={path}"))],
        ] {
            assert!(matches!(
                parse_launch_args(args),
                Ok(LaunchAction::Run { data_dir: Some(selected), .. }) if selected == path
            ));
        }
        assert_eq!(
            select_workspace_override(None, Some(OsString::from(path)))
                .unwrap()
                .as_deref(),
            Some(path)
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_launch_paths_are_rejected() {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(b"workspace\xff".to_vec());
        for args in [
            vec![OsString::from("--data-dir"), path.clone()],
            vec![OsString::from_vec(b"--data-dir=workspace\xff".to_vec())],
        ] {
            assert!(parse_launch_args(args).unwrap_err().contains("UTF-8"));
        }
        assert_eq!(
            select_workspace_override(None, Some(path.clone())).unwrap_err(),
            "MINIMALIST_LIST_WORKSPACE requires a UTF-8 path"
        );
        assert_eq!(
            select_workspace_override(Some("cli".into()), Some(path)).unwrap(),
            Some("cli".into())
        );
    }
}
