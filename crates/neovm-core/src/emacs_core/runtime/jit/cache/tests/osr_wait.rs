//! A normal entry waits for the pending optimized OSR compile its frontend
//! depends on, and is admitted against that job's settled outcome.
//! Threading: each test owns its Context, sources, caches and compiler
//! overrides; no native execution overlaps a cache mutation.

use super::super::{
    OSR_CACHE, OSR_PENDING, OsrProbe, cache_entry_kind_for_test, clear, resolve_compiled_leaf_ptr,
    try_run_compiled, try_run_osr, try_run_osr_probe,
};
use crate::emacs_core::bytecode::{ByteCodeFunction, opcode::Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::bg;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{self, NativeRun, OptOsrRequirement};
use crate::emacs_core::value::Value;

/// Scalar overrides and cache lifetime belong to this invocation.
#[must_use = "dropping the guard restores this compiler thread's test state"]
struct Settings(std::marker::PhantomData<*const ()>);

impl Settings {
    fn enter() -> Self {
        clear();
        compile::force_opt_for_test(Some(compile::OptMode::Opt), Some(compile::OptAdmit::ALL));
        compile::force_opt_profit_for_test(Some(compile::OptProfitMode::PrimitiveLists));
        compile::force_opt_early_for_test(Some(compile::OptEarlyMode::Hot));
        compile::force_opt_max_ops_for_test(Some(48));
        compile::force_inline2_for_test(Some(compile::Inline2Mode::Off));
        compile::force_tier2_for_test(Some(compile::Tier2Knob::from_env(|_| None)));
        compile::force_deopt_for_test(false);
        bg::force_mode_for_test(Some(bg::BgMode::Sync));
        bg::force_deferred_install_for_test(true);
        bg::hold_publish_for_test(true);
        Self(std::marker::PhantomData)
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        bg::hold_publish_for_test(false);
        let _ = bg::publish_held_for_test();
        bg::force_deferred_install_for_test(false);
        clear();
        compile::force_opt_for_test(None, None);
        compile::force_opt_profit_for_test(None);
        compile::force_opt_early_for_test(None);
        compile::force_opt_max_ops_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_tier2_for_test(None);
        bg::force_mode_for_test(None);
    }
}

/// `(lambda (pair n) (dotimes (i n) (setcar pair i)) n)`: one list backedge.
fn list_loop() -> ByteCodeFunction {
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(13),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        2,
    );
    f.jit_runtime().set_hot_for_test();
    f
}

fn pair() -> Value {
    let value = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(value);
    value
}

/// The interpreter's live snapshot at the loop header `1`.
fn snapshot(pair: Value) -> [Value; 3] {
    [pair, Value::make_int(4), Value::make_int(2)]
}

/// Start the header's optimized OSR compile; its backend result is held.
fn pend_osr(ctx: &mut Context, f: &ByteCodeFunction) -> u64 {
    let values = snapshot(pair());
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&values);
    assert!(matches!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { try_run_osr_probe(ctx, f, 1, &values, &[]) },
        OsrProbe::Pending
    ));
    let id = f.jit_runtime().compiled_id().unwrap();
    let tier = OSR_PENDING.with(|p| p.borrow()[&(id, 1)].job.leaf().selected_tier());
    assert_eq!(tier, SelectedTier::Opt);
    id
}

/// Install (and run) the published OSR job at its own back-edge probe.
fn install_osr(ctx: &mut Context, f: &ByteCodeFunction) {
    let pair = pair();
    let values = snapshot(pair);
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&values);
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { try_run_osr(ctx, f, 1, &values, &[]) },
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
}

/// One dispatched call; `Some` when it ran natively.
fn call(ctx: &mut Context, f: &ByteCodeFunction) -> Option<usize> {
    let function = Value::make_bytecode(f.clone());
    crate::emacs_core::eval::push_scratch_gc_root(function);
    let pair = pair();
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let ran = unsafe { try_run_compiled(ctx, f, function, &[pair, Value::make_int(6)]) }.unwrap();
    if ran.is_some() {
        assert_eq!(pair.cons_car(), Value::make_int(5));
    } else {
        assert_eq!(pair.cons_car(), Value::make_int(-1));
    }
    ran
}

fn entry_tier(ctx: &mut Context, f: &ByteCodeFunction) -> SelectedTier {
    let leaf = resolve_compiled_leaf_ptr(ctx, f).expect("a native entry");
    // SAFETY: cached leaves stay allocated; no native frame is live.
    unsafe { &*leaf }.selected_tier()
}

