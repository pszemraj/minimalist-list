//! Native global shortcut registration and event-loop wakeups.

use crate::storage::Settings;
use eframe::egui;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{HotKey, Modifiers},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Requests that a native global shortcut can make of the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlobalAction {
    /// Reveals and focuses the existing window.
    Show,
    /// Reveals the window and opens Quick add.
    QuickAdd,
}

impl GlobalAction {
    /// Every action, in Settings order.
    pub(crate) const ALL: [Self; 2] = [Self::Show, Self::QuickAdd];

    /// Names the action whose chord must differ from this one.
    ///
    /// # Returns
    /// The other action.
    pub(crate) const fn other(self) -> Self {
        match self {
            Self::Show => Self::QuickAdd,
            Self::QuickAdd => Self::Show,
        }
    }

    /// Names the action in Settings and status messages.
    ///
    /// # Returns
    /// The Settings card title.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Show => "Global shortcut",
            Self::QuickAdd => "Quick add shortcut",
        }
    }

    /// Reads the saved chord for this action.
    ///
    /// # Returns
    /// The configured chord; empty means disabled.
    pub(crate) fn configured(self, settings: &Settings) -> &str {
        match self {
            Self::Show => &settings.global_shortcut,
            Self::QuickAdd => &settings.quick_add_shortcut,
        }
    }

    /// Edits the saved chord for this action.
    ///
    /// # Returns
    /// The configured chord, for assignment.
    pub(crate) fn configured_mut(self, settings: &mut Settings) -> &mut String {
        match self {
            Self::Show => &mut settings.global_shortcut,
            Self::QuickAdd => &mut settings.quick_add_shortcut,
        }
    }
}

/// One value for each [`GlobalAction`], indexed by the action itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct PerAction<T>([T; 2]);

impl<T> PerAction<T> {
    /// Builds each action's value.
    ///
    /// # Returns
    /// The values in [`GlobalAction::ALL`] order.
    pub(crate) fn new(build: impl FnMut(GlobalAction) -> T) -> Self {
        Self(GlobalAction::ALL.map(build))
    }

    /// Reads one action's value.
    ///
    /// # Returns
    /// The value belonging to `action`.
    pub(crate) fn get(&self, action: GlobalAction) -> &T {
        let [show, quick_add] = &self.0;
        match action {
            GlobalAction::Show => show,
            GlobalAction::QuickAdd => quick_add,
        }
    }

    /// Edits one action's value.
    ///
    /// # Returns
    /// The value belonging to `action`, for assignment.
    pub(crate) fn get_mut(&mut self, action: GlobalAction) -> &mut T {
        let [show, quick_add] = &mut self.0;
        match action {
            GlobalAction::Show => show,
            GlobalAction::QuickAdd => quick_add,
        }
    }
}

impl<T> std::ops::Index<GlobalAction> for PerAction<T> {
    type Output = T;

    fn index(&self, action: GlobalAction) -> &T {
        self.get(action)
    }
}

impl<T> std::ops::IndexMut<GlobalAction> for PerAction<T> {
    fn index_mut(&mut self, action: GlobalAction) -> &mut T {
        self.get_mut(action)
    }
}

/// One action's active registration and pending native press.
struct Slot {
    current: Option<HotKey>,
    active_id: Arc<AtomicU32>,
    requested: Arc<AtomicBool>,
}

/// Keeps the native registrations alive on the application's event-loop thread.
pub(crate) struct GlobalShortcut {
    manager: GlobalHotKeyManager,
    slots: PerAction<Slot>,
    /// Last configured value tried for each action, so a failure is reported once.
    pub(crate) attempted: PerAction<Option<String>>,
}

/// Parses an optional global chord without reserving ordinary typing or media keys.
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

/// Parses a chord for one action and rejects the other action's configured chord.
///
/// # Arguments
/// - `action` - Action the chord would trigger.
/// - `raw` - Requested chord; empty disables the action.
/// - `settings` - Saved preferences naming the other action's chord.
///
/// # Returns
/// A hotkey, or `None` for an explicitly disabled shortcut.
///
/// # Errors
/// Returns an explanation for invalid, insufficiently modified, or duplicate shortcuts.
pub(crate) fn parse_for(
    action: GlobalAction,
    raw: &str,
    settings: &Settings,
) -> Result<Option<HotKey>, String> {
    let next = parse(raw)?;
    let other = action.other();
    if next.is_some() && next == parse(other.configured(settings)).ok().flatten() {
        return Err(conflict(other));
    }
    Ok(next)
}

