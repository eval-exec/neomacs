//! GNU's macOS keyboard model for a web view, as data and decisions.
//!
//! `src/nsxwidget.m` (239-323, emacs-31.0.90) keeps keyboard events with
//! Emacs unless the page says an input element has focus, and lets `C-g`
//! typed into the page hand focus back.  The AppKit glue in `view.rs`
//! executes these decisions; this module holds what can be pinned without
//! AppKit: GNU's injected script, the message it listens for, and the pure
//! decisions, including the two this port adds at the native boundary (a
//! probe answer that is not a boolean, and a probe that completes after the
//! view's host changed).
//!
//! Not ported from GNU's `keyDown:` (nsxwidget.m:244-278): the
//! `isearch-mode` branch (:247-253, a buffer-local Lisp read the AppKit
//! thread cannot make; keys go to the page's focused input while searching)
//! and the `urlScriptBlocked` branch (:255-260, a response whose
//! `Content-Security-Policy: sandbox` lacks `allow-scripts` sends every key
//! to Emacs without probing).  In such a document the probe itself fails and
//! `Failed` routes to Emacs, which converges on GNU's answer by accident,
//! not by port.

use crate::FocusIntent;

/// GNU's `xwScript` (nsxwidget.m:294-308), verbatim: `xwHasFocus()` answers
/// whether the active element is an INPUT or TEXTAREA, and a `keydown`
/// listener posts "C-g" to the `keyDown` message handler.
pub(super) const GNU_XW_SCRIPT: &str = "function xwHasFocus() {\
  var ae = document.activeElement;\
  if (ae) {\
    var name = ae.nodeName;\
    return name == 'INPUT' || name == 'TEXTAREA';\
  } else {\
    return false;\
  }\
}\
function xwKeyDown(event) {\
  if (event.ctrlKey && event.key == 'g') {\
    window.webkit.messageHandlers.keyDown.postMessage('C-g');\
  }\
}\
document.addEventListener('keydown', xwKeyDown);";

/// The `WKScriptMessageHandler` name GNU registers (nsxwidget.m:93).
pub(super) const KEY_DOWN_MESSAGE_HANDLER: &str = "keyDown";

/// The expression GNU evaluates on every key event (nsxwidget.m:262).
pub(super) const FOCUS_PROBE: &str = "xwHasFocus()";

/// The message body that gives focus back to Emacs (nsxwidget.m:315-323).
pub(super) const GIVE_UP_FOCUS_MESSAGE: &str = "C-g";

/// Which Emacs host view a key probe was issued against.
///
/// The probe completes asynchronously; by then the web view may have moved
/// to another frame's host view, been hidden, or be tearing down.  The view
/// bumps its epoch on every change of host, and a completion whose epoch is
/// stale is not delivered anywhere: the key belonged to a host that is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HostEpoch(u64);

impl HostEpoch {
    pub(super) const fn first() -> Self {
        Self(0)
    }

    pub(super) const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// What `xwHasFocus()` came back with, after the native boundary checked
/// the value's class.  Page JavaScript can redefine the function, so the
/// answer is not trusted to be a boolean until it has been downcast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusProbe {
    /// A Core Foundation Boolean that is true: an INPUT or TEXTAREA has focus.
    Focused,
    /// A Core Foundation Boolean that is false.
    Unfocused,
    /// A nil result with no error (GNU: `else if (result)` fails, nothing
    /// is delivered).  This is the one answer a page can still use to eat
    /// keys (`xwHasFocus = () => null`); it is kept because it is GNU's
    /// behaviour, unlike the non-boolean case below, which GNU cannot
    /// survive and this port has to decide for itself.
    Absent,
    /// A non-nil result that is not a Core Foundation Boolean.  GNU would
    /// send it `boolValue`; this port treats it as "no input focused".
    NotABoolean,
    /// `evaluateJavaScript` reported an error.
    Failed,
}

/// Where a key event that reached the web view is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyRoute {
    /// Forward to the Emacs view's `keyDown:` (`emacswindow` in GNU).
    Emacs,
    /// Let WKWebView handle it (`[super keyDown:event]`).
    WebView,
    /// Deliver nowhere.
    Dropped,
}

/// GNU's decision (nsxwidget.m:262-278) plus the lifecycle guard: a probe
/// that completes after the host changed (`sent != now`) is dropped; the
/// page keeps the key only when `xwHasFocus()` answered true; a probe error
/// goes to Emacs; a nil result is dropped, as GNU's `else if (result)`
/// drops it; a non-boolean answer is the safe default, Emacs.
pub(super) fn route_key(probe: FocusProbe, sent: HostEpoch, now: HostEpoch) -> KeyRoute {
    if sent != now {
        return KeyRoute::Dropped;
    }
    match probe {
        FocusProbe::Focused => KeyRoute::WebView,
        FocusProbe::Unfocused | FocusProbe::Failed | FocusProbe::NotABoolean => KeyRoute::Emacs,
        FocusProbe::Absent => KeyRoute::Dropped,
    }
}

/// What a `keyDown` script message asks for.  Only a string body equal to
/// GNU's "C-g" means anything; the page can post any JSON value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyDownMessage {
    GiveFocusBackToEmacs,
    Ignored,
}

pub(super) fn key_down_message(body: Option<&str>) -> KeyDownMessage {
    match body {
        Some(GIVE_UP_FOCUS_MESSAGE) => KeyDownMessage::GiveFocusBackToEmacs,
        Some(_) | None => KeyDownMessage::Ignored,
    }
}

/// The `focused` value to report after asking the window to move first
/// responder, or `None` when nothing changed: an intent the window refused
/// (`makeFirstResponder:` answered NO) is not a focus change, and repeating
/// the current state is not one either.
pub(super) fn focus_transition(
    previously_focused: bool,
    intent: FocusIntent,
    accepted: bool,
) -> Option<bool> {
    let wanted = intent == FocusIntent::Focus;
    (accepted && wanted != previously_focused).then_some(wanted)
}
