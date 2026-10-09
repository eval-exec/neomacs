use super::*;

#[cfg(feature = "jit")]
use super::gd_b_native::{RawCallTestPolicy, assert_native_frames_warmed};

/// GNU bytecode.c:795 records the callee before execution; eval.c:1974
/// calls signal-hook-function before the signalling activation is unwound.
#[cfg(feature = "jit")]
#[test]
fn compiled_raw_signal_keeps_callee_backtrace_frame() {
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(defvar gd-b-backtrace-seen nil)
         (defalias 'gd-b-backtrace-leaf
           (eval '(byte-compile (lambda (x) (length x))) t))
         (defalias 'gd-b-backtrace-caller
           (eval '(byte-compile (lambda (x) (gd-b-backtrace-leaf x))) t))
         (dotimes (_ 2500) (gd-b-backtrace-caller '(a)))",
    )
    .unwrap();
    assert_native_frames_warmed(&ev, &["gd-b-backtrace-leaf", "gd-b-backtrace-caller"]);
    let result = ev.eval_str(
        "(let ((signal-hook-function
                 (lambda (_kind _data)
                   (setq gd-b-backtrace-seen
                         (backtrace-frame 0 'gd-b-backtrace-leaf)))))
           (condition-case nil (gd-b-backtrace-caller 5)
             (error gd-b-backtrace-seen)))",
    );
    assert_eq!(format_eval_result(&result), "OK (t gd-b-backtrace-leaf 5)");
}

/// The wider frame borrows the native caller's argument slot. Dispatch must
/// inspect it while that caller is still live, before retiring the callee.
#[cfg(feature = "jit")]
#[test]
fn compiled_raw_signal_keeps_native_argument_backtrace_frame() {
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(defvar gd-b-backtrace-seen nil)
         (defalias 'gd-b-backtrace-wide-leaf
           (eval '(byte-compile (lambda (_a _b x) (length x))) t))
         (defalias 'gd-b-backtrace-wide-caller
           (eval '(byte-compile (lambda (x) (gd-b-backtrace-wide-leaf 'a 'b x))) t))
         (dotimes (_ 2500) (gd-b-backtrace-wide-caller '(a)))",
    )
    .unwrap();
    assert_native_frames_warmed(
        &ev,
        &["gd-b-backtrace-wide-leaf", "gd-b-backtrace-wide-caller"],
    );
    let result = ev.eval_str(
        "(let ((signal-hook-function
                 (lambda (_kind _data)
                   (setq gd-b-backtrace-seen
                         (backtrace-frame 0 'gd-b-backtrace-wide-leaf)))))
           (condition-case nil (gd-b-backtrace-wide-caller 5)
             (error gd-b-backtrace-seen)))",
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (t gd-b-backtrace-wide-leaf a b 5)"
    );
}

/// GNU eval.c:1968-1975 runs the signal hook before the signalling
/// activation retires; bytecode.c:795 records all the callee's arguments.
/// The third heap argument is made by the compiled caller and is unrelated
/// to the fourth argument that signals. The observer must collect before
/// making a backtrace snapshot that could itself root that third argument.
#[cfg(feature = "jit")]
#[test]
fn compiled_raw_signal_gc_keeps_wide_native_argument_frame() {
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(defvar gd-b-gc-raw-seen nil)
           (defalias 'gd-b-gc-raw-leaf
             (eval '(byte-compile
                      (lambda (_a _b _payload x) (length x))) t))
           (defalias 'gd-b-gc-raw-caller
             (eval '(byte-compile
                      (lambda (x)
                        (gd-b-gc-raw-leaf
                         'a 'b (list 'payload (make-string 17 ?p)) x))) t))
           (dotimes (_ 2500) (gd-b-gc-raw-caller '(a)))"#,
    )
    .unwrap();
    assert_native_frames_warmed(&ev, &["gd-b-gc-raw-leaf", "gd-b-gc-raw-caller"]);
    let state = (
        ev.depth,
        ev.specpdl.len(),
        ev.condition_stack.len(),
        ev.bc_buf.len(),
    );
    let result = ev.eval_str(
        r#"(let ((signal-hook-function
                  (lambda (_kind _data)
                    (let ((before gcs-done))
                      (garbage-collect)
                      (make-list 4096 (cons 0 0))
                      (setq gd-b-gc-raw-seen
                            (list (> gcs-done before)
                                  (backtrace-frame 0 'gd-b-gc-raw-leaf)))))))
             (condition-case nil (gd-b-gc-raw-caller 5)
               (error gd-b-gc-raw-seen)))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        r#"OK (t (t gd-b-gc-raw-leaf a b (payload "ppppppppppppppppp") 5))"#
    );
    assert_eq!(
        (
            ev.depth,
            ev.specpdl.len(),
            ev.condition_stack.len(),
            ev.bc_buf.len(),
        ),
        state,
        "raw signal dispatch must retire the call and its roots exactly once"
    );
}

/// The framed leaf additionally owns a fresh inner dynamic value. GNU
/// eval.c:1968-1975 lets the observer collect and inspect that live extent
/// before cleanup_bytecode_frame retires it (bytecode.c:795-816).
#[cfg(feature = "jit")]
#[test]
fn compiled_framed_signal_gc_keeps_wide_arguments_and_inner_binding() {
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(defvar gd-b-gc-framed-seen nil)
           (defvar gd-b-gc-framed-live 'outer)
           (defalias 'gd-b-gc-framed-leaf
             (eval '(byte-compile
                      (lambda (_a _b _payload x)
                        (let ((gd-b-gc-framed-live (list 'inner (make-string 19 ?b))))
                          (length x)))) t))
           (defalias 'gd-b-gc-framed-caller
             (eval '(byte-compile
                      (lambda (x)
                        (gd-b-gc-framed-leaf
                         'a 'b (list 'payload (make-string 17 ?p)) x))) t))
           (dotimes (_ 2500) (gd-b-gc-framed-caller '(a)))"#,
    )
    .unwrap();
    assert_native_frames_warmed(&ev, &["gd-b-gc-framed-leaf", "gd-b-gc-framed-caller"]);
    let state = (
        ev.depth,
        ev.specpdl.len(),
        ev.condition_stack.len(),
        ev.bc_buf.len(),
    );
    let result = ev.eval_str(
        r#"(let ((signal-hook-function
                  (lambda (_kind _data)
                    (let ((before gcs-done))
                      (garbage-collect)
                      (make-list 4096 (cons 0 0))
                      (setq gd-b-gc-framed-seen
                            (list (> gcs-done before)
                                  (backtrace-frame 0 'gd-b-gc-framed-leaf)
                                  gd-b-gc-framed-live))))))
             (condition-case nil (gd-b-gc-framed-caller 5)
               (error gd-b-gc-framed-seen)))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        r#"OK (t (t gd-b-gc-framed-leaf a b (payload "ppppppppppppppppp") 5) (inner "bbbbbbbbbbbbbbbbbbb"))"#
    );
    assert_eq!(
        (
            ev.depth,
            ev.specpdl.len(),
            ev.condition_stack.len(),
            ev.bc_buf.len(),
        ),
        state,
        "framed signal dispatch must retire the call and its roots exactly once"
    );
    assert_eq!(
        ev.eval_str("gd-b-gc-framed-live").unwrap(),
        Value::symbol("outer"),
        "the callee's dynamic binding must be restored after the observer"
    );
}
