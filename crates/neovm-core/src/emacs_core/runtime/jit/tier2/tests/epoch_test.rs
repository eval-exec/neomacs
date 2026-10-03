//! Epoch-only invalidation must not consume a source's T2 failure budget.

use super::*;
use crate::emacs_core::builtins::builtin_fset_2;
use crate::emacs_core::jit::bg::{BgMode, force_mode_for_test};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{
    LeafTier, Tier2Knob, Tier2PolicyKnob, force_deopt_for_test, force_profit_gate_for_test,
    force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::jit::feedback::{FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::jit::retreat::RetreatBit;
use crate::emacs_core::jit::stats::compile_stats_snapshot;
use crate::emacs_core::jit::tier2::{T2Origin, T2Upgrade};
use std::sync::atomic::Ordering;

#[test]
fn tier2_inline_epoch_moved_does_not_count_as_feedback_failure() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    force_tier2_for_test(Some(Tier2Knob {
        on: true,
        window: 1,
        loop_credit: 0,
    }));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 1,
    }));
    force_reopt_for_test(Some(ReoptKnobs::defaults()));
    force_mode_for_test(Some(BgMode::Sync));
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    force_inline_for_test(Some(true));
    force_profit_gate_for_test(false);
    force_deopt_for_test(false);
    let mut ctx = Context::new();
    let sq = Value::symbol("neovm--t2-epoch-sq");
    builtin_fset_2(
        &mut ctx,
        sq,
        Value::make_bytecode(function(
            vec![Op::StackRef(0), Op::Dup, Op::Mul, Op::Return],
            vec![],
            1,
        )),
    )
    .expect("install callee");
    // The existing epoch-reoptimization probe: sum the first n squares.
    // The loop makes the MIR leaf's inline-entry deopt precise.
    const CALL_PC: usize = 9;
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(0),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(16),
            Op::StackRef(0),
            Op::Constant(1),
            Op::StackRef(3),
            Op::Call(1),
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::make_int(0), sq],
        1,
    );
    let value = Value::make_bytecode(f.clone());
    crate::emacs_core::eval::push_scratch_gc_root(value);
    let run = |ctx: &mut Context| {
        Value::from_bits(
            cache::try_run_compiled(ctx, &f, value, &[Value::make_int(4)])
                .expect("no signal")
                .expect("precise: resumed, not rerun"),
        )
    };
    // Numeric/call-target feedback can move in the initial profiling window.
    // Wait for the real policy to install a Feedback upgrade.
    let t2 = (0..8)
        .find_map(|_| {
            assert_eq!(run(&mut ctx), Value::make_int(14));
            let id = f.jit_runtime().compiled_id().expect("compiled source");
            let ptr = cache::compiled_leaf_ptr_for_test(id).expect("compiled leaf");
            // SAFETY: this mutator's current and retired cache roots retain
            // leaves until clear; no clear occurs while this test inspects them.
            let leaf = unsafe { &*ptr };
            (leaf.obs.t2.origin == T2Origin::Upgrade(T2Upgrade::Feedback)).then_some(leaf)
        })
        .expect("stable profiling installed a Feedback T2");
    assert_eq!(t2.tier(), LeafTier::Mir);
    let armed = t2.inline_epoch().expect("T2 inlined the square callee");
    assert!(
        t2.tier1_fallback.borrow().is_some(),
        "T2 retained its profiling T1"
    );
    assert_eq!(f.jit_runtime().t2_reopts.load(Ordering::Relaxed), 0);
    assert!(
        !f.jit_runtime()
            .site_retreat
            .has(CALL_PC, RetreatBit::Invalidated)
    );
    let id = f.jit_runtime().compiled_id().unwrap();
    let deopts = compile_stats_snapshot().deopt_causes[DeoptCause::InlineEpochMoved.census_index()];

    builtin_fset_2(
        &mut ctx,
        Value::symbol("neovm--t2-epoch-unrelated"),
        Value::make_int(0),
    )
    .expect("unrelated fset");
    let epoch = ctx.obarray.function_epoch();
    assert_ne!(epoch, armed);
    assert_eq!(
        cache::compiled_leaf_ptr_for_test(id),
        Some(std::ptr::from_ref(t2))
    );
    assert_eq!(
        run(&mut ctx),
        Value::make_int(14),
        "precise resume is correct"
    );
    assert_eq!(
        compile_stats_snapshot().deopt_causes[DeoptCause::InlineEpochMoved.census_index()],
        deopts + 1,
        "the real inline epoch guard deopted",
    );
    assert_eq!(
        f.jit_runtime().t2_reopts.load(Ordering::Relaxed),
        0,
        "an unrelated function write teaches the source no new feedback",
    );
    assert!(
        !f.jit_runtime()
            .site_retreat
            .has(CALL_PC, RetreatBit::Invalidated)
    );
    assert!(t2.retired.get());
    assert_eq!(
        cache::cache_entry_kind_for_test(id),
        "none",
        "rebuild immediately"
    );
    assert_eq!(run(&mut ctx), Value::make_int(14));
    let rebuilt = cache::compiled_leaf_ptr_for_test(id).expect("rebuilt leaf");
    // SAFETY: the current cache owns the rebuilt leaf.
    assert_eq!(unsafe { &*rebuilt }.inline_epoch(), Some(epoch));
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
    force_reopt_for_test(None);
    force_mode_for_test(None);
    force_feedback_mode_for_test(None);
    force_inline_for_test(None);
}
