use super::*;
use crate::tagged::gc::{
    ExecutionTestHomes, ExecutionTestSeed, FunctionStep, PlainSymbolBacking,
    with_test_prepared_cell_execution,
};
use std::num::NonZeroU32;

fn oracle() -> [&'static str; 8] {
    include_str!("identity_oracle.expected")
        .lines()
        .collect::<Vec<_>>()
        .try_into()
        .expect("eight GNU rows")
}
fn selection(row: &str) -> FunctionSelection {
    match row {
        "t" => FunctionSelection::Selected,
        "nil" => FunctionSelection::Changed,
        other => panic!("GNU identity row {other}"),
    }
}
fn finish_same_era<'era>(
    run: &mut RunAccess<'_, 'era>,
    selected: &mut FunctionActivation<'era>,
    home: &OwnerRootSlot<'era>,
) {
    assert_eq!(
        run.sym_call_step(selected, &mut home.clone(), NonZeroU32::new(2).unwrap()),
        Ok(FunctionStep::Returned)
    );
    let answer = ExecutionTestHomes::read(home, run);
    assert_eq!(
        match answer {
            ImmediateResult::Fixnum(value) => i64::from(value).to_string(),
            ImmediateResult::Nil => "nil".to_owned(),
            ImmediateResult::True => "t".to_owned(),
        },
        oracle()[1]
    );
}

pub(super) fn assert_history<'era>(
    run: &mut RunAccess<'_, 'era>,
    home: &OwnerRootSlot<'era>,
    symbol: &SymbolHandle,
    mut query: impl FnMut(
        &mut RunAccess<'_, 'era>,
        &OwnerRootSlot<'era>,
        &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, ParallelEntryError>,
) {
    let mut expected = ExecutionTestHomes::nil(run).unwrap();
    run.sym_function_into(symbol, &mut expected).unwrap();
    let mut replacement = ExecutionTestHomes::nil(run).unwrap();
    run.sym_value_into(symbol, &mut replacement).unwrap();
    let mut selected = FunctionActivation::new(run).unwrap();
    assert_eq!(
        query(run, &expected, &mut selected),
        Ok(selection(oracle()[0]))
    );
    let original_clock = selected.selected_epoch();
    assert!(original_clock.is_some());
    assert_eq!(
        query(run, &expected, &mut selected),
        Err(ParallelEntryError::Function(
            FunctionError::ActiveActivation
        ))
    );
    assert_eq!(selected.selected_epoch(), original_clock);
    run.sym_fset_into(symbol, &replacement, &mut home.clone())
        .unwrap();
    finish_same_era(run, &mut selected, home);
    assert_eq!(
        query(run, &expected, &mut selected),
        Ok(selection(oracle()[2]))
    );
    assert_eq!(selected.selected_epoch(), original_clock);
    assert_eq!(
        ExecutionTestHomes::read(home, run),
        ImmediateResult::from_word(
            crate::tagged::value::TaggedValue::fixnum(oracle()[1].parse().unwrap()).bits()
        )
        .unwrap()
    );
    assert_eq!(
        query(run, &replacement, &mut selected),
        Ok(selection(oracle()[3]))
    );
    assert_ne!(selected.selected_epoch(), original_clock);
    finish_same_era(run, &mut selected, home);
    run.sym_fset_into(symbol, &expected, &mut home.clone())
        .unwrap();
    assert_eq!(
        query(run, &expected, &mut selected),
        Ok(selection(oracle()[4]))
    );
    let rebound = selected.selected_epoch();
    finish_same_era(run, &mut selected, home);
    run.sym_fset_into(symbol, &expected, &mut home.clone())
        .unwrap();
    assert_eq!(
        query(run, &expected, &mut selected),
        Ok(selection(oracle()[5]))
    );
    assert_eq!(selected.selected_epoch(), rebound);
    finish_same_era(run, &mut selected, home);
    run.sym_fmakunbound_into(symbol, &mut home.clone()).unwrap();
    assert_eq!(oracle()[6], "void-function");
    assert_eq!(
        query(run, &expected, &mut selected),
        Err(ParallelEntryError::Function(FunctionError::VoidFunction))
    );
    selected.clear(run).unwrap();
}

