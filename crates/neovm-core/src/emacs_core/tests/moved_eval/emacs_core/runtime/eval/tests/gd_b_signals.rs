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

/// GNU reads `error-conditions` once at `signal_or_quit` (eval.c:1978) and
/// every clause of that signal uses that one list.  A `handler-bind` handler
/// that replaces the property mid-walk must not change the list the later
/// `condition-case` clauses match against.  The clause here is `peer-mid`,
/// which is not the signalled symbol, so the `condition_sym == signal_sym`
/// shortcut cannot mask the re-read.
#[test]
fn signal_conditions_are_read_once_per_signal() {
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(define-error 'peer-mid \"Peer mid\" '(error))
         (define-error 'peer-other \"Peer other\" '(error))
         (define-error 'peer-walk-err2 \"Peer walk 2\" '(peer-mid error))",
    )
    .unwrap();
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(condition-case _outer
                 (condition-case _mid
                     (handler-bind ((peer-walk-err2
                                     (lambda (_e)
                                       (put 'peer-walk-err2 'error-conditions
                                            '(peer-other error)))))
                       (signal 'peer-walk-err2 nil))
                   (peer-mid 'middle-caught))
               (error 'outer-caught))"
        )),
        "OK middle-caught"
    );
}

/// GNU's `find_handler_clause` is `Fmemq` of the clause's handler in the
/// signalled symbol's `error-conditions` list (eval.c:2241-2263).  It has no
/// "the clause names the signalled symbol" shortcut: a customized list that
/// omits the symbol does not catch its own clause.
#[test]
fn condition_clause_matches_only_by_error_conditions_membership() {
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(define-error 'probe-sym \"Probe\" '(error))
         (put 'probe-sym 'error-conditions '(some-parent error))",
    )
    .unwrap();
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(condition-case _e
                 (signal 'probe-sym nil)
               (probe-sym 'self-clause-caught)
               (error 'error-caught))"
        )),
        "OK error-caught"
    );
}

/// The F3 shape.  GNU prints `middle-caught` because the conditions list read
/// before the walk still contains `peer-walk-err`; this row must keep passing
/// through the membership path (not through the removed shortcut).
#[test]
fn handler_bind_mid_walk_put_keeps_the_original_conditions() {
    let mut ev = runtime_startup_context();
    ev.eval_str(
        "(define-error 'peer-walk-err \"Peer walk\" '(error))
         (define-error 'peer-other \"Peer other\" '(error))",
    )
    .unwrap();
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(condition-case _outer
                 (condition-case _mid
                     (handler-bind ((peer-walk-err
                                     (lambda (_e)
                                       (put 'peer-walk-err 'error-conditions
                                            '(peer-other error)))))
                       (signal 'peer-walk-err nil))
                   (peer-walk-err 'middle-caught))
               (error 'outer-caught))"
        )),
        "OK middle-caught"
    );
}

/// GNU `signal_or_quit` re-enters itself for an undefined condition: the
/// first signal's hook runs, `Fget` returns nil (src/eval.c:1978), and
/// `signal_error ("Invalid error symbol", ...)` (1979-1980) raises a fresh
/// ordinary `error`, whose hook runs with that error object.  Neomacs used to
/// run the hook once; the attested oracle prints two calls.
#[test]
fn invalid_error_symbol_reissues_with_its_own_hook_read() {
    let mut ev = runtime_startup_context();
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(let ((calls nil))
               (let ((signal-hook-function
                      (lambda (sym data) (push (list sym data) calls))))
                 (condition-case _e (signal 'no-such-error-sym nil) (error nil))
                 (list (length calls) (reverse calls))))"
        )),
        "OK (2 ((no-such-error-sym nil) (error (\"Invalid error symbol\" no-such-error-sym))))"
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
