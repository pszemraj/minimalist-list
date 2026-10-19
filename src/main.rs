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
    "  -h, --help             Print help\n",
    "  -V, --version          Print version\n\n",
    "Workspace precedence: --data-dir, MINIMALIST_LIST_WORKSPACE, saved setting, platform default.\n",
);

#[derive(Debug, PartialEq, Eq)]
enum LaunchAction {
    Run {
        data_dir: Option<String>,
        start_hidden: bool,
    },
    Help,
    Version,
}

fn parse_launch_args(args: impl IntoIterator<Item = OsString>) -> Result<LaunchAction, String> {
    let mut args = args.into_iter();
    let mut data_dir = None;
    let mut start_hidden = false;

    while let Some(argument) = args.next() {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "-h" | "--help" => return Ok(LaunchAction::Help),
            "-V" | "--version" => return Ok(LaunchAction::Version),
            "--start-hidden" => start_hidden = true,
            "--data-dir" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--data-dir requires a path".to_owned())?;
                if path.is_empty() {
                    return Err("--data-dir requires a non-empty path".to_owned());
                }
                data_dir = Some(path.to_string_lossy().into_owned());
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
    })
}

fn select_workspace_override(
    data_dir: Option<String>,
    environment: Option<OsString>,
) -> Option<String> {
    data_dir.or_else(|| {
        environment
            .filter(|path| !path.is_empty())
            .map(|path| path.to_string_lossy().into_owned())
    })
}

fn claim_launch_instance(
    requested_workspace: &str,
    state_dir: &std::path::Path,
    start_hidden: bool,
) -> Result<instance::LaunchOutcome, String> {
    // Keep failed requests separate so they can display their recovery window.
    let workspace = storage::normalize_workspace_path(requested_workspace)
        .unwrap_or_else(|_| requested_workspace.into());
    instance::claim(&workspace, state_dir, start_hidden)
}

fn main() -> eframe::Result {
    let (data_dir, start_hidden) = match parse_launch_args(env::args_os().skip(1)) {
        Ok(LaunchAction::Run {
            data_dir,
            start_hidden,
        }) => (data_dir, start_hidden),
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
    let (settings, warning) = match storage::load_settings() {
        Ok(settings) => (settings, None),
        Err(error) => (storage::Settings::default(), Some(error)),
    };
    let workspace_override =
        select_workspace_override(data_dir, env::var_os("MINIMALIST_LIST_WORKSPACE"));

    let requested_workspace = workspace_override
        .as_deref()
        .unwrap_or(&settings.workspace_path);
    let state_path = storage::window_state_path();
    let state_dir = state_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let instance = match claim_launch_instance(requested_workspace, state_dir, start_hidden) {
        Ok(instance::LaunchOutcome::Existing) => return Ok(()),
        Ok(instance::LaunchOutcome::Primary(instance)) => instance,
        Err(error) => return Err(eframe::Error::AppCreation(error.into())),
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
    let tray_supported = x11rb::connect(None).is_ok() && tray::linux_host_available();
    #[cfg(not(target_os = "linux"))]
    let tray_supported = true;
    #[cfg(target_os = "linux")]
    let options = {
        let mut options = options;
        if tray_supported {
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
    run_native(
        "Minimalist List",
        options,
        Box::new(move |cc| {
            let mut app = MinimalistApp::new(&cc.egui_ctx, settings, workspace_override, warning);
            app.set_instance_guard(instance, &cc.egui_ctx);
            app.setup_tray(&cc.egui_ctx, tray_supported);
            #[cfg(target_os = "windows")]
            if let Some(window) = cc.winit_window() {
                tray::set_window_taskbar_icon(window);
            }
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            app.update_taskbar_presence(cc.winit_window().map(AsRef::as_ref));
            app.setup_global_shortcut(&cc.egui_ctx);
            if start_hidden {
                app.start_hidden(&cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
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
        let instance::LaunchOutcome::Primary(default_owner) =
            claim_launch_instance(default_workspace.to_str().unwrap(), &root, false).unwrap()
        else {
            panic!("expected a default workspace owner");
        };

        let instance::LaunchOutcome::Primary(recovery_owner) =
            claim_launch_instance(blocked.to_str().unwrap(), &root, false).unwrap()
        else {
            panic!("the failed request must reach its recovery UI");
        };
        assert!(!default_owner.take_show_request());
        assert!(matches!(
            claim_launch_instance(blocked.to_str().unwrap(), &root, false).unwrap(),
            instance::LaunchOutcome::Existing
        ));
        assert!(recovery_owner.take_show_request());
        assert!(!default_owner.take_show_request());

        assert!(matches!(
            claim_launch_instance(default_workspace.to_str().unwrap(), &root, false).unwrap(),
            instance::LaunchOutcome::Existing
        ));
        assert!(default_owner.take_show_request());
        assert!(!recovery_owner.take_show_request());
        drop(recovery_owner);
        drop(default_owner);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_data_dir_argument() {
        let action = parse_launch_args([OsString::from("--data-dir"), OsString::from("/cli")]);
        assert_eq!(
            action,
            Ok(LaunchAction::Run {
                data_dir: Some("/cli".to_owned()),
                start_hidden: false,
            })
        );
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
            })
        );
        assert_eq!(
            parse_launch_args([]),
            Ok(LaunchAction::Run {
                data_dir: None,
                start_hidden: false,
            })
        );
    }

    #[test]
    fn cli_data_dir_precedes_environment() {
        let selected =
            select_workspace_override(Some("/cli".to_owned()), Some(OsString::from("/env")));
        assert_eq!(selected.as_deref(), Some("/cli"));
    }

    #[test]
    fn environment_precedes_saved_settings_when_cli_is_absent() {
        let selected = select_workspace_override(None, Some(OsString::from("/env")));
        assert_eq!(selected.as_deref(), Some("/env"));
    }
}
