//! Rust implementation of the closed lookup ABI for Miri, never a native
//! compilation substitute in the feature manifest or native kernel tests.
use super::*;
use crate::tagged::gc::{
    ExecutionTestHomes, ExecutionTestSeed, PlainSymbolBacking, with_test_prepared_cell_execution,
};

/// # Safety
/// Pointers identify initialized ParallelRuntime and CellInput objects retained
/// through return. The publication borrow belongs to the live activation.
unsafe extern "C" fn lookup(
    runtime: *const ParallelRuntime<'_>,
    _activation: *mut c_void,
    input: *const c_void,
    _output: *mut ScalarOutput,
) -> u32 {
    // SAFETY: execute passes these exact initialized stack objects at the
    // declared ABI and retains the publication borrow through this return.
    let (runtime, input) = unsafe { (&*runtime, &*input.cast::<CellInput>()) };
    if runtime.version.0 != NativeAbiVersion::V1.0 {
        return NativeStatus::UnsupportedAbi as u32;
    }
    let view = runtime.publication.load();
    let table = view.table();
    let slot = input.slot.get();
    if slot < table.slots().get()
        && table
            .chunk((slot / table.chunk_slots().get()).into())
            .is_some()
    {
        NativeStatus::CellReadReady as u32
    } else {
        NativeStatus::SlotAbsent as u32
    }
}
macro_rules! refused_entry {
    ($name:ident, $status:expr) => {
        /// # Safety
        /// Called only at the declared four-pointer ABI while the supplying
        /// closed-kernel owner and initialized stack arguments remain live.
        unsafe extern "C" fn $name(
            _runtime: *const ParallelRuntime<'_>,
            _activation: *mut c_void,
            _input: *const c_void,
            _output: *mut ScalarOutput,
        ) -> u32 {
            $status
        }
    };
}
refused_entry!(absent, NativeStatus::SlotAbsent as u32);
refused_entry!(wrong_kind, NativeStatus::Returned as u32);
refused_entry!(unsupported, NativeStatus::UnsupportedAbi as u32);
refused_entry!(malformed, u32::MAX);

#[test]
fn nmjit_n4_cell_abi_roots_after_lookup_returns() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_test_prepared_cell_execution(
            backing,
            ExecutionTestSeed::FunctionDefinition,
            |_| (),
            |(), admission, run, home, symbol| {
                for operation in [CellRead::GlobalValue, CellRead::Function] {
                    // SAFETY: lookup implements the closed read-only ABI and
                    // this Rust function remains valid for the complete call.
                    unsafe { execute(lookup, operation, admission, run, home, symbol) }.unwrap();
                    assert!(ExecutionTestHomes::is_cons(home, run));
                }
            },
        );
    }
}

#[test]
fn nmjit_n4_cell_abi_refusal_leaves_previous_home_unchanged() {
    let entries: [(ParallelEntry, ParallelEntryError); 4] = [
        (absent, ParallelEntryError::NoSlotBacking),
        (wrong_kind, ParallelEntryError::UnexpectedEntryKind),
        (unsupported, ParallelEntryError::UnsupportedAbi),
        (malformed, ParallelEntryError::MalformedStatus),
    ];
    with_test_prepared_cell_execution(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::Scalar,
        |_| (),
        |(), admission, run, home, symbol| {
            for (entry, expected) in entries {
                admission
                    .with_activation(run, home, |a| a.store_immediate(ImmediateResult::True))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    // SAFETY: each listed Rust entry returns status only,
                    // retains nothing and remains live for the complete call.
                    unsafe { execute(entry, CellRead::GlobalValue, admission, run, home, symbol) },
                    Err(expected)
                );
                assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::True);
            }
        },
    );
}

