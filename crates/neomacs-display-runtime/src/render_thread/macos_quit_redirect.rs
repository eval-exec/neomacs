//! GNU's two independent native quit paths (nsterm.m:6588-6641).
//!
//! App-menu Quit posts the non-key `ns-power-off` event. System termination
//! asks `applicationShouldTerminate:`, whose optional native confirmation
//! reads only a scalar mirrored from completed Lisp variable writes.

use super::application_quit::{
    ALERT_FIRST_BUTTON_RETURN, KeyboardView, PendingApplicationQuit, QuitDelivery, QuitRoute,
    QuitTrigger, TERMINATE_CANCEL_RAW, TERMINATE_NOW_RAW, alert_accepted, native_reply,
    reply as termination_reply, route as menu_quit_route, select_keyboard_view,
};
use crate::thread_comm::InputEvent;
use crossbeam_channel::Sender;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, Imp, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationDelegate,
    NSApplicationTerminateReply, NSEventModifierFlags, NSEventType, NSMenuItem, NSView,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// The pure policy's constants are also checked against the actual SDK ABI.
const _: () = assert!(ALERT_FIRST_BUTTON_RETURN == NSAlertFirstButtonReturn);
const _: () = assert!(TERMINATE_CANCEL_RAW == NSApplicationTerminateReply::TerminateCancel.0);
const _: () = assert!(TERMINATE_NOW_RAW == NSApplicationTerminateReply::TerminateNow.0);

struct QuitRedirectIvars {
    input_tx: Sender<InputEvent>,
}

static PENDING_QUIT: PendingApplicationQuit = PendingApplicationQuit::new();
// Native callbacks cannot borrow an evaluator or a Lisp Value. Publication
// occurs after a successful canonical write, including let/unlet restoration.
// Buffer-local/per-thread context switches without a write still need an
// explicit native mirror resync, as with the other scalar DisplayHost mirrors.
static CONFIRM_QUIT: AtomicBool = AtomicBool::new(false);
static TERMINATE_INPUT: OnceLock<Sender<InputEvent>> = OnceLock::new();
static SELECTED_GUI_ROOT: AtomicU64 = AtomicU64::new(0);

/// The evaluator publishes only a native-root ID, never a Lisp/native object.
pub(super) fn set_selected_gui_frame(frame: Option<u64>) {
    SELECTED_GUI_ROOT.store(frame.unwrap_or(0), Ordering::Release);
}

/// Refresh only at native window creation, adoption and destruction edges.
/// Weak references do not keep a closed window's keyboard view alive.
pub(super) fn refresh_frame_views(frames: &super::frame_windows::GuiFrameWindowManager) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(_main_thread) = objc2::MainThreadMarker::new() else {
        return;
    };
    let mut views = BTreeMap::new();
    frames.for_each_top_level_window(|state| {
        let Some(window) = state.window() else { return };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: the borrowed winit handle guarantees a live NSView for this
        // borrow, and the marker above verifies AppKit's main thread. Only
        // an Objective-C weak reference escapes, never this raw pointer.
        let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
        views.insert(state.render.emacs_frame_id, Weak::new(view));
    });
    QUIT_REDIRECT.with(|slot| slot.borrow_mut().frame_views = views);
}

fn selected_frame_view() -> Option<Retained<NSView>> {
    let frame = SELECTED_GUI_ROOT.load(Ordering::Acquire);
    if frame == 0 {
        return None;
    }
    QUIT_REDIRECT.with(|slot| slot.borrow().frame_views.get(&frame).and_then(Weak::load))
}

pub(super) fn set_confirm_quit(enabled: bool) {
    CONFIRM_QUIT.store(enabled, Ordering::Release);
}

fn queue_quit(input_tx: &Sender<InputEvent>) {
    match PENDING_QUIT.native_terminate(input_tx) {
        QuitDelivery::Delivered => {}
        QuitDelivery::Deferred => tracing::warn!("application Quit deferred: input queue is full"),
        QuitDelivery::Disconnected => {
            tracing::warn!("application Quit dropped: input queue is disconnected")
        }
    }
}

pub(super) fn quit_is_pending() -> bool {
    PENDING_QUIT.is_pending()
}

