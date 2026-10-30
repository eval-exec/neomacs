//! Actual backing publication: old descriptors are retained across intern.
use super::*;
use crate::emacs_core::intern::NameId;
use crate::emacs_core::symbol::LispSymbol;
use crate::tagged::gc::symbol_table::{PublishedSymbolTable, SymbolChunkIndex};
use crate::tagged::gc::{InitialInternError, RegistrationError, SymbolHandle};

fn name_through_table(
    run: &mut RunAccess<'_, '_>,
    table: &PublishedSymbolTable,
    handle: &SymbolHandle,
    id: crate::emacs_core::intern::SymId,
) -> NameId {
    let _reader = run.symbol_ref(handle).expect("matching cell reader");
    let slot = id.0 as usize;
    assert!(slot < table.slots().get());
    let span = table.chunk_slots().get();
    let chunk = table.chunk(SymbolChunkIndex::from(slot / span)).unwrap();
    // SAFETY: this actual era retains the backing; bound and chunk count came
    // from the SAME descriptor. The matching cell reader excludes every writer
    // through the raw derivation and initialized name load. No symbol ref escapes.
    unsafe { (&*chunk.as_ptr().cast::<LispSymbol>().add(slot % span)).name() }
}

fn growth(kind: PlainSymbolBacking) -> std::thread::Result<()> {
    fixture(kind, Query::TableGrowth, |run, homes| {
        let registry = run.owner.registry;
        let old = registry.symbols.tables.view();
        let before = registry.symbols.tables.stats().unwrap();
        let old_name = name_through_table(run, old.table(), &homes.old_handle, homes.old_id);
        intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        let now = registry.symbols.tables.view();
        let after = registry.symbols.tables.stats().unwrap();
        assert!(now.table().slots().get() > old.table().slots().get());
        assert_eq!(
            after.generation,
            before.generation.checked_successor().unwrap()
        );
        assert_eq!(after.retired.get(), before.retired.get() + 1);
        assert!(!std::ptr::eq(old.table(), now.table()));
        assert_eq!(
            old_name,
            name_through_table(run, old.table(), &homes.old_handle, homes.old_id)
        );
        assert_eq!(old.table().chunk(0.into()), now.table().chunk(0.into()));
        let stable = registry.symbols.tables.stats().unwrap();
        intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
        soft_retry(run, &homes.name, &mut homes.dst).unwrap();
        assert_eq!(registry.symbols.tables.stats().unwrap(), stable);
        eprintln!(
            "NMJIT_N3_REAL_RETIRED_PUBLICATION {kind:?} retained={} retired={}",
            after.retained_bytes.get(),
            after.retired.get()
        );
    })
}

pub(super) fn manifest() -> Result<(), String> {
    growth(PlainSymbolBacking::Unprojected).map_err(|_| {
        "actual descriptor replacement/retirement assertion failed after cleanup".to_owned()
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
            let registry = run.owner.registry;
            let old = registry.symbols.tables.view();
            let before = registry.symbols.tables.stats().unwrap();
            let old_name = name_through_table(run, old.table(), &homes.old_handle, homes.old_id);
            let id = homes.target.unwrap();
            assert!(!run.initial_facts_for_test(id).unwrap().present);
            assert!((id.0 as usize) < old.table().slots().get());
            intern_retry(run, &homes.name, &mut homes.dst).unwrap();
            let handle = run.initial_symbol_handle(&homes.dst).unwrap();
            assert_eq!(
                name_through_table(run, old.table(), &handle, id),
                crate::emacs_core::intern::symbol_name_id(id)
            );
            assert_eq!(
                name_through_table(run, old.table(), &homes.old_handle, homes.old_id),
                old_name
            );
            assert!(std::ptr::eq(
                old.table(),
                registry.symbols.tables.view().table()
            ));
            assert_eq!(registry.symbols.tables.stats().unwrap(), before);
            run.sym_set_into(&homes.old_handle, &homes.value, &mut homes.dst)
                .unwrap();
            assert_eq!(
                name_through_table(run, old.table(), &homes.old_handle, homes.old_id),
                old_name
            );
            run.sym_makunbound_into(&homes.old_handle, &mut homes.dst)
                .unwrap();
            assert_eq!(
                name_through_table(run, old.table(), &homes.old_handle, homes.old_id),
                old_name
            );
            eprintln!("NMJIT_N3_REAL_SAME_CHUNK_NO_REPLACEMENT");
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n3_actual_capacity_refusal_preserves_table_and_old_reads() {
    use crate::emacs_core::symbol::parallel::InitialInternFault;
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Boundary,
        |run, homes| {
            let registry = run.owner.registry;
            let old = registry.symbols.tables.view();
            let before = registry.symbols.tables.stats().unwrap();
            let old_name = name_through_table(run, old.table(), &homes.old_handle, homes.old_id);
            let id = homes.target.unwrap();
            let spine_before = run.initial_facts_for_test(id).unwrap().spine_address;
            run.initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)
                .unwrap();
            assert_eq!(
                intern_retry(run, &homes.name, &mut homes.dst),
                Err(InitialInternError::Authority(RegistrationError::Capacity))
            );
            assert_ne!(
                run.initial_facts_for_test(id).unwrap().spine_address,
                spine_before
            );
            let after = registry.symbols.tables.stats().unwrap();
            assert_eq!(after.generation, before.generation);
            assert_eq!(after.retired, before.retired);
            assert!(std::ptr::eq(
                old.table(),
                registry.symbols.tables.view().table()
            ));
            assert_eq!(
                old_name,
                name_through_table(run, old.table(), &homes.old_handle, homes.old_id)
            );
            eprintln!("NMJIT_N3_REAL_CAPACITY_REFUSAL_OLD_TABLE_RETAINED");
        },
    )
    .unwrap();
}
