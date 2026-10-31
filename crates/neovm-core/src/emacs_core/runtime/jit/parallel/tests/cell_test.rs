use super::*;
use crate::tagged::gc::{
    ExecutionTestHomes, ExecutionTestSeed, PlainSymbolBacking, with_test_prepared_cell_execution,
};

pub(super) fn with_cell(
    read: CellRead,
    backing: PlainSymbolBacking,
    seed: ExecutionTestSeed,
    body: impl for<'era, 'owner> FnOnce(
        &ParallelCompiledCellCode,
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
        |quiescent| ParallelCompiledCellCode::compile(quiescent, read),
        |compiled, admission, run, home, symbol| match compiled {
            Ok(code) => {
                assert!(cfg!(feature = "jit"));
                body(code, admission, run, home, symbol);
            }
            Err(ParallelCompileError::BackendUnavailable) => {
                assert!(!cfg!(feature = "jit"));
                eprintln!("NMJIT_N4_CELL_BACKEND_UNAVAILABLE_REFUSED_CLEAN");
            }
            Err(
                error @ (ParallelCompileError::Module(_)
                | ParallelCompileError::Define(_)
                | ParallelCompileError::Finalize(_)
                | ParallelCompileError::UnsupportedPointerWidth
                | ParallelCompileError::AbiLayout),
            ) => {
                *outcome.borrow_mut() = Err(format!("cell compilation: {error:?}"));
            }
        },
    );
    outcome.into_inner()
}

pub(crate) fn varref_manifest() -> Result<(), String> {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_cell(
            CellRead::GlobalValue,
            backing,
            ExecutionTestSeed::Scalar,
            |code, admission, run, home, symbol| {
                code.run(admission, run, home, symbol)
                    .expect("actual parallel VarRef kernel");
                assert_eq!(
                    ExecutionTestHomes::read(home, run).word(),
                    crate::tagged::value::TaggedValue::fixnum(73).bits()
                );
                eprintln!("NMJIT_N4_VARREF_NATIVE_ROOT_OK {backing:?}");
            },
        )?;
    }
    Ok(())
}

#[test]
fn nmjit_n4_native_cell_varref_scalar_plain_and_projected() {
    varref_manifest().unwrap();
}

#[test]
fn nmjit_n4_native_cell_managed_values_and_function_definitions() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for read in [CellRead::GlobalValue, CellRead::Function] {
            with_cell(
                read,
                backing,
                ExecutionTestSeed::FunctionDefinition,
                |code, admission, run, home, symbol| {
                    code.run(admission, run, home, symbol).unwrap();
                    assert!(ExecutionTestHomes::is_cons(home, run));
                    #[cfg(feature = "jit")]
                    {
                        assert_eq!(code.native.emission.matches("atomic_load").count(), 1);
                        assert!(!code.native.emission.contains("call "));
                        assert!(!code.native.emission.contains("store "));
                    }
                },
            )
            .unwrap();
        }
    }
}

#[test]
fn nmjit_n4_native_cell_void_preserves_previous_root() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_cell(
            CellRead::GlobalValue,
            backing,
            ExecutionTestSeed::Scalar,
            |code, admission, run, home, symbol| {
                run.sym_makunbound_into(symbol, &mut home.clone()).unwrap();
                admission
                    .with_activation(run, home, |a| a.store_immediate(ImmediateResult::True))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    code.run(admission, run, home, symbol),
                    Err(ParallelEntryError::CellRead(CellReadError::Global(
                        crate::tagged::gc::GlobalError::VoidVariable
                    )))
                );
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
            },
        )
        .unwrap();
    }
}

#[test]
fn nmjit_n4_native_cell_boundp_reads_after_makunbound() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_cell(
            CellRead::GlobalBoundp,
            backing,
            ExecutionTestSeed::Scalar,
            |code, admission, run, home, symbol| {
                code.run(admission, run, home, symbol).unwrap();
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
                run.sym_makunbound_into(symbol, &mut home.clone()).unwrap();
                code.run(admission, run, home, symbol).unwrap();
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
            },
        )
        .unwrap();
    }
}

#[test]
fn nmjit_n4_native_cell_stale_home_refuses_before_lookup() {
    with_cell(
        CellRead::GlobalValue,
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::Scalar,
        |code, admission, run, stale, symbol| {
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            assert_eq!(
                code.run(admission, run, stale, symbol),
                Err(ParallelEntryError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
            code.run(admission, run, &fresh, symbol).unwrap();
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n4_native_cell_scope_source_guard_test() {
    let source = include_str!("../cell.rs");
    let native = include_str!("../native.rs");
    assert!(source.find("entry(").unwrap() < source.find(".read_cell(").unwrap());
    assert!(source.contains(".symbol_slot(symbol)"));
    assert!(native.contains("EntryPlan::Cell"));
    assert!(native.contains("NativeStatus::CellReadReady"));
    for forbidden in [
        "Context",
        "thread_local!",
        "unwrap(",
        "expect(",
        "panic!(",
        "spine_addr",
    ] {
        assert!(!source.contains(forbidden), "cell ABI contains {forbidden}");
    }
}
