//! Real OSR cache construction, installation and ownership qualify admission.
//! Threading: each test owns its Context, source, leaves and compiler overrides;
//! no native execution overlaps a cache mutation or borrowed leaf inspection.

use super::super::{
    LeafOrigin, NativeRun, OSR_CACHE, OSR_PENDING, OsrEntry, OsrProbe, ReoptLevel, clear,
    evict_compiled, invalidate_for_reopt, record_compiled_obarray, sync_cache_to_current_heap,
    try_run_osr, try_run_osr_probe,
};
use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, opcode::Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::{bg, compile};
use crate::emacs_core::value::Value;
use std::rc::Rc;
use std::sync::Arc;

/// Scalar overrides and existing-cache lifetime belong to this invocation.
/// Threading: this scope stores no Lisp state and touches only this compiler's
/// existing mutator-owned cache; it never assumes other mutators are absent.
#[derive(Debug)]
#[must_use = "dropping the guard restores this compiler thread's test state"]
struct Settings(std::marker::PhantomData<*const ()>);

static_assertions::assert_not_impl_any!(Settings: Send, Sync);
const _: () = assert!(std::mem::size_of::<Settings>() == 0);
impl Settings {
    fn enter() -> Self {
        clear();
        compile::force_opt_for_test(Some(compile::OptMode::Opt), Some(compile::OptAdmit::ALL));
        compile::force_opt_profit_for_test(Some(compile::OptProfitMode::Lists));
        compile::force_opt_early_for_test(Some(compile::OptEarlyMode::Hot));
        compile::force_opt_max_ops_for_test(Some(48));
        compile::force_opt_require_osr_for_test(Some(false));
        compile::force_inline2_for_test(Some(compile::Inline2Mode::Off));
        compile::force_tier2_for_test(Some(compile::Tier2Knob::from_env(|_| None)));
        compile::force_deopt_for_test(false);
        bg::force_mode_for_test(Some(bg::BgMode::Sync));
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
        compile::force_opt_require_osr_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_tier2_for_test(None);
        bg::force_mode_for_test(None);
    }
}

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

fn snapshot(pair: Value) -> [Value; 3] {
    [pair, Value::make_int(4), Value::make_int(2)]
}

fn prepare(ctx: &mut Context, values: &[Value]) {
    // These are the existing compile/cache-owner seams. Pin the same obarray
    // an ordinary compile_cache_entry would pin before its frontend query.
    sync_cache_to_current_heap();
    record_compiled_obarray(Some(&ctx.obarray));
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(values);
}

fn ready(ctx: &Context, f: &ByteCodeFunction, header: usize) -> bool {
    has_ready_opt_osr(
        f.jit_runtime(),
        Some(ctx.obarray.generation()),
        std::iter::once(header),
    )
}

fn install(ctx: &mut Context, f: &ByteCodeFunction, pair: Value) {
    let values = snapshot(pair);
    prepare(ctx, &values);
    assert_eq!(
        try_run_osr(ctx, f, 1, &values, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
}

#[test]
fn opt_osr_evidence_missing_and_exact_source_header_checks_do_not_assign_or_heat() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let source = list_loop();
    let heat = source.jit_runtime().heat();
    assert_eq!(source.jit_runtime().compiled_id(), None);
    assert!(!ready(&ctx, &source, 1));
    assert_eq!(source.jit_runtime().compiled_id(), None);
    assert_eq!(source.jit_runtime().heat(), heat);
    install(&mut ctx, &source, pair());
    assert!(ready(&ctx, &source, 1));
    assert!(!ready(&ctx, &source, 0));
    let other = list_loop();
    assert!(!ready(&ctx, &other, 1));
    assert_eq!(other.jit_runtime().compiled_id(), None);
    assert_eq!(source.jit_runtime().heat(), heat);
    clear();
    assert!(!ready(&ctx, &source, 1));
}

#[test]
fn opt_osr_evidence_actual_negative_and_baseline_entries_do_not_qualify() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let mut refused = list_loop();
    refused.lexical = false;
    let values = snapshot(pair());
    prepare(&mut ctx, &values);
    assert!(matches!(
        try_run_osr_probe(&mut ctx, &refused, 1, &values, &[]),
        OsrProbe::NoTransfer
    ));
    let id = refused.jit_runtime().compiled_id().unwrap();
    assert!(OSR_CACHE.with(|cache| matches!(cache.borrow().get(&(id, 1)), Some(None))));
    assert!(!ready(&ctx, &refused, 1));

    compile::force_opt_for_test(Some(compile::OptMode::Legacy), Some(compile::OptAdmit::ALL));
    let baseline = list_loop();
    install(&mut ctx, &baseline, pair());
    let id = baseline.jit_runtime().compiled_id().unwrap();
    assert!(OSR_CACHE.with(|cache| {
        cache
            .borrow()
            .get(&(id, 1))
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .leaf
            .selected_tier()
            == SelectedTier::Baseline
    }));
    assert!(!ready(&ctx, &baseline, 1));
}

