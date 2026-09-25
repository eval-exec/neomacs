//! The debug leaf guard (design `p1-2-builtin-intrinsics` §6.2, §6.3): while
//! a leaf body runs, every GC safe point, Lisp entry and binding push the
//! leaf contract rules out panics instead of running; and a leaf that
//! changes evaluator state it does not declare is caught on exit.
//!
//! A real leaf cannot reach any of these (its borrow is shared), so each
//! test plays the lying leaf by hand: it holds a `LeafActive` marker and
//! then does what no leaf may do.

use super::*;
use crate::emacs_core::subr::leaf::{LeafActive, LeafOutcome};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The Bcall `gethash` leaf: declares READ_HEAP | MAY_SIGNAL only.
fn gethash_spec() -> &'static crate::emacs_core::subr::leaf::LeafSpec {
    &crate::emacs_core::builtins::leaves::GETHASH
}

/// Run `f` under a live leaf marker; true when the guard panicked. The
/// marker is released either way (its count drops on unwind).
fn panics_under_a_leaf(ctx: &mut Context, f: impl FnOnce(&mut Context)) -> bool {
    let active = LeafActive::enter(gethash_spec(), ctx);
    let panicked = catch_unwind(AssertUnwindSafe(|| f(ctx))).is_err();
    drop(active);
    panicked
}

#[test]
fn leaf_active_asserts_at_every_safe_point() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let car = Value::symbol("car");
    type GuardedEntry = Box<dyn FnOnce(&mut Context)>;
    let cases: Vec<(&str, GuardedEntry)> = vec![
        (
            "funcall",
            Box::new(move |ctx: &mut Context| {
                let _ = ctx.funcall_general(car, vec![Value::NIL]);
            }),
        ),
        (
            "eval",
            Box::new(|ctx: &mut Context| {
                let _ = ctx.eval_sub(Value::fixnum(1));
            }),
        ),
        (
            "a GC safe point",
            Box::new(|ctx: &mut Context| ctx.gc_safe_point_exact()),
        ),
        (
            "a specpdl push",
            Box::new(|ctx: &mut Context| {
                let _ = ctx.try_specbind(
                    crate::emacs_core::intern::intern("leaf-guard-var"),
                    Value::T,
                );
            }),
        ),
        (
            "a function-epoch bump",
            Box::new(|ctx: &mut Context| {
                ctx.obarray
                    .bump_function_epoch(crate::emacs_core::symbol::FunctionEpochBump::SubrRewrite);
            }),
        ),
        (
            "the bytecode interpreter",
            Box::new(|ctx: &mut Context| {
                let mut f = crate::emacs_core::bytecode::ByteCodeFunction::new(
                    LambdaParams::simple(vec![]),
                );
                f.ops = vec![
                    crate::emacs_core::bytecode::opcode::Op::Nil,
                    crate::emacs_core::bytecode::opcode::Op::Return,
                ];
                f.max_stack = 2;
                let _ = crate::emacs_core::bytecode::Vm::from_context(ctx).execute(&f, vec![]);
            }),
        ),
    ];
    for (site, f) in cases {
        assert!(
            panics_under_a_leaf(&mut ctx, f),
            "{site} ran under a live leaf marker"
        );
    }
    // Outside any leaf the same entries run.
    assert_eq!(
        ctx.funcall_general(car, vec![Value::NIL]).expect("car nil"),
        Value::NIL
    );
    ctx.gc_safe_point_exact();
}

#[test]
fn witnesses_catch_an_undeclared_buffer_write() {
    let mut ctx = Context::new();
    ctx.eval_str("(insert \"hello\")").expect("text");
    // Moving point is a buffer write `gethash` does not declare. (Straight
    // through the buffer: `goto-char` would be an `eval` the guard refuses.)
    let active = LeafActive::enter(gethash_spec(), &ctx);
    ctx.buffers
        .current_buffer_mut()
        .expect("a current buffer")
        .goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    let caught = catch_unwind(AssertUnwindSafe(|| active.exit(&ctx, LeafOutcome::Value)));
    assert!(caught.is_err(), "an undeclared point motion is caught");
}

#[test]
fn witnesses_catch_an_undeclared_allocation_on_a_value() {
    let ctx = Context::new();
    let active = LeafActive::enter(gethash_spec(), &ctx);
    let cell = Value::cons(Value::NIL, Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(cell);
    let caught = catch_unwind(AssertUnwindSafe(|| active.exit(&ctx, LeafOutcome::Value)));
    assert!(caught.is_err(), "a value path that allocates is caught");

    // An error path may allocate its signal data.
    let active = LeafActive::enter(gethash_spec(), &ctx);
    let _ = Value::cons(Value::NIL, Value::NIL);
    active.exit(&ctx, LeafOutcome::Signal);
}

#[test]
fn witnesses_catch_a_bounce_after_a_side_effect() {
    let mut ctx = Context::new();
    let active = LeafActive::enter(gethash_spec(), &ctx);
    // A binding pushed and left behind: the specpdl witness.
    ctx.specpdl.push(SpecBinding::GcRoot { value: Value::NIL });
    let caught = catch_unwind(AssertUnwindSafe(|| active.exit(&ctx, LeafOutcome::Generic)));
    assert!(
        caught.is_err(),
        "a Generic exit after a side effect is caught"
    );
    ctx.specpdl.pop();
}

#[test]
fn a_clean_leaf_call_passes_its_witnesses() {
    let mut ctx = Context::new();
    let table = ctx
        .eval_str("(let ((h (make-hash-table))) (puthash 1 2 h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let got = crate::emacs_core::subr::leaf::call_checked(
        gethash_spec(),
        &ctx,
        &[Value::fixnum(1), table],
    )
    .expect("a value");
    assert_eq!(got, Value::fixnum(2));
    let err = crate::emacs_core::subr::leaf::call_checked(
        gethash_spec(),
        &ctx,
        &[Value::fixnum(1), Value::fixnum(5)],
    );
    assert!(matches!(
        err,
        Err(crate::emacs_core::subr::leaf::LeafExit::Signal(_))
    ));
}
