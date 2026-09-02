use super::focus::{
    FOCUS_PROBE, GIVE_UP_FOCUS_MESSAGE, GNU_XW_SCRIPT, KEY_DOWN_MESSAGE_HANDLER, KeyRoute,
    focus_transition, route_key,
};
use crate::FocusIntent;

#[test]
fn keys_stay_with_emacs_unless_the_page_has_an_input_focused() {
    assert_eq!(route_key(Ok(true)), KeyRoute::WebView);
    assert_eq!(route_key(Ok(false)), KeyRoute::Emacs);
    assert_eq!(
        route_key(Err(())),
        KeyRoute::Emacs,
        "GNU: probe error -> Emacs"
    );
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
