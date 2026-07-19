//! Native application entry point and window configuration.

mod app;
mod model;
mod storage;

use app::MinimalistApp;
use eframe::egui;
use egui::viewport::WindowLevel;
use std::env;

fn main() -> eframe::Result {
    let (settings, warning) = match storage::load_settings() {
        Ok(settings) => (settings, None),
        Err(error) => (storage::Settings::default(), Some(error)),
    };
    let workspace_override = env::var_os("MINIMALIST_LIST_WORKSPACE")
        .filter(|path| !path.is_empty())
        .map(|path| path.to_string_lossy().into_owned());

    let level = if settings.always_on_top {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("minimalist-list")
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([380.0, 460.0])
            .with_decorations(settings.window_decorations)
            .with_window_level(level),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Minimalist List",
        options,
        Box::new(move |cc| {
            Ok(Box::new(MinimalistApp::new(
                cc,
                settings,
                workspace_override,
                warning,
            )))
        }),
    )
}
