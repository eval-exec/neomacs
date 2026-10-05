//! OSR-backed normal selection preserves original MIR and native list behavior.
//! Threading: contexts, sources, cache entries and scalar settings belong to
//! this test invocation; no leaf inspection overlaps execution or invalidation.

use super::frontend_tests::{Settings, count_loop};
use super::lists_frontend_tests::{list_loop, normalize};
use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{captured_clif, function};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, CompiledLeaf, Context, NativeRun, OptAdmit, OptEarlyMode, OptOsrRequirement,
    OptProfitMode, RegallocPolicy, Value, compile_bytecode_function_requested,
    force_opt_early_for_test, force_opt_for_test, force_opt_profit_for_test,
    opt_require_osr_scope_for_test,
};
use crate::emacs_core::jit::stats::CompileOrigin;
use crate::emacs_core::jit::tier2::T2Upgrade;

/// The existing cache alone is scoped here; no Lisp value or new mutator state
/// is retained. Every test inspects leaves only outside native execution.
#[derive(Debug)]
#[must_use = "dropping the guard restores this compiler thread's test state"]
struct CacheScope(std::marker::PhantomData<*const ()>);

static_assertions::assert_not_impl_any!(CacheScope: Send, Sync);
const _: () = assert!(std::mem::size_of::<CacheScope>() == 0);
impl CacheScope {
    fn enter() -> Self {
        cache::clear();
        Self(std::marker::PhantomData)
    }
}
impl Drop for CacheScope {
    fn drop(&mut self) {
        cache::clear();
    }
}

fn compile(ctx: &Context, f: &ByteCodeFunction, tier: CompileTier) -> CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin: CompileOrigin::Dispatch,
            tier,
        },
    )
    .unwrap()
}

fn pair() -> Value {
    let value = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(value);
    value
}

#[test]
fn opt_require_osr_normal_only_sources_keep_numeric_mir_and_list_legacy_clif() {
    let _settings = Settings::enter();
    let _require = opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let mut ctx = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    for list in [false, true] {
        for tier in [CompileTier::T1, CompileTier::Upgrade(T2Upgrade::Retier)] {
            let source = if list { list_loop() } else { count_loop(0) };
            source.jit_runtime().set_hot_for_test();
            let heat = source.jit_runtime().heat();
            force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
            let mut guarded = None;
            let guarded_clif = captured_clif(|| guarded = Some(compile(&ctx, &source, tier)));
            let guarded = guarded.unwrap();
            assert_eq!(
                guarded.selected_tier(),
                if list {
                    SelectedTier::Baseline
                } else {
                    SelectedTier::Mir
                }
            );
            assert_eq!(
                source.jit_runtime().compiled_id(),
                None,
                "the prerequisite must not assign a source id"
            );
            assert_eq!(source.jit_runtime().heat(), heat);
            force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
            let legacy_clif = captured_clif(|| {
                compile(&ctx, &source, tier);
            });
            assert!(!guarded_clif.is_empty());
            assert_eq!(normalize(guarded_clif), normalize(legacy_clif));
            if list {
                let pair = pair();
                assert_eq!(
                    guarded.call(
                        &mut ctx as *mut Context as *mut u8,
                        &[pair, Value::make_int(4)]
                    ),
                    NativeRun::Ok(Value::make_int(4).bits())
                );
                assert_eq!(pair.cons_car(), Value::make_int(3));
            } else {
                assert_eq!(
                    guarded.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(4)]),
                    NativeRun::Ok(Value::make_int(4).bits())
                );
            }
        }
    }
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_require_osr_ready_list_osr_then_normal_cache_selects_opt_and_runs_mutation() {
    let _settings = Settings::enter();
    let _require = opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let _cache = CacheScope::enter();
    let mut ctx = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let source = list_loop();
    source.jit_runtime().set_hot_for_test();
    let pair = pair();
    let snapshot = [pair, Value::make_int(4), Value::make_int(2)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut ctx, &source, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
    let osr = cache::osr_leaf_ptr_for_test(&source, 1).unwrap();
    assert_eq!(unsafe { &*osr }.selected_tier(), SelectedTier::Opt);
    // The real normal cache seam pins ownership before its frontier query and
    // holds COMPILED mutably while reading the distinct existing OSR cache.
    let function_value = Value::make_bytecode(source.clone());
    crate::emacs_core::eval::push_scratch_gc_root(function_value);
    assert_eq!(
        cache::try_run_compiled(
            &mut ctx,
            &source,
            function_value,
            &[pair, Value::make_int(6)]
        )
        .unwrap(),
        Some(Value::make_int(6).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(5));
    let entry = cache::resolve_compiled_leaf_ptr(&mut ctx, &source).unwrap();
    assert_eq!(unsafe { &*entry }.selected_tier(), SelectedTier::Opt);
    assert_eq!(ctx.jit_root_stack_top, 0);
    cache::clear();
}

fn numeric_then_list() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(9),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Pop,
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(23),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(11),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        2,
    )
}

