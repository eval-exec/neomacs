//! Compact opt helper admission and service through the normal native seam.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::bg::{BgMode, force_mode_for_test};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::lowering::{RegallocChoice, RegallocPolicy};
use crate::emacs_core::jit::compile::{
    CompileRequest, OptMode, OptPasses, OptProfitMode, Tier2Knob, Tier2PolicyKnob,
    compile_bytecode_function_requested, force_deopt_for_test, force_opt_for_test,
    force_opt_passes_for_test, force_opt_profit_for_test, force_profit_gate_for_test,
    force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::jit::feedback::{FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::value::Value;

struct Settings;
impl Settings {
    fn enter(mode: OptMode) -> Self {
        force_opt_for_test(Some(mode), None);
        force_opt_profit_for_test(Some(OptProfitMode::Off));
        force_opt_passes_for_test(Some(OptPasses::default()));
        force_tier2_for_test(Some(Tier2Knob {
            on: true,
            window: 20,
            loop_credit: 64,
        }));
        force_tier2_policy_for_test(Some(policy()));
        force_mode_for_test(Some(BgMode::Sync));
        force_feedback_mode_for_test(Some(FeedbackMode::Off));
        force_inline_for_test(Some(false));
        force_profit_gate_for_test(false);
        force_deopt_for_test(false);
        force_observe_for_test(ObserveOverride {
            entry_count: true,
            ..Default::default()
        });
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_profit_for_test(None);
        force_opt_passes_for_test(None);
        force_tier2_for_test(None);
        force_tier2_policy_for_test(None);
        force_mode_for_test(None);
        force_feedback_mode_for_test(None);
        force_inline_for_test(None);
        force_profit_gate_for_test(true);
        force_deopt_for_test(false);
        force_observe_for_test(ObserveOverride::default());
    }
}
fn policy() -> Tier2PolicyKnob {
    Tier2PolicyKnob {
        stable: 2,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }
}

/// Isolate the cold policy from the allocator and legacy MIR admissions.
/// The fixture has neither loop/recursive work nor a dynamic call target.
fn policy_fixture(ops_len: usize) -> (ByteCodeFunction, CompiledLeaf) {
    force_opt_for_test(Some(OptMode::Off), None);
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(7)],
        0,
    );
    let mut leaf = compile_bytecode_function_requested(
        &f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: CompileTier::T1,
        },
    )
    .unwrap();
    assert!(leaf.obs.t2.profiling());
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    leaf.call_heavy = true;
    leaf.regalloc = RegallocChoice::Full;
    leaf.obs.t2.policy.borrow_mut().ops_len = ops_len;
    (f, leaf)
}

#[test]
fn tier2_opt_compact_helper_still_requires_stable_feedback() {
    let _settings = Settings::enter(OptMode::Opt);
    let _ctx = Context::new();
    let (f, leaf) = policy_fixture(47);
    force_opt_for_test(Some(OptMode::Opt), None);
    assert_eq!(decide(&leaf), T2Decision::Keep);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(leaf.obs.t2.budget.get(), 2);
    f.jit_runtime()
        .widen_numeric(0, f.executable_ops().len(), NumericFeedback::Float);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(
        request_decision(&leaf, f.jit_runtime()),
        Some(T2Decision::Upgrade(T2Upgrade::Feedback))
    );
    release(&leaf);
}

#[test]
fn tier2_opt_compact_helper_admission_is_bounded_at_sixty_four_ops() {
    let _settings = Settings::enter(OptMode::Opt);
    let _ctx = Context::new();
    for (ops_len, expected) in [
        (0, T2Decision::Keep),
        (64, T2Decision::Upgrade(T2Upgrade::Feedback)),
        (65, T2Decision::Keep),
    ] {
        let (f, leaf) = policy_fixture(ops_len);
        force_opt_for_test(Some(OptMode::Opt), None);
        assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
        assert_eq!(request_decision(&leaf, f.jit_runtime()), Some(expected));
        release(&leaf);
    }
}

