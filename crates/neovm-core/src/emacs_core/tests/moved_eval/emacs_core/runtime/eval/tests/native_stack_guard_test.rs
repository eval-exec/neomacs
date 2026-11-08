//! Ruling-1 native-stack guard regression (lead 2026-11-08 13:12Z).
//!
//! The evaluator recurses on the native thread stack (byte-compiled calls in
//! the Tier-0 VM and JIT leaves, interpreted calls through eval_sub), and
//! probes it periodically to continue on a fresh stacker segment.  The probe
//! constants were tuned for release frames; with debug frames one 16-level
//! sampling interval consumes more than the 128 KiB red zone, so on the
//! 8 MiB macOS main thread the process reached the kernel/Rust guard page
//! before any probe could grow, and died of SIGABRT with no Lisp error.
//! Measured on arm64 (macmini, debug binary, 8 MiB main-thread stack): the
//! abort came at Lisp depth ~120, with debug frames consuming tens of KiB
//! per level, so one 16-level sampling interval exceeds the 128 KiB red
//! zone; the instrumented run logged ZERO probe-low, grow and
//! check-exhausted events before it aborted, and the same abort occurs with
//! NEOVM_JIT=0, so the interpreter/VM recursion path is affected too.
//!
//! These tests are the red-before/green-after pair for the fix.  They run in
//! the debug lib-test binary (nextest memory wrapper unsets RUST_MIN_STACK,
//! so the test thread stack is small) and assert GNU-compatible clean
//! outcomes; a native stack overflow aborts the whole test process, which is
//! exactly the regression.

use super::*;

/// A byte-compiled self-recursion below GNU's bytecode-stack depth completes
/// with the value GNU returns.  GNU 31.1 (a360712c9d27) on the same form:
/// `result: 30000` (its 4 MiB bytecode stack is not reached).  Neo must
/// continue on grown segments instead of dying on the thread stack.
#[test]
fn byte_compiled_recursion_below_the_bytecode_budget_completes_in_debug() {
    let mut ev = runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defun nsg-deep (n) (if (= n 0) 0 (1+ (nsg-deep (1- n)))))
             (byte-compile 'nsg-deep)
             (dotimes (_ 40) (nsg-deep 100))
             (let ((max-lisp-eval-depth most-positive-fixnum))
               (nsg-deep 30000)))"#,
    );
    assert_eq!(format_eval_result(&result), "OK 30000");
}

/// The interpreted (non-byte-compiled) recursion is not bounded by GNU's
/// bytecode-stack budget: it must end at max-lisp-eval-depth on grown
/// segments, never on the thread stack.  This is the native-stack test the
/// ruling-2 4 MiB budget does not bound.
#[test]
fn interpreted_recursion_ends_at_max_depth_not_at_the_thread_stack() {
    let mut ev = runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defun nsg-walk (n) (if (= n 0) 0 (1+ (nsg-walk (1- n)))))
             (let ((max-lisp-eval-depth 300000))
               (condition-case err (nsg-walk 400000) (error err))))"#,
    );
    let rendered = format_eval_result(&result);
    assert!(
        rendered.starts_with("OK (excessive-lisp-nesting "),
        "expected a clean depth error, got {rendered}"
    );
}

/// The Lisp-thread kind of the same #92 coverage: a `make-thread` body runs
/// interpreted recursion on its own execution context, which the 4 MiB
/// bytecode-stack budget does not bound either. The clean Lisp depth error
/// must arrive through `thread-join`, never a native signal.
#[test]
fn interpreted_recursion_in_a_lisp_thread_ends_at_max_depth() {
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defun nsg-walk-thread (n)
               (if (= n 0) 0 (1+ (nsg-walk-thread (1- n)))))
             (thread-join
              (make-thread
               (lambda ()
                 (let ((max-lisp-eval-depth 300000))
                   (condition-case err (nsg-walk-thread 400000) (error err)))))))"#,
    );
    let rendered = format_eval_result(&result);
    assert!(
        rendered.starts_with("OK (excessive-lisp-nesting "),
        "expected a clean depth error from the Lisp thread, got {rendered}"
    );
}
