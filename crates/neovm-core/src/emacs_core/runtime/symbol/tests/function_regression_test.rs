//! Regression tests for LispSymbol::function GNU-parity.
//!
//! All 5 tests pass today (the semantic is already GNU-equivalent —
//! `Qnil` and `None` both mean "unbound function"). They must continue
//! to pass after F1 flips the field type to a bare `Value` with
//! `Value::NIL` as the unbound sentinel.

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn make_ctx() -> Context {
    Context::new()
}

fn eval(ctx: &mut Context, src: &str) -> Value {
    ctx.eval_str(src).expect("eval")
}

#[test]
fn fboundp_unbound_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    assert_eq!(eval(&mut ctx, "(fboundp 'fn-unbound-a)"), Value::NIL);
}

#[test]
fn fset_then_fboundp_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(fset 'fn-foo 'bar)");
    assert_eq!(eval(&mut ctx, "(fboundp 'fn-foo)"), Value::T);
}

#[test]
fn fmakunbound_resets_fboundp() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(fset 'fn-mu 'bar)");
    assert_eq!(eval(&mut ctx, "(fboundp 'fn-mu)"), Value::T);
    eval(&mut ctx, "(fmakunbound 'fn-mu)");
    assert_eq!(eval(&mut ctx, "(fboundp 'fn-mu)"), Value::NIL);
}

#[test]
fn fset_to_nil_is_unbound() {
    // GNU semantic: Qnil in the function slot means unbound.
    // (fset 'foo nil) and (fboundp 'foo) should return nil.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(fset 'fn-fn-nil nil)");
    assert_eq!(eval(&mut ctx, "(fboundp 'fn-fn-nil)"), Value::NIL);
}

#[test]
fn symbol_function_survives_gc() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(fset 'fn-gc (lambda () 42))");
    ctx.gc_collect();
    assert_eq!(eval(&mut ctx, "(funcall 'fn-gc)"), Value::fixnum(42));
}