/// Retry without blocking AppKit, preserving the queue-full deferral contract.
pub(super) fn deliver_pending_quit(input_tx: &Sender<InputEvent>) {
    match PENDING_QUIT.retry(input_tx) {
        Some(QuitDelivery::Delivered) => {
            tracing::info!("application Quit delivered after input drained")
        }
        Some(QuitDelivery::Disconnected) => {
            tracing::warn!("application Quit dropped: input queue is disconnected")
        }
        Some(QuitDelivery::Deferred) | None => {}
    }
}

type Terminate = extern "C-unwind" fn(&NSApplication, Sel, Option<&AnyObject>);

extern "C-unwind" fn terminate(
    _application: &NSApplication,
    _selector: Sel,
    _sender: Option<&AnyObject>,
) {
    // Like GNU's EmacsApp::terminate:, never call the superclass: only Lisp
    // may exit after save prompts and hooks. The sender is installed before
    // replacing the method and lives for this one global application's life.
    if let Some(input_tx) = TERMINATE_INPUT.get() {
        queue_quit(input_tx);
    }
}

fn install_terminate_override(app: &NSApplication, input_tx: &Sender<InputEvent>) {
    TERMINATE_INPUT.get_or_init(|| input_tx.clone());
    let Some(method) = app.class().instance_method(sel!(terminate:)) else {
        tracing::warn!("application Quit setup skipped: terminate: method is unavailable");
        return;
    };
    // Winit uses this same seam for sendEvent: (winit-appkit/src/app.rs).
    // Install after EventLoop creation, which initializes the global app.
    // SAFETY: Terminate has NSApplication's exact void(id, SEL, id) ABI and
    // nullable sender. It imposes no extra requirements on callers, retains
    // no borrowed Objective-C objects, and has process-long code lifetime.
    let replacement = unsafe { std::mem::transmute::<Terminate, Imp>(terminate) };
    if std::ptr::fn_addr_eq(replacement, method.implementation()) {
        return;
    }
    // SAFETY: as above; installation is on AppKit's main thread, and winit
    // supports only this single shared application/event loop. Like GNU's
    // override, this intentionally replaces termination for the app's life.
    let _original = unsafe { method.set_implementation(replacement) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = QuitRedirectIvars]
    #[name = "NeomacsQuitRedirect"]
    struct QuitRedirect;

    // SAFETY: NSObject supplies this protocol's required object operations;
    // define_class! manages the Rust ivars and NSObject initialization.
    unsafe impl NSObjectProtocol for QuitRedirect {}

    // SAFETY: the optional selectors below have the NSApplicationDelegate
    // signatures, run on AppKit's main thread, and retain no borrowed values.
    unsafe impl NSApplicationDelegate for QuitRedirect {
        #[unsafe(method(applicationSupportsSecureRestorableState:))]
        fn application_supports_secure_restorable_state(&self, _app: &NSApplication) -> bool {
            // GNU EmacsApp answers YES (nsterm.m:6583-6586).
            true
        }

        #[unsafe(method(applicationShouldTerminate:))]
        fn application_should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            let reply = termination_reply(CONFIRM_QUIT.load(Ordering::Acquire), || {
                // No TLS RefCell borrow spans AppKit's nested modal loop.
                let alert = NSAlert::new(self.mtm());
                alert.setAlertStyle(NSAlertStyle::Critical);
                alert.setMessageText(&NSString::from_str(
                    "Exit requested.  Would you like to Save Buffers and Exit, or Cancel the request?",
                ));
                let _ = alert.addButtonWithTitle(&NSString::from_str("Save Buffers and Exit"));
                let _ = alert.addButtonWithTitle(&NSString::from_str("Cancel"));
                alert_accepted(alert.runModal())
            });
            NSApplicationTerminateReply(native_reply(reply))
        }
    }

    impl QuitRedirect {
        #[unsafe(method(neomacsQuit:))]
        fn quit(&self, _sender: Option<&NSMenuItem>) {
            let app = NSApplication::sharedApplication(self.mtm());
            let event = app.currentEvent();
            let trigger = match event.as_ref() {
                Some(event) if event.r#type() == NSEventType::KeyDown => {
                    if event.modifierFlags().contains(NSEventModifierFlags::Command) {
                        QuitTrigger::CommandKeyDown
                    } else {
                        QuitTrigger::OtherKeyDown
                    }
                }
                _ => QuitTrigger::NonKey,
            };
            match menu_quit_route(trigger) {
                QuitRoute::ApplicationQuit => queue_quit(&self.ivars().input_tx),
                QuitRoute::Keyboard => {
                    // GNU EmacsMenu::performKeyEquivalent: calls the selected
                    // frame's keyDown: (nsmenu.m:519-525). Winit's initial
                    // first responder is its keyboard view, inside a plain
                    // content-view container (window_delegate.rs:893-904).
                    // Let that view emit the real modifiers and key event;
                    // Lisp's ns-command-modifier policy then applies normally.
                    let key = app
                        .keyWindow()
                        .and_then(|window| window.initialFirstResponder());
                    let main = app
                        .mainWindow()
                        .and_then(|window| window.initialFirstResponder());
                    let selected = selected_frame_view();
                    let view = match select_keyboard_view(
                        key.is_some(), main.is_some(), selected.is_some(),
                    ) {
                        Some(KeyboardView::KeyWindow) => key,
                        Some(KeyboardView::MainWindow) => main,
                        Some(KeyboardView::SelectedFrame) => selected,
                        None => None,
                    };
                    if let (Some(view), Some(event)) = (view, event) {
                        view.keyDown(&event);
                    }
                    // As in GNU, no selected view means the key equivalent
                    // is consumed; it must not become a quit request.
                }
            }
        }
    }
);

