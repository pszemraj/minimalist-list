//! Native global reveal shortcut registration and event-loop wakeups.

use eframe::egui;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{HotKey, Modifiers},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Keeps the native registration alive on the application's event-loop thread.
pub(crate) struct GlobalShortcut {
    manager: GlobalHotKeyManager,
    current: Option<HotKey>,
    active_id: Arc<AtomicU32>,
    requested: Arc<AtomicBool>,
    pub(crate) attempted: Option<String>,
}

/// Parses an optional reveal chord without reserving ordinary typing or media keys.
///
/// # Returns
/// A hotkey, or `None` for an explicitly disabled shortcut.
///
/// # Errors
/// Returns an explanation for invalid or insufficiently modified shortcuts.
pub(crate) fn parse(raw: &str) -> Result<Option<HotKey>, String> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let key: HotKey = raw
        .trim()
        .parse()
        .map_err(|error| format!("Invalid shortcut: {error}"))?;
    let primary = key.mods.intersects(Modifiers::CONTROL | Modifiers::META);
    let secondary = key.mods.intersects(Modifiers::ALT | Modifiers::SHIFT)
        || key.mods.contains(Modifiers::CONTROL | Modifiers::META);
    let code = key.key.to_string();
    let ordinary = code.starts_with("Key")
        || code.starts_with("Digit")
        || code == "Space"
        || code
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
    if !primary || !secondary || !ordinary {
        return Err(
            "Use Ctrl or Cmd with another modifier and a letter, number, Space, or function key."
                .into(),
        );
    }
    Ok(Some(key))
}

/// Formats a configured chord with the current platform's modifier names.
///
/// # Returns
/// A readable chord, or Disabled for an empty binding.
pub(crate) fn label(raw: &str) -> String {
    if raw.trim().is_empty() {
        return "Disabled".into();
    }
    raw.replace(
        "CommandOrControl",
        if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        },
    )
    .replace("Control", "Ctrl")
    .replace("Command", "Cmd")
    .replace(
        "Alt",
        if cfg!(target_os = "macos") {
            "Option"
        } else {
            "Alt"
        },
    )
    .replace("Key", "")
    .replace("Digit", "")
}

impl GlobalShortcut {
    /// Creates the native manager and installs its event-loop wakeup callback.
    ///
    /// # Returns
    /// An unregistered manager tied to the initialized native context.
    ///
    /// # Errors
    /// Returns native initialization errors, including an unavailable X11 display.
    pub(crate) fn new(ctx: &egui::Context) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        let _connection = x11rb::connect(None)
            .map_err(|_| "Global shortcuts need an X11 display. On native Wayland, bind an app launch in desktop settings.".to_owned())?;
        let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string())?;
        let active_id = Arc::new(AtomicU32::new(0));
        let requested = Arc::new(AtomicBool::new(false));
        let event_id = active_id.clone();
        let event_requested = requested.clone();
        let context = ctx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if accepts_event(event_id.load(Ordering::Acquire), event.id, event.state) {
                event_requested.store(true, Ordering::Release);
                context.request_repaint();
            }
        }));
        Ok(Self {
            manager,
            current: None,
            active_id,
            requested,
            attempted: None,
        })
    }

    /// Replaces the registration, keeping the old shortcut if the new one fails.
    ///
    /// # Returns
    /// Success when the requested chord or disabled state is active.
    ///
    /// # Errors
    /// Returns parse, conflict, or native registration errors without persisting the choice.
    pub(crate) fn configure(&mut self, raw: &str) -> Result<(), String> {
        let next = parse(raw)?;
        if self.current == next {
            return Ok(());
        }
        if let Some(next) = next {
            self.manager.register(next).map_err(|error| {
                format!("Shortcut unavailable: {error}. Choose another combination.")
            })?;
        }
        if let Some(previous) = self.current
            && let Err(error) = self.manager.unregister(previous)
        {
            if let Some(next) = next {
                let _ = self.manager.unregister(next);
            }
            return Err(format!("Could not release the previous shortcut: {error}"));
        }
        self.active_id
            .store(next.map_or(0, |key| key.id), Ordering::Release);
        self.requested.store(false, Ordering::Release);
        self.current = next;
        Ok(())
    }

    /// Takes a pending request; holding a shortcut never hides the window.
    ///
    /// # Returns
    /// Whether a matching native press requested the window.
    pub(crate) fn take_show_request(&self) -> bool {
        self.requested.swap(false, Ordering::AcqRel)
    }
}

fn accepts_event(active: u32, id: u32, state: HotKeyState) -> bool {
    active != 0 && active == id && state == HotKeyState::Pressed
}

impl Drop for GlobalShortcut {
    fn drop(&mut self) {
        self.active_id.store(0, Ordering::Release);
        if let Some(key) = self.current {
            let _ = self.manager.unregister(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_chords_accept_configuration_and_reject_typing_or_media_capture() {
        assert!(parse("CommandOrControl+Alt+Shift+KeyL").unwrap().is_some());
        assert!(parse("Ctrl+Shift+Space").unwrap().is_some());
        assert!(parse(" ").unwrap().is_none());
        for invalid in [
            "KeyL",
            "Shift+KeyL",
            "Ctrl+KeyL",
            "Ctrl++L",
            "Ctrl+Shift+MediaPlayPause",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn only_a_matching_pressed_registration_requests_reveal() {
        assert!(accepts_event(42, 42, HotKeyState::Pressed));
        assert!(!accepts_event(42, 42, HotKeyState::Released));
        assert!(!accepts_event(42, 43, HotKeyState::Pressed));
        assert!(!accepts_event(0, 0, HotKeyState::Pressed));
        assert_eq!(label(""), "Disabled");
        assert!(!label("CommandOrControl+Alt+Shift+KeyL").contains("Key"));
    }
}
