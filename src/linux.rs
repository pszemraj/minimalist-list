//! Applies desktop X11 cursor settings before the first native window is created.

use crate::linux_cursor::parse_xsettings;
use std::ffi::CString;
use winit::{
    event_loop::EventLoop,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
};
use x11rb::protocol::xproto::ConnectionExt;

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
