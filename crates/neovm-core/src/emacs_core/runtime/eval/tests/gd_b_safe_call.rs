//! Native safe calls use GNU's catch-all condition handler, not a dynamic
//! `inhibit-debugger` binding (eval.c:3230-3241).

use super::*;

fn safe_call_context() -> Context {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.eval_str(
        r#"(defvar gd-b-safe-events nil)
           (defvar gd-b-safe-hooks nil)
           (defvar gd-b-safe-debug nil)
           (setq signal-hook-function
                 (lambda (symbol data)
                   (setq gd-b-safe-hooks
                         (cons (list symbol data inhibit-redisplay inhibit-debugger)
                               gd-b-safe-hooks)))
                 debugger (lambda (&rest args)
                            (setq gd-b-safe-debug (cons args gd-b-safe-debug))))"#,
    )
    .expect("safe-call observers");
    ctx
}

#[test]
fn gd_b_safe_call_barrier_stops_outer_handlers_after_inner_handlers() {
    let mut ctx = safe_call_context();
    let outer = ctx
        .eval_str("(lambda (_) (setq gd-b-safe-events (cons 'outer gd-b-safe-events)))")
        .unwrap();
    ctx.push_condition_frame(ConditionFrame::HandlerBind {
        conditions: Value::symbol("error"),
        handler: outer,
        mute_span: 0,
    });
    let callback = ctx
        .eval_str(
            r#"(lambda ()
                 (handler-bind-1
                  (lambda () (signal 'error '("safe failure")))
                  '(error)
                  (lambda (_)
                    (setq gd-b-safe-events
                          (cons (list 'inner inhibit-redisplay inhibit-debugger)
                                gd-b-safe-events)))))"#,
        )
        .unwrap();
    let spec_depth = ctx.specpdl.len();
    assert_eq!(ctx.safe_funcall(callback, vec![]).unwrap(), Value::NIL);
    assert_eq!(ctx.specpdl.len(), spec_depth);
    assert_eq!(ctx.condition_stack.len(), 1);
    assert_eq!(
        format_eval_result(&ctx.eval_str("(list gd-b-safe-events gd-b-safe-hooks gd-b-safe-debug inhibit-redisplay inhibit-debugger)")),
        "OK (((inner t nil)) ((error (\"safe failure\") t nil)) nil nil nil)"
    );
}

#[test]
fn gd_b_safe_call_debug_on_signal_overrides_the_qt_barrier() {
    for (debug_on_signal, inhibit_debugger, expected) in [
        (false, false, "OK nil"),
        (true, false, "OK ((error (error \"safe failure\")))"),
        (false, true, "OK nil"),
        (true, true, "OK nil"),
    ] {
        let mut ctx = safe_call_context();
        ctx.assign("debug-on-error", Value::T);
        ctx.assign("debug-on-signal", Value::bool(debug_on_signal));
        ctx.assign("inhibit-debugger", Value::bool(inhibit_debugger));
        let callback = ctx
            .eval_str("(lambda () (signal 'error '(\"safe failure\")))")
            .unwrap();
        assert_eq!(ctx.safe_funcall(callback, vec![]).unwrap(), Value::NIL);
        assert_eq!(
            format_eval_result(&ctx.eval_str("gd-b-safe-debug")),
            expected
        );
        assert_eq!(
            ctx.eval_str("inhibit-debugger").unwrap(),
            Value::bool(inhibit_debugger)
        );
        assert!(ctx.condition_stack.is_empty());
        assert!(ctx.specpdl.is_empty());
    }
}

#[test]
fn gd_b_safe_call_qt_handler_catches_quit_and_non_error_conditions() {
    for callback_source in [
        "(lambda () (signal 'quit nil))",
        "(lambda () (signal 'gd-b-safe-condition '(payload)))",
    ] {
        let mut ctx = Context::new();
        ctx.eval_str("(put 'gd-b-safe-condition 'error-conditions '(gd-b-safe-condition))")
            .unwrap();
        let callback = ctx.eval_str(callback_source).unwrap();
        assert_eq!(ctx.safe_funcall(callback, vec![]).unwrap(), Value::NIL);
        assert!(ctx.condition_stack.is_empty());
        assert!(ctx.specpdl.is_empty());
    }
}

#[test]
fn gd_b_safe_call_preserves_values_and_nonlocal_throws() {
    fn call_safe(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        ctx.safe_funcall(args[0], vec![])
    }
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::new(
        "gd-b-safe-call",
        NativeFn::ContextVec(call_safe),
        SubrArity::new(1, Some(1)),
    ));
    assert_eq!(
        format_eval_result(&ctx.eval_str(
            "(list (gd-b-safe-call (lambda () 42)) (catch 'gd-b-safe-escape (gd-b-safe-call (lambda () (throw 'gd-b-safe-escape 'escaped))) 'swallowed))"
        )),
        "OK (42 escaped)"
    );
    assert!(ctx.condition_stack.is_empty());
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn gd_b_safe_call_its_own_binding_is_outside_the_qt_handler() {
    for operation in ["let", "unlet"] {
        let mut ctx = Context::new();
        let callback = ctx.eval_str("(lambda () 42)").unwrap();
        ctx.eval_str(&format!(
            r#"(add-variable-watcher
                 'inhibit-redisplay
                 (lambda (_symbol _value operation _where)
                   (if (eq operation '{operation})
                     (signal 'error '("safe binding")))))"#,
        ))
        .unwrap();
        let flow = ctx
            .safe_funcall(callback, vec![])
            .expect_err("binding signal must escape");
        assert_eq!(
            crate::emacs_core::error::format_flow_with_eval(&ctx, &flow),
            "(error (\"safe binding\"))"
        );
        assert!(ctx.condition_stack.is_empty());
        assert!(ctx.specpdl.is_empty());
    }
}
