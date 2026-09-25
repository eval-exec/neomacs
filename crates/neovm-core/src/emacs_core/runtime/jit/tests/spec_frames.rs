//! Backtrace frames of speculated calls (design `p1-1-direct-native-calls`
//! 0b, P1.0 S1.3 = U0.2): a compiled caller's call of a symbol records the
//! SYMBOL, as GNU's `Bcall` does (`record_in_backtrace (call_fun = TOP)`,
//! src/bytecode.c:792-796), not the byte-code object the symbol names -- so
//! `mapbacktrace` and `backtrace-frame` see what the interpreter shows -- and
//! the object stays alive while it runs even after its symbol is redefined
//! (GNU's bytecode frame holds `fp->fun`, src/bytecode.c:518).

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::print::print_value;

/// A chain of compiled callers over every frame shape the speculated path
/// pushes (one, two and three arguments), warmed until every caller runs
/// natively, with a callee that walks the backtrace.
const CHAIN: &str = r#"(progn
  (defvar neovm--fr-seen nil)
  (defun neovm--fr-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--fr-seen (nreverse fs))
      (list (backtrace-frame 0 'neovm--fr-show) (backtrace-frame 1 'neovm--fr-show))))
  (defun neovm--fr-inner (x) (if (eq x 'show) (neovm--fr-show) (+ x 1)))
  (defun neovm--fr-outer (x y) (if y (neovm--fr-inner x) 0))
  (defun neovm--fr-mid3 (a b c) (list (neovm--fr-inner a) b c))
  (defun neovm--fr-top (x) (neovm--fr-mid3 x 1 2))
  (dolist (f '(neovm--fr-show neovm--fr-inner neovm--fr-outer neovm--fr-mid3 neovm--fr-top))
    (byte-compile f))
  (dotimes (i 6000) (neovm--fr-outer i 1) (neovm--fr-top i)))"#;

/// The innermost frames `neovm--fr-show` saw, and what `backtrace-frame`
/// answered from inside it.
fn observe(ev: &mut Context, call: &str) -> (String, String) {
    let spec_calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    let frames = ev.eval_str(call).expect("runs");
    assert!(
        SPEC_CALL_COUNT.load(Ordering::Relaxed) > spec_calls,
        "the chain ran through speculated calls"
    );
    let seen = ev
        .eval_str("(seq-take neovm--fr-seen 3)")
        .expect("frames recorded");
    (print_value(&seen), print_value(&frames))
}

#[test]
fn speculated_calls_record_the_called_symbol_in_their_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(CHAIN).expect("chain defined and warmed");
    let (seen, frames) = observe(&mut ev, "(neovm--fr-outer 'show 7)");
    assert_eq!(
        seen, "((neovm--fr-show nil) (neovm--fr-inner (show)) (neovm--fr-outer (show 7)))",
        "one- and two-argument frames name their symbols"
    );
    assert_eq!(
        frames, "((t neovm--fr-show) (t neovm--fr-inner show))",
        "backtrace-frame finds the callee by symbol and shows the caller's symbol"
    );
    let (seen, _) = observe(&mut ev, "(neovm--fr-top 'show)");
    assert_eq!(
        seen, "((neovm--fr-show nil) (neovm--fr-inner (show)) (neovm--fr-mid3 (show 1 2)))",
        "a three-argument frame names its symbol"
    );
}

/// A compiled activation whose symbol is redefined under it keeps its
/// object: the frames record only the symbol, so the redefinition pins what
/// the cell held while a frame records the symbol, and a collection inside
/// the activation must not free it. Observed through a weak table, as GNU
/// would show it (its bytecode frames hold `fp->fun`); once the activations
/// are gone, the next collection lets it go.
#[test]
fn a_running_function_outlives_its_redefinition_and_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        r#"(progn
  (defvar neovm--pin-tab (make-hash-table :test 'eq :weakness 'key))
  (defvar neovm--pin-armed nil)
  (defvar neovm--pin-during nil)
  (fset 'neovm--pin-f
        (byte-compile
         (lambda (n)
           (if (= n 0)
               (if neovm--pin-armed
                   (progn
                     (fset 'neovm--pin-f #'ignore)
                     (garbage-collect)
                     (setq neovm--pin-during (hash-table-count neovm--pin-tab))
                     0)
                 0)
             (1+ (neovm--pin-f (1- n)))))))
  (fset 'neovm--pin-g (byte-compile (lambda (n) (neovm--pin-f n))))
  (dotimes (_ 6000) (neovm--pin-g 3)))"#,
    )
    .expect("defined and warmed");
    ev.eval_str("(puthash (symbol-function 'neovm--pin-f) t neovm--pin-tab)")
        .expect("weakly held");
    let spec_calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    let result = ev
        .eval_str("(let ((neovm--pin-armed t)) (neovm--pin-g 3))")
        .expect("runs");
    assert!(
        SPEC_CALL_COUNT.load(Ordering::Relaxed) > spec_calls,
        "the recursion ran through speculated calls"
    );
    assert_eq!(print_value(&result), "3", "the old definition finished");
    let during = ev.eval_str("neovm--pin-during").expect("recorded");
    assert_eq!(
        print_value(&during),
        "1",
        "the running definition survived the collection under it"
    );
    assert!(
        !cache::redefined_pins_for_test().is_empty(),
        "the redefinition pinned the old definition"
    );
    let after = ev
        .eval_str("(progn (garbage-collect) (hash-table-count neovm--pin-tab))")
        .expect("collects");
    assert_eq!(
        print_value(&after),
        "0",
        "with no frame left, the old definition is collectable"
    );
    assert!(
        cache::redefined_pins_for_test().is_empty(),
        "and its pin is gone"
    );
}
