//! Native app-bundle reopen requests without replacing winit's application delegate.

use eframe::egui;
use objc2::encode::Encode;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::{MainThreadMarker, ffi, sel};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use std::cell::RefCell;
use std::ffi::CString;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

type ReopenCallback = unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication, Bool) -> Bool;

struct ReopenState {
    delegate: Retained<ProtocolObject<dyn NSApplicationDelegate>>,
    requested: AtomicBool,
    context: egui::Context,
    previous: Option<ReopenCallback>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Weak<ReopenState>>> = const { RefCell::new(None) };
}

unsafe extern "C-unwind" fn handle_reopen(
    delegate: &AnyObject,
    selector: Sel,
    application: &NSApplication,
    has_visible_windows: Bool,
) -> Bool {
    let state = ACTIVE.with(|active| active.borrow().as_ref().and_then(Weak::upgrade));
    let Some(state) = state.filter(|state| {
        std::ptr::eq(
            delegate,
            Retained::as_ptr(&state.delegate).cast::<AnyObject>(),
        )
    }) else {
        return Bool::YES;
    };
    state.requested.store(true, Ordering::Release);
    state.context.request_repaint();
    if let Some(previous) = state.previous {
        // SAFETY: This is the previous implementation of the same AppKit selector,
        // with its original receiver and exact BOOL/object argument signature.
        unsafe { previous(delegate, selector, application, has_visible_windows) };
    }
    // The app handles the request by revealing its existing eframe window.
    Bool::NO
}

fn reopen_implementation() -> Imp {
    // SAFETY: Objective-C erases method signatures to IMP. Both pointer types use
    // C-unwind; registration below supplies the callback's exact type encoding.
    unsafe { std::mem::transmute::<ReopenCallback, Imp>(handle_reopen) }
}

/// Retains reopen state while preserving the native event loop's delegate object.
pub(crate) struct ReopenHandler {
    state: Rc<ReopenState>,
    class: &'static AnyClass,
    previous: Option<Imp>,
}

impl ReopenHandler {
    /// Adds reopen handling after the native event loop has installed its delegate.
    ///
    /// Installation never activates the application or reveals a hidden startup
    /// window. Winit's delegate object, class, Rust ivars, and lifecycle methods
    /// remain intact; only the optional AppKit reopen selector is intercepted.
    ///
    /// # Arguments
    ///
    /// - `context` - Initialized eframe context to wake on an actual reopen request.
    ///
    /// # Returns
    ///
    /// A retained handler that must remain alive while the application runs.
    ///
    /// # Errors
    ///
    /// Returns an error outside the main thread, before native delegate setup, or
    /// when the runtime cannot register the reopen method.
    pub(crate) fn install(context: &egui::Context) -> Result<Self, String> {
        let marker =
            MainThreadMarker::new().ok_or("macOS reopen handler requires the main thread")?;
        let application = NSApplication::sharedApplication(marker);
        let delegate = application
            .delegate()
            .ok_or("macOS reopen handler requires an initialized native application delegate")?;
        if ACTIVE.with(|active| active.borrow().as_ref().and_then(Weak::upgrade).is_some()) {
            return Err("macOS reopen handler is already installed".into());
        }
        let class = AsRef::<AnyObject>::as_ref(&*delegate).class();
        let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
        let implementation = reopen_implementation();
        let previous = class
            .instance_method(selector)
            .map(|method| method.implementation())
            .filter(|previous| !std::ptr::fn_addr_eq(*previous, implementation));
        let encoding = CString::new(format!("{}@:@{}", Bool::ENCODING, Bool::ENCODING))
            .map_err(|error| error.to_string())?;
        // SAFETY: The main-thread delegate remains the same object and class. The
        // selector is NSApplicationDelegate's optional reopen method; the callback
        // and type encoding use its documented object/object/BOOL -> BOOL ABI.
        unsafe {
            if class.instance_method(selector).is_some() {
                ffi::class_replaceMethod(
                    class as *const AnyClass as *mut AnyClass,
                    selector,
                    implementation,
                    encoding.as_ptr(),
                );
            } else if !ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                selector,
                implementation,
                encoding.as_ptr(),
            )
            .as_bool()
            {
                return Err("macOS reopen handler could not register the reopen method".into());
            }
        }
        let state = Rc::new(ReopenState {
            delegate,
            requested: AtomicBool::new(false),
            context: context.clone(),
            // SAFETY: An existing implementation has the same selector signature.
            previous: previous
                .map(|previous| unsafe { std::mem::transmute::<Imp, ReopenCallback>(previous) }),
        });
        ACTIVE.with(|active| *active.borrow_mut() = Some(Rc::downgrade(&state)));
        Ok(Self {
            state,
            class,
            previous,
        })
    }

    /// Consumes pending bundle reopen requests, coalescing repeated activations.
    ///
    /// # Returns
    ///
    /// Whether Finder or LaunchServices requested the existing app window.
    pub(crate) fn take_show_request(&self) -> bool {
        self.state.requested.swap(false, Ordering::AcqRel)
    }
}

impl Drop for ReopenHandler {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            if active
                .borrow()
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|state| Rc::ptr_eq(&state, &self.state))
            {
                *active.borrow_mut() = None;
            }
        });
        let selector = sel!(applicationShouldHandleReopen:hasVisibleWindows:);
        if let Some(previous) = self.previous
            && let Some(method) = self.class.instance_method(selector)
            && std::ptr::fn_addr_eq(method.implementation(), reopen_implementation())
        {
            // SAFETY: Restore this selector's original implementation with its
            // unchanged ABI, but never overwrite a later replacement.
            unsafe { method.set_implementation(previous) };
        }
        // Objective-C has no method-removal API. An added callback remains valid
        // executable code and returns AppKit's default YES without active state.
    }
}
