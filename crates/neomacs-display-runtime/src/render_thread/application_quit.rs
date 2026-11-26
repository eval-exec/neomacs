//! Native Quit routing and native termination confirmation are separate policies.

use crate::thread_comm::InputEvent;
use crossbeam_channel::{Sender, TrySendError};
use std::sync::atomic::{AtomicBool, Ordering};

/// The delegate's answer is independent of the semantic Quit input route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminationReply {
    Now,
    Cancel,
}

// These SDK values are kept scalar so the policy remains Linux-testable.
// The AppKit adapter asserts that its native constants have these values.
pub(super) const ALERT_FIRST_BUTTON_RETURN: isize = 1000;
pub(super) const TERMINATE_CANCEL_RAW: usize = 0;
pub(super) const TERMINATE_NOW_RAW: usize = 1;

pub(super) const fn alert_accepted(response: isize) -> bool {
    response == ALERT_FIRST_BUTTON_RETURN
}

pub(super) const fn native_reply(reply: TerminationReply) -> usize {
    match reply {
        TerminationReply::Now => TERMINATE_NOW_RAW,
        TerminationReply::Cancel => TERMINATE_CANCEL_RAW,
    }
}

/// GNU `applicationShouldTerminate:` asks only when ns-confirm-quit is non-nil
/// (nsterm.m:6628-6641). The AppKit adapter supplies the native alert itself.
pub(super) fn reply(confirm_enabled: bool, ask: impl FnOnce() -> bool) -> TerminationReply {
    if !confirm_enabled {
        return TerminationReply::Now;
    }
    if ask() {
        TerminationReply::Now
    } else {
        TerminationReply::Cancel
    }
}

/// Provenance extracted from the native event, independently of its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QuitTrigger {
    CommandKeyDown,
    OtherKeyDown,
    NonKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QuitRoute {
    Keyboard,
    ApplicationQuit,
}

/// GNU forwards menu key equivalents to keyDown: without comparing characters
/// (nsmenu.m:519-525). A menu click or native Quit action posts KEY_NS_POWER_OFF
/// (nsterm.m:6588-6604). Caps Lock and input-layout text cannot change the route.
pub(super) fn route(trigger: QuitTrigger) -> QuitRoute {
    match trigger {
        QuitTrigger::CommandKeyDown => QuitRoute::Keyboard,
        QuitTrigger::OtherKeyDown | QuitTrigger::NonKey => QuitRoute::ApplicationQuit,
    }
}

/// Which live keyboard view receives the original command-key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyboardView {
    KeyWindow,
    MainWindow,
    SelectedFrame,
}