fn with_named(
    backing: PlainSymbolBacking,
    seed: ExecutionTestSeed,
    body: impl for<'era, 'owner> FnOnce(
        &ParallelCompiledNamedCode,
        &ParallelExecutionAdmission<'era>,
        &mut RunAccess<'owner, 'era>,
        &OwnerRootSlot<'era>,
        &SymbolHandle,
    ),
) -> Result<(), String> {
    let outcome = std::cell::RefCell::new(Ok(()));
    with_test_prepared_cell_execution(
        backing,
        seed,
        ParallelCompiledNamedCode::compile,
        |compiled, admission, run, home, symbol| match compiled {
            Ok(code) => {
                assert!(cfg!(feature = "jit"));
                body(code, admission, run, home, symbol);
            }
            Err(ParallelCompileError::BackendUnavailable) => {
                assert!(!cfg!(feature = "jit"));
                eprintln!("NMJIT_N4_NAMED_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            }
            Err(
                error @ (ParallelCompileError::Module(_)
                | ParallelCompileError::Define(_)
                | ParallelCompileError::Finalize(_)
                | ParallelCompileError::UnsupportedPointerWidth
                | ParallelCompileError::AbiLayout),
            ) => {
                *outcome.borrow_mut() = Err(format!("named compilation: {error:?}"));
            }
        },
    );
    outcome.into_inner()
}
pub(crate) fn named_manifest() -> Result<(), String> {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_named(
            backing,
            ExecutionTestSeed::WritableFunctionTwin,
            |code, admission, run, home, symbol| {
                assert_history(run, home, symbol, |run, expected, selected| {
                    code.select(admission, run, home, symbol, symbol, expected, selected)
                });
                eprintln!("NMJIT_N4_NAMED_NATIVE_GNU_OK {backing:?}");
            },
        )?;
    }
    Ok(())
}
#[test]
fn nmjit_n4_native_named_guard_roots_old_call_and_reselects() {
    named_manifest().unwrap();
}
#[test]
fn nmjit_n4_native_named_guard_refuses_unsupported_definition() {
    assert_eq!(oracle()[7], "invalid-function");
    with_named(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::UnsupportedFunction,
        |code, admission, run, home, symbol| {
            let expected = ExecutionTestHomes::nil(run).unwrap();
            let mut selected = FunctionActivation::new(run).unwrap();
            assert_eq!(
                code.select(
                    admission,
                    run,
                    home,
                    symbol,
                    symbol,
                    &expected,
                    &mut selected
                ),
                Err(ParallelEntryError::Function(
                    FunctionError::UnsupportedDefinition
                ))
            );
            assert_eq!(selected.selected_epoch(), None);
            assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
            selected.clear(run).unwrap();
        },
    )
    .unwrap();
}
#[test]
fn nmjit_n4_native_named_guard_stale_operand_preserves_home() {
    with_named(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::WritableFunctionTwin,
        |code, admission, run, home, symbol| {
            let stale = ExecutionTestHomes::nil(run).unwrap();
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            let mut selected = FunctionActivation::new(run).unwrap();
            assert_eq!(
                code.select(
                    admission,
                    run,
                    &fresh,
                    symbol,
                    symbol,
                    &stale,
                    &mut selected
                ),
                Err(ParallelEntryError::Function(FunctionError::Authority(
                    RegistrationError::StaleRegistration
                )))
            );
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
            assert_eq!(selected.selected_epoch(), None);
            selected.clear(run).unwrap();
            let _ = home;
        },
    )
    .unwrap();
}
#[test]
fn nmjit_n4_named_guard_scope_source_guard_test() {
    let source = include_str!("../identity.rs");
    let cell = include_str!("../cell.rs");
    assert!(source.find("::lookup(").unwrap() < source.find(".select_named_function(").unwrap());
    assert!(source.contains("target != designator"));
    assert!(cell.contains("atomic"));
    for forbidden in [
        "Context",
        "thread_local!",
        "unwrap(",
        "expect(",
        "spine_addr",
    ] {
        assert!(!source.contains(forbidden));
    }
}