impl QuitRedirect {
    fn new(mtm: objc2::MainThreadMarker, input_tx: Sender<InputEvent>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(QuitRedirectIvars { input_tx });
        // SAFETY: NSObject's designated initializer after subclass ivars.
        unsafe { msg_send![super(this), init] }
    }
}

struct QuitRedirectSlot {
    target: Option<Retained<QuitRedirect>>,
    // Window registration may precede target installation. Keep these weak
    // views in the same main-thread owner independently of target readiness.
    frame_views: BTreeMap<u64, Weak<NSView>>,
}

thread_local! {
    // AppKit's delegate/menu target references are weak. This existing
    // main-thread owner retains the target and tracks weak keyboard views,
    // without carrying any Lisp state across threads. Take retained objects
    // out and drop the borrow before AppKit/winit calls: they can re-enter
    // refresh_frame_views(), which needs to borrow this same owner mutably.
    static QUIT_REDIRECT: RefCell<QuitRedirectSlot> = const { RefCell::new(QuitRedirectSlot {
        target: None,
        frame_views: BTreeMap::new(),
    }) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Install native termination policy even when the menu is not available yet.
pub(super) fn install(input_tx: &Sender<InputEvent>) {
    if INSTALLED.with(Cell::get) {
        return;
    }
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        tracing::warn!("application Quit setup skipped: not on the AppKit main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    install_terminate_override(&app, input_tx);
    let target = if let Some(target) = QUIT_REDIRECT.with(|slot| slot.borrow().target.clone()) {
        target
    } else {
        // NSObject initialization must also run without a slot borrow.
        let target = QuitRedirect::new(mtm, input_tx.clone());
        QUIT_REDIRECT.with(|slot| slot.borrow_mut().target = Some(target.clone()));
        target
    };
    // Winit explicitly guarantees that it does not install an app delegate
    // (winit-appkit/src/lib.rs). The object is retained by QUIT_REDIRECT.
    app.setDelegate(Some(ProtocolObject::from_ref(&*target)));

    let Some(main_menu) = app.mainMenu() else {
        return;
    };
    let Some(app_menu) = main_menu.itemAtIndex(0).and_then(|item| item.submenu()) else {
        return;
    };
    for index in 0..app_menu.numberOfItems() {
        let Some(item) = app_menu.itemAtIndex(index) else {
            continue;
        };
        let action = item.action();
        if action == Some(sel!(neomacsQuit:)) {
            INSTALLED.with(|installed| installed.set(true));
            return;
        }
        if action != Some(sel!(terminate:)) {
            continue;
        }
        let target_object: Retained<AnyObject> = target.clone().into();
        // SAFETY: selector is defined above; main-thread TLS retains target.
        unsafe {
            item.setAction(Some(sel!(neomacsQuit:)));
            item.setTarget(Some(&target_object));
        }
        INSTALLED.with(|installed| installed.set(true));
        tracing::info!("AppKit Quit now posts ns-power-off");
        return;
    }
}
