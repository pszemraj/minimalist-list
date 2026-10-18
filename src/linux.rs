//! Linux display cursor preferences and X11 taskbar hints.

use crate::linux_cursor::parse_xsettings;
use std::ffi::{CStr, CString};
use winit::{
    event_loop::EventLoop,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, MapState, PropMode,
    },
    wrapper::ConnectionExt as _,
};

/// Sets the X11 taskbar hint before mapping or requests its update from the window manager.
///
/// Wayland is a no-op: its standard client protocols do not expose a taskbar exclusion hint.
///
/// # Arguments
/// - `window` - Live native window, used on its event-loop thread.
/// - `hidden` - Whether to exclude this window from the taskbar.
///
/// # Returns
/// Success when the hint has been submitted, or on Wayland.
///
/// # Errors
/// Returns native handle, display connection, or X11 protocol errors.
pub(crate) fn set_taskbar_hidden(
    window: &winit::window::Window,
    hidden: bool,
) -> Result<(), String> {
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let window_id = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).map_err(|e| e.to_string())?,
        RawWindowHandle::Wayland(_) => return Ok(()),
        _ => return Err("Unsupported Linux window handle".into()),
    };
    let handle = window.display_handle().map_err(|error| error.to_string())?;
    let RawDisplayHandle::Xlib(handle) = handle.as_raw() else {
        return Err("X11 window has no Xlib display handle".into());
    };
    let display = handle.display.ok_or("X11 display is unavailable")?;
    let xlib = x11_dl::xlib::Xlib::open().map_err(|error| error.to_string())?;
    // SAFETY: The live window owns this Xlib display, and XDisplayString returns
    // an immutable, NUL-terminated string that remains borrowed for this call.
    let name = unsafe { (xlib.XDisplayString)(display.as_ptr().cast()) };
    if name.is_null() {
        return Err("X11 display name is unavailable".into());
    }
    // SAFETY: XDisplayString above returned a checked, live C string.
    let name = unsafe { CStr::from_ptr(name) };
    let name = name.to_str().map_err(|error| error.to_string())?;
    update_taskbar_hint(name, window_id, hidden).map_err(|error| error.to_string())
}

fn update_taskbar_hint(
    display: &str,
    window: u32,
    hidden: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (connection, _) = x11rb::connect(Some(display))?;
    let state = connection
        .intern_atom(false, b"_NET_WM_STATE")?
        .reply()?
        .atom;
    let skip = connection
        .intern_atom(false, b"_NET_WM_STATE_SKIP_TASKBAR")?
        .reply()?
        .atom;
    let attributes = connection.get_window_attributes(window)?.reply()?;
    if attributes.map_state == MapState::UNMAPPED {
        let reply = connection
            .get_property(false, window, state, AtomEnum::ATOM, 0, 262_144)?
            .reply()?;
        if reply.bytes_after != 0 {
            return Err("X11 window state property is too large".into());
        }
        if reply.type_ != u32::from(AtomEnum::NONE) && reply.type_ != u32::from(AtomEnum::ATOM) {
            return Err("X11 window state property is not an atom list".into());
        }
        let current = if reply.type_ == u32::from(AtomEnum::NONE) {
            Vec::new()
        } else {
            reply
                .value32()
                .ok_or("X11 window state property is not a 32-bit atom list")?
                .collect()
        };
        let merged = crate::linux_taskbar::merge_state(&current, skip, hidden);
        connection
            .change_property32(PropMode::REPLACE, window, state, AtomEnum::ATOM, &merged)?
            .check()?;
    } else {
        let root = connection.get_geometry(window)?.reply()?.root;
        let event = ClientMessageEvent::new(
            32,
            window,
            state,
            crate::linux_taskbar::state_message(skip, hidden),
        );
        connection
            .send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
    }
    connection.flush()?;
    Ok(())
}

/// Runs eframe on its supported external event loop after configuring the X11 display.
///
/// # Arguments
/// - `app_name` - Native application name.
/// - `options` - Window, event-loop, renderer, and persistence options.
/// - `creator` - Application initialization callback.
///
/// # Returns
/// Success after the native application exits.
///
/// # Errors
/// Returns native event-loop initialization or execution errors.
pub(crate) fn run_native(
    app_name: &str,
    mut options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
) -> eframe::Result {
    let mut builder = EventLoop::<eframe::UserEvent>::with_user_event();
    if let Some(hook) = options.event_loop_builder.take() {
        hook(&mut builder);
    }
    let event_loop = builder.build()?;
    configure_cursor_display(&event_loop);
    let mut app = eframe::create_native(app_name, options, creator, &event_loop);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn configure_cursor_display(event_loop: &EventLoop<eframe::UserEvent>) {
    let Ok(handle) = event_loop.display_handle() else {
        return;
    };
    let RawDisplayHandle::Xlib(handle) = handle.as_raw() else {
        return;
    };
    let Some(display) = handle.display else {
        return;
    };
    let Some(data) = desktop_settings() else {
        return;
    };
    let Some(settings) = parse_xsettings(&data) else {
        return;
    };
    let (theme, size) = settings.overrides(
        std::env::var_os("XCURSOR_THEME").is_some(),
        std::env::var_os("XCURSOR_SIZE").is_some(),
    );
    let Ok(xcursor) = x11_dl::xcursor::Xcursor::open() else {
        return;
    };
    if let Some(theme) = theme.and_then(|theme| CString::new(theme).ok()) {
        // SAFETY: The display is borrowed from the live X11 event loop; this runs
        // on its owning thread before any window or cached cursor is created.
        unsafe {
            (xcursor.XcursorSetTheme)(display.as_ptr().cast(), theme.as_ptr());
        }
    }
    if let Some(size) = size {
        // SAFETY: Same live, owning-thread X11 display, with checked positive size.
        unsafe {
            (xcursor.XcursorSetDefaultSize)(display.as_ptr().cast(), size);
        }
    }
}

fn desktop_settings() -> Option<Vec<u8>> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let selection = connection
        .intern_atom(false, format!("_XSETTINGS_S{screen}").as_bytes())
        .ok()?
        .reply()
        .ok()?
        .atom;
    let owner = connection
        .get_selection_owner(selection)
        .ok()?
        .reply()
        .ok()?
        .owner;
    if owner == 0 {
        return None;
    }
    let property = connection
        .intern_atom(false, b"_XSETTINGS_SETTINGS")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let reply = connection
        .get_property(false, owner, property, property, 0, 262_144)
        .ok()?
        .reply()
        .ok()?;
    // An incomplete property must not apply only some of the desktop preferences.
    (reply.bytes_after == 0 && reply.format == 8).then_some(reply.value)
}