/// A temporarily inactive application can have neither a key nor main window.
/// GNU still forwards to its selected frame's view when one exists.
pub(super) fn select_keyboard_view(
    key_available: bool,
    main_available: bool,
    selected_available: bool,
) -> Option<KeyboardView> {
    if key_available {
        Some(KeyboardView::KeyWindow)
    } else if main_available {
        Some(KeyboardView::MainWindow)
    } else if selected_available {
        Some(KeyboardView::SelectedFrame)
    } else {
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum QuitDelivery {
    Delivered,
    Deferred,
    Disconnected,
}

/// Coalesce a request while the bounded frontend queue has no capacity.
/// AppKit must never block; the event loop retries after draining input.
pub(super) struct PendingApplicationQuit {
    pending: AtomicBool,
}

impl PendingApplicationQuit {
    pub(super) const fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    /// GNU's native terminate: always posts a semantic application request;
    /// it neither consults key bindings nor runs the delegate's confirmation.
    pub(super) fn native_terminate(&self, input: &Sender<InputEvent>) -> QuitDelivery {
        match input.try_send(InputEvent::ApplicationQuit { emacs_frame_id: 0 }) {
            Ok(()) => QuitDelivery::Delivered,
            Err(TrySendError::Full(_)) => {
                self.pending.store(true, Ordering::Release);
                QuitDelivery::Deferred
            }
            Err(TrySendError::Disconnected(_)) => QuitDelivery::Disconnected,
        }
    }

    pub(super) fn retry(&self, input: &Sender<InputEvent>) -> Option<QuitDelivery> {
        self.pending
            .swap(false, Ordering::AcqRel)
            .then(|| self.native_terminate(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_quit_queues_a_semantic_request_instead_of_super_q() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let quit = PendingApplicationQuit::new();
        assert_eq!(quit.native_terminate(&tx), QuitDelivery::Delivered);
        assert!(matches!(
            rx.recv().unwrap(),
            InputEvent::ApplicationQuit { emacs_frame_id: 0 }
        ));
        assert!(!quit.is_pending());
        assert_eq!(quit.retry(&tx), None);
    }

    #[test]
    fn application_quit_retries_after_queue_capacity_returns() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send(InputEvent::WindowClose { emacs_frame_id: 42 })
            .unwrap();
        let quit = PendingApplicationQuit::new();
        assert_eq!(quit.native_terminate(&tx), QuitDelivery::Deferred);
        assert!(quit.is_pending());
        assert_eq!(quit.retry(&tx), Some(QuitDelivery::Deferred));
        assert!(quit.is_pending());
        assert!(matches!(
            rx.recv().unwrap(),
            InputEvent::WindowClose { emacs_frame_id: 42 }
        ));
        assert_eq!(quit.retry(&tx), Some(QuitDelivery::Delivered));
        assert!(matches!(
            rx.recv().unwrap(),
            InputEvent::ApplicationQuit { emacs_frame_id: 0 }
        ));
        assert!(!quit.is_pending());
        assert_eq!(quit.retry(&tx), None);
    }

    #[test]
    fn disconnected_application_quit_does_not_keep_retrying() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send(InputEvent::WindowClose { emacs_frame_id: 42 })
            .unwrap();
        let quit = PendingApplicationQuit::new();
        assert_eq!(quit.native_terminate(&tx), QuitDelivery::Deferred);
        drop(rx);
        assert_eq!(quit.retry(&tx), Some(QuitDelivery::Disconnected));
        assert!(!quit.is_pending());
        assert_eq!(quit.native_terminate(&tx), QuitDelivery::Disconnected);
    }

    #[test]
    fn native_quit_confirmation_disabled_does_not_ask() {
        assert_eq!(
            reply(false, || panic!(
                "nil ns-confirm-quit must not open an alert"
            )),
            TerminationReply::Now
        );
    }

    #[test]
    fn native_quit_confirmation_enabled_asks_once_and_maps_answer() {
        use std::cell::Cell;

        for (accepted, expected) in [
            (true, TerminationReply::Now),
            (false, TerminationReply::Cancel),
        ] {
            let calls = Cell::new(0);
            let actual = reply(true, || {
                calls.set(calls.get() + 1);
                accepted
            });
            assert_eq!(actual, expected);
            assert_eq!(calls.get(), 1, "one native confirmation per request");
        }
    }

    #[test]
    fn menu_quit_capslock_and_non_latin_command_keydown_stays_keyboard() {
        // Caps Lock Q, a non-Latin character, and Command-QWERTY all carry
        // Command+KeyDown provenance. Characters are intentionally absent
        // from the policy input: GNU forwards the original native event.
        assert_eq!(route(QuitTrigger::CommandKeyDown), QuitRoute::Keyboard);
    }

    #[test]
    fn menu_quit_non_command_keydown_and_non_key_actions_request_application_quit() {
        assert_eq!(route(QuitTrigger::OtherKeyDown), QuitRoute::ApplicationQuit);
        assert_eq!(route(QuitTrigger::NonKey), QuitRoute::ApplicationQuit);
    }

    #[test]
    fn command_keydown_prefers_key_then_main_then_selected_frame_view() {
        for (available, expected) in [
            ((true, true, true), Some(KeyboardView::KeyWindow)),
            ((false, true, true), Some(KeyboardView::MainWindow)),
            ((false, false, true), Some(KeyboardView::SelectedFrame)),
        ] {
            assert_eq!(
                select_keyboard_view(available.0, available.1, available.2),
                expected
            );
        }
    }

    #[test]
    fn command_keydown_without_any_frame_view_is_consumed_without_quitting() {
        assert_eq!(route(QuitTrigger::CommandKeyDown), QuitRoute::Keyboard);
        assert_eq!(select_keyboard_view(false, false, false), None);
    }

    #[test]
    fn native_quit_confirmation_accepts_only_the_first_button() {
        assert!(alert_accepted(1000));
        for response in [1001, 1002, 0, -1000, -1001] {
            assert!(!alert_accepted(response));
        }
    }

    #[test]
    fn native_quit_confirmation_reply_matches_gnu() {
        assert_eq!(native_reply(TerminationReply::Cancel), 0);
        assert_eq!(native_reply(TerminationReply::Now), 1);
    }
}
