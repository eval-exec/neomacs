//! T-A4 (design `p1-3-per-symbol-versions` §4.4 C4, §11): the Tier-0 symbol
//! call cache re-proves a stale entry through its symbol's stamp instead of
//! re-reading the function cell, and never for an empty entry, a changed
//! binding or an overrides toggle.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::stats::epoch::{CacheRefresh, EpochCounters};
use crate::emacs_core::symbol::{Obarray, force_fn_stamps_for_test};
use crate::emacs_core::value::LambdaParams;

/// `(lambda (x) x)`.
fn identity() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("argument")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Return];
    f.max_stack = 2;
    Value::make_bytecode(f)
}

/// A caller of `(CALLEE 42)`.
fn caller_of(callee: SymId) -> ByteCodeFunction {
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let callee_idx = caller.add_constant(Value::from_sym_id(callee));
    let argument_idx = caller.add_constant(Value::fixnum(42));
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::Constant(argument_idx),
        Op::Call(1),
        Op::Return,
    ];
    caller.max_stack = 2;
    caller
}

/// Run CALLER and answer its value with the function-cell reads it made.
fn run_counting(eval: &mut Context, caller: &ByteCodeFunction) -> (Value, usize) {
    crate::emacs_core::symbol::reset_function_cell_lookup_count();
    let value = Vm::from_context(eval)
        .execute(caller, vec![])
        .expect("the call runs");
    (
        value,
        crate::emacs_core::symbol::function_cell_lookup_count(),
    )
}

fn refreshes(base: &EpochCounters) -> u64 {
    EpochCounters::snapshot()
        .since(base)
        .refresh_for(CacheRefresh::SymbolCall)
}

/// A harness with one cached `(callee 42)` call.
fn primed(stamps: bool) -> (Context, SymId, ByteCodeFunction) {
    crate::test_utils::init_test_tracing();
    force_fn_stamps_for_test(Some(stamps));
    let mut eval = Context::new_minimal_vm_harness();
    let callee = intern("neovm--ccr-callee");
    eval.obarray.set_symbol_function_id(callee, identity());
    let caller = caller_of(callee);
    let (value, reads) = run_counting(&mut eval, &caller);
    assert_eq!(value, Value::fixnum(42));
    assert_eq!(reads, 1, "the first call fills the entry");
    (eval, callee, caller)
}

/// After an unrelated redefinition the entry is re-proven by the stamp: the
/// call reads no function cell.
#[test]
fn an_unrelated_redefinition_refreshes_without_a_cell_read() {
    let (mut eval, _, caller) = primed(true);
    eval.obarray
        .set_symbol_function_id(intern("neovm--ccr-unrelated"), Value::fixnum(1));
    let base = EpochCounters::snapshot();
    let (value, reads) = run_counting(&mut eval, &caller);
    assert_eq!(value, Value::fixnum(42));
    assert_eq!(reads, 0, "the stamp proved the entry");
    assert_eq!(refreshes(&base), 1);
    let base = EpochCounters::snapshot();
    let (_, reads) = run_counting(&mut eval, &caller);
    assert_eq!(
        (reads, refreshes(&base)),
        (0, 0),
        "current again: a plain hit"
    );
    force_fn_stamps_for_test(None);
}

/// With the knob off the entry is refilled from the cell, as before.
#[test]
fn the_knob_off_refills_from_the_cell() {
    let (mut eval, _, caller) = primed(false);
    eval.obarray
        .set_symbol_function_id(intern("neovm--ccr-unrelated"), Value::fixnum(1));
    let base = EpochCounters::snapshot();
    let (value, reads) = run_counting(&mut eval, &caller);
    assert_eq!(value, Value::fixnum(42));
    assert_eq!(reads, 1);
    assert_eq!(refreshes(&base), 0);
    force_fn_stamps_for_test(None);
}

/// Redefining the called symbol misses: the next call reads the cell and
/// runs the new definition.
#[test]
fn a_related_redefinition_misses() {
    let (mut eval, callee, caller) = primed(true);
    let mut plus_one = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("argument")],
        optional: vec![],
        rest: None,
    });
    plus_one.lexical = true;
    plus_one.ops = vec![Op::StackRef(0), Op::Add1, Op::Return];
    plus_one.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(callee, Value::make_bytecode(plus_one));
    let base = EpochCounters::snapshot();
    let (value, reads) = run_counting(&mut eval, &caller);
    assert_eq!(value, Value::fixnum(43));
    assert_eq!(reads, 1);
    assert_eq!(refreshes(&base), 0);
    force_fn_stamps_for_test(None);
}

/// The compiler-overrides toggle raises the floor: no entry made before it
/// refreshes, and while the overrides are active the call resolves
/// generically.
#[test]
fn an_overrides_toggle_voids_every_entry() {
    let (mut eval, callee, _) = primed(true);
    let overrides = intern(crate::emacs_core::eval::INTERNAL_COMPILER_FUNCTION_OVERRIDES);
    eval.sync_cached_runtime_binding_by_id(overrides, Value::cons(Value::NIL, Value::NIL));
    let base = EpochCounters::snapshot();
    let target = Vm::from_context(&mut eval).resolve_stack_call_target(Value::from_sym_id(callee));
    assert!(matches!(target, ResolvedStackCallTarget::Generic));
    assert_eq!(refreshes(&base), 0);
    eval.sync_cached_runtime_binding_by_id(overrides, Value::NIL);
    let target = Vm::from_context(&mut eval).resolve_stack_call_target(Value::from_sym_id(callee));
    assert!(matches!(target, ResolvedStackCallTarget::ByteCode { .. }));
    assert_eq!(refreshes(&base), 0, "refilled, not refreshed");
    force_fn_stamps_for_test(None);
}

/// The EMPTY entry (`nil`'s slot, epoch `u64::MAX`) never refreshes into a
/// hit.
#[test]
fn an_empty_entry_never_refreshes() {
    crate::test_utils::init_test_tracing();
    force_fn_stamps_for_test(Some(true));
    let ob = Obarray::new();
    let mut cache = SymbolByteCodeCallCache::new();
    let base = EpochCounters::snapshot();
    assert!(
        cache
            .get(
                crate::emacs_core::intern::NIL_SYM_ID,
                ob.function_epoch(),
                &ob
            )
            .is_none()
    );
    assert_eq!(refreshes(&base), 0);
    force_fn_stamps_for_test(None);
}
