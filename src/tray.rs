//! Native tray icon, menu events, and Linux tray-host detection.

use eframe::egui;
use std::sync::mpsc::{self, Receiver};
#[cfg(target_os = "linux")]
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

/// Actions sent from the native tray to the application's event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    QuickAdd,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Result of checking whether an existing native tray can still reopen the window.
pub(crate) enum Availability {
    Available,
    Unavailable,
    /// The check was inconclusive or an absence is still awaiting confirmation.
    #[cfg(any(target_os = "linux", target_os = "windows", test))]
    Unknown,
}

impl Availability {
    /// Keeps an established tray's hiding behavior during an inconclusive check.
    ///
    /// # Returns
    /// Whether hiding remains possible; applications without a tray use `Unavailable`.
    pub(crate) fn can_hide(self) -> bool {
        self != Self::Unavailable
    }
}

/// Keeps the native icon alive and receives its menu actions.
pub struct Tray {
    _icon: TrayIcon,
    actions: Receiver<Action>,
    #[cfg(target_os = "linux")]
    host: zbus::blocking::Proxy<'static>,
    #[cfg(target_os = "linux")]
    host_loss: Cell<LinuxHostLoss>,
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
        let quick_add = MenuItem::new("Quick add", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[&show, &quick_add, &quit])
            .map_err(|error| error.to_string())?;
        let icon = icon_data();
        let icon = Icon::from_rgba(icon.rgba, icon.width, icon.height)
            .map_err(|error| error.to_string())?;
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
        let quick_add_id = quick_add.id().clone();
        let quit_id = quit.id().clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == show_id {
                Action::Show
            } else if event.id == quick_add_id {
                Action::QuickAdd
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
            #[cfg(target_os = "linux")]
            host_loss: Cell::new(LinuxHostLoss::Clear),
        })
    }

    /// Returns the next pending native action, if any.
    ///
    /// # Returns
    ///
    /// A Show, Quick add, or Quit action from the icon or its menu.
    pub fn next_action(&self) -> Option<Action> {
        self.actions.try_recv().ok()
    }

    /// Checks whether a native tray host is still available.
    ///
    /// # Returns
    ///
    /// Whether the host is present, absent, or temporarily could not be checked.
    pub(crate) fn availability(&self) -> Availability {
        #[cfg(target_os = "linux")]
        {
            let result = self
                .host
                .get_property::<bool>("IsStatusNotifierHostRegistered");
            let closed = self.host.connection().is_closed();
            let availability = linux_availability(&result, closed);
            let displays_asleep = availability == Availability::Unavailable
                && !closed
                && gnome_displays_asleep(self.host.connection());
            let previous = self.host_loss.get();
            let confirmed = confirm_linux_availability(
                availability,
                displays_asleep,
                &self.host_loss,
                Instant::now(),
            );
            let current = self.host_loss.get();
            if current != previous {
                match current {
                    LinuxHostLoss::Clear => {
                        log::debug!("Linux tray loss check cleared ({availability:?})")
                    }
                    LinuxHostLoss::DisplaysAsleep => log::debug!(
                        "Linux tray loss deferred while displays are asleep (reply: {result:?})"
                    ),
                    LinuxHostLoss::Since(_) => log::debug!(
                        "Linux tray watcher unavailable; waiting for confirmation (connection closed: {closed}, reply: {result:?})"
                    ),
                }
            }
            confirmed
        }
        #[cfg(target_os = "windows")]
        {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn FindWindowA(
                    class_name: *const std::ffi::c_char,
                    window_name: *const std::ffi::c_char,
                ) -> *mut std::ffi::c_void;
            }
            // A live icon object can outlast Explorer or precede its startup.
            if unsafe { FindWindowA(c"Shell_TrayWnd".as_ptr(), std::ptr::null()) }.is_null() {
                Availability::Unavailable
            } else {
                Availability::Available
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        Availability::Available
    }
}

