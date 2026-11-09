//! Background Linux tray installation and registration checks.

use super::{Availability, TRAY_ID, build_icon};
use eframe::egui;
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use tray_icon::TrayIcon;
use zbus::{
    blocking::{Connection, Proxy, fdo::DBusProxy},
    names::OwnedBusName,
};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const CHECK_INTERVAL: Duration = Duration::from_secs(3);
const LOSS_GRACE: Duration = Duration::from_secs(5);

/// Publishes Linux tray reachability without blocking the UI on native calls.
pub(super) struct Monitor {
    availability: Arc<Mutex<Availability>>,
    ready: Receiver<()>,
    // Disconnecting wakes the worker. Never join an OS call that may be stuck.
    _stop: Sender<()>,
}

impl Monitor {
    /// Starts the background worker and wakes the event loop on reachability changes.
    ///
    /// # Returns
    /// The monitor for this process's tray icon.
    ///
    /// # Errors
    /// Returns an error if the worker thread cannot start.
    pub(super) fn start(ctx: egui::Context) -> Result<Self, String> {
        let availability = Arc::new(Mutex::new(Availability::Unavailable));
        let state = availability.clone();
        let (stop, stopped) = mpsc::channel();
        let (initialized, ready) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("linux-tray".into())
            .spawn(move || monitor(ctx, state, initialized, stopped))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            availability,
            ready,
            _stop: stop,
        })
    }

    /// Gives the first observation a bounded chance to arrive before hidden startup.
    pub(super) fn wait_for_startup(&self) {
        let _ = self.ready.recv_timeout(Duration::from_secs(1));
    }

    /// Returns the latest reachability decision without calling D-Bus.
    ///
    /// # Returns
    /// The cached decision, initially unavailable until registration is verified.
    pub(super) fn availability(&self) -> Availability {
        self.availability
            .lock()
            .map_or(Availability::Unavailable, |availability| *availability)
    }
}

fn monitor(
    ctx: egui::Context,
    availability: Arc<Mutex<Availability>>,
    initialized: SyncSender<()>,
    stopped: Receiver<()>,
) {
    let mut watcher = None;
    let mut icon = None;
    let mut item = None;
    let mut reachability = Reachability::default();
    let mut initialized = Some(initialized);
    loop {
        if watcher
            .as_ref()
            .is_none_or(|watcher: &Proxy<'_>| watcher.connection().is_closed())
        {
            watcher = watcher_proxy().ok();
        }
        let result = watcher.as_ref().map_or(Ok(false), |watcher| {
            registered(watcher, &mut icon, &mut item)
        });
        let registered = matches!(result, Ok(true));
        let inactive = !registered
            && reachability.availability != Availability::Unavailable
            && watcher
                .as_ref()
                .is_some_and(|watcher| session_inactive(watcher.connection()));
        let previous = reachability.availability;
        let current = reachability.observe(registered, inactive, Instant::now());
        if previous != current {
            log::debug!(
                "Linux tray reachability {previous:?} -> {current:?} (registered: {result:?}, session inactive: {inactive})"
            );
            *availability.lock().unwrap() = current;
            ctx.request_repaint();
        }
        if let Some(ready) = initialized.take() {
            let _ = ready.send(());
        }
        // Only replace a dead icon after publishing that it cannot reopen the window.
        if current == Availability::Unavailable
            && let Some(watcher) = &watcher
            && let Some(name) = &item
            && DBusProxy::new(watcher.connection())
                .and_then(|bus| Ok(bus.name_has_owner(name.clone().into_inner())?))
                == Ok(false)
        {
            icon = None;
            item = None;
        }
        if !matches!(
            stopped.recv_timeout(CHECK_INTERVAL),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            break;
        }
    }
}

fn registered(
    watcher: &Proxy<'_>,
    icon: &mut Option<TrayIcon>,
    item: &mut Option<OwnedBusName>,
) -> Result<bool, String> {
    if !watcher
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .map_err(|error| error.to_string())?
    {
        return Ok(false);
    }
    if icon.is_none() {
        *icon = Some(build_icon()?);
    }
    let bus = DBusProxy::new(watcher.connection()).map_err(|error| error.to_string())?;
    if item.is_none() {
        // The ksni backend exports a well-known name for each icon in this process.
        let prefix = format!("org.kde.StatusNotifierItem-{}-", std::process::id());
        *item = bus
            .list_names()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|name| name.as_str().starts_with(&prefix));
    }
    let Some(name) = item else {
        return Ok(false);
    };
    let registered_items = || watcher.get_property::<Vec<String>>("RegisteredStatusNotifierItems");
    let mut entries = registered_items().map_err(|error| error.to_string())?;
    // Some watchers report the unique connection name rather than the registration name.
    // Requiring a live owner also rejects stale entries left by a disconnected item.
    let owner = bus
        .get_name_owner(name.clone().into_inner())
        .map_err(|error| error.to_string())?;
    let contains_item =
        |entries: &[String]| has_item(entries, name.as_str()) || has_item(entries, owner.as_str());
    if !contains_item(&entries) {
        // ksni re-registers on owner changes. If it failed or the host forgot
        // the item without changing owners, register the existing export again.
        watcher
            .call_method("RegisterStatusNotifierItem", &(name.as_str(),))
            .map_err(|error| error.to_string())?;
        entries = registered_items().map_err(|error| error.to_string())?;
    }
    if !contains_item(&entries) {
        return Ok(false);
    }
    // Check that the exported object also responds. A watcher can hold an
    // entry even while the item is blocked servicing an earlier request.
    let id = property(
        watcher.connection(),
        owner.as_str(),
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "Id",
    )
    .and_then(|value| Ok(String::try_from(value)?))
    .map_err(|error| error.to_string())?;
    Ok(id == TRAY_ID)
}

