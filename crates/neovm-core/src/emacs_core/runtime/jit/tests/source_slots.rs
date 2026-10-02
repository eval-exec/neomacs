//! Closure source slots (P2.1 C5, `NEOVM_JIT_SPEC_SOURCES`): a call whose
//! recorded target is one closure source calls the source's armed leaf
//! through the slot; any other callee, and every call the fast path
//! declines, runs the site's generic call with the same result or signal.

use super::source_slots::{SOURCE_SLOT_ARMINGS, SOURCE_SLOT_DECLINED, SOURCE_SLOT_FAST_CALLS};
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
    // The explicit direct cases below exercise broad source reach, which
    // the self-only policy intentionally declines.
    force_direct_sites_for_test(Some(DirectSitesMode::All));
    force_direct_memory_for_test(Some(false));
    // These scenarios count source-shim calls. The direct-path scenarios
    // opt back in explicitly, so the ambient knob cannot bypass a shim
    // whose counter this test expects to observe on every instance.
    force_direct_call_for_test(Some(false));
}

fn off() {
    force_feedback_mode_for_test(None);
    force_spec_sources_for_test(None);
    force_direct_sites_for_test(None);
    force_direct_memory_for_test(None);
    force_slow_spec_for_test(None);
    force_direct_call_for_test(None);
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
        matches!(site.target(), CallTarget::Sources(ref s) if s.len() == 1),
        "under `use` compiled sites do not record: the lattice is the interpreter's"
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

/// The caller leaf's one source slot.
fn source_slot(leaf: &CompiledLeaf) -> &SpecSlot {
    let mut slots = leaf.source_spec_slots();
    let slot = slots.next().expect("a source slot");
    assert!(slots.next().is_none());
    slot
}

/// `NEOVM_JIT_DIRECT_CALL`: the first call through the shim arms the
/// slot's direct entry with the source's register entry; later instances
/// enter the leaf from the site (no shim call), each with its own captured
/// constant. The leaf walk that retires the leaf clears the entry, and a
/// retirement that moves the leaf-slot epoch sends the site back to the
/// shim, which re-arms.
#[test]
fn with_direct_calls_instances_enter_the_leaf_from_the_site() {
    on(true);
    force_direct_call_for_test(Some(true));
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    record(&caller, 2, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    let slot = source_slot(&leaf);
    assert!(
        slot.direct_entry().is_null(),
        "unarmed until the first call"
    );
    let fast = fast_calls();
    assert_eq!(
        run(&mut ev, &leaf, &[instance(&proto, 10), Value::make_int(5)]),
        Ok(Value::make_int(15))
    );
    assert_eq!(fast_calls() - fast, 1, "the first call took the shim");
    let armed = proto
        .jit_runtime()
        .armed_leaf_slot(cache::leaf_slot_epoch())
        .expect("armed");
    // SAFETY: the cache holds the source's leaf.
    assert_eq!(slot.direct_entry(), unsafe { (*armed).entry });
    let fast = fast_calls();
    for k in [20, 30, 40] {
        assert_eq!(
            run(&mut ev, &leaf, &[instance(&proto, k), Value::make_int(5)]),
            Ok(Value::make_int(5 + k))
        );
    }
    assert_eq!(fast_calls(), fast, "direct: no shim call");
    // The walk that retires the source's leaf (`cache::unlink_spec_slots`,
    // over the cached leaves; this caller is not cached) clears the entry.
    assert_eq!(leaf.unlink_spec_slots_to(armed), 1);
    assert!(slot.direct_entry().is_null());
    assert_eq!(
        run(&mut ev, &leaf, &[instance(&proto, 2), Value::make_int(5)]),
        Ok(Value::make_int(7))
    );
    assert!(!slot.direct_entry().is_null(), "re-armed by the shim");
    // Another source misses the guard; the depth limit declines to the
    // generic call with its error, as the knob-off site does.
    assert_eq!(
        run(
            &mut ev,
            &leaf,
            &[Value::make_bytecode(adder(100)), Value::make_int(1)]
        ),
        Ok(Value::make_int(101))
    );
    let saved = (ev.depth, ev.max_depth);
    ev.max_depth = ev.depth.max(100);
    ev.depth = ev.max_depth;
    let deep = run(&mut ev, &leaf, &[instance(&proto, 2), Value::make_int(1)]);
    (ev.depth, ev.max_depth) = saved;
    assert!(
        deep.unwrap_err().contains("max-lisp-eval-depth"),
        "the depth error"
    );
    off();
}

/// A callee whose arity the site does not match exactly never arms: its
/// calls keep the shim (and the generic call), with the generic answers.
#[test]
fn with_direct_calls_an_inexact_callee_keeps_the_shim() {
    on(true);
    force_direct_call_for_test(Some(true));
    let mut ev = Context::new();
    // (lambda (x &optional y) x): the site passes one argument.
    let proto = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: vec![SymId(2)],
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::StackRef(1), Op::Return];
        f.constants = vec![Value::make_int(0)].into();
        f.max_stack = 4;
        f.seal_hand_assembled_ops();
        f.jit_runtime()
            .set_reopt_level_for_test(ReoptLevel::BaselineOnly);
        f
    };
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    record(&caller, 2, Value::make_bytecode(proto.clone()));
    let leaf = compile(&ev, &caller);
    for i in 0..3 {
        let f = Value::make_bytecode(proto.clone());
        assert_eq!(
            run(&mut ev, &leaf, &[f, Value::make_int(i)]),
            Ok(Value::make_int(i))
        );
    }
    assert!(source_slot(&leaf).direct_entry().is_null(), "never armed");
    off();
}

/// Without direct calls the slot still remembers the source's leaf, so the
/// shim arms it once, not per call; a moved leaf-slot epoch re-arms it.
#[test]
fn the_slot_remembers_its_leaf_and_re_arms_on_a_new_epoch() {
    on(true);
    force_direct_call_for_test(Some(false));
    let mut ev = Context::new();
    let proto = adder(1);
    arm(&mut ev, &proto);
    let caller = funcall_caller();
    record(&caller, 2, instance(&proto, 1));
    let leaf = compile(&ev, &caller);
    let armings = SOURCE_SLOT_ARMINGS.load(Ordering::Relaxed);
    for k in 0..4 {
        assert_eq!(
            run(&mut ev, &leaf, &[instance(&proto, k), Value::make_int(1)]),
            Ok(Value::make_int(1 + k))
        );
    }
    assert_eq!(
        SOURCE_SLOT_ARMINGS.load(Ordering::Relaxed) - armings,
        1,
        "armed once"
    );
    let slot = source_slot(&leaf);
    assert!(
        slot.direct_entry().is_null(),
        "no direct entry with the knob off"
    );
    // Retire an unrelated leaf: the epoch moves, the source's slot is
    // re-armed on its next call through the cache.
    let other = adder(9);
    arm(&mut ev, &other);
    cache::evict_compiled(other.jit_runtime().compiled_id().unwrap());
    let armings = SOURCE_SLOT_ARMINGS.load(Ordering::Relaxed);
    for _ in 0..3 {
        run(&mut ev, &leaf, &[instance(&proto, 2), Value::make_int(1)]);
    }
    assert!(
        SOURCE_SLOT_ARMINGS.load(Ordering::Relaxed) - armings <= 1,
        "re-armed at most once"
    );
    off();
}
