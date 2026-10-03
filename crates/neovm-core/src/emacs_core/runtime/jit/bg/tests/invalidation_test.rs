//! Redefinitions between a compile's front and its install (P2.4 B8):
//! T-I1 (a speculated callee), the proven-fresh spec re-arm, T-I2 (an
//! MIR-inlined callee) and T-I3 (an unrelated redefinition under a guarded
//! inline). The backend's result is held back, so each redefinition lands
//! while the compile is pending; every result is checked against the
//! interpreter's.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::stats::epoch::{EpochCounters, SpecRevalidation};
use crate::emacs_core::jit::try_run_compiled;
use crate::emacs_core::value::{LambdaParams, Value, ValueKind};

fn deferred_held() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
    hold_publish_for_test(true);
    force_deopt_for_test(false);
}

fn done() {
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

fn sym(name: &str) -> (Value, SymId) {
    let s = Value::symbol(name);
    let ValueKind::Symbol(id) = s.kind() else {
        panic!("symbol")
    };
    (s, id)
}

fn bytecode(arity: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=arity).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

/// `(lambda () (if nil 0 N))`: two blocks, so no tier inlines it (a spec
/// site stays a call).
fn two_block_constant(n: i64) -> ByteCodeFunction {
    bytecode(
        0,
        vec![
            Op::Constant(0),
            Op::GotoIfNil(4),
            Op::Constant(1),
            Op::Return,
            Op::Constant(2),
            Op::Return,
        ],
        vec![Value::NIL, Value::make_int(0), Value::make_int(n)],
    )
}

/// `(lambda (x) (+ (G) x 1))`.
fn calls_g(g: Value) -> ByteCodeFunction {
    bytecode(
        1,
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::StackRef(1),
            Op::Add,
            Op::Constant(1),
            Op::Add,
            Op::Return,
        ],
        vec![g, Value::make_int(1)],
    )
}

fn call(ctx: *mut Context, f: &ByteCodeFunction, x: i64) -> Option<i64> {
    try_run_compiled(ctx, f, Value::NIL, &[Value::make_int(x)])
        .expect("no signal")
        .map(|bits| Value::from_bits(bits).as_fixnum().expect("fixnum"))
}

/// `f` run by the interpreter: a fresh copy (a clone would share `f`'s
/// tiering state), pinned to Tier 0.
fn interpreted(ev: &mut Context, f: &ByteCodeFunction, x: i64) -> i64 {
    let twin = bytecode(
        f.params.required.len() as u32,
        f.ops.clone(),
        f.constants.to_vec(),
    );
    twin.jit_runtime().set_cold_for_test();
    let value = ev
        .apply(Value::make_bytecode(twin), vec![Value::make_int(x)])
        .expect("interpreter");
    value.as_fixnum().expect("fixnum")
}

fn kind(f: &ByteCodeFunction) -> &'static str {
    cache::cache_entry_kind_for_test(f.jit_runtime().compiled_id().expect("assigned"))
}

fn spec(kind: SpecRevalidation) -> u64 {
    EpochCounters::snapshot().spec[kind as usize]
}

/// T-I1: the speculated callee is redefined while its caller's compile is
/// pending. The installed leaf's spec slot keeps the front's epoch, so its
/// first call re-validates, finds the binding changed and calls the new
/// definition.
#[test]
fn jit_bg_redefined_spec_callee_takes_effect_after_install() {
    deferred_held();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let (g, g_id) = sym("jit-bg-t-i1-g");
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(two_block_constant(1)));
    let f = calls_g(g);
    assert_eq!(call(ctx, &f, 5), None);
    assert_eq!(kind(&f), "pending");
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(two_block_constant(2)));
    assert_eq!(publish_held_for_test(), 1);
    let changed = spec(SpecRevalidation::BindingChanged);
    assert_eq!(call(ctx, &f, 5), Some(8), "5 + (new g = 2) + 1");
    assert_eq!(kind(&f), "compiled");
    assert_eq!(spec(SpecRevalidation::BindingChanged), changed + 1);
    assert_eq!(interpreted(&mut ev, &f, 5), 8);
    done();
}

/// An unrelated redefinition while the compile is pending moves the epoch
/// but not the callee's binding: the install proves the binding unchanged
/// and re-stamps the slot, so the first call needs no re-validation.
#[test]
fn jit_bg_install_restamps_a_spec_slot_whose_binding_held() {
    deferred_held();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let (g, g_id) = sym("jit-bg-restamp-g");
    let (_, h_id) = sym("jit-bg-restamp-unrelated");
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(two_block_constant(1)));
    let f = calls_g(g);
    assert_eq!(call(ctx, &f, 5), None);
    ev.obarray
        .set_symbol_function_id(h_id, Value::make_bytecode(two_block_constant(9)));
    assert_eq!(publish_held_for_test(), 1);
    let (rearmed, changed) = (
        spec(SpecRevalidation::Rearmed),
        spec(SpecRevalidation::BindingChanged),
    );
    assert_eq!(call(ctx, &f, 5), Some(7));
    assert_eq!(
        spec(SpecRevalidation::Rearmed),
        rearmed,
        "re-stamped at install"
    );
    assert_eq!(spec(SpecRevalidation::BindingChanged), changed);
    assert!(stats_snapshot().spec_restamps >= 1);
    done();
}

