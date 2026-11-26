//! Non-dispatch matcher callers: GNU `Fmemq` semantics.
//!
//! The dispatch path's read-once rows live in
//! `oracle_signal_conditions_once.rs`.  This file covers the callers that
//! read a signalled symbol's conditions for their own question:
//!
//! - `print_error_message` decides the file-error family with
//!   `Fmemq (Qfile_error, error_conditions)` (src/print.c:1111-1139).  The
//!   row below customizes `file-error`'s own list to omit it; GNU then does
//!   not take the file-error formatting path, while the old
//!   "the clause names the signalled symbol" shortcut did.
//!
//! The remaining callers are pinned by GNU source and unit rows rather than a
//! batch oracle:
//! - `has_active_condition_handler_for_signal` is an internal predicate over
//!   the active clause stack; `active_condition_handler_uses_error_conditions_membership`
//!   (gd_b_signals) is its row.
//! - the debugger gate and `command_error_severity` use GNU `signal_quit_p`
//!   (src/eval.c:2199-2210); `signal_quit_predicate_has_gnu_identity_and_membership`
//!   is its row.
//! - the redisplay handler is `internal_condition_case_1 (..., list_of_error,
//!   ...)` with `list_of_error = list1 (Qerror)` (src/xdisp.c:18033-18037,
//!   38399); a redisplay row needs the TUI/GUI harness, which this slice does
//!   not run.
//! - `unlock-file` is `internal_condition_case_1 (unlock_file, file,
//!   list1 (Qfile_error), ...)` (src/filelock.c:717-720); provoking the
//!   native file-error is a filelock-specific harness.
//! - `command-error-default-function`'s `is_minibuffer_quit` is
//!   `Fmemq (Qminibuffer_quit, conditions)` (src/keyboard.c:1054-1057); its
//!   branch exits the batch session, so it is cited, not exercised here.
use crate::common;

#[test]
fn file_error_family_is_fmemq_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (put 'file-error 'error-conditions '(error))
  (list (error-message-string '(file-error "a" "b"))
        (condition-case e (signal 'file-error '("a" "b"))
          (error (error-message-string e)))))"#;
    // Filled from the attested oracle in refresh mode, then required to match
    // in verify mode.
    let expected = expect_test::expect![[
        r#""OK (\"File error: \\\"a\\\", \\\"b\\\"\" \"File error: \\\"a\\\", \\\"b\\\"\")""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}
