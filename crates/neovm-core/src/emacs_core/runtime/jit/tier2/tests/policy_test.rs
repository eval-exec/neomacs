use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::Tier2Knob;
use crate::emacs_core::jit::compile::{
    Tier2PolicyKnob, force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::value::{LambdaParams, Value};

fn knob(window: u32, loop_credit: u32) -> Tier2Knob {
    Tier2Knob {
        on: true,
        window,
        loop_credit,
    }
}
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
    f
}
fn seven() -> ByteCodeFunction {
    lexical_fn(
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(7)],
    )
}
fn countdown_loop() -> ByteCodeFunction {
    lexical_fn(
        1,
        vec![
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNotNil(0),
            Op::Return,
        ],
        vec![Value::make_int(0)],
    )
}

fn policy(stable: u32, attempts: u32) -> Tier2PolicyKnob {
    Tier2PolicyKnob {
        stable,
        attempts,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }
}
fn current(f: &crate::emacs_core::bytecode::ByteCodeFunction) -> &'static CompiledLeaf {
    let ptr = cache::compiled_leaf_ptr_for_test(f.jit_runtime().compiled_id().unwrap()).unwrap();
    // SAFETY: cache keeps current/retired leaves alive until clear, not used here.
    unsafe { &*ptr }
}
fn run(
    ctx: &mut Context,
    f: &crate::emacs_core::bytecode::ByteCodeFunction,
    args: &[Value],
) -> Value {
    Value::from_bits(
        cache::try_run_compiled(ctx, f, Value::NIL, args)
            .unwrap()
            .unwrap(),
    )
}
#[test]
fn tier2_policy_budget_counts_reservations_and_floor() {
    assert!(affordable(100, 100, 200, 0, 2, 1));
    assert!(!affordable(100, 700, 201, 0, 2, 1));
    assert!(affordable(500, 0, 500, 50_000, 2, 0));
    assert!(!affordable(500, 0, 501, 50_000, 2, 0));
    assert!(affordable(u64::MAX, u64::MAX, u64::MAX, 0, 0, 0));
}
#[test]
fn tier2_policy_knobs_and_stress_are_bounded() {
    let defaults = Tier2PolicyKnob::from_env(|_| None);
    assert_eq!(
        defaults,
        Tier2PolicyKnob {
            stable: 4000,
            attempts: 4,
            budget_pct: 2,
            floor_ms: 5,
            max_reopt: 3
        }
    );
    let stress =
        Tier2PolicyKnob::from_env(|name| (name == "NEOVM_JIT_T2_STRESS").then(|| "1".to_owned()));
    assert_eq!((stress.stable, stress.budget_pct), (1, 0));
}
#[test]
fn tier2_policy_requires_a_stable_work_window() {
    force_tier2_for_test(Some(knob(3, 64)));
    force_tier2_policy_for_test(Some(policy(2, 4)));
    let mut ctx = Context::new();
    let f = seven();
    for _ in 0..3 {
        assert_eq!(run(&mut ctx, &f, &[]), Value::make_int(7));
    }
    let old = current(&f);
    assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    assert_eq!(old.obs.t2.budget.get(), 2);
    assert!(
        old.obs.t2.source.borrow().is_some(),
        "unstable windows retain the source"
    );
    run(&mut ctx, &f, &[]);
    assert_eq!(old.obs.t2.budget.get(), 1);
    run(&mut ctx, &f, &[]);
    assert!(old.obs.t2.due().is_some());
    run(&mut ctx, &f, &[]);
    assert!(!std::ptr::eq(old, current(&f)));
    assert_eq!(
        old.obs.t2.reserved_us.get(),
        0,
        "installed upgrades release the estimate"
    );
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
#[test]
fn tier2_policy_changed_feedback_rearms_before_upgrade() {
    force_tier2_for_test(Some(knob(3, 64)));
    force_tier2_policy_for_test(Some(policy(2, 4)));
    let mut ctx = Context::new();
    let f = seven();
    for _ in 0..3 {
        run(&mut ctx, &f, &[]);
    }
    let old = current(&f);
    f.jit_runtime()
        .widen_numeric(0, f.executable_ops().len(), NumericFeedback::Float);
    for _ in 0..2 {
        run(&mut ctx, &f, &[]);
    }
    assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    assert_eq!(old.obs.t2.budget.get(), 2);
    for _ in 0..2 {
        run(&mut ctx, &f, &[]);
    }
    assert!(old.obs.t2.due().is_some());
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
#[test]
fn tier2_policy_budget_denial_rearms_current_native_leaf() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    force_tier2_for_test(Some(knob(1, 64)));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 1,
        floor_ms: 0,
        max_reopt: 3,
    }));
    let mut ctx = Context::new();
    let f = seven();
    run(&mut ctx, &f, &[]);
    let old = current(&f);
    old.obs.compile_us.set(u32::MAX);
    run(&mut ctx, &f, &[]);
    assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    assert_eq!(old.obs.t2.budget.get(), 1);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    run(&mut ctx, &f, &[]);
    assert!(std::ptr::eq(old, current(&f)));
    // A temporarily full ledger must not permanently disable this source.
    old.obs.compile_us.set(1);
    run(&mut ctx, &f, &[]);
    assert!(old.obs.t2.due().is_some());
    run(&mut ctx, &f, &[]);
    assert!(!std::ptr::eq(old, current(&f)));
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
/// X8: the second long call serves the upgraded loop; both calls retain the
/// exact GNU 255-taken-back-edge quit cadence.
#[test]
fn tier2_loop_second_call_uses_upgrade_without_changing_polls() {
    force_tier2_for_test(Some(knob(15_000, 64)));
    force_tier2_policy_for_test(Some(policy(4_000, 4)));
    let mut ctx = Context::new();
    let f = countdown_loop();
    run(&mut ctx, &f, &[Value::make_int(1_000_000)]);
    let t1 = current(&f);
    assert_eq!(t1.obs.t2.due(), Some(T2Upgrade::Feedback));
    crate::emacs_core::eval::reset_bytecode_branch_poll_count();
    run(&mut ctx, &f, &[Value::make_int(1_000_000)]);
    let t2 = current(&f);
    assert_eq!(
        crate::emacs_core::eval::bytecode_branch_poll_count(),
        1_000_000 / 255
    );
    assert!(!std::ptr::eq(t1, t2));
    assert!(std::ptr::eq(
        &**t2.tier1_fallback.borrow().as_ref().unwrap(),
        t1
    ));
    assert!(!t1.retired.get(), "retained fallback remains live");
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
/// C9: a conclusive T2 arithmetic deopt widens the source and returns to its
/// retained T1; the persistent ban bounds future upgrades.
#[test]
fn tier2_deopt_reverts_and_preserves_retreat_and_ban() {
    force_tier2_for_test(Some(knob(1, 64)));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        max_reopt: 1,
        ..policy(1, 4)
    }));
    let mut ctx = Context::new();
    let f = countdown_loop();
    run(&mut ctx, &f, &[Value::make_int(10)]);
    run(&mut ctx, &f, &[Value::make_int(10)]);
    let t1 = current(&f);
    run(&mut ctx, &f, &[Value::make_int(10)]);
    let t2 = current(&f);
    assert_ne!(std::ptr::from_ref(t1), std::ptr::from_ref(t2));
    let outcome = super::super::super::reopt::note_deopt(
        &ctx,
        &f,
        t2,
        super::super::super::reopt::LeafOrigin::Entry,
        super::super::super::reopt::DeoptEvent::Precise {
            pc: 1,
            stack: &[Value::make_float(1.0)],
            cause: Some(super::super::super::reopt::DeoptCause::ArithOperands(
                NumericFeedback::Float,
            )),
        },
    );
    assert_eq!(
        outcome,
        super::super::super::reopt::ReoptVerdict::Invalidated
    );
    assert!(std::ptr::eq(t1, current(&f)));
    assert!(t2.retired.get());
    assert_eq!(
        f.jit_runtime()
            .t2_reopts
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert!(
        f.jit_runtime()
            .site_retreat
            .has(1, super::super::super::retreat::RetreatBit::Invalidated)
    );
    assert_eq!(
        request_decision(t1, f.jit_runtime()),
        Some(T2Decision::Keep)
    );
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// A load-time loop can go Due during its last activation. It must not
/// consume admission capacity while waiting for an entry that may never come.
#[test]
fn tier2_policy_run_once_due_leaves_do_not_reserve_cpu() {
    force_tier2_for_test(Some(knob(1, 64)));
    force_tier2_policy_for_test(Some(policy(1, 4)));
    let mut ctx = Context::new();
    let before = LEDGER.with(Cell::get).reserved;
    let functions: Vec<_> = (0..8).map(|_| countdown_loop()).collect();
    for f in &functions {
        run(&mut ctx, f, &[Value::make_int(1_000)]);
        let leaf = current(f);
        assert_eq!(leaf.obs.t2.due(), Some(T2Upgrade::Feedback));
        assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
        assert_eq!(LEDGER.with(Cell::get).reserved, before);
    }
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

/// A Due decision is only an admission check. Other work can spend the
/// budget before the next entry, which must retry rather than compile anyway.
#[test]
fn tier2_policy_due_upgrade_rechecks_budget_at_compile_seam() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    force_tier2_for_test(Some(knob(1, 64)));
    force_tier2_policy_for_test(Some(policy(1, 4)));
    let mut ctx = Context::new();
    let f = seven();
    run(&mut ctx, &f, &[]);
    run(&mut ctx, &f, &[]);
    let old = current(&f);
    assert!(old.obs.t2.due().is_some());
    let saved = LEDGER.with(Cell::get);
    LEDGER.with(|c| {
        c.set(Ledger {
            spent: u64::MAX,
            reserved: saved.reserved,
        })
    });
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        budget_pct: 1,
        ..policy(1, 4)
    }));
    run(&mut ctx, &f, &[]);
    assert!(std::ptr::eq(old, current(&f)));
    assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    assert_eq!(old.obs.t2.budget.get(), 1);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    LEDGER.with(|c| c.set(saved));
    force_tier2_policy_for_test(Some(policy(1, 4)));
    run(&mut ctx, &f, &[]);
    assert!(old.obs.t2.due().is_some());
    run(&mut ctx, &f, &[]);
    assert!(!std::ptr::eq(old, current(&f)));
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}