#[test]
fn nmjit_n4_fboundp_abi_matches_gnu_after_function_writes() {
    use super::super::fboundp_tests::{assert_empty, assert_transitions};
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_test_prepared_cell_execution(
            backing,
            ExecutionTestSeed::Scalar,
            |_| (),
            |(), admission, run, home, symbol| {
                assert_empty(run, home, |run| {
                    // SAFETY: lookup is the same closed Rust descriptor kernel
                    // used by the other ABI fixtures; it retains no address.
                    unsafe {
                        execute(
                            lookup,
                            CellRead::FunctionBoundp,
                            admission,
                            run,
                            home,
                            symbol,
                        )
                    }
                });
            },
        );
        with_test_prepared_cell_execution(
            backing,
            ExecutionTestSeed::WritableFunctionDefinition,
            |_| (),
            |(), admission, run, home, symbol| {
                assert_transitions(run, home, symbol, |run| {
                    // SAFETY: the static Rust lookup implements this closed
                    // ABI and is alive throughout the call and sealed epilogue.
                    unsafe {
                        execute(
                            lookup,
                            CellRead::FunctionBoundp,
                            admission,
                            run,
                            home,
                            symbol,
                        )
                    }
                });
            },
        );
    }
}

#[test]
fn nmjit_n4_named_abi_roots_old_selection_after_lookup_returns() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        with_test_prepared_cell_execution(
            backing,
            ExecutionTestSeed::WritableFunctionTwin,
            |_| (),
            |(), admission, run, home, symbol| {
                super::super::identity_tests::assert_history(
                    run,
                    home,
                    symbol,
                    |run, expected, selected| {
                        // SAFETY: lookup is the retained closed Rust descriptor kernel;
                        // all native-like temporaries die before the rooted selection.
                        unsafe {
                            super::super::identity::execute(
                                lookup, admission, run, home, symbol, symbol, expected, selected,
                            )
                        }
                    },
                );
            },
        );
    }
}

#[test]
fn nmjit_n4_named_abi_refusals_preserve_activation_and_home() {
    with_test_prepared_cell_execution(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::UnsupportedFunction,
        |_| (),
        |(), admission, run, home, symbol| {
            let expected = ExecutionTestHomes::nil(run).unwrap();
            let mut selected = FunctionActivation::new(run).unwrap();
            // SAFETY: static lookup implements the retained closed descriptor ABI.
            let result = unsafe {
                super::super::identity::execute(
                    lookup,
                    admission,
                    run,
                    home,
                    symbol,
                    symbol,
                    &expected,
                    &mut selected,
                )
            };
            assert_eq!(
                result,
                Err(ParallelEntryError::Function(
                    FunctionError::UnsupportedDefinition
                ))
            );
            assert_eq!(selected.selected_epoch(), None);
            assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
            // SAFETY: absent is a static status-only entry with the same ABI.
            let result = unsafe {
                super::super::identity::execute(
                    absent,
                    admission,
                    run,
                    home,
                    symbol,
                    symbol,
                    &expected,
                    &mut selected,
                )
            };
            assert_eq!(result, Err(ParallelEntryError::NoSlotBacking));
            assert_eq!(selected.selected_epoch(), None);
            assert_eq!(ExecutionTestHomes::read(home, run), ImmediateResult::Nil);
            selected.clear(run).unwrap();
        },
    );
    with_test_prepared_cell_execution(
        PlainSymbolBacking::Unprojected,
        ExecutionTestSeed::WritableFunctionTwin,
        |_| (),
        |(), admission, run, _home, symbol| {
            let stale = ExecutionTestHomes::nil(run).unwrap();
            run.clear_root_slots().unwrap();
            let fresh = ExecutionTestHomes::nil(run).unwrap();
            let mut selected = FunctionActivation::new(run).unwrap();
            // SAFETY: this static closed lookup retains no borrowed pointer.
            let result = unsafe {
                super::super::identity::execute(
                    lookup,
                    admission,
                    run,
                    &fresh,
                    symbol,
                    symbol,
                    &stale,
                    &mut selected,
                )
            };
            assert_eq!(
                result,
                Err(ParallelEntryError::Function(FunctionError::Authority(
                    RegistrationError::StaleRegistration
                )))
            );
            assert_eq!(selected.selected_epoch(), None);
            assert_eq!(ExecutionTestHomes::read(&fresh, run), ImmediateResult::Nil);
            selected.clear(run).unwrap();
        },
    );
}