#[test]
fn tier2_opt_compact_helper_off_and_legacy_keep_the_old_worth_policy() {
    let _settings = Settings::enter(OptMode::Off);
    let _ctx = Context::new();
    for mode in [OptMode::Off, OptMode::Legacy] {
        let (f, leaf) = policy_fixture(47);
        force_opt_for_test(Some(mode), None);
        assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
        assert_eq!(
            request_decision(&leaf, f.jit_runtime()),
            Some(T2Decision::Keep)
        );
        assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
    }
}

#[test]
fn tier2_opt_compact_helper_respects_the_persistent_reopt_ban() {
    let _settings = Settings::enter(OptMode::Opt);
    let _ctx = Context::new();
    let (f, leaf) = policy_fixture(47);
    force_opt_for_test(Some(OptMode::Opt), None);
    f.jit_runtime()
        .t2_reopts
        .store(3, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        request_decision(&leaf, f.jit_runtime()),
        Some(T2Decision::Keep)
    );
    assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
}

#[test]
fn tier2_opt_compact_helper_respects_the_compile_cpu_budget() {
    let _settings = Settings::enter(OptMode::Opt);
    let _ctx = Context::new();
    let (f, leaf) = policy_fixture(47);
    force_opt_for_test(Some(OptMode::Opt), None);
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        budget_pct: 1,
        floor_ms: 0,
        ..policy()
    }));
    leaf.obs.compile_us.set(u32::MAX);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
    assert_eq!(leaf.obs.t2.budget.get(), i64::from(policy().stable));
    assert_eq!(leaf.obs.t2.policy.borrow().attempts, 1);
    assert_eq!(leaf.obs.t2.state.get(), T2State::BudgetWait);
}

#[test]
fn tier2_opt_compact_builtin_helper_serves_native_spec_calls() {
    const CALLS: u64 = 400;
    let _settings = Settings::enter(OptMode::Opt);
    let mut ctx = Context::new();
    // A statically named builtin does not supply dynamic call-target credit.
    let helper = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(intern("length"))],
        1,
    );
    let arg = Value::vector(vec![Value::NIL; 3]);
    let expected = Vm::from_context(&mut ctx)
        .execute(&helper, vec![arg])
        .unwrap();
    let helper = Value::make_bytecode(helper);
    let name = intern("neovm--t2-opt-compact-builtin-helper");
    ctx.obarray.set_symbol_function_id(name, helper);
    let caller = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(name)],
        1,
    ));
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    crate::emacs_core::eval::push_scratch_gc_roots(&[arg, helper, caller]);
    let helper_data = helper.get_bytecode_data().unwrap();
    let caller_data = caller.get_bytecode_data().unwrap();
    for _ in 0..CALLS {
        assert_eq!(
            cache::try_run_compiled(&mut ctx, caller_data, caller, &[arg]).unwrap(),
            Some(expected.bits())
        );
    }
    let id = helper_data.jit_runtime().compiled_id().unwrap();
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    // SAFETY: the current leaf and retained T1 stay in this mutator's cache;
    // no cache clear happens while either reference is inspected.
    let upgraded = unsafe { &*ptr };
    assert!(
        upgraded.call_heavy,
        "allocator retiering cannot admit this helper"
    );
    assert_eq!(upgraded.selected_tier(), SelectedTier::Opt);
    assert_eq!(
        upgraded.obs.t2.origin,
        T2Origin::Upgrade(T2Upgrade::Feedback)
    );
    let t1 = upgraded.tier1_fallback.borrow();
    let t1 = t1.as_ref().expect("T1 retained for precise deopt recovery");
    assert_eq!(
        t1.obs.t2.state.get(),
        T2State::Upgraded(T2Upgrade::Feedback)
    );
    assert!(!t1.retired.get());
    assert!(
        upgraded.obs.entries.get() * 10 >= (CALLS - 22) * 9,
        "opt serves at least 90% of helper calls after both work windows"
    );
    let caller_id = caller_data.jit_runtime().compiled_id().unwrap();
    let caller_ptr = cache::compiled_leaf_ptr_for_test(caller_id).unwrap();
    let caller_leaf = unsafe { &*caller_ptr };
    assert!(
        caller_leaf
            .bytecode_spec_slots()
            .any(|slot| std::ptr::eq(slot.leaf_ptr(), upgraded)),
        "the normal native caller seam relinks to the opt helper"
    );
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
}