/// Distinguishes a missing Linux host from a failed availability query.
///
/// A closed connection or an explicit D-Bus service/owner/disconnection error
/// confirms that the existing tray cannot be reached. Timeouts and other
/// inconclusive property-read errors retain the established hiding behavior.
///
/// # Arguments
/// - `result` - Native host property's reply or D-Bus error.
/// - `connection_closed` - Whether the session-bus connection has closed.
///
/// # Returns
/// The state reported by this probe, or `Unknown` for an inconclusive query.
#[cfg(target_os = "linux")]
fn linux_availability(result: &Result<bool, zbus::Error>, connection_closed: bool) -> Availability {
    if connection_closed {
        return Availability::Unavailable;
    }
    match result {
        Ok(true) => Availability::Available,
        Ok(false) => Availability::Unavailable,
        Err(error) => match zbus::fdo::Error::from(error.clone()) {
            zbus::fdo::Error::ServiceUnknown(_)
            | zbus::fdo::Error::NameHasNoOwner(_)
            | zbus::fdo::Error::Disconnected(_) => Availability::Unavailable,
            _ => Availability::Unknown,
        },
    }
}

#[cfg(target_os = "linux")]
const HOST_LOSS_GRACE: Duration = Duration::from_secs(5);

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxHostLoss {
    Clear,
    DisplaysAsleep,
    Since(Instant),
}

/// Reads GNOME's display power state without changing it.
///
/// Unknown state and desktops without this interface keep the normal tray fallback.
#[cfg(target_os = "linux")]
fn gnome_displays_asleep(bus: &zbus::blocking::Connection) -> bool {
    const DISPLAY_CONFIG: &str = "org.gnome.Mutter.DisplayConfig";
    bus.call_method(
        Some(DISPLAY_CONFIG),
        "/org/gnome/Mutter/DisplayConfig",
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(DISPLAY_CONFIG, "PowerSaveMode"),
    )
    .and_then(|reply| reply.body().deserialize::<zbus::zvariant::OwnedValue>())
    .ok()
    .and_then(|value| i32::try_from(value).ok())
    .is_some_and(|mode| matches!(mode, 1..=3))
}

#[cfg(target_os = "linux")]
fn confirm_linux_availability(
    availability: Availability,
    displays_asleep: bool,
    loss: &Cell<LinuxHostLoss>,
    now: Instant,
) -> Availability {
    if availability != Availability::Unavailable {
        loss.set(LinuxHostLoss::Clear);
        return availability;
    }
    if displays_asleep {
        loss.set(LinuxHostLoss::DisplaysAsleep);
        return Availability::Unknown;
    }
    // GNOME can briefly release the watcher name while changing session modes.
    // After the displays wake, give the watcher a fresh chance to return.
    let LinuxHostLoss::Since(first_missing) = loss.get() else {
        loss.set(LinuxHostLoss::Since(now));
        return Availability::Unknown;
    };
    if now.duration_since(first_missing) < HOST_LOSS_GRACE {
        return Availability::Unknown;
    }
    Availability::Unavailable
}

/// Reports whether Linux has a registered desktop tray host.
///
/// # Returns
///
/// Whether the desktop exposes a usable StatusNotifier host.
#[cfg(target_os = "linux")]
pub(crate) fn linux_host_available() -> bool {
    linux_host().is_ok()
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

/// Creates the template-friendly checklist icon used by the native tray.
///
/// # Returns
///
/// The transparent 32-by-32 RGBA icon.
pub(crate) fn icon_data() -> egui::IconData {
    egui::IconData {
        rgba: icon_rgba(),
        width: 32,
        height: 32,
    }
}

/// Loads the full-color application icon used by native windows and the taskbar.
///
/// # Returns
/// The bundled RGBA application icon.
///
/// # Panics
/// Panics if the bundled application PNG is invalid.
pub(crate) fn window_icon_data() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/minimalist-list.png"))
        .expect("invalid bundled application icon")
}

