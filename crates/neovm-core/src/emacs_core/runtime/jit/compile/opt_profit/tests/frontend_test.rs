//! Exercise selected and fallback frontends with the surrounding stack off.
//! Threading: settings and leaf ownership are local to each nextest test.

use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    OptAdmit, OptProfitMode, RegallocPolicy, Tier2Knob, compile_bytecode_function_requested,
    force_deopt_for_test, force_inline2_for_test, force_opt_for_test, force_opt_profit_for_test,
    force_tier2_for_test,
};
use crate::emacs_core::jit::tier2::T2Upgrade;

/// Threading: scalar compiler overrides belong to this test invocation and
/// reset on drop; this scope contains no Lisp state or mutator cache.
pub(super) struct Settings;
impl Settings {
    pub(super) fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_profit_for_test(Some(OptProfitMode::Loops));
        super::super::force_opt_early_for_test(Some(super::super::OptEarlyMode::Off));
        super::super::force_opt_max_ops_for_test(Some(0));
        force_inline2_for_test(Some(super::super::Inline2Mode::Off));
        force_tier2_for_test(Some(Tier2Knob::from_env(|_| None)));
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(Some(
            crate::emacs_core::jit::bg::BgMode::Sync,
        ));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_profit_for_test(None);
        super::super::force_opt_early_for_test(None);
        super::super::force_opt_max_ops_for_test(None);
        force_inline2_for_test(None);
        force_tier2_for_test(None);
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(None);
    }
}

pub(super) fn count_loop(pairs: usize) -> super::super::ByteCodeFunction {
    let mut ops = Vec::new();
    for _ in 0..pairs {
        ops.extend([Op::Nil, Op::Pop]);
    }
    let base = u32::try_from(ops.len()).unwrap();
    ops.extend([
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(base + 9),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(base + 1),
        Op::Return,
    ]);
    function(ops, vec![super::super::Value::make_int(0)], 1)
}

pub(super) fn compile(
    f: &super::super::ByteCodeFunction,
    tier: CompileTier,
) -> super::super::CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier,
        },
    )
    .unwrap()
}

#[test]
fn opt_profit_t1_keeps_legacy_mir_and_retier_selects_opt() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    assert_eq!(
        compile(&f, CompileTier::T1).selected_tier(),
        SelectedTier::Mir
    );
    assert_eq!(
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier)).selected_tier(),
        SelectedTier::Opt,
    );
}

#[test]
fn opt_profit_selected_retier_matches_the_interpreter() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    let leaf = compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    let mut context = super::super::Context::new();
    for n in [0, 1, 20, 1000] {
        let args = [super::super::Value::make_int(n)];
        let expected = super::super::Vm::from_context(&mut context)
            .execute(&f, args.to_vec())
            .unwrap();
        assert_eq!(
            leaf.call(&mut context as *mut super::super::Context as *mut u8, &args),
            super::super::NativeRun::Ok(expected.bits()),
        );
    }
}

#[test]
fn opt_profit_loops_do_not_exclude_float_feedback() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    f.jit_runtime().widen_numeric(
        6,
        f.executable_ops().len(),
        crate::emacs_core::jit::NumericFeedback::Float,
    );
    assert_eq!(
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier)).selected_tier(),
        SelectedTier::Opt,
    );
}

#[test]
fn opt_profit_rejected_helper_is_clif_identical_to_legacy() {
    let _settings = Settings::enter();
    let f = function(vec![Op::StackRef(0), Op::Add1, Op::Return], Vec::new(), 1);
    let tier = CompileTier::Upgrade(T2Upgrade::Retier);
    let selected = captured_clif(|| {
        compile(&f, tier);
    });
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let legacy = captured_clif(|| {
        compile(&f, tier);
    });
    let mask = |records: Vec<String>| {
        records
            .into_iter()
            .map(|text| mask_code_text(&text.replace('_', "")))
            .collect::<Vec<_>>()
    };
    assert!(!selected.is_empty());
    assert_eq!(mask(selected), mask(legacy));
}

#[test]
fn opt_profit_refused_ssa_returns_to_legacy_mir() {
    let _settings = Settings::enter();
    // This is a qualifying loop, but the existing 1000-op opt construction
    // budget refuses it. Legacy MIR still supports this source shape.
    let oversized = count_loop(501);
    assert_eq!(
        compile(&oversized, CompileTier::Upgrade(T2Upgrade::Retier)).selected_tier(),
        SelectedTier::Mir,
    );
}

#[test]
fn opt_profit_legacy_ignores_the_profitability_knob_in_generated_clif() {
    let _settings = Settings::enter();
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let f = count_loop(0);
    force_opt_profit_for_test(Some(OptProfitMode::Off));
    let off = captured_clif(|| {
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    });
    force_opt_profit_for_test(Some(OptProfitMode::Kernels));
    let kernels = captured_clif(|| {
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    });
    let normalize = |records: Vec<String>| {
        records
            .into_iter()
            .map(|text| mask_code_text(&text.replace('_', "")))
            .collect::<Vec<_>>()
    };
    assert!(!off.is_empty());
    assert_eq!(normalize(off), normalize(kernels));
}

#[test]
fn opt_profit_existing_reopt_ban_still_prevents_opt_emission() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    f.jit_runtime()
        .set_reopt_level_for_test(crate::emacs_core::jit::ReoptLevel::BaselineOnly);
    assert_eq!(
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier)).selected_tier(),
        SelectedTier::Baseline,
    );
}

#[test]
fn opt_profit_osr_cache_matches_interpreter_and_legacy_entry_guards() {
    use super::super::{Context, NativeRun, Value, Vm};
    use crate::emacs_core::jit::cache;

    let _settings = Settings::enter();
    let reference = count_loop(0);
    reference.jit_runtime().set_cold_for_test();
    let mut context = Context::new();
    let expected = Vm::from_context(&mut context)
        .execute(&reference, vec![Value::make_int(2000)])
        .unwrap();
    for (mode, tier) in [
        (OptMode::Opt, SelectedTier::Opt),
        (OptMode::Legacy, SelectedTier::Baseline),
    ] {
        force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
        let f = count_loop(0);
        let snapshot = [Value::make_int(2000), Value::make_int(42)];
        context.bc_buf.clear();
        context.bc_buf.extend_from_slice(&snapshot);
        assert_eq!(
            cache::try_run_osr(&mut context, &f, 1, &snapshot, &[]),
            Some(NativeRun::Ok(expected.bits())),
        );
        let leaf = cache::osr_leaf_ptr_for_test(&f, 1).expect("installed OSR leaf");
        // No cache mutation occurs while reading this mutator-owned leaf.
        assert_eq!(unsafe { &*leaf }.selected_tier(), tier);
        let bad = [Value::make_int(2000), Value::NIL];
        context.bc_buf.clear();
        context.bc_buf.extend_from_slice(&bad);
        let Some(NativeRun::DeoptAt(frame)) = cache::try_run_osr(&mut context, &f, 1, &bad, &[])
        else {
            panic!("OSR must preserve the invalid entry snapshot");
        };
        assert_eq!(frame.pc, 1);
        assert_eq!(frame.stack.as_slice(), &bad);
        assert_eq!(context.jit_root_stack_top, 0);
        cache::clear();
    }
}
