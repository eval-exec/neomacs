use super::*;
use crate::tagged::gc::symbol_table::{SymbolChunkCount, SymbolChunkSlots, SymbolSlotCount};
use std::num::NonZeroUsize;
use std::ptr::NonNull;

struct Backing {
    chunks: Vec<Box<u8>>,
}
impl Backing {
    fn new() -> Self {
        Self {
            chunks: (0..4).map(|value| Box::new(value)).collect(),
        }
    }
    fn addresses(&self, count: usize) -> impl Iterator<Item = SymbolChunkAddress> + '_ {
        self.chunks.iter().take(count).map(|chunk| {
            // SAFETY: the fixture retains each separately allocated box until
            // after its publication owner and all views have been dropped.
            unsafe { SymbolChunkAddress::from_allocation(NonNull::from(&**chunk).cast()) }
        })
    }
    fn initial(&self) -> OwnedSymbolTable {
        PreparedSymbolTable::new(shape(1), PointerTableGeneration::INITIAL)
            .unwrap()
            .initialize(self.addresses(1))
            .unwrap()
    }
}

fn shape(chunks: usize) -> SymbolTableShape {
    SymbolTableShape::new(
        SymbolSlotCount::from(chunks * 2),
        SymbolChunkCount::from(chunks),
        SymbolChunkSlots::from(NonZeroUsize::new(2).unwrap()),
    )
    .unwrap()
}

#[test]
fn nmjit_n3_publication_preserves_two_predecessors_and_live_view() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let old = tables.view();
    let original = old.table().chunk(0.into());
    let first = tables.try_prepare(shape(2));
    let first_charge = first.charged;
    let first = first.result.unwrap().publish(backing.addresses(2)).unwrap();
    assert!(matches!(first, PublicationChange::Published { .. }));
    let middle = tables.view();
    let second = tables.try_prepare(shape(3));
    let second_charge = second.charged;
    let second = second
        .result
        .unwrap()
        .publish(backing.addresses(3))
        .unwrap();
    assert!(matches!(second, PublicationChange::Published { .. }));
    assert_eq!(old.table().chunk_count().get(), 1);
    assert_eq!(middle.table().chunk_count().get(), 2);
    assert_eq!(tables.view().table().chunk_count().get(), 3);
    assert_eq!(old.table().chunk(0.into()), original);
    assert_eq!(tables.stats().unwrap().retired.get(), 2);
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        table_bytes(shape(1)).unwrap()
            + first_charge.get()
            + first.charged().get()
            + second_charge.get()
            + second.charged().get()
    );
}

#[test]
fn nmjit_n3_publication_equal_shape_keeps_pointer_generation_and_charge() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let before = tables.view();
    let bytes = tables.retained_bytes().unwrap();
    let preparation = tables.try_prepare(shape(1));
    assert_eq!(preparation.charged.get(), 0);
    let change = preparation
        .result
        .unwrap()
        .publish(backing.addresses(1))
        .unwrap();
    assert!(matches!(change, PublicationChange::Unchanged { .. }));
    assert_eq!(change.charged().get(), 0);
    assert!(std::ptr::eq(before.table(), tables.view().table()));
    assert_eq!(
        tables.stats().unwrap().generation,
        PointerTableGeneration::INITIAL
    );
    assert_eq!(tables.retained_bytes().unwrap(), bytes);
}

#[test]
fn nmjit_n3_publication_retiree_limit_refuses_before_mutation() {
    let backing = Backing::new();
    let limits = TableLimits::new(TableLimits::DEFAULT_BYTES, RetiredTableCount::new(0));
    let tables = PublishedSymbolTables::new(backing.initial(), limits).unwrap();
    let preparation = tables.try_prepare(shape(2));
    assert!(matches!(
        preparation.result,
        Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables))
    ));
    assert_eq!(preparation.charged.get(), 0);
    assert_eq!(tables.view().table().chunk_count().get(), 1);
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
}