#[test]
fn tier2_opt_budget_denial_retries_native_helper_after_stable_windows() {
    const DENIED_CALLS: u64 = 40;
    const SERVED_CALLS: u64 = 400;
    let _settings = Settings::enter(OptMode::Opt);
    let mut ctx = Context::new();
    let helper = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(intern("length"))],
        1,
    );
    let arg = Value::vector(vec![Value::NIL; 3]);
    let expected = Vm::from_context(&mut ctx)
        .execute(&helper, vec![arg])
        .unwrap();
    let helper = Value::make_bytecode(helper);
    let name = intern("neovm--t2-opt-budget-retry-helper");
    ctx.obarray.set_symbol_function_id(name, helper);
    let caller = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(name)],
        1,
    ));
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    crate::emacs_core::eval::push_scratch_gc_roots(&[arg, helper, caller]);
    let helper_data = helper.get_bytecode_data().unwrap();
    let caller_data = caller.get_bytecode_data().unwrap();
    let call = |ctx: &mut Context| {
        assert_eq!(
            cache::try_run_compiled(ctx, caller_data, caller, &[arg]).unwrap(),
            Some(expected.bits())
        );
    };
    call(&mut ctx);
    let original_id = helper_data.jit_runtime().compiled_id().unwrap();
    let original_ptr = cache::compiled_leaf_ptr_for_test(original_id).unwrap();
    // SAFETY: this mutator's cache retains the current leaf and later its T1
    // fallback; neither the cache nor the rooted caller is cleared here.
    let original = unsafe { &*original_ptr };
    assert_eq!(original.selected_tier(), SelectedTier::Baseline);
    original.obs.compile_us.set(u32::MAX);
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        budget_pct: 1,
        floor_ms: 0,
        ..policy()
    }));
    let denied_before = stats().budget_denied;
    for _ in 0..DENIED_CALLS {
        call(&mut ctx);
    }
    assert!(stats().budget_denied > denied_before);
    assert_eq!(
        original.obs.t2.state.get(),
        T2State::BudgetWait,
        "temporary budget denial must retain the normal T1 service window"
    );
    assert!((1..=i64::from(policy().stable)).contains(&original.obs.t2.budget.get()));
    assert_eq!(original.obs.t2.reserved_us.get(), 0);
    assert!(original.obs.t2.source.borrow().is_some());
    assert_eq!(original.obs.t2.policy.borrow().attempts, 1);
    assert!(!original.retired.get());
    assert_eq!(helper_data.jit_runtime().compiled_id(), Some(original_id));
    let caller_ptr =
        cache::compiled_leaf_ptr_for_test(caller_data.jit_runtime().compiled_id().unwrap())
            .unwrap();
    let caller_leaf = unsafe { &*caller_ptr };
    assert!(
        caller_leaf
            .bytecode_spec_slots()
            .any(|slot| { std::ptr::eq(slot.leaf_ptr(), original) }),
        "denied requests leave normal caller slots linked to the retained T1"
    );

    // A denial must not reset the stability history or repeatedly scan it.
    // Changed numeric feedback is captured when affordability reopens a fresh
    // window; denied windows use no instability attempts or reservations.
    let unstable_before = stats().unstable;
    let frozen_version = original.obs.t2.policy.borrow().version.clone();
    helper_data.jit_runtime().widen_numeric(
        0,
        helper_data.executable_ops().len(),
        NumericFeedback::Float,
    );
    for _ in 0..policy().stable {
        call(&mut ctx);
    }
    assert_eq!(stats().unstable, unstable_before);
    assert_eq!(original.obs.t2.policy.borrow().version, frozen_version);
    assert_eq!(original.obs.t2.policy.borrow().attempts, 1);
    assert_eq!(original.obs.t2.state.get(), T2State::BudgetWait);
    assert_eq!(original.obs.t2.reserved_us.get(), 0);

    // Make the resource policy affordable deterministically; the earlier
    // denial did not consume a reservation and cannot permanently keep T1.
    original.obs.compile_us.set(1);
    force_tier2_policy_for_test(Some(policy()));
    for _ in 0..SERVED_CALLS {
        call(&mut ctx);
    }
    let upgraded_ptr =
        cache::compiled_leaf_ptr_for_test(helper_data.jit_runtime().compiled_id().unwrap())
            .unwrap();
    let upgraded = unsafe { &*upgraded_ptr };
    assert_ne!(original_ptr, upgraded_ptr);
    assert_eq!(upgraded.selected_tier(), SelectedTier::Opt);
    assert_eq!(
        upgraded.obs.t2.origin,
        T2Origin::Upgrade(T2Upgrade::Feedback)
    );
    assert_eq!(
        original.obs.t2.state.get(),
        T2State::Upgraded(T2Upgrade::Feedback)
    );
    assert_eq!(original.obs.t2.reserved_us.get(), 0);
    assert_eq!(original.obs.t2.budget.get(), DISARMED);
    let retained = upgraded.tier1_fallback.borrow();
    assert!(std::ptr::eq(&**retained.as_ref().unwrap(), original));
    assert!(!original.retired.get());
    assert!(upgraded.obs.entries.get() * 10 >= (SERVED_CALLS - 3) * 9);
    let caller_ptr =
        cache::compiled_leaf_ptr_for_test(caller_data.jit_runtime().compiled_id().unwrap())
            .unwrap();
    let caller_leaf = unsafe { &*caller_ptr };
    assert!(
        caller_leaf
            .bytecode_spec_slots()
            .any(|slot| { std::ptr::eq(slot.leaf_ptr(), upgraded) })
    );
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
}

