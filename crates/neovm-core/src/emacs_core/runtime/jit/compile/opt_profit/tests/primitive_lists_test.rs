//! Explicit bytecode calls stay on the original frontend, even after fusion.
//! Threading: source objects, existing caches and scalar compiler overrides
//! belong to the test invocation; no leaf mutation overlaps native execution.

use super::frontend_tests::{Settings, compile, count_loop};
use super::lists_frontend_tests::{list_loop, normalize};
use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{captured_clif, function};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, Context, NativeRun, OptAdmit, OptEarlyMode, OptProfitMode,
    force_opt_early_for_test, force_opt_for_test, force_opt_max_ops_for_test,
    force_opt_profit_for_test,
};
use crate::emacs_core::jit::tier2::T2Upgrade;
use crate::emacs_core::jit::{NumericFeedback, cache, inline, stats};
use crate::emacs_core::value::Value;

/// Test-owned scalar fuser/observer overrides and existing-cache lifetime.
/// Threading: contains no Lisp state and clears only this mutator's cache.
struct FixtureScopes;
impl FixtureScopes {
    fn enter() -> Self {
        cache::clear();
        inline::force_inline_for_test(Some(true));
        stats::force_observe_for_test(stats::ObserveOverride {
            stats: true,
            naming: false,
            entry_count: false,
        });
        Self
    }
}
impl Drop for FixtureScopes {
    fn drop(&mut self) {
        cache::clear();
        inline::force_inline_for_test(None);
        stats::force_observe_for_test(stats::ObserveOverride {
            stats: false,
            naming: false,
            entry_count: false,
        });
    }
}

#[test]
fn opt_profit_primitive_lists_rejects_all_explicit_call_forms_even_if_dead_or_profitable() {
    let _settings = Settings::enter();
    let _context = Context::new();
    assert_eq!(
        OptProfitMode::parse(Some(" primitive-lists ")),
        OptProfitMode::PrimitiveLists
    );
    assert_eq!(OptProfitMode::parse(None), OptProfitMode::Off);
    let calls = [
        Op::Call(1),
        Op::Apply(1),
        Op::CallBuiltin(0, 1),
        Op::CallBuiltinSym(crate::emacs_core::intern::intern("length"), 1),
    ];
    for call in calls {
        // These are immutable policy inputs, not executable stack fixtures.
        // The original j17 classifier accepts the arithmetic/call balance.
        for ops in [
            vec![Op::Setcar, Op::Add1, call.clone(), Op::Goto(0), Op::Return],
            vec![Op::Setcar, Op::Add1, Op::Goto(0), Op::Return, call],
        ] {
            let heavy = super::super::body_is_call_heavy(&ops, &[]);
            assert!(!heavy);
            for mode in [
                OptProfitMode::Lists,
                OptProfitMode::Loops,
                OptProfitMode::Kernels,
            ] {
                assert!(body_admitted(mode, &ops, heavy, true));
                force_opt_profit_for_test(Some(mode));
                assert!(primitive_osr_source_admitted(&ops));
            }
            assert!(!body_admitted(
                OptProfitMode::PrimitiveLists,
                &ops,
                heavy,
                true
            ));
            force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
            assert!(!primitive_osr_source_admitted(&ops));
            assert!(!osr_admitted(&ops, &[], ops.len()));
            assert!(body_admitted(OptProfitMode::Off, &ops, true, false));
        }
    }
}