#[test]
fn nmjit_n3_publication_refusal_charges_retained_capacity_once() {
    let backing = Backing::new();
    let initial_bytes = table_bytes(shape(1)).unwrap();
    let limits = TableLimits::new(
        RetainedTableBytes::new(initial_bytes),
        TableLimits::DEFAULT_RETIRED,
    );
    let tables = PublishedSymbolTables::new(backing.initial(), limits).unwrap();
    let preparation = tables.try_prepare(shape(2));
    assert!(matches!(
        preparation.result,
        Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes))
    ));
    assert!(preparation.charged.get() >= size_of::<OwnedSymbolTable>());
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        initial_bytes + preparation.charged.get()
    );
    let again = tables.try_prepare(shape(2));
    assert!(matches!(
        again.result,
        Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes))
    ));
    assert_eq!(again.charged.get(), 0);
    assert_eq!(tables.view().table().chunk_count().get(), 1);
}

#[test]
fn nmjit_n3_publication_dropped_preparation_charges_no_candidate() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let before = tables.retained_bytes().unwrap().get();
    let preparation = tables.try_prepare(shape(2));
    let capacity_charge = preparation.charged.get();
    assert!(preparation.result.is_ok());
    drop(preparation);
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        before + capacity_charge
    );
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
    assert_eq!(tables.view().table().chunk_count().get(), 1);
}

#[test]
fn nmjit_n3_publication_bad_addresses_never_publish() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let before = tables.view();
    let incomplete = tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(1));
    assert_eq!(
        incomplete,
        Err(TablePublicationError::Table(TableError::Shape))
    );
    let changed_prefix = tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(3).skip(1));
    assert_eq!(
        changed_prefix,
        Err(TablePublicationError::Table(TableError::Shape))
    );
    assert!(std::ptr::eq(before.table(), tables.view().table()));
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
}

#[test]
fn nmjit_n3_publication_shape_and_generation_refusals_are_total() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    assert!(matches!(
        tables.try_prepare(shape(0)).result,
        Err(TablePublicationError::Table(TableError::Shape))
    ));
    let changed_span =
        SymbolTableShape::new(4.into(), 1.into(), NonZeroUsize::new(4).unwrap().into()).unwrap();
    assert!(matches!(
        tables.try_prepare(changed_span).result,
        Err(TablePublicationError::Table(TableError::Shape))
    ));
    let exhausted = PreparedSymbolTable::new(
        shape(1),
        PointerTableGeneration(std::num::NonZeroU64::new(u64::MAX).unwrap()),
    )
    .unwrap()
    .initialize(backing.addresses(1))
    .unwrap();
    let exhausted = PublishedSymbolTables::new(exhausted, TableLimits::default()).unwrap();
    assert!(matches!(
        exhausted.try_prepare(shape(2)).result,
        Err(TablePublicationError::Table(
            TableError::GenerationExhausted
        ))
    ));
}

#[test]
fn nmjit_n3_publication_poison_retains_readable_owned_tables() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(2))
        .unwrap();
    let before = tables.view();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ledger = tables.ledger.lock().unwrap();
        panic!("NMJIT_N3_TEST_POISONED_METADATA_LOCK");
    }));
    assert!(outcome.is_err());
    assert!(matches!(
        tables.try_prepare(shape(3)).result,
        Err(TablePublicationError::Poisoned)
    ));
    assert_eq!(tables.stats(), Err(TablePublicationError::Poisoned));
    assert!(std::ptr::eq(before.table(), tables.view().table()));
    assert_eq!(before.table().chunk_count().get(), 2);
    // Poisoning does not drain ownership of the predecessor either.
    let poisoned = tables.ledger.lock().err().unwrap().into_inner();
    assert_eq!(poisoned.retired.len(), 1);
    assert_eq!(poisoned.retired[0].table().chunk_count().get(), 1);
}

#[test]
fn nmjit_n3_publication_live_preparation_refuses_busy_without_waiting() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let held = tables.try_prepare(shape(2)).result.unwrap();
    let refused = tables.try_prepare(shape(3));
    assert!(matches!(refused.result, Err(TablePublicationError::Busy)));
    assert_eq!(refused.charged.get(), 0);
    assert_eq!(tables.stats(), Err(TablePublicationError::Busy));
    assert_eq!(tables.retained_bytes(), Err(TablePublicationError::Busy));
    assert_eq!(tables.view().table().chunk_count().get(), 1);
    drop(held);
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
    assert!(tables.try_prepare(shape(2)).result.is_ok());
}
