//! P0.12 §11 GC and re-entrant unwind tests, compiled in both configurations.

use super::EvalError;
use crate::emacs_core::eval::{Context, ShutdownRequest};
use crate::emacs_core::value::Value;

fn check_lisp_invariant(source: &str) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = true;
    let result = eval.eval_str(source).expect("unwind invariant evaluates");
    assert_eq!(result, Value::T, "the Lisp invariant must remain true");
}

#[test]
fn signal_payload_survives_gc_during_three_frame_unwind() {
    check_lisp_invariant(
        r#"
        (progn
          (fset 'flow-word-signal-inner '(lambda ()
            (unwind-protect
                (signal 'error (list (make-string 80 120)))
              (garbage-collect)
              (make-list 2048 "discard"))))
          (fset 'flow-word-signal-middle '(lambda ()
            (unwind-protect (flow-word-signal-inner)
              (garbage-collect)
              (make-list 2048 "discard"))))
          (fset 'flow-word-signal-outer '(lambda ()
            (unwind-protect (flow-word-signal-middle)
              (garbage-collect)
              (make-list 2048 "discard"))))
          (condition-case caught
              (flow-word-signal-outer)
            (error (and (eq (car caught) 'error)
                        (string-equal (car (cdr caught)) (make-string 80 120))))))
    "#,
    );
}

#[test]
fn throw_value_survives_gc_in_cleanup() {
    check_lisp_invariant(
        r#"
        (string-equal
          (catch 'flow-word-throw
            (unwind-protect
                (throw 'flow-word-throw (make-string 80 121))
              (garbage-collect)
              (make-list 2048 "discard")))
          (make-string 80 121))
    "#,
    );
}

#[test]
fn cleanup_signal_caught_inside_cleanup_keeps_outer_flow() {
    check_lisp_invariant(
        r#"
        (progn
          (setq flow-word-inner-signal-caught nil)
          (condition-case outer
              (unwind-protect
                  (signal 'error (list (make-string 80 111)))
                (condition-case inner
                    (signal 'error (list (make-string 80 105)))
                  (error (setq flow-word-inner-signal-caught
                               (string-equal (car (cdr inner)) (make-string 80 105)))))
                (garbage-collect)
                (make-list 2048 "discard"))
            (error (and flow-word-inner-signal-caught
                        (string-equal (car (cdr outer)) (make-string 80 111))))))
    "#,
    );
}

#[test]
fn cleanup_throw_replaces_outer_signal() {
    check_lisp_invariant(
        r#"
        (string-equal
          (catch 'flow-word-replacement
            (unwind-protect
                (signal 'error (list (make-string 80 111)))
              (garbage-collect)
              (throw 'flow-word-replacement (make-string 80 114))))
          (make-string 80 114))
    "#,
    );
}

#[test]
fn kill_emacs_inside_condition_case_is_not_caught() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let error = eval
        .eval_str(
            r#"
        (progn
          (setq flow-word-shutdown-handler-ran nil)
          (condition-case nil
              (kill-emacs 23)
            (error (setq flow-word-shutdown-handler-ran t))))
    "#,
        )
        .expect_err("kill-emacs must propagate shutdown");
    let EvalError::Shutdown(request) = error else {
        panic!("shutdown bypasses condition-case")
    };
    let expected = ShutdownRequest {
        exit_code: 23,
        restart: false,
    };
    assert_eq!(request, expected);
    assert_eq!(eval.shutdown_request(), Some(expected));
    assert_eq!(
        eval.obarray()
            .symbol_value("flow-word-shutdown-handler-ran"),
        Some(&Value::NIL)
    );
}