#[test]
fn opt_osr_evidence_pending_opt_requires_real_install_before_admission() {
    let _settings = Settings::enter();
    bg::force_deferred_install_for_test(true);
    bg::hold_publish_for_test(true);
    let mut ctx = Context::new();
    let source = list_loop();
    let pair = pair();
    let values = snapshot(pair);
    prepare(&mut ctx, &values);
    assert!(matches!(
        try_run_osr_probe(&mut ctx, &source, 1, &values, &[]),
        OsrProbe::Pending
    ));
    let id = source.jit_runtime().compiled_id().unwrap();
    assert!(OSR_PENDING.with(|cache| cache.borrow().contains_key(&(id, 1))));
    assert!(!ready(&ctx, &source, 1));
    assert_eq!(pair.cons_car(), Value::make_int(-1));
    assert_eq!(bg::publish_held_for_test(), 1);
    // Backend publication is not mutator installation and cannot qualify.
    assert!(!ready(&ctx, &source, 1));
    assert!(OSR_PENDING.with(|cache| cache.borrow().contains_key(&(id, 1))));
    assert_eq!(
        try_run_osr(&mut ctx, &source, 1, &values, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
    assert!(ready(&ctx, &source, 1));
    assert!(!OSR_PENDING.with(|cache| cache.borrow().contains_key(&(id, 1))));
}

#[test]
fn opt_osr_evidence_cache_owner_and_reentrant_borrow_fail_closed() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let source = list_loop();
    install(&mut ctx, &source, pair());
    assert!(ready(&ctx, &source, 1));
    assert!(!has_ready_opt_osr(
        source.jit_runtime(),
        None,
        std::iter::once(1)
    ));
    assert!(!has_ready_opt_osr(
        source.jit_runtime(),
        Some(ctx.obarray.generation().wrapping_add(1)),
        std::iter::once(1)
    ));
    let heap = COMPILED_HEAP.with(|owner| owner.get()).unwrap();
    let old = COMPILED_HEAP.with(|owner| owner.replace(Some(heap.wrapping_add(1))));
    assert!(!ready(&ctx, &source, 1));
    COMPILED_HEAP.with(|owner| owner.set(old));
    OSR_CACHE.with(|cache| {
        let _borrow = cache.borrow_mut();
        assert!(
            !ready(&ctx, &source, 1),
            "a read-only admission query must not panic or mutate"
        );
    });
    assert!(ready(&ctx, &source, 1));
}

#[test]
fn opt_osr_evidence_reopt_and_grown_closure_prefix_revoke_existing_leaf() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let source = list_loop();
    let pair = pair();
    install(&mut ctx, &source, pair);
    assert!(ready(&ctx, &source, 1));
    source.jit_runtime().note_reopt(ReoptLevel::NoInline, 3);
    assert!(
        !ready(&ctx, &source, 1),
        "stale compiled level is rejected even before eviction"
    );
    let values = snapshot(pair);
    invalidate_for_reopt(
        &source,
        LeafOrigin::Osr {
            header_pc: 1,
            snapshot: &values,
        },
        ReoptLevel::BaselineOnly,
        crate::emacs_core::jit::reopt::Reprofile::Immediate,
    );
    assert!(!ready(&ctx, &source, 1));

    let closure_source = list_loop();
    install(&mut ctx, &closure_source, pair);
    assert!(ready(&ctx, &closure_source, 1));
    let id = closure_source.jit_runtime().compiled_id().unwrap();
    assert_eq!(
        closure_source.jit_runtime().note_patched_prefix(1),
        Some(id)
    );
    assert!(
        !ready(&ctx, &closure_source, 1),
        "a leaf compiled for a narrower dynamic prefix is stale"
    );
    evict_compiled(id);
    assert!(!ready(&ctx, &closure_source, 1));
}

#[test]
fn opt_osr_evidence_authenticates_actual_leaf_hold_with_old_marker_alive() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let source = list_loop();
    install(&mut ctx, &source, pair());
    let id = source.jit_runtime().compiled_id().unwrap();
    let entry = OSR_CACHE.with(|cache| cache.borrow_mut().remove(&(id, 1)).unwrap().unwrap());
    let OsrEntry {
        leaf,
        stack_depth,
        bind_depth,
    } = entry;
    let mut leaf = Rc::try_unwrap(leaf)
        .unwrap_or_else(|_| panic!("no native frame retains this completed OSR leaf"));
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    let old_holds = std::mem::replace(
        &mut leaf.feedback_holds,
        vec![Arc::new(RuntimeState::new())].into_boxed_slice(),
    );
    // The actual observation key and its old registered owner remain alive,
    // modelling address reuse before an old leaf's later hold field drops.
    assert!(!old_holds.is_empty());
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    OSR_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            (id, 1),
            Some(OsrEntry {
                leaf: Rc::new(leaf),
                stack_depth,
                bind_depth,
            }),
        )
    });
    assert!(
        !ready(&ctx, &source, 1),
        "a live matching registry key is not actual leaf ownership"
    );
    let entry = OSR_CACHE.with(|cache| cache.borrow_mut().remove(&(id, 1)).unwrap().unwrap());
    let mut leaf =
        Rc::try_unwrap(entry.leaf).unwrap_or_else(|_| panic!("query takes no Rc ownership"));
    leaf.feedback_holds = old_holds;
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    OSR_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            (id, 1),
            Some(OsrEntry {
                leaf: Rc::new(leaf),
                stack_depth,
                bind_depth,
            }),
        )
    });
    assert!(ready(&ctx, &source, 1));
}
