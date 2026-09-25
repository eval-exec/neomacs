//! Closure source slots (P2.1 C5, `NEOVM_JIT_SPEC_SOURCES`): a call whose
//! recorded target is one closure source calls the source's armed leaf
//! through the slot; any other callee, and every call the fast path
//! declines, runs the site's generic call with the same result or signal.

use super::source_slots::{SOURCE_SLOT_DECLINED, SOURCE_SLOT_FAST_CALLS};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::feedback::{CallTarget, FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::{ReoptLevel, cache};
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
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f.jit_runtime()
        .set_reopt_level_for_test(ReoptLevel::BaselineOnly);
    f
}

/// `(lambda (f x) (funcall f x))`: the call of the argument is ops[2].
fn funcall_caller() -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Call(1), Op::Return],
        Vec::new(),
    )
}

/// `(lambda (x) (+ x k))` with `k` in its constant 0: a closure source
/// whose instances differ in that constant, as `make-closure` patches it.
fn adder(k: i64) -> ByteCodeFunction {
    lexical_fn(
        1,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(k)],
    )
}

/// An instance of `proto` with its constant 0 replaced (one source).
fn instance(proto: &ByteCodeFunction, k: i64) -> Value {
    let mut f = proto.clone();
    let mut consts: Vec<Value> = f.constants.iter().copied().collect();
    consts[0] = Value::make_int(k);
    f.constants = consts.into();
    Value::make_bytecode(f)
}

/// Compile `proto`'s source and arm its leaf slot, as its first native
/// run from the interpreter would. Its constant 0 is per instance, as
/// `make-closure` records a patched prefix (the leaf loads it through the
/// executing instance's constant base).
fn arm(ev: &mut Context, proto: &ByteCodeFunction) {
    assert_eq!(proto.jit_runtime().note_patched_prefix(1), None);
    let ctx = ev as *mut Context;
    let v = Value::make_bytecode(proto.clone());
    let got = crate::emacs_core::jit::try_run_compiled(ctx, proto, v, &[Value::make_int(0)])
        .expect("no signal");
    assert!(got.is_some(), "the source compiled");
    cache::arm_leaf_slot(ctx, proto);
    assert!(
        proto
            .jit_runtime()
            .armed_leaf_slot(cache::leaf_slot_epoch())
            .is_some()
    );
}

/// Record `target` at the caller's call site, as the interpreter would.
fn record(caller: &ByteCodeFunction, pc: usize, target: Value) {
    caller
        .jit_runtime()
        .call_sites_for(caller.executable_ops(), &caller.constants)
        .site_at(pc)
        .expect("a non-constant site")
        .observe(target);
}

fn compile(ev: &Context, f: &ByteCodeFunction) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles")
}

