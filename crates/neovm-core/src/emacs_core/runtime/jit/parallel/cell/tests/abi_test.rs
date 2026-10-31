//! Rust implementation of the closed lookup ABI for Miri, never a native
//! compilation substitute in the feature manifest or native kernel tests.
use super::*;
use crate::tagged::gc::{
    ExecutionTestHomes, ExecutionTestSeed, PlainSymbolBacking, with_test_prepared_cell_execution,
};

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
