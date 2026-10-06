//! Call-target recording in compiled code (P2.1 C3/C4, `NEOVM_JIT_FEEDBACK`):
//! a leaf's non-constant call sites count and record through the `_prof`
//! shims, an `apply` site records the applied function, a speculated
//! mapping builtin its callback, and with the knob off nothing is recorded
//! (and no table is built).

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::feedback::{
    CallTarget, FeedbackMode, SiteShape, force_feedback_mode_for_test,
};
use crate::emacs_core::jit::{ReoptLevel, RuntimeState};
use crate::emacs_core::value::LambdaParams;

fn lexical_fn(nargs: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    f.seal_hand_assembled_ops();
    // The baseline: the tier whose generic calls record.
    f.jit_runtime()
        .set_reopt_level_for_test(ReoptLevel::BaselineOnly);
    f
}

/// `(lambda (x) (+ x 1))`, a source for the sites to call.
fn add1() -> ByteCodeFunction {
    lexical_fn(1, vec![Op::StackRef(0), Op::Add1, Op::Return], Vec::new())
}

fn compile(ev: &Context, f: &ByteCodeFunction) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles")
}

fn run(ev: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call(ev as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("expected a value, got {other:?}"),
    }
}

fn sources(target: CallTarget) -> Vec<*const RuntimeState> {
    match target {
        CallTarget::Sources(v) => v.iter().map(std::sync::Arc::as_ptr).collect(),
        other => panic!("expected sources, got {other:?}"),
    }
}

/// `(lambda (f x) (funcall f x))` records the source of every closure it
/// calls, counting each call, and the leaf holds its source's table.
#[test]
fn a_non_constant_call_site_records_and_counts_through_the_prof_shim() {
    force_feedback_mode_for_test(Some(FeedbackMode::Record));
    let mut ev = Context::new();
    let caller = lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Call(1), Op::Return],
        Vec::new(),
    );
    let leaf = compile(&ev, &caller);
    let proto = add1();
    for i in 0..3 {
        // A fresh `make-closure`-style instance each time: one source.
        let f = Value::make_bytecode(proto.clone());
        assert_eq!(
            run(&mut ev, &leaf, &[f, Value::make_int(i)]),
            Value::make_int(i + 1)
        );
    }
    let site = caller
        .jit_runtime()
        .call_sites()
        .expect("the compile built the table")
        .site_at(2)
        .expect("a non-constant site");
    assert_eq!(site.shape(), SiteShape::Callee);
    assert_eq!(site.count(), 3);
    assert_eq!(
        sources(site.target()),
        vec![proto.jit_runtime().state_ptr()]
    );
    assert!(
        leaf.feedback_holds
            .iter()
            .any(|s| std::ptr::eq(std::sync::Arc::as_ptr(s), caller.jit_runtime().state_ptr())),
        "the leaf keeps its source's table alive"
    );
    // A second source makes it polymorphic.
    let other = Value::make_bytecode(add1());
    run(&mut ev, &leaf, &[other, Value::make_int(1)]);
    assert_eq!(sources(site.target()).len(), 2);
    force_feedback_mode_for_test(None);
}

/// `(lambda (f l) (apply f l))`: the site records the applied function.
#[test]
fn an_apply_site_records_the_applied_function() {
    force_feedback_mode_for_test(Some(FeedbackMode::Record));
    let mut ev = Context::new();
    let caller = lexical_fn(
        2,
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::from_sym_id(intern("apply"))],
    );
    let leaf = compile(&ev, &caller);
    let proto = add1();
    let args = Value::list(vec![Value::make_int(41)]);
    let f = Value::make_bytecode(proto.clone());
    assert_eq!(run(&mut ev, &leaf, &[f, args]), Value::make_int(42));
    let site = caller
        .jit_runtime()
        .call_sites()
        .unwrap()
        .site_at(3)
        .unwrap();
    assert_eq!(site.shape(), SiteShape::ApplyArg);
    assert_eq!(site.count(), 1);
    assert_eq!(
        sources(site.target()),
        vec![proto.jit_runtime().state_ptr()]
    );
    force_feedback_mode_for_test(None);
}

