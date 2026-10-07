//! Native application entry point and window configuration.

mod app;
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
    "  -h, --help             Print help\n",
    "  -V, --version          Print version\n\n",
    "Workspace precedence: --data-dir, MINIMALIST_LIST_WORKSPACE, saved setting, XDG default.\n",
);

#[derive(Debug, PartialEq, Eq)]
enum LaunchAction {
    Run { data_dir: Option<String> },
    Help,
    Version,
}

fn parse_launch_args(args: impl IntoIterator<Item = OsString>) -> Result<LaunchAction, String> {
    let mut args = args.into_iter();
    let mut data_dir = None;

    while let Some(argument) = args.next() {
        let text = argument.to_string_lossy();
        match text.as_ref() {
            "-h" | "--help" => return Ok(LaunchAction::Help),
            "-V" | "--version" => return Ok(LaunchAction::Version),
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

    Ok(LaunchAction::Run { data_dir })
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

fn main() -> eframe::Result {
    let data_dir = match parse_launch_args(env::args_os().skip(1)) {
        Ok(LaunchAction::Run { data_dir }) => data_dir,
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

    let level = if settings.always_on_top {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("minimalist-list")
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([320.0, 280.0])
            .with_transparent(true)
            .with_decorations(settings.window_decorations)
            .with_window_level(level),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    // Winit cannot hide or restore native Wayland windows. Use XWayland when available.
    #[cfg(target_os = "linux")]
    let tray_supported = x11rb::connect(None).is_ok();
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
    eframe::run_native(
        "Minimalist List",
        options,
        Box::new(move |cc| {
            let mut app = MinimalistApp::new(&cc.egui_ctx, settings, workspace_override, warning);
            app.setup_tray(&cc.egui_ctx, tray_supported);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_data_dir_argument() {
        let action = parse_launch_args([OsString::from("--data-dir"), OsString::from("/cli")]);
        assert_eq!(
            action,
            Ok(LaunchAction::Run {
                data_dir: Some("/cli".to_owned())
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
