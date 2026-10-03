//! B11 upgrade jobs: a running T1 remains native until install; failed or
//! invalidated jobs cannot remove it, and install never re-arms old slots.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::lowering::RegallocChoice;
use crate::emacs_core::jit::compile::{
    CompiledLeaf, Tier2Knob, Tier2PolicyKnob, force_deopt_for_test, force_tier2_for_test,
    force_tier2_policy_for_test,
};
use crate::emacs_core::jit::reopt::{LeafOrigin, Reprofile};
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::jit::tier2::{T2Origin, T2State, T2Upgrade};
use crate::emacs_core::jit::{Plan, ReoptLevel, try_run_compiled};
use crate::emacs_core::value::Value;

fn setup() -> Context {
    let mut knob = Tier2Knob::from_env(|_| None);
    knob.on = true;
    knob.window = 2;
    force_tier2_for_test(Some(knob));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }));
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(false);
    force_deopt_for_test(false);
    force_observe_for_test(ObserveOverride {
        entry_count: true,
        ..Default::default()
    });
    Context::new()
}

fn done() {
    hold_publish_for_test(false);
    let _ = publish_held_for_test();
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
}

fn constant(value: Value) -> ByteCodeFunction {
    let mut f = function(vec![Op::Constant(0), Op::Return], vec![value], 0);
    f.lexical = false; // Isolate Fast -> Full jobs from Feedback/MIR policy.
    f.jit_runtime().set_hot_for_test();
    f
}

fn run(ctx: &mut Context, f: &ByteCodeFunction) -> Option<usize> {
    try_run_compiled(ctx, f, Value::NIL, &[]).expect("no signal")
}

fn current(f: &ByteCodeFunction) -> &'static CompiledLeaf {
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let ptr = cache::compiled_leaf_ptr_for_test(id).expect("native leaf");
    // SAFETY: this mutator retains current and retired leaves until clear;
    // these tests never clear while the reference remains live.
    unsafe { &*ptr }
}

fn request(ctx: &mut Context, f: &ByteCodeFunction) -> &'static CompiledLeaf {
    let answer = f.constants[0].bits();
    assert_eq!(run(ctx, f), Some(answer));
    let old = current(f);
    assert_eq!(old.regalloc, RegallocChoice::Fast);
    assert_eq!(run(ctx, f), Some(answer));
    assert_eq!(run(ctx, f), Some(answer));
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
    force_deferred_install_for_test(true);
    old
}

#[test]
fn jit_bg_upgrade_serves_old_native_until_successful_install() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    let mut ctx = setup();
    let f = constant(Value::make_int(7));
    let old = request(&mut ctx, &f);
    let hold = f.jit_runtime().deferred_heat();
    let before = stats_snapshot();
    hold_publish_for_test(true);
    for _ in 0..20 {
        assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
        assert!(std::ptr::eq(current(&f), old));
        assert!(!old.retired.get());
        assert_eq!(f.jit_runtime().deferred_heat(), hold);
        assert!(matches!(f.jit_runtime().dispatch(), Plan::Compiled));
    }
    let id = f.jit_runtime().compiled_id().unwrap();
    assert_eq!(cache::cache_entry_kind_for_test(id), "upgrading");
    assert!(old.obs.t2.reserved_us.get() > 0);
    assert!(
        cache::resolve_compiled_leaf_ptr(&mut ctx, &f).is_none(),
        "old raw slots must not bypass future probes"
    );
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Upgrade as usize],
        before.enqueued[JobClass::Upgrade as usize] + 1
    );
    assert_eq!(publish_held_for_test(), 1);
    assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
    let upgraded = current(&f);
    assert!(!std::ptr::eq(upgraded, old));
    assert_eq!(upgraded.regalloc, RegallocChoice::Full);
    assert_eq!(upgraded.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Retier));
    assert!(old.retired.get());
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(old.obs.t2.state.get(), T2State::Upgraded(T2Upgrade::Retier));
    assert!(
        upgraded.tier1_fallback.borrow().is_none(),
        "T1' retires its old leaf"
    );
    assert_eq!(
        stats_snapshot().installed[JobClass::Upgrade as usize],
        before.installed[JobClass::Upgrade as usize] + 1
    );
    done();
}