/// `(lambda (f l) (mapcar f l))`: the speculated builtin call records its
/// callback first.
#[test]
fn a_mapping_builtin_records_its_callback() {
    force_feedback_mode_for_test(Some(FeedbackMode::Record));
    let mut ev = Context::new();
    let caller = lexical_fn(
        2,
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::from_sym_id(intern("mapcar"))],
    );
    let leaf = compile(&ev, &caller);
    let proto = add1();
    let f = Value::make_bytecode(proto.clone());
    let l = Value::list(vec![Value::make_int(1), Value::make_int(2)]);
    let out = run(&mut ev, &leaf, &[f, l]);
    assert_eq!(
        crate::emacs_core::print::print_value(&out),
        "(2 3)",
        "mapcar ran"
    );
    let site = caller
        .jit_runtime()
        .call_sites()
        .unwrap()
        .site_at(3)
        .unwrap();
    assert_eq!(site.shape(), SiteShape::CallbackArg);
    assert_eq!(site.count(), 1, "one record per mapcar call");
    assert_eq!(
        sources(site.target()),
        vec![proto.jit_runtime().state_ptr()]
    );
    force_feedback_mode_for_test(None);
}

/// With the knob off, a compile builds no table and nothing records.
#[test]
fn with_the_knob_off_nothing_records() {
    force_feedback_mode_for_test(Some(FeedbackMode::Off));
    let mut ev = Context::new();
    let caller = lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Call(1), Op::Return],
        Vec::new(),
    );
    let leaf = compile(&ev, &caller);
    let f = Value::make_bytecode(add1());
    assert_eq!(
        run(&mut ev, &leaf, &[f, Value::make_int(1)]),
        Value::make_int(2)
    );
    assert!(caller.jit_runtime().call_sites().is_none());
    assert!(leaf.feedback_holds.is_empty());
    force_feedback_mode_for_test(None);
}

/// `record` stops recording at a compiled site after its profiling window
/// (the count saturates there and a later target is not seen); `census`
/// records every call, and a transition after the window counts as late.
#[test]
fn compiled_sites_record_inside_their_window_unless_census() {
    use crate::emacs_core::jit::feedback::STABLE_WINDOW;
    for mode in [FeedbackMode::Record, FeedbackMode::Census] {
        force_feedback_mode_for_test(Some(mode));
        let mut ev = Context::new();
        let caller = lexical_fn(
            2,
            vec![Op::StackRef(1), Op::StackRef(1), Op::Call(1), Op::Return],
            Vec::new(),
        );
        let leaf = compile(&ev, &caller);
        let proto = add1();
        let f = Value::make_bytecode(proto.clone());
        run(&mut ev, &leaf, &[f, Value::make_int(1)]);
        let site = caller
            .jit_runtime()
            .call_sites()
            .unwrap()
            .site_at(2)
            .unwrap();
        site.set_count_for_test(STABLE_WINDOW - 1);
        run(&mut ev, &leaf, &[f, Value::make_int(1)]);
        assert_eq!(
            site.count(),
            STABLE_WINDOW,
            "{mode:?}: the last call in the window"
        );
        // After the window: another source.
        run(
            &mut ev,
            &leaf,
            &[Value::make_bytecode(add1()), Value::make_int(1)],
        );
        match mode {
            FeedbackMode::Record => {
                assert_eq!(site.count(), STABLE_WINDOW, "the window closed");
                assert_eq!(sources(site.target()).len(), 1, "not seen");
                assert_eq!(site.late(), 0);
            }
            _ => {
                assert_eq!(site.count(), STABLE_WINDOW + 1);
                assert_eq!(sources(site.target()).len(), 2, "seen");
                assert_eq!(site.late(), 1, "a late transition");
            }
        }
    }
    force_feedback_mode_for_test(None);
}
