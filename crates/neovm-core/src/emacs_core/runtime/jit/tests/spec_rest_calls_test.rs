//! Speculated calls of `&rest` callees.
//!
//! A `&rest` callee -- every `cl-generic` dispatcher, `bindat-get-field` --
//! takes the spec shim's fast path: its frame is built in the shim's buffer
//! with the tail consed into the last slot, as the interpreter's `run_frame`
//! (GNU `setup_frame`'s `Flist`, src/bytecode.c:545-546) builds it, while the backtrace frame
//! records the call as made -- the SYMBOL with the call's own arguments, as
//! GNU's `Bcall` records them (`record_in_backtrace (call_fun = TOP)`,
//! src/bytecode.c:792-796).

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;

fn shim_fast() -> u64 {
    SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed)
}

/// Every shape of a `&rest` frame: missing optionals, an empty tail, a
/// one- and a three-element tail, and a callee with nothing but `&rest`.
const REST_CALLS: &str = r#"(progn
  (defvar neovm--rest-seen nil)
  (defun neovm--rest-f (a &optional b &rest r)
    (when (eq a 'show)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (when (and (symbolp f) (null neovm--rest-seen)
                    (string-prefix-p "neovm--rest-" (symbol-name f)))
           (setq neovm--rest-seen (list f args))))))
    (list a b r))
  (defun neovm--rest-only (&rest r) (if r (length r) r))
  (defun neovm--rest-1 (x) (neovm--rest-f x))
  (defun neovm--rest-2 (x) (neovm--rest-f x 2))
  (defun neovm--rest-3 (x) (neovm--rest-f x 2 3))
  (defun neovm--rest-5 (x) (neovm--rest-f x 2 3 4 5))
  (defun neovm--rest-0 () (neovm--rest-only))
  (defun neovm--rest-4 (x) (neovm--rest-only x x x x))
  (dolist (f '(neovm--rest-f neovm--rest-only neovm--rest-1 neovm--rest-2
               neovm--rest-3 neovm--rest-5 neovm--rest-0 neovm--rest-4))
    (byte-compile f))
  (dotimes (i 6000)
    (neovm--rest-1 i) (neovm--rest-2 i) (neovm--rest-3 i) (neovm--rest-5 i)
    (neovm--rest-0) (neovm--rest-4 i)))"#;

fn warmed() -> Context {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    // An engagement test: the force harness's point is that no call takes
    // the fast path.
    force_slow_spec_for_test(Some(false));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(REST_CALLS).expect("defined and warmed");
    ev
}

/// Run CALL, which must reach the callee through the shim's fast path, and
/// print its value.
fn fast_call(ev: &mut Context, call: &str) -> String {
    let before = shim_fast();
    let value = ev.eval_str(call).expect("runs");
    assert!(
        shim_fast() > before,
        "{call}: the spec fast path ran the call"
    );
    print_value(&value)
}

#[test]
fn rest_callees_take_the_spec_fast_path_with_interpreter_frames() {
    let mut ev = warmed();
    assert_eq!(fast_call(&mut ev, "(neovm--rest-1 1)"), "(1 nil nil)");
    assert_eq!(fast_call(&mut ev, "(neovm--rest-2 1)"), "(1 2 nil)");
    assert_eq!(fast_call(&mut ev, "(neovm--rest-3 1)"), "(1 2 (3))");
    assert_eq!(fast_call(&mut ev, "(neovm--rest-5 1)"), "(1 2 (3 4 5))");
    assert_eq!(fast_call(&mut ev, "(neovm--rest-0)"), "nil");
    assert_eq!(fast_call(&mut ev, "(neovm--rest-4 'x)"), "4");
    // A fresh tail per call: the callee may keep and mutate it.
    assert_eq!(
        print_value(
            &ev.eval_str("(eq (nth 2 (neovm--rest-5 1)) (nth 2 (neovm--rest-5 1)))")
                .expect("runs")
        ),
        "nil"
    );
}

/// The callee's frame records what the caller called -- the symbol with
/// the call's own arguments -- not the frame the shim built for the leaf.
#[test]
fn a_rest_callee_frame_records_the_symbol_and_the_calls_arguments() {
    let mut ev = warmed();
    ev.eval_str("(setq neovm--rest-seen nil)").expect("reset");
    fast_call(&mut ev, "(neovm--rest-5 'show)");
    let seen = ev.eval_str("neovm--rest-seen").expect("recorded");
    assert_eq!(print_value(&seen), "(neovm--rest-f (show 2 3 4 5))");
}

/// The consed tail is the callee's argument and survives a collection
/// inside the callee: the leaf holds it from its entry on.
#[test]
fn a_rest_tail_survives_a_collection_in_the_callee() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    force_slow_spec_for_test(Some(false));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        r#"(progn
  (defvar neovm--rest-gc nil)
  (defun neovm--rest-keep (&rest r)
    (when neovm--rest-gc (garbage-collect))
    (list r (length r)))
  (defun neovm--rest-caller (x)
    (neovm--rest-keep (list x) (cons x x) (make-string 3 ?a) (vector x)))
  (byte-compile 'neovm--rest-keep)
  (byte-compile 'neovm--rest-caller)
  (dotimes (i 6000) (neovm--rest-caller i)))"#,
    )
    .expect("defined and warmed");
    let value = fast_call(&mut ev, "(let ((neovm--rest-gc t)) (neovm--rest-caller 9))");
    assert_eq!(value, r#"(((9) (9 . 9) "aaa" [9]) 4)"#);
}
