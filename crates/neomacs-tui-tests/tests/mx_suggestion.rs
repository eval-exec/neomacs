#![cfg(unix)]
//! Live GNU oracle for `execute-extended-command`'s asynchronous suggestion.
//!
//! GNU (`lisp/simple.el`, `execute-extended-command`) shows "You can run the
//! command ... with M-x ..." when the invoked command has no key binding and
//! `extended-command-suggest-shorter` is non-nil. The message is scheduled with
//! `run-at-time` at `suggest-key-bindings` seconds -- two seconds when the
//! command itself wrote the echo area -- and held for two more with `sit-for`,
//! after which `with-temp-message` restores the command's own message.
//!
//! The shared harness turns that four-second transient off by default
//! (`QUIET_MX_SUGGESTION_EVAL`) so full-grid tests never race it; this test
//! turns it back on in BOTH editors and pins both phases against live GNU.
//! `whitespace-mode` is the oracle case from the CI shards: it has no key
//! binding and its `define-minor-mode` body writes "Whitespace mode enabled in
//! current buffer".

use crate::support::*;
use std::time::Duration;

#[test]
fn mx_of_an_unbound_command_that_messages_matches_gnu_through_the_suggestion_phase() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(&mut gnu, &mut neo, "(setq suggest-key-bindings t)");

    invoke_mx_command(&mut gnu, &mut neo, "whitespace-mode");

    // Phase 2: the binding suggestion, two seconds after the command.
    wait_for_both_mx_suggestion(
        &mut gnu,
        &mut neo,
        "whitespace-mode",
        Duration::from_secs(10),
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display("mx_unbound_message_command_suggestion_phase", &gnu, &neo);

    // Phase 3: the suggestion's `sit-for` ends and the command's message is
    // restored. Both editors must hold the same message and the same lighter.
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Whitespace mode enabled in current buffer"))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_pair_exact_display("mx_unbound_message_command_restored_message", &gnu, &neo);
}
