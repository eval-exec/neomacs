//! Teardown controls ported out of the orphaned N1 file.
//! The body takes a real reader lease, then the shared symbol-intern fixture
//! retires the owner through the current retry helper and finishes the era.
use super::fixture::{Query, fixture};
use super::*;
use crate::tagged::gc::PlainSymbolBacking;

#[test]
fn nmjit_n1_noop_fixture_finishes_cleanly() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Existing,
        |run, homes| {
            let _reader = run
                .symbol_ref(&homes.old_handle)
                .expect("N1 no-op reader lease");
        },
    )
    .expect("N1 no-op fixture completion");
}

#[derive(Debug, PartialEq, Eq)]
enum ControlledReturn {
    EntryNotRequested,
}

#[test]
fn nmjit_n1_early_return_fixture_finishes_cleanly() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Existing,
        |run, homes| {
            let mut early_return = || -> Result<(), ControlledReturn> {
                let _reader = run
                    .symbol_ref(&homes.old_handle)
                    .expect("N1 early-return reader lease");
                Err(ControlledReturn::EntryNotRequested)
            };
            assert_eq!(early_return(), Err(ControlledReturn::EntryNotRequested));
        },
    )
    .expect("N1 early-return fixture completion");
}
