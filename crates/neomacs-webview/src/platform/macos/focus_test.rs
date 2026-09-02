use super::focus::{
    FOCUS_PROBE, FocusProbe, GIVE_UP_FOCUS_MESSAGE, GNU_XW_SCRIPT, HostEpoch,
    KEY_DOWN_MESSAGE_HANDLER, KeyDownMessage, KeyRoute, focus_transition, key_down_message,
    route_key,
};
use crate::FocusIntent;

/// GNU nsxwidget.m:262-278: the page keeps a key only when `xwHasFocus()`
/// answered true; an error goes to Emacs; a nil result falls through GNU's
/// `else if (result)` and is delivered nowhere.
#[test]
fn keys_stay_with_emacs_unless_the_page_has_an_input_focused() {
    let now = HostEpoch::first();
    assert_eq!(route_key(FocusProbe::Focused, now, now), KeyRoute::WebView);
    assert_eq!(route_key(FocusProbe::Unfocused, now, now), KeyRoute::Emacs);
    assert_eq!(
        route_key(FocusProbe::Failed, now, now),
        KeyRoute::Emacs,
        "GNU: probe error -> Emacs"
    );
    assert_eq!(
        route_key(FocusProbe::Absent, now, now),
        KeyRoute::Dropped,
        "GNU: nil result -> nothing"
    );
}

/// Page JavaScript can redefine `xwHasFocus` to return anything.  GNU sends
/// `boolValue` to whatever came back; this port never trusts the class and
/// routes a non-boolean answer to Emacs, the same as "no input focused".
#[test]
fn a_probe_answer_that_is_not_a_boolean_goes_to_emacs() {
    let now = HostEpoch::first();
    assert_eq!(
        route_key(FocusProbe::NotABoolean, now, now),
        KeyRoute::Emacs
    );
}

/// The probe completes asynchronously.  If the view moved to another host,
/// was detached, or is tearing down in the meantime, the key belonged to a
/// host that is gone and is delivered nowhere, whatever the page answered.
#[test]
fn a_probe_that_completes_after_the_host_changed_is_dropped() {
    let sent = HostEpoch::first();
    let now = sent.next();
    assert_eq!(route_key(FocusProbe::Focused, sent, now), KeyRoute::Dropped);
    assert_eq!(
        route_key(FocusProbe::Unfocused, sent, now),
        KeyRoute::Dropped
    );
    assert_eq!(route_key(FocusProbe::Failed, sent, now), KeyRoute::Dropped);
    assert_eq!(route_key(FocusProbe::Absent, sent, now), KeyRoute::Dropped);
    assert_eq!(
        route_key(FocusProbe::NotABoolean, sent, now),
        KeyRoute::Dropped
    );
    assert_ne!(sent, now);
    assert_eq!(sent.next(), now);
}

/// nsxwidget.m:315-323: only the string "C-g" gives focus back.  The page
/// can post any JSON value to the handler; nothing else means anything.
#[test]
fn only_gnus_c_g_string_gives_focus_back() {
    assert_eq!(
        key_down_message(Some("C-g")),
        KeyDownMessage::GiveFocusBackToEmacs
    );
    assert_eq!(key_down_message(Some("c-g")), KeyDownMessage::Ignored);
    assert_eq!(key_down_message(Some("")), KeyDownMessage::Ignored);
    assert_eq!(
        key_down_message(None),
        KeyDownMessage::Ignored,
        "a non-string body (number, object, null) is not a message"
    );
    assert_eq!(GIVE_UP_FOCUS_MESSAGE, "C-g");
}

#[test]
fn only_an_accepted_change_of_first_responder_is_a_focus_change() {
    assert_eq!(
        focus_transition(false, FocusIntent::Focus, true),
        Some(true)
    );
    assert_eq!(focus_transition(true, FocusIntent::Blur, true), Some(false));
    assert_eq!(
        focus_transition(false, FocusIntent::Focus, false),
        None,
        "the window refused: nothing moved"
    );
    assert_eq!(
        focus_transition(true, FocusIntent::Focus, true),
        None,
        "already focused"
    );
    assert_eq!(focus_transition(false, FocusIntent::Blur, true), None);
}

/// The script is GNU's, and its two halves must agree with the native
/// constants the glue registers: the probe calls the function the script
/// defines, and the page posts to the handler name we register.
#[test]
fn injected_script_matches_the_native_hooks() {
    assert!(GNU_XW_SCRIPT.starts_with("function xwHasFocus() {"));
    assert!(GNU_XW_SCRIPT.contains("return name == 'INPUT' || name == 'TEXTAREA';"));
    assert!(GNU_XW_SCRIPT.contains(&format!(
        "window.webkit.messageHandlers.{KEY_DOWN_MESSAGE_HANDLER}.postMessage('{GIVE_UP_FOCUS_MESSAGE}');"
    )));
    assert!(GNU_XW_SCRIPT.ends_with("document.addEventListener('keydown', xwKeyDown);"));
    assert_eq!(FOCUS_PROBE, "xwHasFocus()");
}