fn conflict(owner: GlobalAction) -> String {
    format!(
        "Already used by the {}. Choose another combination.",
        owner.title().to_lowercase()
    )
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

/// Formats a saved chord for its editable field.
///
/// # Returns
/// A readable chord, or empty text for a disabled binding.
pub(crate) fn input_text(raw: &str) -> String {
    if raw.is_empty() {
        String::new()
    } else {
        label(raw)
    }
}

/// Formats every action's saved chord for its editable field.
///
/// # Returns
/// Field text for each action.
pub(crate) fn input_texts(settings: &Settings) -> PerAction<String> {
    PerAction::new(|action| input_text(action.configured(settings)))
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
        let slots = PerAction::new(|_| Slot {
            current: None,
            active_id: Arc::new(AtomicU32::new(0)),
            requested: Arc::new(AtomicBool::new(false)),
        });
        let shared = PerAction::new(|action| {
            let slot = slots.get(action);
            (slot.active_id.clone(), slot.requested.clone())
        });
        let context = ctx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            let active = PerAction::new(|action| shared.get(action).0.load(Ordering::Acquire));
            if let Some(action) = route(&active, event.id, event.state) {
                shared.get(action).1.store(true, Ordering::Release);
                context.request_repaint();
            }
        }));
        Ok(Self {
            manager,
            slots,
            attempted: PerAction::default(),
        })
    }

    /// Replaces one action's registration, keeping the old shortcut if the new one fails.
    ///
    /// # Arguments
    /// - `action` - Request the chord triggers.
    /// - `raw` - Configured chord; empty disables the action.
    ///
    /// # Returns
    /// Success when the requested chord or disabled state is active.
    ///
    /// # Errors
    /// Returns parse, conflict, or native registration errors without persisting the choice.
    pub(crate) fn configure(&mut self, action: GlobalAction, raw: &str) -> Result<(), String> {
        let next = parse(raw)?;
        if self.slots.get(action).current == next {
            return Ok(());
        }
        if next.is_some() && self.slots.get(action.other()).current == next {
            return Err(conflict(action.other()));
        }
        if let Some(next) = next {
            self.manager.register(next).map_err(|error| {
                format!("Shortcut unavailable: {error}. Choose another combination.")
            })?;
        }
        let slot = self.slots.get_mut(action);
        if let Some(previous) = slot.current
            && let Err(error) = self.manager.unregister(previous)
        {
            if let Some(next) = next {
                let _ = self.manager.unregister(next);
            }
            return Err(format!("Could not release the previous shortcut: {error}"));
        }
        slot.active_id
            .store(next.map_or(0, |key| key.id), Ordering::Release);
        slot.requested.store(false, Ordering::Release);
        slot.current = next;
        Ok(())
    }

    /// Takes a pending request; holding a shortcut never hides the window.
    ///
    /// # Returns
    /// Whether a matching native press requested this action.
    pub(crate) fn take_request(&self, action: GlobalAction) -> bool {
        self.slots
            .get(action)
            .requested
            .swap(false, Ordering::AcqRel)
    }
}

fn accepts_event(active: u32, id: u32, state: HotKeyState) -> bool {
    active != 0 && active == id && state == HotKeyState::Pressed
}

/// Finds the action whose active registration matches a pressed native hotkey.
fn route(active: &PerAction<u32>, id: u32, state: HotKeyState) -> Option<GlobalAction> {
    GlobalAction::ALL
        .into_iter()
        .find(|action| accepts_event(active[*action], id, state))
}

impl Drop for GlobalShortcut {
    fn drop(&mut self) {
        for action in GlobalAction::ALL {
            let slot = &self.slots[action];
            slot.active_id.store(0, Ordering::Release);
            if let Some(key) = slot.current {
                let _ = self.manager.unregister(key);
            }
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
    fn only_a_matching_pressed_registration_requests_its_action() {
        assert!(accepts_event(42, 42, HotKeyState::Pressed));
        assert!(!accepts_event(42, 42, HotKeyState::Released));
        assert!(!accepts_event(42, 43, HotKeyState::Pressed));
        assert!(!accepts_event(0, 0, HotKeyState::Pressed));
        let ids = PerAction([42, 7]);
        assert_eq!(
            route(&ids, 42, HotKeyState::Pressed),
            Some(GlobalAction::Show)
        );
        assert_eq!(
            route(&ids, 7, HotKeyState::Pressed),
            Some(GlobalAction::QuickAdd)
        );
        assert_eq!(route(&ids, 7, HotKeyState::Released), None);
        assert_eq!(route(&ids, 8, HotKeyState::Pressed), None);
        assert_eq!(route(&PerAction([42, 0]), 0, HotKeyState::Pressed), None);
        assert_eq!(label(""), "Disabled");
        assert!(!label("CommandOrControl+Alt+Shift+KeyL").contains("Key"));
    }

    #[test]
    fn default_chords_parse_and_differ_and_empty_disables_one_action() {
        let mut settings = Settings::default();
        assert_eq!(
            settings.quick_add_shortcut,
            "CommandOrControl+Alt+Shift+KeyN"
        );
        let show = parse_for(GlobalAction::Show, &settings.global_shortcut, &settings);
        let quick = parse_for(
            GlobalAction::QuickAdd,
            &settings.quick_add_shortcut,
            &settings,
        );
        assert!(show.clone().unwrap().is_some() && quick.clone().unwrap().is_some());
        assert_ne!(show.unwrap(), quick.unwrap());
        for action in GlobalAction::ALL {
            assert_eq!(action.other().other(), action);
            assert_eq!(PerAction::new(|each| each)[action], action);
        }
        settings.quick_add_shortcut.clear();
        let texts = input_texts(&settings);
        assert_eq!(texts[GlobalAction::QuickAdd], "");
        assert_eq!(texts[GlobalAction::Show], label(&settings.global_shortcut));
        assert!(
            parse_for(GlobalAction::QuickAdd, "", &settings)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_chord_equal_to_the_other_actions_is_rejected_however_it_is_spelled() {
        let settings = Settings::default();
        for spelled in [
            "CommandOrControl+Alt+Shift+KeyL",
            "CommandOrControl+Shift+Alt+KeyL",
            " CommandOrControl+Alt+Shift+KeyL ",
        ] {
            let error = parse_for(GlobalAction::QuickAdd, spelled, &settings).unwrap_err();
            assert!(error.contains("global shortcut"), "{spelled}: {error}");
        }
        let error =
            parse_for(GlobalAction::Show, &settings.quick_add_shortcut, &settings).unwrap_err();
        assert!(error.contains("quick add shortcut"), "{error}");
        assert!(parse_for(GlobalAction::QuickAdd, "Ctrl+Alt+Shift+KeyM", &settings).is_ok());
    }
}
