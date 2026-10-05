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