#[test]
fn opt_require_osr_numeric_header_cannot_qualify_separate_list_backedge() {
    let _settings = Settings::enter();
    let _require = opt_require_osr_scope_for_test(OptOsrRequirement::Required);
    let _cache = CacheScope::enter();
    let mut ctx = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    // Pin the real normal cache's current obarray through an unrelated numeric
    // MIR source; this source's entry remains absent until its OSR evidence.
    let numeric = count_loop(0);
    assert_eq!(
        cache::try_run_compiled(&mut ctx, &numeric, Value::NIL, &[Value::make_int(0)]).unwrap(),
        Some(Value::make_int(0).bits())
    );
    let source = numeric_then_list();
    source.jit_runtime().set_hot_for_test();
    let pair = pair();
    let snapshot = [pair, Value::make_int(4), Value::make_int(2)];
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut ctx, &source, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    let osr = cache::osr_leaf_ptr_for_test(&source, 1).unwrap();
    assert_eq!(unsafe { &*osr }.selected_tier(), SelectedTier::Opt);
    assert!(cache::has_ready_opt_osr(
        source.jit_runtime(),
        Some(ctx.obarray.generation()),
        std::iter::once(1)
    ));
    assert_eq!(
        ready_osr_front(FrontChoice::SelectedAfterMir, &source, Some(&ctx.obarray)),
        FrontChoice::Legacy
    );
    ctx.bc_buf.clear();
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut ctx, &source, 11, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(4).bits()))
    );
    assert_eq!(
        ready_osr_front(FrontChoice::SelectedAfterMir, &source, Some(&ctx.obarray)),
        FrontChoice::SelectedAfterMir
    );
    let selected = compile(&ctx, &source, CompileTier::T1);
    assert_eq!(selected.selected_tier(), SelectedTier::Opt);
    assert_eq!(
        selected.call(
            &mut ctx as *mut Context as *mut u8,
            &[pair, Value::make_int(6)]
        ),
        NativeRun::Ok(Value::make_int(6).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(5));
    cache::clear();
}

#[test]
fn opt_require_osr_off_keeps_old_selection_and_legacy_ignores_on() {
    let _settings = Settings::enter();
    let ctx = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let source = list_loop();
    source.jit_runtime().set_hot_for_test();
    {
        let _off = opt_require_osr_scope_for_test(OptOsrRequirement::Optional);
        assert_eq!(
            compile(&ctx, &source, CompileTier::T1).selected_tier(),
            SelectedTier::Opt
        );
    }
    {
        let _on = opt_require_osr_scope_for_test(OptOsrRequirement::Required);
        assert_eq!(
            compile(&ctx, &source, CompileTier::T1).selected_tier(),
            SelectedTier::Baseline
        );
        assert_eq!(
            ready_osr_front(FrontChoice::Current, &source, None),
            FrontChoice::Current
        );
    }
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let original = captured_clif(|| {
        compile(&ctx, &source, CompileTier::T1);
    });
    let required = {
        let _on = opt_require_osr_scope_for_test(OptOsrRequirement::Required);
        captured_clif(|| {
            compile(&ctx, &source, CompileTier::T1);
        })
    };
    assert!(!original.is_empty());
    assert_eq!(normalize(original), normalize(required));
    assert_eq!(source.jit_runtime().compiled_id(), None);
}