#[test]
fn opt_profit_primitive_lists_retains_list_span_size_and_original_numeric_mir() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let list = list_loop();
    list.jit_runtime().set_hot_for_test();
    assert!(body_admitted(
        OptProfitMode::PrimitiveLists,
        list.executable_ops(),
        false,
        true
    ));
    assert_eq!(
        compile(&list, CompileTier::T1).selected_tier(),
        SelectedTier::Opt
    );
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLists,
        list.executable_ops(),
        true,
        true
    ));
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLists,
        &[Op::Setcar, Op::Throw, Op::Goto(0)],
        false,
        true
    ));
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLists,
        &[Op::Car, Op::Add1, Op::Goto(1)],
        false,
        true
    ));
    force_opt_max_ops_for_test(Some(list.executable_ops().len() - 1));
    assert!(!body_admitted(
        OptProfitMode::PrimitiveLists,
        list.executable_ops(),
        false,
        true
    ));
    force_opt_max_ops_for_test(Some(48));
    let numeric = count_loop(0);
    numeric.jit_runtime().set_hot_for_test();
    for tier in [CompileTier::T1, CompileTier::Upgrade(T2Upgrade::Retier)] {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        let mut selected = None;
        let selected_clif = captured_clif(|| selected = Some(compile(&numeric, tier)));
        let selected = selected.unwrap();
        assert_eq!(selected.selected_tier(), SelectedTier::Mir);
        force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
        let legacy_clif = captured_clif(|| {
            compile(&numeric, tier);
        });
        assert!(!selected_clif.is_empty());
        assert_eq!(normalize(selected_clif), normalize(legacy_clif));
        assert_eq!(
            selected.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(4)]),
            NativeRun::Ok(Value::make_int(4).bits())
        );
    }
}

fn calling_list_loop() -> ByteCodeFunction {
    let callee = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![], 1);
    callee.jit_runtime().set_hot_for_test();
    let callee = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee);
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(17),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::StackSet(1),
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
        vec![Value::make_int(0), callee],
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

#[test]
fn opt_profit_primitive_lists_original_call_survives_real_fusion_and_uses_one_legacy_frontend() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _fixture = FixtureScopes::enter();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    force_opt_max_ops_for_test(Some(48));
    let source = calling_list_loop();
    assert!(!super::super::body_is_call_heavy(
        source.executable_ops(),
        &source.constants
    ));
    assert!(body_admitted(
        OptProfitMode::Lists,
        source.executable_ops(),
        false,
        true
    ));
    let feedback = vec![NumericFeedback::FixnumOnly; source.executable_ops().len()];
    let fused = inline::fuse_calls(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        2,
        &feedback,
    )
    .expect("forced legacy fuser must splice the warmed constant callee");
    assert_eq!(fused.regions.len(), 1);
    assert!(!fused.is_v2());
    assert!(fused.ops.len() <= 48);
    assert!(!fused.ops.iter().any(|op| matches!(
        op,
        Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..)
    )));
    assert!(
        osr_admitted(&fused.ops, &fused.constants, source.executable_ops().len()),
        "the final-slice-only policy loses the original call evidence"
    );
    assert!(!primitive_osr_source_admitted(source.executable_ops()));
    stats::reset_compile_stats();
    let mut selected = None;
    let selected_clif = captured_clif(|| selected = Some(compile(&source, CompileTier::T1)));
    let selected = selected.unwrap();
    let counts = stats::compile_stats_snapshot();
    assert_eq!(selected.selected_tier(), SelectedTier::Baseline);
    assert_eq!(selected_clif.len(), 1);
    assert_eq!(
        counts.mir_build_failed + counts.mir_tier_rejected,
        1,
        "reject before SSA and do not retry MIR/static fusion"
    );
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let legacy_clif = captured_clif(|| {
        compile(&source, CompileTier::T1);
    });
    assert_eq!(normalize(selected_clif), normalize(legacy_clif));
    let pair = pair();
    assert_eq!(
        selected.call(
            &mut ctx as *mut Context as *mut u8,
            &[pair, Value::make_int(4)]
        ),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_profit_primitive_lists_calling_osr_keeps_actual_baseline_and_list_mutation() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _fixture = FixtureScopes::enter();
    force_opt_profit_for_test(Some(OptProfitMode::PrimitiveLists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    force_opt_max_ops_for_test(Some(48));
    let source = calling_list_loop();
    let pair = pair();
    let snapshot = [pair, Value::make_int(6), Value::make_int(2)];
    ctx.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut ctx, &source, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(6).bits()))
    );
    let leaf = cache::osr_leaf_ptr_for_test(&source, 1).unwrap();
    assert_eq!(unsafe { &*leaf }.selected_tier(), SelectedTier::Baseline);
    assert_eq!(pair.cons_car(), Value::make_int(5));
    assert_eq!(ctx.jit_root_stack_top, 0);
}
