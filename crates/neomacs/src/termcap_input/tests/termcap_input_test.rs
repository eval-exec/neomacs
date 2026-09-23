use super::{numbered_function_key_capability, xterm_compatible_term};

#[test]
fn numbered_function_key_capabilities_match_gnu_ranges() {
    assert_eq!(numbered_function_key_capability(10), None);
    assert_eq!(numbered_function_key_capability(11).as_deref(), Some("F1"));
    assert_eq!(numbered_function_key_capability(19).as_deref(), Some("F9"));
    assert_eq!(numbered_function_key_capability(20).as_deref(), Some("FA"));
    assert_eq!(numbered_function_key_capability(45).as_deref(), Some("FZ"));
    assert_eq!(numbered_function_key_capability(46).as_deref(), Some("Fa"));
    assert_eq!(numbered_function_key_capability(63).as_deref(), Some("Fr"));
    assert_eq!(numbered_function_key_capability(64), None);
}

#[test]
fn xterm_compatible_terms_match_gnu_terminal_aliases() {
    assert!(xterm_compatible_term("xterm"));
    assert!(xterm_compatible_term("xterm-256color"));
    assert!(xterm_compatible_term("screen-256color"));
    assert!(xterm_compatible_term("tmux-256color"));
    assert!(xterm_compatible_term("st-256color"));
    assert!(xterm_compatible_term("konsole-256color"));
    assert!(!xterm_compatible_term("vt100"));
}
