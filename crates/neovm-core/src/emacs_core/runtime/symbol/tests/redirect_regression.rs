//! Regression tests gating the Phase 10 SymbolValue deletion refactor.
//! Every test here must pass on every phase of the refactor.
//!
//! Context: uses `Context::new()` (full builtins, no Lisp-macro bootstrap).
//! Where GNU subr.el macros (with-temp-buffer, setq-default) would be needed,
//! equivalent builtin-only forms are substituted:
//!   - `with-temp-buffer BODY` → `(save-current-buffer (set-buffer BUF) (unwind-protect BODY (kill-buffer BUF)))`
//!   - `(setq-default SYM VAL)` → `(set-default 'SYM VAL)`
//! The tested semantics are identical; only the syntactic sugar differs.

use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn make_ctx() -> Context {
    Context::new()
}

fn eval(ctx: &mut Context, src: &str) -> Value {
    ctx.eval_str(src).expect("eval")
}

#[test]
fn plainval_void_after_makunbound_signals_void_variable() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setq foo-phase10-a 42)");
    assert_eq!(eval(&mut ctx, "foo-phase10-a"), Value::fixnum(42));
    eval(&mut ctx, "(makunbound 'foo-phase10-a)");
    let result = ctx.eval_str("foo-phase10-a");
    assert!(
        result.is_err(),
        "reading unbound symbol should signal void-variable, got {:?}",
        result
    );
}

#[test]
fn plainval_nil_is_distinct_from_unbound() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setq foo-phase10-b nil)");
    assert_eq!(eval(&mut ctx, "foo-phase10-b"), Value::NIL);
    assert_eq!(eval(&mut ctx, "(boundp 'foo-phase10-b)"), Value::T);
}

#[test]
fn cross_buffer_localized_isolation_matches_gnu() {
    // GNU 31 returns (9 1) for the equivalent with-temp-buffer form.
    // We expand with-temp-buffer manually using builtins only so the test
    // does not depend on the subr.el bootstrap.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    let result = eval(
        &mut ctx,
        r#"(let ((test-buf (get-buffer-create " *phase10-mlv-test*")))
             (setq vm-mlv-preserve-global 1)
             (save-current-buffer
               (set-buffer test-buf)
               (unwind-protect
                 (progn
                   (set (make-local-variable 'vm-mlv-preserve-global) 9)
                   (make-local-variable 'vm-mlv-preserve-global)
                   (list vm-mlv-preserve-global
                         (default-value 'vm-mlv-preserve-global)))
                 (kill-buffer test-buf))))"#,
    );
    let printed = crate::emacs_core::print::print_value(&result);
    assert_eq!(printed, "(9 1)");
}

#[test]
fn varalias_chain_forwards_reads_and_writes() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setq phase10-c 100)");
    eval(&mut ctx, "(defvaralias 'phase10-b 'phase10-c)");
    eval(&mut ctx, "(defvaralias 'phase10-a 'phase10-b)");
    assert_eq!(eval(&mut ctx, "phase10-a"), Value::fixnum(100));
    eval(&mut ctx, "(setq phase10-a 200)");
    assert_eq!(eval(&mut ctx, "phase10-c"), Value::fixnum(200));
}

#[test]
fn forwarded_conditional_slot_fill_column_works() {
    // Uses set-default (builtin) instead of setq-default (subr.el macro), and
    // expands with-temp-buffer using builtins only.
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(set-default 'fill-column 70)");
    let test_buf_name = " *phase10-fill-col-test*";
    let v = eval(
        &mut ctx,
        &format!(
            r#"(let ((test-buf (get-buffer-create "{}")))
                 (save-current-buffer
                   (set-buffer test-buf)
                   (unwind-protect
                     (progn
                       (make-local-variable 'fill-column)
                       (setq fill-column 40)
                       (list fill-column (default-value 'fill-column)))
                     (kill-buffer test-buf))))"#,
            test_buf_name
        ),
    );
    let printed = crate::emacs_core::print::print_value(&v);
    assert_eq!(printed, "(40 70)");
}

#[test]
fn plainval_survives_forced_gc() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();
    eval(&mut ctx, "(setq phase10-gc-test (cons 1 2))");
    let before = eval(&mut ctx, "phase10-gc-test");
    ctx.gc_collect();
    let after = eval(&mut ctx, "phase10-gc-test");
    let car_before = crate::emacs_core::value::eq_value(&before.cons_car(), &Value::fixnum(1));
    let cdr_after = crate::emacs_core::value::eq_value(&after.cons_cdr(), &Value::fixnum(2));
    assert!(car_before, "car should be 1 before GC");
    assert!(
        cdr_after,
        "cdr should be 2 after GC — stale if GC trace missed it"
    );
}
