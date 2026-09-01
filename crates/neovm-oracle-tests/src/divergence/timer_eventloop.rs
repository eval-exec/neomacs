//! Divergence tests: timer, idle-timer, and event loop stubs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_timer_functions_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'run-at-time)
  (fboundp 'run-with-timer)
  (fboundp 'run-with-idle-timer)
  (fboundp 'cancel-timer)
  (fboundp 'cancel-function-timers))"#,
        expect,
    );
}

#[test]
fn divergence_current_idle_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function timep)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'current-idle-time)
  (fboundp 'current-time)
  (fboundp 'float-time)
  (timep (current-time))
  (float-time (current-time)))"#,
        expect,
    );
}

#[test]
fn divergence_time_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp (format-time-string "%Y-%m-%d"))
  (stringp (format-time-string "%H:%M:%S" nil t))
  (stringp (format-time-string "%s"))
  (> (length (format-time-string "%Y-%m-%d %T")) 5))"#,
        expect,
    );
}

#[test]
fn divergence_time_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((t1 (current-time))
        (t2 (time-add t1 60)))
  (list (time-less-p t1 t2)
        (>= (float-time (time-subtract t2 t1)) 59)
        (time-equal-p t1 t1)))"#,
        expect,
    );
}

#[test]
fn divergence_time_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 2024 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (consp (parse-time-string "2024-01-15 10:30:00"))
  (decoded-time-year (parse-time-string "2024-01-15"))
  (decoded-time-month (parse-time-string "March 15, 2024")))"#,
        expect,
    );
}

#[test]
fn divergence_encode_decode_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1705322730.0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((encoded (encode-time 30 45 12 15 1 2024 t)))
  (list (consp encoded)
        (float-time encoded)
        (>= (float-time encoded) 0)))"#,
        expect,
    );
}

#[test]
fn divergence_sleep_for_exists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'sleep-for)
  (fboundp 'sit-for)
  (subrp (symbol-function 'sit-for)))"#,
        expect,
    );
}

#[test]
fn divergence_accept_process_output_exists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'accept-process-output)
  (fboundp 'waiting-for-user-input-p)
  (fboundp 'input-pending-p))"#,
        expect,
    );
}

#[test]
fn divergence_timer_throw_propagates_to_outer_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK thrown-from-timer""#]];
    // A non-local `throw` raised from inside a timer callback must propagate
    // out to the matching `(catch TAG …)` that surrounds the `accept-process-
    // output` wait loop.  GNU's `timer-event-handler` wraps the call in
    // `condition-case-unless-debug err … (error …)`, which catches `error`
    // signals only; a `throw` is not an error, so it propagates past the
    // handler to the outer `catch`.  This is the core of jsonrpc-request's
    // continuation protocol (eglot/copilot/lsp): the throw that completes the
    // synchronous request comes FROM a zero-delay `(run-at-time 0 nil …)`.
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case e
    (catch 'my-tag
      (run-at-time 0 nil (lambda () (throw 'my-tag 'thrown-from-timer)))
      (let ((n 0))
        (while (< n 60)
          (setq n (1+ n))
          (accept-process-output nil 0.05)))
      'NO-THROW-loop-finished)
  (error (cons 'ERR e)))"#,
        expect,
    );
}

#[test]
fn divergence_timer_jsonrpc_shape_throw_completes_wait() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK done""#]];
    // jsonrpc-request shape: a `(catch TAG …)` whose body launches a zero-delay
    // timer that `(throw TAG …)` and then spins in `(while t (accept-process-
    // output nil …))`.  The throw must unblock the otherwise-infinite wait by
    // propagating to the catch, yielding the thrown value.
    crate::common::assert_oracle_parity_expect(
        r#"(catch 'tag
  (run-at-time 0 nil (lambda () (throw 'tag 'done)))
  (while t (accept-process-output nil 1)))"#,
        expect,
    );
}

#[test]
fn divergence_timer_error_is_caught_not_propagated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK wait-finished-normally""#]];
    // An `error` (signal) raised from a timer callback must NOT propagate out of
    // the wait — `timer-event-handler`'s `condition-case-unless-debug err …
    // (error …)` swallows it (logging "Error running timer…").  The surrounding
    // wait loop continues and returns normally.  This guards against
    // over-correcting and propagating signals as well as throws.
    crate::common::assert_oracle_parity_expect(
        r#"(catch 'my-tag
  (run-at-time 0 nil (lambda () (error "boom from timer")))
  (let ((n 0))
    (while (< n 20)
      (setq n (1+ n))
      (accept-process-output nil 0.05)))
  'wait-finished-normally)"#,
        expect,
    );
}