#[test]
fn tier2_opt_budget_wait_freezes_use_feedback_and_reopens_a_fresh_window() {
    const DENIED_CALLS: u64 = 40;
    const SERVED_CALLS: u64 = 400;
    let _settings = Settings::enter(OptMode::Opt);
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    let mut ctx = Context::new();
    let length = intern("length");
    let alternative = intern("neovm--t2-budget-wait-length-alternative");
    let alternative_function = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(length)],
        1,
    ));
    ctx.obarray
        .set_symbol_function_id(alternative, alternative_function);
    // The callable is an argument: this Call site uses the normal Use shim,
    // whose counted feedback must stop while the compiler budget is closed.
    let helper = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Call(1), Op::Return],
        Vec::new(),
        2,
    );
    let arg = Value::vector(vec![Value::NIL; 3]);
    let callback = Value::from_sym_id(length);
    let expected = Vm::from_context(&mut ctx)
        .execute(&helper, vec![callback, arg])
        .unwrap();
    let helper = Value::make_bytecode(helper);
    let name = intern("neovm--t2-opt-budget-wait-helper");
    ctx.obarray.set_symbol_function_id(name, helper);
    let caller = Value::make_bytecode(function(
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::from_sym_id(name)],
        2,
    ));
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    crate::emacs_core::eval::push_scratch_gc_roots(&[arg, helper, caller, alternative_function]);
    let helper_data = helper.get_bytecode_data().unwrap();
    let caller_data = caller.get_bytecode_data().unwrap();
    let call = |ctx: &mut Context, callback: Value| {
        assert_eq!(
            cache::try_run_compiled(ctx, caller_data, caller, &[callback, arg]).unwrap(),
            Some(expected.bits())
        );
    };
    call(&mut ctx, callback);
    let original_id = helper_data.jit_runtime().compiled_id().unwrap();
    let original_ptr = cache::compiled_leaf_ptr_for_test(original_id).unwrap();
    // SAFETY: this mutator retains the current helper and later its exact T1
    // fallback throughout the rooted caller's normal native invocations.
    let original = unsafe { &*original_ptr };
    original.obs.compile_us.set(u32::MAX);
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        budget_pct: 1,
        floor_ms: 0,
        ..policy()
    }));
    let denied_before = stats().budget_denied;
    for _ in 0..DENIED_CALLS {
        call(&mut ctx, callback);
    }
    assert!(stats().budget_denied > denied_before);
    let site = helper_data
        .jit_runtime()
        .call_sites()
        .unwrap()
        .site_at(2)
        .unwrap();
    let frozen_count = site.count();
    for _ in 0..DENIED_CALLS {
        call(&mut ctx, callback);
    }
    assert_eq!(
        site.count(),
        frozen_count,
        "Use recording must close through repeated budget-denied windows; state={:?}, layout={}/{}",
        original.obs.t2.state.get(),
        std::mem::size_of::<T2State>(),
        std::mem::align_of::<T2State>(),
    );
    assert_eq!(
        <&'static str>::from(original.obs.t2.state.get()),
        "budget_wait"
    );
    assert_eq!(original.obs.t2.policy.borrow().attempts, 1);
    assert_eq!(original.obs.t2.reserved_us.get(), 0);
    assert!(original.obs.t2.source.borrow().is_some());
    assert_eq!(helper_data.jit_runtime().compiled_id(), Some(original_id));
    assert!(matches!(site.target(), CallTarget::Sym(sym) if sym == length));

    let changed_callback = Value::from_sym_id(alternative);
    for _ in 0..policy().stable {
        call(&mut ctx, changed_callback);
    }
    assert_eq!(site.count(), frozen_count);
    assert!(matches!(site.target(), CallTarget::Sym(sym) if sym == length));
    assert_eq!(original.obs.t2.policy.borrow().attempts, 1);

    // The next affordable request only reopens sampling. The changed callable
    // was invisible during BudgetWait and must be observed in a full fresh
    // stable window before the original helper can be upgraded.
    original.obs.compile_us.set(1);
    force_tier2_policy_for_test(Some(policy()));
    for _ in 0..policy().stable {
        if original.obs.t2.state.get() == T2State::Idle {
            break;
        }
        call(&mut ctx, changed_callback);
    }
    assert_eq!(original.obs.t2.state.get(), T2State::Idle);
    assert_eq!(original.obs.t2.budget.get(), i64::from(policy().stable));
    assert_eq!(original.obs.t2.reserved_us.get(), 0);
    assert_eq!(helper_data.jit_runtime().compiled_id(), Some(original_id));
    assert!(
        site.count() > frozen_count,
        "fresh sampling records normal calls again"
    );
    assert!(matches!(site.target(), CallTarget::Mega));
    for _ in 0..policy().stable {
        call(&mut ctx, changed_callback);
    }
    assert_eq!(helper_data.jit_runtime().compiled_id(), Some(original_id));
    assert_eq!(original.obs.t2.state.get(), T2State::Idle);
    assert_eq!(original.obs.t2.policy.borrow().attempts, 2);
    for _ in 0..SERVED_CALLS {
        call(&mut ctx, changed_callback);
    }
    let upgraded_ptr =
        cache::compiled_leaf_ptr_for_test(helper_data.jit_runtime().compiled_id().unwrap())
            .unwrap();
    let upgraded = unsafe { &*upgraded_ptr };
    assert_ne!(original_ptr, upgraded_ptr);
    assert_eq!(upgraded.selected_tier(), SelectedTier::Opt);
    assert_eq!(
        upgraded.obs.t2.origin,
        T2Origin::Upgrade(T2Upgrade::Feedback)
    );
    assert_eq!(original.obs.t2.reserved_us.get(), 0);
    let retained = upgraded.tier1_fallback.borrow();
    assert!(std::ptr::eq(&**retained.as_ref().unwrap(), original));
    assert!(!original.retired.get());
    assert!(upgraded.obs.entries.get() * 10 >= (SERVED_CALLS - 3) * 9);
    let caller_ptr =
        cache::compiled_leaf_ptr_for_test(caller_data.jit_runtime().compiled_id().unwrap())
            .unwrap();
    let caller_leaf = unsafe { &*caller_ptr };
    assert!(
        caller_leaf
            .bytecode_spec_slots()
            .any(|slot| { std::ptr::eq(slot.leaf_ptr(), upgraded) })
    );
    let final_count = site.count();
    for _ in 0..policy().stable {
        call(&mut ctx, changed_callback);
    }
    assert_eq!(
        site.count(),
        final_count,
        "installed opt code closes T1 recording"
    );
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
}