#[test]
fn jit_bg_failed_upgrade_keeps_the_old_leaf_native() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    let mut ctx = setup();
    let f = constant(Value::make_int(7));
    let old = request(&mut ctx, &f);
    fail_next_backend_for_test();
    assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
    assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
    assert!(std::ptr::eq(current(&f), old));
    assert!(!old.retired.get());
    assert!(
        matches!(f.jit_runtime().dispatch(), Plan::Compiled),
        "upgrade failure never rejects the working T1"
    );
    assert_eq!(old.obs.t2.state.get(), T2State::Kept);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    done();
}

#[test]
fn jit_bg_pending_upgrade_roots_both_leaves_and_idle_drain_installs() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    let mut ctx = setup();
    let value = ctx.eval_str("(list 1 2)").expect("heap constant");
    let f = constant(value);
    let old = request(&mut ctx, &f);
    let (_, before) = cache::compiled_cache_probe();
    hold_publish_for_test(true);
    assert_eq!(run(&mut ctx, &f), Some(value.bits()));
    let (_, pending) = cache::compiled_cache_probe();
    assert!(
        pending >= before + 1,
        "pending front and running T1 are rooted"
    );
    let mut roots = Vec::new();
    cache::collect_jit_reloc_gc_roots(&mut roots);
    assert!(roots.iter().filter(|v| v.bits() == value.bits()).count() >= 2);
    assert_eq!(publish_held_for_test(), 1);
    cache::drain_ready_pending(Some(&ctx));
    assert!(old.retired.get());
    assert_eq!(current(&f).regalloc, RegallocChoice::Full);
    assert_eq!(run(&mut ctx, &f), Some(value.bits()));
    done();
}

#[test]
fn jit_bg_deopt_of_old_leaf_supersedes_pending_upgrade() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    let mut ctx = setup();
    let f = constant(Value::make_int(7));
    let old = request(&mut ctx, &f);
    hold_publish_for_test(true);
    assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
    let discarded = stats_snapshot().discarded[DiscardReason::Superseded as usize];
    cache::invalidate_for_reopt(
        &f,
        LeafOrigin::Entry,
        ReoptLevel::NoInline,
        Reprofile::Window,
    );
    assert!(old.retired.get());
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(pending_count(), 0);
    assert_eq!(
        stats_snapshot().discarded[DiscardReason::Superseded as usize],
        discarded + 1
    );
    assert_eq!(publish_held_for_test(), 1);
    cache::drain_ready_pending(Some(&ctx));
    let id = f.jit_runtime().compiled_id().unwrap();
    assert_eq!(
        cache::cache_entry_kind_for_test(id),
        "deferred-reopt",
        "cancelled result cannot resurrect a stale leaf"
    );
    done();
}

#[test]
fn jit_bg_retier_job_class_is_opt_in() {
    let mut knob = Tier2Knob::from_env(|_| None);
    force_tier2_for_test(Some(knob));
    assert_eq!(JobClass::for_origin_any(CompileOrigin::Retier), None);
    knob.on = true;
    force_tier2_for_test(Some(knob));
    assert_eq!(
        JobClass::for_origin_any(CompileOrigin::Retier),
        Some(JobClass::Upgrade)
    );
    assert!(
        JobClass::Osr < JobClass::FirstSight
            && JobClass::FirstSight < JobClass::Entry
            && JobClass::Entry < JobClass::Upgrade
    );
    force_tier2_for_test(None);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn jit_bg_worker_upgrade_keeps_t1_running_until_install() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    let mut ctx = setup();
    let f = constant(Value::make_int(7));
    let old = request(&mut ctx, &f);
    force_deferred_install_for_test(false);
    force_mode_for_test(Some(BgMode::Threaded));
    let before = stats_snapshot().installed[JobClass::Upgrade as usize];
    {
        let _held = hold_workers_for_test();
        for _ in 0..20 {
            assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
            assert!(std::ptr::eq(current(&f), old));
            assert!(!old.retired.get());
        }
        let id = f.jit_runtime().compiled_id().unwrap();
        assert_eq!(cache::cache_entry_kind_for_test(id), "upgrading");
    }
    assert!(quiesce_for_test(std::time::Duration::from_secs(60)));
    cache::drain_ready_pending(Some(&ctx));
    assert!(old.retired.get());
    assert_eq!(current(&f).regalloc, RegallocChoice::Full);
    assert_eq!(
        stats_snapshot().installed[JobClass::Upgrade as usize],
        before + 1
    );
    assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
    done();
}

