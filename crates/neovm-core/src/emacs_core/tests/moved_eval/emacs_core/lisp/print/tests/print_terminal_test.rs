//! The terminal-handle printer row.
//!
//! `terminal::pure` lives in neovm-display, which eval cannot name.
//! Assertions are unchanged.

#[test]
fn print_terminal_handle_special_form() {
    crate::test_utils::init_test_tracing();
    let list = crate::emacs_core::terminal::pure::builtin_terminal_list(vec![]).unwrap();
    let items = neovm_value::value::list_to_vec(&list).expect("terminal-list should return a list");
    let handle = items
        .first()
        .expect("terminal-list should contain one handle");

    let printed = crate::emacs_core::print::print_value(handle);
    assert!(printed.starts_with("#<terminal "));
    assert!(printed.contains("on initial_terminal>"));
}