/// `(lambda (x) (* x x))`: one block, so the MIR tier inlines it.
fn square() -> ByteCodeFunction {
    bytecode(1, vec![Op::Dup, Op::Mul, Op::Return], vec![])
}

/// T-I2: the MIR-inlined callee is redefined while the caller's compile is
/// pending. The inline dependency registered at the front removes the
/// pending entry (superseded); the recompile inlines the new body.
#[test]
fn jit_bg_redefined_inlined_callee_supersedes_the_pending_compile() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    deferred_held();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let (sq, sq_id) = sym("jit-bg-t-i2-sq");
    ev.obarray
        .set_symbol_function_id(sq_id, Value::make_bytecode(square()));
    // (lambda (x) (+ (sq x) 1))
    let f = bytecode(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Constant(1),
            Op::Add,
            Op::Return,
        ],
        vec![sq, Value::make_int(1)],
    );
    assert_eq!(call(ctx, &f, 3), None);
    assert_eq!(kind(&f), "pending");
    let superseded = stats_snapshot().discarded[DiscardReason::Superseded as usize];
    // (lambda (x) (+ x x))
    ev.obarray.set_symbol_function_id(
        sq_id,
        Value::make_bytecode(bytecode(1, vec![Op::Dup, Op::Add, Op::Return], vec![])),
    );
    assert_eq!(
        kind(&f),
        "none",
        "the redefinition removed the pending entry"
    );
    assert_eq!(
        stats_snapshot().discarded[DiscardReason::Superseded as usize],
        superseded + 1
    );
    assert_eq!(publish_held_for_test(), 1, "its job finishes into nowhere");
    hold_publish_for_test(false);
    assert_eq!(call(ctx, &f, 3), None, "recompiled, pending again");
    assert_eq!(call(ctx, &f, 3), Some(7), "3 + 3 + 1: the new body");
    assert_eq!(interpreted(&mut ev, &f, 3), 7);
    done();
}

/// T-I3: a guarded MIR inline (a precise body) whose front armed epoch E
/// would fail its inline-entry guard on every call once the epoch moved.
/// An unrelated redefinition during the flight discards it at install
/// (epoch moved) and it is requested again; after two such discards the
/// compile runs in line.
#[test]
fn jit_bg_moved_epoch_discards_a_guarded_inline_leaf_twice_then_compiles_in_line() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    deferred_held();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let (sq, sq_id) = sym("jit-bg-t-i3-sq");
    let (g, g_id) = sym("jit-bg-t-i3-g");
    let (_, h_id) = sym("jit-bg-t-i3-unrelated");
    ev.obarray
        .set_symbol_function_id(sq_id, Value::make_bytecode(square()));
    // (lambda (y) (if y (1+ y) 0)): two blocks, called, not inlined.
    let g_fn = bytecode(
        1,
        vec![
            Op::StackRef(0),
            Op::GotoIfNil(5),
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
            Op::Constant(0),
            Op::Return,
        ],
        vec![Value::make_int(0)],
    );
    ev.obarray
        .set_symbol_function_id(g_id, Value::make_bytecode(g_fn));
    // (lambda (a) (g (sq a))): sq inlined, g a residual call (precise MIR).
    let f = bytecode(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![g, sq],
    );
    let moved = stats_snapshot().discarded[DiscardReason::EpochMoved as usize];
    for round in 0..2 {
        assert_eq!(call(ctx, &f, 3), None, "round {round}: pending");
        assert_eq!(kind(&f), "pending");
        ev.obarray
            .set_symbol_function_id(h_id, Value::make_bytecode(two_block_constant(round)));
        assert_eq!(publish_held_for_test(), 1);
        assert_eq!(call(ctx, &f, 3), None, "round {round}: discarded");
        assert_eq!(kind(&f), "none");
    }
    assert_eq!(
        stats_snapshot().discarded[DiscardReason::EpochMoved as usize],
        moved + 2
    );
    let entries = stats_snapshot().enqueued[JobClass::Entry as usize];
    assert_eq!(call(ctx, &f, 3), Some(10), "compiled in line: g(9) = 10");
    assert_eq!(kind(&f), "compiled");
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Entry as usize],
        entries,
        "the caller's compile was not deferred"
    );
    assert_eq!(interpreted(&mut ev, &f, 3), 10);
    done();
}

/// T-I5: the tagged heap a pending compile's front read is replaced while
/// its backend runs (another Context on this thread brings its own). The
/// leaf must never install: the cache drops it when it notices the new
/// heap, or the install's heap check discards it.
#[test]
fn jit_bg_a_replaced_heap_never_installs_a_pending_leaf() {
    deferred_held();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = bytecode(
        1,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
    );
    assert_eq!(call(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    let gone = |s: &BgStats| {
        s.discarded[DiscardReason::HeapChanged as usize]
            + s.discarded[DiscardReason::Superseded as usize]
    };
    let before = gone(&stats_snapshot());
    let replacement = Context::new();
    assert_ne!(
        crate::tagged::gc::current_tagged_heap_identity(),
        None,
        "the replacement's heap is current"
    );
    assert_eq!(publish_held_for_test(), 1);
    cache::drain_ready_pending(None);
    assert_eq!(kind(&f), "none", "nothing installed");
    assert_eq!(pending_count(), 0);
    assert_eq!(gone(&stats_snapshot()), before + 1);
    drop(replacement);
    done();
}