#[test]
fn jit_bg_feedback_upgrade_retains_and_reverts_to_its_t1() {
    let mut ctx = setup();
    let (ops, constants, arity) = corpus().swap_remove(3);
    let f = function(ops, constants, arity);
    let run_loop = |ctx: &mut Context| {
        try_run_compiled(ctx, &f, Value::NIL, &[Value::make_int(1024)]).expect("no signal")
    };
    assert_eq!(run_loop(&mut ctx), Some(Value::make_int(1024).bits()));
    let old = current(&f);
    assert_eq!(old.regalloc, RegallocChoice::Full);
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Feedback));
    force_deferred_install_for_test(true);
    hold_publish_for_test(true);
    assert_eq!(run_loop(&mut ctx), Some(Value::make_int(1024).bits()));
    assert!(std::ptr::eq(current(&f), old));
    assert!(!old.retired.get());
    assert_eq!(publish_held_for_test(), 1);
    assert_eq!(run_loop(&mut ctx), Some(Value::make_int(1024).bits()));
    let t2 = current(&f);
    assert_eq!(t2.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Feedback));
    assert!(
        !old.retired.get(),
        "a retained fallback remains valid for revert"
    );
    assert!(
        t2.tier1_fallback
            .borrow()
            .as_ref()
            .is_some_and(|leaf| std::ptr::eq(leaf.as_ref(), old))
    );
    assert!(cache::revert_t2_to_t1(&f, t2));
    assert!(std::ptr::eq(current(&f), old));
    assert!(!old.retired.get());
    assert!(t2.retired.get());
    assert!(t2.tier1_fallback.borrow().is_none());
    assert_eq!(run_loop(&mut ctx), Some(Value::make_int(1024).bits()));
    done();
}

/// The mutator's cache is initialized by T1 before the budget ledger is
/// initialized by its request. Its pending worker job remains unsettled
/// when the thread's TLS destructors run; teardown must not panic while
/// cancellation charges that job's reserved compile estimate.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn jit_bg_pending_upgrade_mutator_thread_exit_is_safe() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    {
        // Only the workers are held, by a parent-owned RAII guard. The
        // child creates all Lisp/cache state itself and sends none back.
        let _held = hold_workers_for_test();
        std::thread::spawn(|| {
            let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
                crate::emacs_core::jit::compile::OptMode::Legacy,
            );
            let mut ctx = setup();
            let f = constant(Value::make_int(7));
            let old = request(&mut ctx, &f);
            force_deferred_install_for_test(false);
            force_mode_for_test(Some(BgMode::Threaded));
            assert_eq!(run(&mut ctx, &f), Some(Value::make_int(7).bits()));
            let id = f.jit_runtime().compiled_id().expect("compiled T1");
            assert_eq!(cache::cache_entry_kind_for_test(id), "upgrading");
            assert_eq!(pending_count(), 1);
            assert!(old.obs.t2.reserved_us.get() > 0);
            // Deliberately leave the pending upgrade to TLS destruction.
            // Context::drop retracts the heap without clearing COMPILED.
        })
        .join()
        .expect("pending-upgrade TLS destruction must complete");
    }
    assert!(quiesce_for_test(std::time::Duration::from_secs(10)));
}
