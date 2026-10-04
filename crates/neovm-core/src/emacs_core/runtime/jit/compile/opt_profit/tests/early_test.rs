//! Early candidate selection preserves useful MIR before attempting SSA.
//! Threading: all test settings and source heat belong to the owning test.
use super::super::*;
use super::frontend_tests::{Settings, count_loop};
use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::stats::CompileOrigin;

fn compile_origin(f: &ByteCodeFunction, origin: CompileOrigin) -> CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin,
            tier: CompileTier::T1,
        },
    )
    .unwrap()
}

fn heap_loop() -> ByteCodeFunction {
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
fn opt_early_knob_defaults_off_and_modes_are_explicit() {
    assert_eq!(OptEarlyMode::parse(None), OptEarlyMode::Off);
    assert_eq!(OptEarlyMode::parse(Some("invalid")), OptEarlyMode::Off);
    assert_eq!(OptEarlyMode::parse(Some("hot")), OptEarlyMode::Hot);
    assert_eq!(OptEarlyMode::parse(Some("on")), OptEarlyMode::On);
}

#[test]
fn opt_early_off_t1_is_clif_identical_to_legacy_mir() {
    let _settings = Settings::enter();
    let f = count_loop(0);
    force_opt_early_for_test(Some(OptEarlyMode::Off));
    let off = captured_clif(|| {
        compile_origin(&f, CompileOrigin::FirstSight);
    });
    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let legacy = captured_clif(|| {
        compile_origin(&f, CompileOrigin::FirstSight);
    });
    let normalize = |records: Vec<String>| {
        records
            .into_iter()
            .map(|text| mask_code_text(&text.replace('_', "")))
            .collect::<Vec<_>>()
    };
    assert!(!off.is_empty());
    assert_eq!(normalize(off), normalize(legacy));
}

#[test]
fn opt_early_on_preserves_successful_numeric_mir() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_early_for_test(Some(OptEarlyMode::On));
    force_opt_max_ops_for_test(Some(64));
    let f = count_loop(0);
    assert_eq!(
        front(
            CompileRequest {
                regalloc: RegallocPolicy::Full,
                bypass_profit_gate: false,
                origin: CompileOrigin::FirstSight,
                tier: CompileTier::T1,
            },
            f.executable_ops(),
            false,
            f.jit_runtime()
        ),
        FrontChoice::SelectedAfterMir
    );
    assert_eq!(
        compile_origin(&f, CompileOrigin::FirstSight).selected_tier(),
        SelectedTier::Mir
    );
}

#[test]
fn opt_early_on_selects_cold_first_sight_heap_loop_but_requires_heat_at_dispatch() {
    let _settings = Settings::enter();
    let mut context = Context::new();
    force_opt_early_for_test(Some(OptEarlyMode::On));
    force_opt_max_ops_for_test(Some(64));
    let f = heap_loop();
    assert_eq!(f.jit_runtime().heat(), 0);
    assert_eq!(
        compile_origin(&f, CompileOrigin::Dispatch).selected_tier(),
        SelectedTier::Baseline
    );
    let selected = compile_origin(&f, CompileOrigin::FirstSight);
    assert_eq!(selected.selected_tier(), SelectedTier::Opt);
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    assert_eq!(
        selected.call(
            &mut context as *mut Context as *mut u8,
            &[pair, Value::make_int(4)]
        ),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    assert_eq!(pair.cons_car(), Value::make_int(3));
    f.jit_runtime().set_hot_for_test();
    assert_eq!(
        compile_origin(&f, CompileOrigin::Dispatch).selected_tier(),
        SelectedTier::Opt
    );
}

#[test]
fn opt_early_hot_declines_cold_first_sight_and_max_ops_preserves_baseline() {
    let _settings = Settings::enter();
    let _context = Context::new();
    let f = heap_loop();
    force_opt_early_for_test(Some(OptEarlyMode::Hot));
    assert_eq!(
        compile_origin(&f, CompileOrigin::FirstSight).selected_tier(),
        SelectedTier::Baseline
    );
    force_opt_early_for_test(Some(OptEarlyMode::On));
    force_opt_max_ops_for_test(Some(f.executable_ops().len() - 1));
    assert_eq!(
        compile_origin(&f, CompileOrigin::FirstSight).selected_tier(),
        SelectedTier::Baseline
    );
}

#[test]
fn opt_early_refused_dynamic_variable_loop_keeps_legacy_fallback() {
    let _settings = Settings::enter();
    let _context = Context::new();
    force_opt_early_for_test(Some(OptEarlyMode::On));
    force_opt_max_ops_for_test(Some(64));
    force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::default()));
    let f = function(
        vec![Op::VarRef(0), Op::Add1, Op::VarSet(0), Op::Goto(0)],
        vec![Value::symbol("early-refused-variable")],
        0,
    );
    assert_eq!(
        compile_origin(&f, CompileOrigin::FirstSight).selected_tier(),
        SelectedTier::Baseline
    );
}
