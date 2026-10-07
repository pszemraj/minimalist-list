//! Native tray icon, menu events, and Linux tray-host detection.

use eframe::egui;
use std::sync::mpsc::{self, Receiver};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

/// Actions sent from the native tray to the application's event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    Quit,
}

/// Keeps the native icon alive and receives its menu actions.
pub struct Tray {
    _icon: TrayIcon,
    actions: Receiver<Action>,
    #[cfg(target_os = "linux")]
    host: zbus::blocking::Proxy<'static>,
}

impl Tray {
    /// Creates the icon and forwards native actions to egui's event loop.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context to wake when a tray action arrives.
    /// - `supports_hiding` - Whether the selected window backend supports hide and restore.
    ///
    /// # Returns
    ///
    /// The icon and its action receiver.
    ///
    /// # Errors
    ///
    /// Returns an error if native tray support is unavailable.
    pub fn new(ctx: &egui::Context, supports_hiding: bool) -> Result<Self, String> {
        if !supports_hiding {
            return Err("Hiding to the tray requires X11 or XWayland on Linux.".into());
        }
        #[cfg(target_os = "linux")]
        let host = linux_host()?;
        let menu = Menu::new();
        let show = MenuItem::new("Show Minimalist List", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[&show, &quit])
            .map_err(|error| error.to_string())?;
        let icon = Icon::from_rgba(icon_rgba(), 32, 32).map_err(|error| error.to_string())?;
        let builder = TrayIconBuilder::new()
            .with_id("minimalist-list")
            .with_tooltip("Minimalist List")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false);
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icon);
        let icon = builder.build().map_err(|error| error.to_string())?;

        let (sender, actions) = mpsc::channel();
        let menu_sender = sender.clone();
        let menu_ctx = ctx.clone();
        let show_id = show.id().clone();
        let quit_id = quit.id().clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == show_id {
                Action::Show
            } else if event.id == quit_id {
                Action::Quit
            } else {
                return;
            };
            let _ = menu_sender.send(action);
            menu_ctx.request_repaint();
        }));
        let tray_ctx = ctx.clone();
        let tray_id = icon.id().clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(event, TrayIconEvent::Click {
                id, button: MouseButton::Left, button_state: MouseButtonState::Up, ..
            } if id == tray_id)
            {
                let _ = sender.send(Action::Show);
                tray_ctx.request_repaint();
            }
        }));
        Ok(Self {
            _icon: icon,
            actions,
            #[cfg(target_os = "linux")]
            host,
        })
    }

    /// Returns the next pending native action, if any.
    ///
    /// # Returns
    ///
    /// A Show or Quit action from the icon or its menu.
    pub fn next_action(&self) -> Option<Action> {
        self.actions.try_recv().ok()
    }

    /// Checks whether a native tray host is still available.
    ///
    /// # Returns
    ///
    /// Whether the hidden window can be reopened through the tray.
    pub fn available(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self
            .host
            .get_property::<bool>("IsStatusNotifierHostRegistered")
            .unwrap_or(false);
        #[cfg(not(target_os = "linux"))]
        true
    }
}

#[cfg(target_os = "linux")]
fn linux_host() -> Result<zbus::blocking::Proxy<'static>, String> {
    let connection = zbus::blocking::connection::Builder::session()
        .and_then(|builder| {
            builder
                .method_timeout(std::time::Duration::from_secs(1))
                .build()
        })
        .map_err(|error| error.to_string())?;
    let host: zbus::blocking::Proxy<'static> = zbus::blocking::proxy::Builder::new(&connection)
        .destination("org.kde.StatusNotifierWatcher")
        .and_then(|builder| builder.path("/StatusNotifierWatcher"))
        .and_then(|builder| builder.interface("org.kde.StatusNotifierWatcher"))
        .and_then(|builder| {
            builder
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
        })
        .map_err(|error| error.to_string())?;
    if !host
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .unwrap_or(false)
    {
        return Err(
            "No desktop tray host is available. Close and minimize use their normal behavior."
                .into(),
        );
    }
    Ok(host)
}

fn icon_rgba() -> Vec<u8> {
    let mut rgba = vec![0; 32 * 32 * 4];
    // Three checklist rows remain recognizable on both light and dark panels.
    for y in 0..32 {
        for x in 0..32 {
            let row = [7, 15, 23]
                .iter()
                .any(|start| y >= *start && y < *start + 3);
            if row && ((5..8).contains(&x) || (12..27).contains(&x)) {
                rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&[70, 175, 145, 255]);
            }
        }
    }
    rgba
}
