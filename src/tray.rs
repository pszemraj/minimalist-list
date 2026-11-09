//! Native tray icon, menu events, and Linux tray-host detection.

use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
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

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::linux_host_available;

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
    /// Requires a confirmed reopen path before hiding a visible window.
    ///
    /// # Returns
    /// Whether the tray can reopen the window.
    pub(crate) fn can_hide(self) -> bool {
        self == Self::Available
    }
}

/// Keeps the native icon alive and receives its menu actions.
pub struct Tray {
    #[cfg(not(target_os = "linux"))]
    _icon: TrayIcon,
    actions: Receiver<Action>,
    #[cfg(target_os = "linux")]
    monitor: linux::Monitor,
}

impl Tray {
    /// Creates the icon and forwards native actions to egui's event loop.
    ///
    /// # Arguments
    ///
    /// - `ctx` - Context to wake when a tray action arrives.
    ///
    /// # Returns
    ///
    /// The icon and its action receiver.
    ///
    /// # Errors
    ///
    /// Returns an error if native tray support is unavailable.
    pub fn new(ctx: &egui::Context) -> Result<Self, String> {
        let (sender, actions) = mpsc::channel();
        install_handlers(ctx, sender);
        Ok(Self {
            #[cfg(not(target_os = "linux"))]
            _icon: build_icon()?,
            actions,
            #[cfg(target_os = "linux")]
            monitor: linux::Monitor::start(ctx.clone())?,
        })
    }

    /// Waits briefly for the first background result before starting hidden.
    #[cfg(target_os = "linux")]
    pub(crate) fn wait_for_startup(&self) {
        self.monitor.wait_for_startup();
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
        return self.monitor.availability();
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

const TRAY_ID: &str = "minimalist-list";
const MENU_ACTIONS: [(Action, &str, &str); 3] = [
    (Action::Show, "minimalist-show", "Show Minimalist List"),
    (Action::QuickAdd, "minimalist-quick-add", "Quick add"),
    (Action::Quit, "minimalist-quit", "Quit"),
];

fn install_handlers(ctx: &egui::Context, sender: Sender<Action>) {
    let menu_sender = sender.clone();
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some((action, _, _)) = MENU_ACTIONS.iter().find(|(_, id, _)| event.id.0 == *id) {
            let _ = menu_sender.send(*action);
            menu_ctx.request_repaint();
        }
    }));
    let tray_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(event, TrayIconEvent::Click {
            id, button: MouseButton::Left, button_state: MouseButtonState::Up, ..
        } if id.as_ref() == TRAY_ID)
        {
            let _ = sender.send(Action::Show);
            tray_ctx.request_repaint();
        }
    }));
}

fn build_icon() -> Result<TrayIcon, String> {
    let menu = Menu::new();
    for (_, id, label) in MENU_ACTIONS {
        menu.append(&MenuItem::with_id(id, label, true, None))
            .map_err(|error| error.to_string())?;
    }
    let icon = icon_data();
    let icon =
        Icon::from_rgba(icon.rgba, icon.width, icon.height).map_err(|error| error.to_string())?;
    let builder = TrayIconBuilder::new()
        .with_id(TRAY_ID)
        .with_tooltip("Minimalist List")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false);
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon);
    builder.build().map_err(|error| error.to_string())
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
