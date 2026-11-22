//! Signal `error-conditions` parity with GNU Emacs 31.1.
//!
//! GNU `signal_or_quit` reads the signalled symbol's `error-conditions` once
//! after the signal hook (src/eval.c:1978) and every clause of that signal
//! matches against that one list with `find_handler_clause`'s `Fmemq`/`t`
//! rule (src/eval.c:2241-2263).  There is no "the clause names the signalled
//! symbol" shortcut.
//!
//! These rows were red on Neomacs before the read-once + membership fix:
//! - `signal_conditions_read_once_oracle` (probe B): a handler-bind handler
//!   replaces the property mid-walk; the middle clause for a *parent* in the
//!   original list must still catch.
//! - `condition_clause_membership_oracle` (probe C): a customized list that
//!   omits the signalled symbol must not match that symbol's own clause.
//! - `handler_bind_mid_walk_keeps_conditions_oracle` (probe A, the F3 shape)
//!   passes through membership rather than the removed shortcut.
//!
//! Snapshot mode compares Neomacs with the inline GNU expectations.  Verify
//! mode runs the attested oracle first and then requires Neomacs to match it.
use crate::common;

#[test]
fn signal_conditions_read_once_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (define-error 'peer-mid "Peer mid" '(error))
  (define-error 'peer-other "Peer other" '(error))
  (define-error 'peer-walk-err2 "Peer walk 2" '(peer-mid error))
  (condition-case _outer
      (condition-case _mid
          (handler-bind ((peer-walk-err2
                          (lambda (_e)
                            (put 'peer-walk-err2 'error-conditions
                                 '(peer-other error)))))
            (signal 'peer-walk-err2 nil))
        (peer-mid 'middle-caught))
    (error 'outer-caught)))"#;
    let expected = expect_test::expect![[r#""OK middle-caught""#]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn condition_clause_membership_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (define-error 'probe-sym "Probe" '(error))
  (put 'probe-sym 'error-conditions '(some-parent error))
  (condition-case _e
      (signal 'probe-sym nil)
    (probe-sym 'self-clause-caught)
    (error 'error-caught)))"#;
    let expected = expect_test::expect![[r#""OK error-caught""#]];
    common::assert_oracle_parity_expect(form, expected);
}

/// GNU's invalid-error-symbol path re-enters `signal_or_quit`, so the hook
/// runs once for the original undefined symbol and once for the fresh
/// `error` signal (src/eval.c:1978-1980).
#[test]
fn invalid_error_symbol_reissues_with_hook_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((calls nil))
  (let ((signal-hook-function
         (lambda (sym data) (push (list sym data) calls))))
    (condition-case _e (signal 'no-such-error-sym nil) (error nil))
    (list (length calls) (reverse calls))))"#;
    let expected = expect_test::expect![[
        r#""OK (2 ((no-such-error-sym nil) (error (\"Invalid error symbol\" no-such-error-sym))))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn handler_bind_mid_walk_keeps_conditions_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (define-error 'peer-walk-err "Peer walk" '(error))
  (define-error 'peer-other "Peer other" '(error))
  (condition-case _outer
      (condition-case _mid
          (handler-bind ((peer-walk-err
                          (lambda (_e)
                            (put 'peer-walk-err 'error-conditions
                                 '(peer-other error)))))
            (signal 'peer-walk-err nil))
        (peer-walk-err 'middle-caught))
    (error 'outer-caught)))"#;
    let expected = expect_test::expect![[r#""OK middle-caught""#]];
    common::assert_oracle_parity_expect(form, expected);
}
