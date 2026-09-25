//! The compile pipeline's fixed-cost machinery (P2.4 B0-B4): compile origins
//! and phase timers through the real cache seams.

use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::stats::{self, CompileOrigin, CompilePhase};
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
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

fn observe_stats() {
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
}

/// `(lambda (x) (+ x 1))`.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

/// A dispatch-seam compile lands in the `dispatch` origin row, and its phase
/// split covers the backend (setup, codegen, finalize) and sums to the
/// stall aggregate.
#[test]
fn jit_pipeline_dispatch_compile_is_split_by_phase() {
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let f_val = Value::make_bytecode(f.clone());
    let got = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(41)])
        .expect("no signal");
    assert_eq!(got, Some(Value::make_int(42).bits()));
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 1);
    let row = s.origins[CompileOrigin::Dispatch as usize];
    assert_eq!((row.count, row.ok, row.us), (1, 1, s.total_us), "{s:?}");
    for phase in [
        CompilePhase::Gate,
        CompilePhase::Lower,
        CompilePhase::Codegen,
        CompilePhase::Finalize,
    ] {
        assert!(s.phase_ns[phase as usize] > 0, "{phase:?} unclaimed: {s:?}");
    }
    let split_us = s.phase_ns.iter().sum::<u64>() / 1_000;
    assert!(
        split_us.abs_diff(s.total_us) <= 1,
        "split {split_us}us vs stall {}us",
        s.total_us
    );
}

/// A spec site's first call into an uncompiled callee is a `first_sight`
/// compile; a compile outside the cache is `direct` and is not a stall.
#[test]
fn jit_pipeline_first_sight_and_direct_origins() {
    force_deopt_for_test(false);
    observe_stats();
    stats::reset_compile_stats();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_some());
    let g = add1();
    compile_bytecode_function_with(&g, None).expect("compiles");
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.origins[CompileOrigin::FirstSight as usize].count, 1);
    assert_eq!(
        s.origins[CompileOrigin::Direct as usize].count,
        0,
        "only the cache seams run a compile clock"
    );
    assert_eq!(s.total_compiles, 1);
}
