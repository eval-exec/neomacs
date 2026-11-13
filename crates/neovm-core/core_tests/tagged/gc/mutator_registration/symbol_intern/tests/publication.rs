//! Actual backing publication: old descriptors are retained across intern.
use super::fixture::*;
use super::*;
use crate::symbol::parallel::test_facts::SymbolTableCensus;
use crate::tagged::gc::{InitialInternError, PlainSymbolBacking, RegistrationError};

fn census(run: &RunAccess<'_, '_>) -> SymbolTableCensus {
    run.symbol_table_census_for_test().unwrap()
}

fn growth(kind: PlainSymbolBacking) -> std::thread::Result<()> {
    fixture(kind, Query::TableGrowth, |run, homes| {
        let old = census(run);
        let old_name =
            run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id);
        intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        let now = census(run);
        assert!(now.slots() > old.slots());
        assert_eq!(now.generation(), old.generation_successor().unwrap());
        assert_eq!(now.retired(), old.retired() + 1);
        assert_ne!(old.address(), now.address());
        assert_eq!(
            old_name,
            run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id)
        );
        assert_eq!(old.chunk0(), now.chunk0());
        let stable = census(run);
        intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
        soft_retry(run, &homes.name, &mut homes.dst).unwrap();
        assert_eq!(census(run), stable);
        eprintln!(
            "NMJIT_N3_REAL_RETIRED_PUBLICATION {kind:?} retained={} retired={}",
            now.retained_bytes(),
            now.retired()
        );
    })
}

#[test]
fn nmjit_n3_actual_growth_retains_old_descriptor_and_chunk_provenance() {
    growth(PlainSymbolBacking::Unprojected).unwrap();
}

#[test]
fn nmjit_n3_actual_projected_growth_retains_old_descriptor() {
    growth(PlainSymbolBacking::VoidProjection).unwrap();
}

#[test]
fn nmjit_n3_actual_same_chunk_gap_fill_keeps_old_pointer_usable() {
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::SameChunk,
        |run, homes| {
            let old = census(run);
            let old_name =
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id);
            let id = homes.target.unwrap();
            assert!(!run.initial_facts_copy_for_test(id).unwrap().present());
            assert!((id.to_raw() as usize) < old.slots());
            intern_retry(run, &homes.name, &mut homes.dst).unwrap();
            let handle = run.initial_symbol_handle(&homes.dst).unwrap();
            assert_eq!(
                run.symbol_table_name_at_for_test(old.address(), &handle, id),
                crate::intern::symbol_name_id(id)
            );
            assert_eq!(
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id),
                old_name
            );
            let now = census(run);
            assert_eq!(now.address(), old.address());
            assert_eq!(now, old);
            run.sym_set_into(&homes.old_handle, &homes.value, &mut homes.dst)
                .unwrap();
            assert_eq!(
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id),
                old_name
            );
            run.sym_makunbound_into(&homes.old_handle, &mut homes.dst)
                .unwrap();
            assert_eq!(
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id),
                old_name
            );
            eprintln!("NMJIT_N3_REAL_SAME_CHUNK_NO_REPLACEMENT");
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n3_actual_capacity_refusal_preserves_table_and_old_reads() {
    use crate::symbol::parallel::test_facts::InitialInternFault;
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Boundary,
        |run, homes| {
            let old = census(run);
            let old_name =
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id);
            let id = homes.target.unwrap();
            let spine_before = run.initial_facts_copy_for_test(id).unwrap().spine_address();
            run.arm_initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)
                .unwrap();
            assert_eq!(
                intern_retry(run, &homes.name, &mut homes.dst),
                Err(InitialInternError::Authority(RegistrationError::Capacity))
            );
            assert_ne!(
                run.initial_facts_copy_for_test(id).unwrap().spine_address(),
                spine_before
            );
            let after = census(run);
            assert_eq!(after.generation(), old.generation());
            assert_eq!(after.retired(), old.retired());
            assert_eq!(after.address(), old.address());
            assert_eq!(
                old_name,
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id)
            );
            eprintln!("NMJIT_N3_REAL_CAPACITY_REFUSAL_OLD_TABLE_RETAINED");
        },
    )
    .unwrap();
}
