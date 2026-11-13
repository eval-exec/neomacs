use super::*;

#[cfg(feature = "jit")]
use super::gd_b_native::{RawCallTestPolicy, assert_native_frames_warmed};

#[test]
fn compiled_signal_dispatch_precedes_binding_retirement() {
    #[cfg(feature = "jit")]
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(defvar gd-b-live 'outer)
         (defvar gd-b-seen nil)
         (defalias 'gd-b-f
           (byte-compile (lambda (x) (let ((gd-b-live 'inner)) (length x)))))
         (dotimes (_ 1000) (gd-b-f '(a)))",
    )
    .unwrap();
    #[cfg(feature = "jit")]
    assert_native_frames_warmed(&ev, &["gd-b-f"]);
    let result = ev.eval_str(
        "(let ((signal-hook-function
                 (lambda (_kind _data) (setq gd-b-seen gd-b-live))))
           (condition-case nil (gd-b-f 5) (error gd-b-seen)))",
    );
    assert_eq!(format_eval_result(&result), "OK inner");
    assert_eq!(ev.eval_str("gd-b-live").unwrap(), Value::symbol("outer"));
}

#[test]
fn compiled_handler_bind_observes_live_binding_and_can_throw() {
    #[cfg(feature = "jit")]
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(defvar gd-b-live 'outer)
         (defalias 'gd-b-f
           (byte-compile (lambda (x) (let ((gd-b-live 'inner)) (length x)))))
         (dotimes (_ 1000) (gd-b-f '(a)))",
    )
    .unwrap();
    #[cfg(feature = "jit")]
    assert_native_frames_warmed(&ev, &["gd-b-f"]);
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(list (catch 'gd-b-escape
                     (handler-bind-1 (lambda () (gd-b-f 5))
                                     '(error)
                                     (lambda (_) (throw 'gd-b-escape gd-b-live))))
                   gd-b-live)"
        )),
        "OK (inner outer)"
    );
}

#[test]
fn compiled_debugger_observes_leaf_debug_on_error_binding() {
    #[cfg(feature = "jit")]
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(defvar gd-b-live 'outer)
         (defvar gd-b-seen nil)
         (defalias 'gd-b-f
           (byte-compile
             (lambda (x) (let ((gd-b-live 'inner) (debug-on-error t)) (length x)))))
         (dotimes (_ 1000) (gd-b-f '(a)))",
    )
    .unwrap();
    #[cfg(feature = "jit")]
    assert_native_frames_warmed(&ev, &["gd-b-f"]);
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(let ((debugger (lambda (&rest args) (setq gd-b-seen (list gd-b-live args))))
                   (debug-ignored-errors nil))
               (condition-case err (gd-b-f 5)
                 ((debug error) (list (car err) gd-b-seen gd-b-live))))"
        )),
        "OK (wrong-type-argument (inner (error (wrong-type-argument sequencep 5))) outer)"
    );
}