fn has_item(entries: &[String], name: &str) -> bool {
    entries
        .iter()
        .any(|entry| entry.split(['/', '@']).next() == Some(name))
}

/// Checks for a tray host when choosing between X11 and native Wayland.
///
/// # Returns
/// Whether the watcher currently reports a registered host.
pub(crate) fn linux_host_available() -> bool {
    watcher_proxy()
        .and_then(|watcher| watcher.get_property::<bool>("IsStatusNotifierHostRegistered"))
        .unwrap_or(false)
}

fn watcher_proxy() -> zbus::Result<Proxy<'static>> {
    let connection = zbus::blocking::connection::Builder::session()?
        .method_timeout(Duration::from_secs(1))
        .build()?;
    zbus::blocking::proxy::Builder::new(&connection)
        .destination(WATCHER)?
        .path("/StatusNotifierWatcher")?
        .interface(WATCHER)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

fn session_inactive(bus: &Connection) -> bool {
    let power = property(
        bus,
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
        "PowerSaveMode",
    );
    if power
        .ok()
        .and_then(|value| i32::try_from(value).ok())
        .is_some_and(|mode| matches!(mode, 1..=3))
    {
        return true;
    }
    bus.call_method(
        Some("org.gnome.ScreenSaver"),
        "/org/gnome/ScreenSaver",
        Some("org.gnome.ScreenSaver"),
        "GetActive",
        &(),
    )
    .and_then(|reply| reply.body().deserialize::<bool>())
    .unwrap_or(false)
}

fn property(
    bus: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> zbus::Result<zbus::zvariant::OwnedValue> {
    bus.call_method(
        Some(destination),
        path,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(interface, name),
    )?
    .body()
    .deserialize()
}

struct Reachability {
    availability: Availability,
    missing_since: Option<Instant>,
}

impl Default for Reachability {
    fn default() -> Self {
        Self {
            availability: Availability::Unavailable,
            missing_since: None,
        }
    }
}

impl Reachability {
    fn observe(&mut self, registered: bool, inactive: bool, now: Instant) -> Availability {
        if registered {
            self.missing_since = None;
            self.availability = Availability::Available;
        } else if self.availability != Availability::Unavailable {
            if inactive {
                self.missing_since = None;
                self.availability = Availability::Unknown;
            } else if now.duration_since(*self.missing_since.get_or_insert(now)) >= LOSS_GRACE {
                self.availability = Availability::Unavailable;
            } else {
                self.availability = Availability::Unknown;
            }
        }
        self.availability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachability_needs_registration_and_bounds_loss_only_while_active() {
        use Availability::*;
        let mut reachability = Reachability::default();
        let start = Instant::now();
        let mut sample = |registered, inactive, ticks| {
            reachability.observe(registered, inactive, start + LOSS_GRACE * ticks)
        };
        assert_eq!(sample(false, false, 0), Unavailable);
        assert_eq!(sample(true, false, 1), Available);
        assert_eq!(sample(false, false, 2), Unknown);
        assert_eq!(sample(false, true, 3), Unknown);
        assert_eq!(sample(false, true, 20), Unknown);
        assert_eq!(sample(false, false, 21), Unknown);
        assert_eq!(sample(false, false, 22), Unavailable);
        assert_eq!(sample(false, false, 23), Unavailable);
        assert_eq!(sample(true, false, 24), Available);
    }

    #[test]
    fn registered_entries_match_the_exact_item_not_other_indicators() {
        let item = "org.kde.StatusNotifierItem-100-1";
        assert!(has_item(&[item.into()], item));
        assert!(has_item(&[format!("{item}/StatusNotifierItem")], item));
        assert!(has_item(&[":1.12@/StatusNotifierItem".into()], ":1.12"));
        assert!(!has_item(&[format!("{item}0/StatusNotifierItem")], item));
    }
}
