//! Route the AppKit Quit command through Emacs.
//!
//! winit installs an application menu whose Quit item sends `terminate:` with
//! Cmd-Q as its key equivalent. AppKit then terminates the process without
//! consulting Emacs, so unsaved buffers are lost. GNU Emacs overrides
//! `-[EmacsApp terminate:]` to post `KEY_NS_POWER_OFF` instead
//! (`src/nsterm.m:6588`), and `lisp/term/ns-win.el:195` binds that event to
//! `save-buffers-kill-emacs`.
//!
//! Neomacs is a winit application and does not own the `NSApplication` class,
//! so it retargets the existing Quit item to an object that feeds the same
//! command into the input transport as `s-q`, the binding GNU installs for
//! the same key at `lisp/term/ns-win.el:141`. The command prompts and runs
//! the normal kill-emacs hooks.

use crate::thread_comm::InputEvent;
use crossbeam_channel::{Sender, TrySendError};
use neomacs_display_protocol::modifier_policy::{TransportModifierBit, TransportModifierBits};
use neovm_core::keyboard::FrontendKey;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
// `AnyThread` is imported anonymously: only its `alloc` method is used.
use objc2::{AnyThread as _, DefinedClass, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenuItem};
use objc2_foundation::NSObject;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};

struct QuitRedirectIvars {
    input_tx: Sender<InputEvent>,
}

/// Latched when the bounded input queue was full, so the Quit request is
/// retried from the event loop instead of being dropped or blocking AppKit.
static PENDING_QUIT: AtomicBool = AtomicBool::new(false);

fn quit_key_event() -> InputEvent {
    let mut modifiers = TransportModifierBits::default();
    modifiers.set(TransportModifierBit::Super);
    InputEvent::Key {
        key: FrontendKey::Character('q'),
        modifiers: modifiers.bits(),
        pressed: true,
        // Zero selects whichever frame is current, as for `WindowClose`
        // events from the render thread.
        emacs_frame_id: 0,
    }
}

fn queue_quit(input_tx: &Sender<InputEvent>) {
    match input_tx.try_send(quit_key_event()) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            PENDING_QUIT.store(true, Ordering::Release);
            tracing::warn!("cmd-Q deferred: input queue is full");
        }
        Err(TrySendError::Disconnected(_)) => {
            tracing::warn!("cmd-Q dropped: input queue is disconnected");
        }
    }
}

/// Whether a Cmd-Q request is still waiting for input-queue space.
pub(super) fn quit_is_pending() -> bool {
    PENDING_QUIT.load(Ordering::Acquire)
}

/// Retry a Quit request that was deferred while the input queue was full.
///
/// Called from `about_to_wait`, which runs after every event batch and never
/// blocks the AppKit main thread.
pub(super) fn deliver_pending_quit(input_tx: &Sender<InputEvent>) {
    if !PENDING_QUIT.swap(false, Ordering::AcqRel) {
        return;
    }
    match input_tx.try_send(quit_key_event()) {
        Ok(()) => tracing::info!("cmd-Q delivered after the input queue drained"),
        Err(TrySendError::Full(_)) => {
            PENDING_QUIT.store(true, Ordering::Release);
        }
        Err(TrySendError::Disconnected(_)) => {
            tracing::warn!("cmd-Q dropped: input queue is disconnected");
        }
    }
}

define_class!(
    /// Menu-item target that forwards Quit to the Emacs input channel.
    #[unsafe(super(NSObject))]
    #[ivars = QuitRedirectIvars]
    #[name = "NeomacsQuitRedirect"]
    struct QuitRedirect;

    impl QuitRedirect {
        #[unsafe(method(neomacsQuit:))]
        fn quit(&self, _sender: Option<&NSMenuItem>) {
            queue_quit(&self.ivars().input_tx);
        }
    }
);

impl QuitRedirect {
    fn new(input_tx: Sender<InputEvent>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(QuitRedirectIvars { input_tx });
        // SAFETY: NSObject's designated initializer; the subclass adds only
        // the ivars set above and no initializer of its own.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    /// The menu's target is a weak reference, so this owner must outlive the
    /// application menu. Both live for the life of the main thread.
    static QUIT_REDIRECT: RefCell<Option<Retained<QuitRedirect>>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Point winit's application-menu Quit item at Emacs.
///
/// Idempotent, and a no-op when the menu is not present yet or this is not
/// the AppKit main thread.
pub(super) fn install(input_tx: &Sender<InputEvent>) {
    if INSTALLED.with(Cell::get) {
        return;
    }
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        tracing::warn!("cmd-Q redirect skipped: not on the AppKit main thread");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else {
        return;
    };
    // winit's app menu is the submenu attached to the first top-level item
    // (`winit-appkit/src/menu.rs`). Find Quit by its selector rather than by
    // title so a localized process name cannot hide it.
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

        let target = QuitRedirect::new(input_tx.clone());
        let target_object: Retained<AnyObject> = target.clone().into();
        // SAFETY: the selector is defined above, and the target outlives the
        // menu through QUIT_REDIRECT.
        unsafe {
            item.setAction(Some(sel!(neomacsQuit:)));
            item.setTarget(Some(&target_object));
        }
        QUIT_REDIRECT.with(|slot| *slot.borrow_mut() = Some(target));
        INSTALLED.with(|installed| installed.set(true));
        tracing::info!("cmd-Q redirect installed on the AppKit Quit item");
        return;
    }
}
