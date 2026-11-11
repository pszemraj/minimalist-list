//! Native reopen and termination requests on winit's application delegate.

use crate::tray::Action;
use block2::RcBlock;
use eframe::egui;
use objc2::encode::Encode;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::{MainThreadMarker, ffi, sel};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};
use objc2_core_foundation::{CFRunLoop, kCFRunLoopCommonModes};
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::rc::{Rc, Weak};

type ReopenCallback = unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication, Bool) -> Bool;
type TerminateCallback =
    unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication) -> NSApplicationTerminateReply;

struct DelegateState {
    delegate: Retained<ProtocolObject<dyn NSApplicationDelegate>>,
    show_requested: Cell<bool>,
    quit_requested: Cell<bool>,
    context: egui::Context,
    previous: Option<ReopenCallback>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Weak<DelegateState>>> = const { RefCell::new(None) };
}

fn active_delegate(delegate: &AnyObject) -> Option<Rc<DelegateState>> {
    ACTIVE
        .with(|active| active.borrow().as_ref().and_then(Weak::upgrade))
        .filter(|state| {
            std::ptr::eq(
                delegate,
                Retained::as_ptr(&state.delegate).cast::<AnyObject>(),
            )
        })
}

unsafe extern "C-unwind" fn handle_reopen(
    delegate: &AnyObject,
    selector: Sel,
    application: &NSApplication,
    has_visible_windows: Bool,
) -> Bool {
    let Some(state) = active_delegate(delegate) else {
        return Bool::YES;
    };
    state.show_requested.set(true);
    state.context.request_repaint();
    if let Some(previous) = state.previous {
        // SAFETY: This is the previous implementation of the same AppKit selector,
        // with its original receiver and exact BOOL/object argument signature.
        unsafe { previous(delegate, selector, application, has_visible_windows) };
    }
    // The app handles the request by revealing its existing eframe window.
    Bool::NO
}

unsafe extern "C-unwind" fn handle_termination(
    delegate: &AnyObject,
    _selector: Sel,
    application: &NSApplication,
) -> NSApplicationTerminateReply {
    let Some(state) = active_delegate(delegate) else {
        return NSApplicationTerminateReply::TerminateNow;
    };
    // The native folder picker is modal inside winit's event handler. Waiting
    // for a save frame there would block both the picker and termination.
    if application.modalWindow().is_some() {
        return NSApplicationTerminateReply::TerminateCancel;
    }
    state.quit_requested.set(true);
    state.context.request_repaint();
    NSApplicationTerminateReply::TerminateLater
}

fn reopen_implementation() -> Imp {
    // SAFETY: Objective-C erases method signatures to IMP. Both pointer types use
    // C-unwind; registration below supplies the callback's exact type encoding.
    unsafe { std::mem::transmute::<ReopenCallback, Imp>(handle_reopen) }
}

fn install_termination_method(class: &'static AnyClass) -> Result<(), String> {
    let selector = sel!(applicationShouldTerminate:);
    // SAFETY: The IMP erases the signature. The encoding below matches the
    // NSApplicationDelegate method's object arguments and NSUInteger return value.
    let implementation =
        unsafe { std::mem::transmute::<TerminateCallback, Imp>(handle_termination) };
    if let Some(method) = class.instance_method(selector) {
        return if std::ptr::fn_addr_eq(method.implementation(), implementation) {
            Ok(())
        } else {
            Err("macOS application delegate already handles termination".into())
        };
    }
    let encoding = CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))
        .map_err(|error| error.to_string())?;
    // SAFETY: This adds only the optional selector above on the main-thread
    // delegate class, using the documented AppKit signature.
    let added = unsafe {
        ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            implementation,
            encoding.as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err("macOS application delegate could not register the termination method".into());
    }
    Ok(())
}

/// Retains the app's request state alongside the native event loop's delegate.
pub(crate) struct AppDelegateHandler {
    state: Rc<DelegateState>,
    class: &'static AnyClass,
    previous: Option<Imp>,
}

impl AppDelegateHandler {
    /// Adds reopen and quit handling to the native event loop's delegate.
    ///
    /// Installation never activates the application or reveals a hidden startup
    /// window. Winit retains ownership of its delegate and event loop.
    ///
    /// # Arguments
    ///
    /// - `context` - Initialized eframe context to wake on a native request.
    ///
    /// # Returns
    ///
    /// A retained handler that must remain alive while the application runs.
    ///
    /// # Errors
    ///
    /// Returns an error outside the main thread, before native delegate setup, or
    /// when the runtime cannot register the delegate methods.
    pub(crate) fn install(context: &egui::Context) -> Result<Self, String> {
        let marker =
            MainThreadMarker::new().ok_or("macOS app delegate handler requires the main thread")?;
        let application = NSApplication::sharedApplication(marker);
        let delegate = application
            .delegate()
            .ok_or("macOS app delegate handler requires an initialized application delegate")?;
        if ACTIVE.with(|active| active.borrow().as_ref().and_then(Weak::upgrade).is_some()) {
            return Err("macOS app delegate handler is already installed".into());
        }
        let class = AsRef::<AnyObject>::as_ref(&*delegate).class();
        install_termination_method(class)?;
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
        let state = Rc::new(DelegateState {
            delegate,
            show_requested: Cell::new(false),
            quit_requested: Cell::new(false),
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

    /// Consumes a pending reopen and reports Quit until AppKit receives a reply.
    ///
    /// # Returns
    ///
    /// The actions for this frame.
    pub(crate) fn window_actions(&self) -> impl Iterator<Item = Action> {
        [
            self.state
                .show_requested
                .replace(false)
                .then_some(Action::Show),
            self.state.quit_requested.get().then_some(Action::Quit),
        ]
        .into_iter()
        .flatten()
    }

    /// Answers a deferred AppKit Quit after the current event handler returns.
    ///
    /// # Arguments
    /// - `saved` - Whether all pending changes were saved.
    ///
    /// # Returns
    /// Whether there was an AppKit request to answer.
    ///
    /// # Panics
    /// Panics outside the main thread or if the main run loop is unavailable.
    pub(crate) fn reply_to_quit(&self, saved: bool) -> bool {
        if !self.state.quit_requested.replace(false) {
            return false;
        }
        let marker = MainThreadMarker::new().expect("macOS quit reply must be on the main thread");
        let application = NSApplication::sharedApplication(marker);
        let reply = RcBlock::new(move || application.replyToApplicationShouldTerminate(saved));
        let run_loop = CFRunLoop::main().expect("macOS must have a main run loop");
        // SAFETY: The main run loop copies the block and runs it in a common mode,
        // including AppKit's modal mode while waiting for the termination reply.
        // Replying from there lets winit handle termination outside its active callback.
        unsafe {
            run_loop.perform_block(kCFRunLoopCommonModes.map(AsRef::as_ref), Some(&reply));
        }
        run_loop.wake_up();
        true
    }
}

impl Drop for AppDelegateHandler {
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
        // Objective-C has no method-removal API. Added callbacks return AppKit's
        // default replies when there is no active state.
    }
}