fn run(ev: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Result<Value, String> {
    match leaf.call(ev as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::Signal => Err(match take_pending_flow().expect("a flow") {
            crate::emacs_core::error::Flow::Signal(sig) => format!(
                "signal {} {:?}",
                sig.symbol_name(),
                sig.data
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
            ),
            other => format!("{other:?}"),
        }),
        other => panic!("expected a value or a signal, got {other:?}"),
    }
}

fn on(spec: bool) {
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    force_spec_sources_for_test(Some(spec));
}

fn off() {
    force_feedback_mode_for_test(None);
    force_spec_sources_for_test(None);
    force_slow_spec_for_test(None);
}

fn fast_calls() -> u64 {
    SOURCE_SLOT_FAST_CALLS.load(Ordering::Relaxed)
}

fn declined() -> u64 {
    SOURCE_SLOT_DECLINED.load(Ordering::Relaxed)
}

/// Instances of the recorded source take the slot: one fast call each,
/// every instance with its own captured constant.
#[test]
fn instances_of_the_recorded_source_call_through_the_slot() {
    on(true);
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    record(&caller, 2, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    assert_eq!(
        leaf.spec_slot_kinds
            .iter()
            .filter(|k| **k == SpecSlotKind::Source)
            .count(),
        1,
        "one source slot"
    );
    let before = fast_calls();
    for k in [10, 20, 30] {
        let f = instance(&proto, k);
        assert_eq!(
            run(&mut ev, &leaf, &[f, Value::make_int(5)]),
            Ok(Value::make_int(5 + k))
        );
    }
    assert_eq!(fast_calls() - before, 3, "every instance took the slot");
    // The leaf holds the source it speculated on.
    assert!(
        leaf.feedback_holds
            .iter()
            .any(|s| std::ptr::eq(std::sync::Arc::as_ptr(s), proto.jit_runtime().state_ptr()))
    );
    off();
}

/// Another source, a symbol and a non-function miss the guard and take the
/// generic call: same answers, no fast call; the misses reach the lattice.
#[test]
fn other_callees_miss_to_the_generic_call() {
    on(true);
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    record(&caller, 2, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    let before = fast_calls();
    let other = Value::make_bytecode(adder(100));
    assert_eq!(
        run(&mut ev, &leaf, &[other, Value::make_int(1)]),
        Ok(Value::make_int(101))
    );
    let not_fn = run(&mut ev, &leaf, &[Value::make_int(3), Value::make_int(1)]);
    assert!(
        not_fn.unwrap_err().contains("invalid-function"),
        "a non-function signals"
    );
    assert_eq!(fast_calls(), before, "no miss took the slot");
    let site = caller
        .jit_runtime()
        .call_sites()
        .unwrap()
        .site_at(2)
        .unwrap();
    assert!(
        matches!(site.target(), CallTarget::Mega),
        "the generic call recorded its misses"
    );
    off();
}

/// Every call the fast path declines runs the generic call, which answers
/// or signals exactly as it does with the knob off: a wrong argument count,
/// the depth limit, `NEOVM_JIT_FORCE_SLOW_SPEC`, a source with no leaf.
#[test]
fn declined_calls_answer_like_the_generic_call() {
    let cases = |ev: &mut Context, leaf: &CompiledLeaf, proto: &ByteCodeFunction| {
        let mut out = Vec::new();
        let f = instance(proto, 2);
        out.push(run(ev, leaf, &[f, Value::make_int(1)]));
        // The depth limit: the generic call signals at it.
        let saved = (ev.depth, ev.max_depth);
        ev.max_depth = ev.depth.max(100);
        ev.depth = ev.max_depth;
        out.push(run(ev, leaf, &[f, Value::make_int(1)]));
        (ev.depth, ev.max_depth) = saved;
        out
    };
    // Knob off: the reference answers.
    off();
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    let reference = {
        let leaf = compile(&ev, &caller);
        assert!(
            !leaf.spec_slot_kinds.contains(&SpecSlotKind::Source),
            "knob off"
        );
        cases(&mut ev, &leaf, &proto)
    };
    // Knob on.
    on(true);
    record(&caller, 2, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    let before = declined();
    assert_eq!(cases(&mut ev, &leaf, &proto), reference);
    assert!(declined() > before, "the depth case was declined");
    // Forced slow: every call declined, same answers.
    force_slow_spec_for_test(Some(true));
    // The attention word folds the harness knob in.
    ev.refresh_attention_for_test();
    let (fast, before) = (fast_calls(), declined());
    assert_eq!(cases(&mut ev, &leaf, &proto), reference);
    assert_eq!(fast_calls(), fast, "forced slow takes no fast call");
    assert_eq!(declined() - before, 2);
    off();
}

/// A wrong argument count at a source site signals like the generic call.
#[test]
fn a_wrong_argument_count_signals_like_the_generic_call() {
    // (lambda (f) (funcall f)): the call of the argument is ops[1].
    let caller0 = || {
        lexical_fn(
            1,
            vec![Op::StackRef(0), Op::Call(0), Op::Return],
            Vec::new(),
        )
    };
    off();
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let plain = caller0();
    let plain_leaf = compile(&ev, &plain);
    let reference = run(&mut ev, &plain_leaf, &[instance(&proto, 3)]);
    assert!(reference.is_err(), "wrong-number-of-arguments");
    on(true);
    let caller = caller0();
    record(&caller, 1, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    assert!(leaf.spec_slot_kinds.contains(&SpecSlotKind::Source));
    assert_eq!(run(&mut ev, &leaf, &[instance(&proto, 3)]), reference);
    off();
}

/// A polymorphic or megamorphic site gets no slot in a T1 compile.
#[test]
fn only_a_monomorphic_source_gets_a_slot() {
    on(true);
    let ev = Context::new();
    let caller = funcall_caller();
    record(&caller, 2, Value::make_bytecode(adder(1)));
    record(&caller, 2, Value::make_bytecode(adder(2)));
    let leaf = compile(&ev, &caller);
    assert!(!leaf.spec_slot_kinds.contains(&SpecSlotKind::Source));
    off();
}