fn held(f: &ByteCodeFunction) -> bool {
    let rt = f.jit_runtime();
    rt.deferred_heat() > rt.heat()
}

#[test]
fn osr_wait_entry_waits_for_pending_opt_osr_then_selects_opt() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    let source = list_loop();
    let id = pend_osr(&mut ctx, &source);
    assert_eq!(call(&mut ctx, &source), None);
    assert_eq!(cache_entry_kind_for_test(id), "deferred-osr");
    assert!(held(&source), "the waiting entry holds the dispatcher");
    // The loop's own wrap installs the finished job first.
    assert_eq!(bg::publish_held_for_test(), 1);
    install_osr(&mut ctx, &source);
    assert!(!held(&source), "settlement releases the entry's hold");
    bg::force_deferred_install_for_test(false);
    assert_eq!(call(&mut ctx, &source), Some(Value::make_int(6).bits()));
    assert_eq!(entry_tier(&mut ctx, &source), SelectedTier::Opt);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn osr_wait_publication_releases_the_entry_to_install_the_job_first() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    let source = list_loop();
    let id = pend_osr(&mut ctx, &source);
    assert_eq!(call(&mut ctx, &source), None);
    assert_eq!(cache_entry_kind_for_test(id), "deferred-osr");
    // The job finishes but its loop never wraps again to install it: the
    // publication itself releases the entry, whose next call installs it.
    assert_eq!(bg::publish_held_for_test(), 1);
    assert!(!held(&source), "publication releases the entry's hold");
    bg::force_deferred_install_for_test(false);
    assert_eq!(call(&mut ctx, &source), Some(Value::make_int(6).bits()));
    assert!(OSR_PENDING.with(|p| p.borrow().is_empty()));
    assert!(OSR_CACHE.with(|c| c.borrow()[&(id, 1)].is_some()));
    assert_eq!(entry_tier(&mut ctx, &source), SelectedTier::Opt);
}

#[test]
fn osr_wait_entry_stays_parked_however_hot_until_the_job_settles() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    let source = list_loop();
    let id = pend_osr(&mut ctx, &source);
    assert_eq!(call(&mut ctx, &source), None);
    for _ in 0..5_000 {
        source.jit_runtime().bump_heat();
    }
    assert!(held(&source), "no heat ends the park");
    assert!(source.jit_runtime().tier_up_parked());
    assert_eq!(call(&mut ctx, &source), None);
    assert_eq!(cache_entry_kind_for_test(id), "deferred-osr");
}

#[test]
fn osr_wait_dropped_job_releases_the_entry_to_its_settled_tier() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    let source = list_loop();
    let id = pend_osr(&mut ctx, &source);
    assert_eq!(call(&mut ctx, &source), None);
    assert!(held(&source));
    // Cancellation (a cache clear, invalidation, or eviction) also settles.
    OSR_PENDING.with(|p| p.borrow_mut().clear());
    assert!(!held(&source));
    bg::force_deferred_install_for_test(false);
    assert_eq!(call(&mut ctx, &source), Some(Value::make_int(6).bits()));
    assert_ne!(cache_entry_kind_for_test(id), "deferred-osr");
    assert_eq!(entry_tier(&mut ctx, &source), SelectedTier::Baseline);
}

#[test]
fn osr_wait_settlement_keeps_a_newer_foreign_hold() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    let source = list_loop();
    pend_osr(&mut ctx, &source);
    assert_eq!(call(&mut ctx, &source), None);
    let foreign = source.jit_runtime().heat() + 5000;
    source.jit_runtime().defer_tier_up(foreign);
    assert_eq!(bg::publish_held_for_test(), 1);
    install_osr(&mut ctx, &source);
    assert_eq!(source.jit_runtime().deferred_heat(), foreign);
}

#[test]
fn osr_wait_frontend_without_osr_requirement_compiles_at_once() {
    let _settings = Settings::enter();
    let _require = compile::opt_require_osr_scope_for_test(OptOsrRequirement::Optional);
    let mut ctx = Context::new();
    let source = list_loop();
    let id = pend_osr(&mut ctx, &source);
    bg::force_deferred_install_for_test(false);
    assert_eq!(call(&mut ctx, &source), Some(Value::make_int(6).bits()));
    assert_ne!(cache_entry_kind_for_test(id), "deferred-osr");
    assert!(!source.jit_runtime().tier_up_parked(), "nothing waits");
}
