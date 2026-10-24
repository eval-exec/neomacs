//! Primitive loops: numeric loops of primitive opcodes are admitted (the
//! list requirement of primitive-lists dropped), explicit bytecode calls
//! still are not, and a numeric OSR leaf guards its counter at the entry.
//! Threading: each test owns its context, sources and leaves; no cache
//! mutation overlaps a native entry.

use super::frontend_tests::{Settings, count_loop};
use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    Context, NativeRun, OptProfitMode, Value, force_opt_osr_entry_for_test,
    force_opt_profit_for_test,
};

fn numeric_loop_ops() -> Vec<Op> {
    count_loop(0).executable_ops().to_vec()
}

#[test]
fn opt_profit_primitive_loops_parses_and_admits_numeric_loops() {
    let _settings = Settings::enter();
    assert_eq!(
        OptProfitMode::parse(Some("primitive-loops")),
        OptProfitMode::PrimitiveLoops
    );
    let ops = numeric_loop_ops();
    let admitted = |mode| body_admitted(mode, &ops, CallDensity::Sparse, KernelHeat::Hot);
    assert!(admitted(OptProfitMode::PrimitiveLoops));
    assert!(
        !admitted(OptProfitMode::PrimitiveLists),
        "lists still need a list op"
    );
    assert!(admitted(OptProfitMode::Loops));
}

#[test]
fn opt_profit_primitive_loops_still_rejects_explicit_calls() {
    let _settings = Settings::enter();
    let mut ops = numeric_loop_ops();
    // Replace the counter's `1+` with an explicit call of the same arity.
    let at = ops
        .iter()
        .position(|op| matches!(op, Op::Add1))
        .expect("the counter step");
    ops[at] = Op::Call(0);
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLoops,
        &ops,
        CallDensity::Sparse,
        KernelHeat::Hot
    ));
    assert!(body_admitted(
        OptProfitMode::Loops,
        &ops,
        CallDensity::Sparse,
        KernelHeat::Hot
    ));
}

#[test]
fn opt_profit_primitive_loops_numeric_headers_are_osr_evidence() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    assert_eq!(osr_evidence_headers(&f).count(), 0);
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLoops));
    assert_eq!(osr_evidence_headers(&f).collect::<Vec<_>>(), [1]);
}

#[test]
fn opt_profit_primitive_loops_numeric_osr_runs_opt_and_deopts_at_entry() {
    let _settings = Settings::enter();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLoops));
    force_opt_osr_entry_for_test(Some(true));
    let mut context = Context::new();
    let f = count_loop(0);
    let snapshot = [Value::make_int(2000), Value::make_int(42)];
    context.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut context, &f, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(2000).bits()))
    );
    let leaf = cache::osr_leaf_ptr_for_test(&f, 1).expect("installed numeric OSR leaf");
    assert_eq!(unsafe { &*leaf }.selected_tier(), SelectedTier::Opt);
    let bad = [Value::make_int(2000), Value::NIL];
    context.bc_buf.clear();
    context.bc_buf.extend_from_slice(&bad);
    let Some(NativeRun::DeoptAt(frame)) = cache::try_run_osr(&mut context, &f, 1, &bad, &[]) else {
        panic!("a nil counter deoptimizes at the entry");
    };
    assert_eq!(frame.pc, 1);
    assert_eq!(frame.stack.as_slice(), &bad);
    assert_eq!(context.jit_root_stack_top, 0);
    force_opt_osr_entry_for_test(None);
    cache::clear();
}