/// Sets Windows' separate large taskbar icon as well as the viewport's small icon.
///
/// # Arguments
/// - `window` - Root window belonging to this application instance.
///
/// # Panics
/// Panics if the bundled image is invalid or cannot form a native icon.
#[cfg(target_os = "windows")]
pub(crate) fn set_window_taskbar_icon(window: &winit::window::Window) {
    use winit::platform::windows::WindowExtWindows;
    let data = window_icon_data();
    let icon = winit::window::Icon::from_rgba(data.rgba, data.width, data.height)
        .expect("invalid bundled application icon");
    window.set_taskbar_icon(Some(icon));
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

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_host_checks_distinguish_absence_from_transient_failures() {
        use super::{Availability, linux_availability};
        use zbus::fdo::Error;

        assert_eq!(
            linux_availability(&Ok(true), false),
            Availability::Available
        );
        assert_eq!(
            linux_availability(&Ok(false), false),
            Availability::Unavailable
        );
        for error in [
            Error::ServiceUnknown("watcher is not registered".into()),
            Error::NameHasNoOwner("watcher has no owner".into()),
            Error::Disconnected("bus disconnected".into()),
        ] {
            assert_eq!(
                linux_availability(&Err(zbus::Error::FDO(Box::new(error))), false),
                Availability::Unavailable
            );
        }
        for error in [
            Error::NoReply("temporary timeout".into()),
            Error::Timeout("temporary timeout".into()),
            Error::TimedOut("temporary timeout".into()),
            Error::Failed("temporary property failure".into()),
        ] {
            assert_eq!(
                linux_availability(&Err(zbus::Error::FDO(Box::new(error))), false),
                Availability::Unknown
            );
        }
        assert_eq!(
            linux_availability(&Ok(true), true),
            Availability::Unavailable
        );
        assert_eq!(
            linux_availability(&Err(zbus::Error::Failure("connection lost".into())), true),
            Availability::Unavailable
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_tray_loss_requires_absence_on_separated_checks() {
        use super::{
            Availability::*, Cell, HOST_LOSS_GRACE, Instant, LinuxHostLoss,
            confirm_linux_availability,
        };

        for intervening in [Available, Unknown] {
            let start = Instant::now();
            let loss = Cell::new(LinuxHostLoss::Clear);
            let check =
                |reply, elapsed| confirm_linux_availability(reply, false, &loss, start + elapsed);
            assert_eq!(check(Unavailable, HOST_LOSS_GRACE / 2), Unknown);
            assert_eq!(check(Unavailable, HOST_LOSS_GRACE * 3 / 4), Unknown);
            assert_eq!(check(intervening, HOST_LOSS_GRACE), intervening);
            assert_eq!(check(Unavailable, HOST_LOSS_GRACE * 2), Unknown);
            assert_eq!(check(Unavailable, HOST_LOSS_GRACE * 3), Unavailable);
            assert_eq!(check(Available, HOST_LOSS_GRACE * 4), Available);
            assert_eq!(check(Unavailable, HOST_LOSS_GRACE * 5), Unknown);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_tray_loss_waits_for_display_wake_before_confirming_absence() {
        use super::{
            Availability::*, Cell, HOST_LOSS_GRACE, Instant, LinuxHostLoss,
            confirm_linux_availability,
        };

        let start = Instant::now();
        let loss = Cell::new(LinuxHostLoss::Clear);
        let check = |displays_asleep, intervals| {
            confirm_linux_availability(
                Unavailable,
                displays_asleep,
                &loss,
                start + HOST_LOSS_GRACE * intervals,
            )
        };
        assert_eq!(check(false, 0), Unknown);
        assert_eq!(check(true, 2), Unknown);
        assert_eq!(check(true, 12), Unknown);
        assert_eq!(check(false, 13), Unknown);
        assert_eq!(check(false, 14), Unavailable);
    }

    #[test]
    fn shell_and_window_icons_have_all_required_sizes_and_transparency() {
        let window = super::window_icon_data();
        assert_eq!((window.width, window.height), (256, 256));
        assert!(
            window
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] == 0)
        );
        let ico = include_bytes!("../assets/minimalist-list.ico");
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 7, 0]);
        let mut dimensions = Vec::new();
        for entry in ico[6..6 + 7 * 16].as_chunks::<16>().0 {
            let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            let image = eframe::icon_data::from_png_bytes(&ico[offset..offset + length]).unwrap();
            assert_eq!(image.width, image.height);
            dimensions.push(image.width);
        }
        assert_eq!(dimensions, [16, 24, 32, 48, 64, 128, 256]);
    }
}
