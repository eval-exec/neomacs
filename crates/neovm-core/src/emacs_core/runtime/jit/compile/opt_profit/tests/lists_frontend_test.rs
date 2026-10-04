//! List-only selection preserves numeric MIR and the original entry frontend.
//! Threading: each test owns its contexts, sources and scalar compiler settings.

use super::frontend_tests::{Settings, compile, count_loop};
use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, Context, NativeRun, OptAdmit, OptEarlyMode, OptProfitMode, RegallocPolicy,
    Value, force_opt_early_for_test, force_opt_for_test, force_opt_profit_for_test,
};
use crate::emacs_core::jit::stats::CompileOrigin;
use crate::emacs_core::jit::tier2::T2Upgrade;

pub(super) fn normalize(records: Vec<String>) -> Vec<String> {
    records
        .into_iter()
        .map(|text| mask_code_text(&text.replace('_', "")))
        .collect()
}

pub(super) fn list_loop() -> ByteCodeFunction {
    function(
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
    )
}

#[test]
fn opt_profit_lists_numeric_t1_and_retier_keep_legacy_mir_and_clif() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::On));
    let f = count_loop(0);
    for tier in [CompileTier::T1, CompileTier::Upgrade(T2Upgrade::Retier)] {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        let mut filtered = None;
        let selected = captured_clif(|| filtered = Some(compile(&f, tier)));
        assert_eq!(filtered.unwrap().selected_tier(), SelectedTier::Mir);
        force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
        let legacy = captured_clif(|| {
            assert_eq!(compile(&f, tier).selected_tier(), SelectedTier::Mir);
        });
        assert!(!selected.is_empty());
        assert_eq!(normalize(selected), normalize(legacy));
    }
}

#[test]
fn opt_profit_lists_early_uses_list_policy_and_existing_source_heat() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    let numeric = count_loop(0);
    let list = list_loop();
    let request = CompileRequest {
        regalloc: RegallocPolicy::Full,
        bypass_profit_gate: false,
        origin: CompileOrigin::Dispatch,
        tier: CompileTier::T1,
    };
    assert_eq!(
        front(request, list.executable_ops(), false, list.jit_runtime()),
        FrontChoice::Legacy
    );
    list.jit_runtime().set_hot_for_test();
    numeric.jit_runtime().set_hot_for_test();
    assert_eq!(
        front(request, list.executable_ops(), false, list.jit_runtime()),
        FrontChoice::SelectedAfterMir
    );
    assert_eq!(
        front(
            request,
            numeric.executable_ops(),
            false,
            numeric.jit_runtime()
        ),
        FrontChoice::Legacy
    );
    assert_eq!(
        compile(&numeric, CompileTier::T1).selected_tier(),
        SelectedTier::Mir
    );
    assert_eq!(
        compile(&list, CompileTier::T1).selected_tier(),
        SelectedTier::Opt
    );
}

#[test]
fn opt_profit_lists_selected_list_upgrade_preserves_result_and_mutation() {
    let _settings = Settings::enter();
    let mut context = Context::new();
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    let f = list_loop();
    let selected = compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    assert_eq!(selected.selected_tier(), SelectedTier::Opt);
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(pair);
    assert_eq!(
        selected.call(
            &mut context as *mut Context as *mut u8,
            &[pair, Value::make_int(4)]
        ),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let legacy = compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    assert_eq!(legacy.selected_tier(), SelectedTier::Baseline);
    assert_eq!(
        legacy.call(
            &mut context as *mut Context as *mut u8,
            &[pair, Value::make_int(6)]
        ),
        NativeRun::Ok(Value::make_int(6).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(5));
}

#[test]
fn opt_profit_lists_legacy_selector_ignores_the_filter() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let f = list_loop();
    force_opt_profit_for_test(Some(OptProfitMode::Off));
    let off = captured_clif(|| {
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    });
    force_opt_profit_for_test(Some(OptProfitMode::Lists));
    force_opt_early_for_test(Some(OptEarlyMode::On));
    let lists = captured_clif(|| {
        compile(&f, CompileTier::Upgrade(T2Upgrade::Retier));
    });
    assert!(!off.is_empty());
    assert_eq!(normalize(off), normalize(lists));
}