#[test]
fn tier2_opt_compile_seam_denial_preserves_t1_and_reopens_sampling() {
    let _settings = Settings::enter(OptMode::Opt);
    let _ctx = Context::new();
    let (f, leaf) = policy_fixture(47);
    force_opt_for_test(Some(OptMode::Opt), None);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(
        request_decision(&leaf, f.jit_runtime()),
        Some(T2Decision::Upgrade(T2Upgrade::Feedback))
    );
    assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
    leaf.obs.t2.state.set(T2State::Due(T2Upgrade::Feedback));
    leaf.obs.compile_us.set(u32::MAX);
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        budget_pct: 1,
        floor_ms: 0,
        ..policy()
    }));
    let snapshot = leaf.obs.t2.policy.borrow().version.clone();
    assert!(!reserve_compile(&leaf));
    assert!(wait_for_opt_budget(&leaf, T2Upgrade::Feedback));
    assert_eq!(leaf.obs.t2.state.get(), T2State::BudgetWait);
    assert_eq!(leaf.obs.t2.policy.borrow().version, snapshot);
    assert_eq!(leaf.obs.t2.policy.borrow().attempts, 1);
    assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
    f.jit_runtime()
        .widen_numeric(0, f.executable_ops().len(), NumericFeedback::Float);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(leaf.obs.t2.policy.borrow().version, snapshot);
    force_tier2_policy_for_test(Some(policy()));
    leaf.obs.compile_us.set(1);
    assert_eq!(request_decision(&leaf, f.jit_runtime()), None);
    assert_eq!(leaf.obs.t2.state.get(), T2State::Idle);
    assert_eq!(leaf.obs.t2.budget.get(), i64::from(policy().stable));
    assert_ne!(leaf.obs.t2.policy.borrow().version, snapshot);
    assert_eq!(leaf.obs.t2.policy.borrow().attempts, 1);
    assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
}

#[test]
fn tier2_opt_budget_wait_keeps_main_legacy_compile_seam_policy() {
    let _settings = Settings::enter(OptMode::Off);
    let _ctx = Context::new();
    for mode in [OptMode::Off, OptMode::Legacy] {
        let (f, leaf) = policy_fixture(47);
        force_opt_for_test(Some(mode), None);
        leaf.obs.t2.state.set(T2State::Due(T2Upgrade::Feedback));
        assert!(!wait_for_opt_budget(&leaf, T2Upgrade::Feedback));
        assert_eq!(leaf.obs.t2.state.get(), T2State::Due(T2Upgrade::Feedback));
        rearm_fallback(f.jit_runtime(), &leaf);
        assert_eq!(leaf.obs.t2.state.get(), T2State::Idle);
        assert_eq!(leaf.obs.t2.policy.borrow().attempts, 0);
        assert_eq!(leaf.obs.t2.reserved_us.get(), 0);
    }
}
