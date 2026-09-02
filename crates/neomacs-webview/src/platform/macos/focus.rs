//! GNU's macOS keyboard model for a web view, as data and decisions.
//!
//! `src/nsxwidget.m` (236-330) keeps keyboard events with Emacs unless the
//! page says an input element has focus, and lets `C-g` typed into the page
//! hand focus back.  The AppKit glue in `view.rs` executes these decisions;
//! this module holds what can be pinned without AppKit: GNU's injected
//! script, the message it listens for, and the two pure decisions.

use crate::FocusIntent;

/// GNU's `xwScript` (nsxwidget.m:283-303), verbatim: `xwHasFocus()` answers
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

/// The message body that gives focus back to Emacs (nsxwidget.m:318-322).
pub(super) const GIVE_UP_FOCUS_MESSAGE: &str = "C-g";

/// Where a key event that reached the web view is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyRoute {
    /// Forward to the Emacs view's `keyDown:` (`emacswindow` in GNU).
    Emacs,
    /// Let WKWebView handle it (`[super keyDown:event]`).
    WebView,
}

/// GNU's decision (nsxwidget.m:262-275): the page keeps the key only when
/// `xwHasFocus()` answered true; a probe error goes to Emacs too.
pub(super) fn route_key(probe: Result<bool, ()>) -> KeyRoute {
    match probe {
        Ok(true) => KeyRoute::WebView,
        Ok(false) | Err(()) => KeyRoute::Emacs,
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
