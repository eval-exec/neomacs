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
        tables
            .ledger
            .hold_metadata_for_test(|| panic!("NMJIT_N3_TEST_POISONED_METADATA_LOCK"));
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
    let counts = tables.with_owners_for_test(|_current, predecessors| {
        predecessors
            .iter()
            .map(|owner| owner.table.table().chunk_count().get())
            .collect::<Vec<_>>()
    });
    assert_eq!(counts, vec![1]);
}

#[test]
fn nmjit_n3_publication_byte_quote_overflow_is_table_capacity() {
    use crate::tagged::gc::publication::{
        PayloadPlan, PublicationLedger, PublicationPayload, RetainedBytes, RetentionLimits,
        RetiredCount,
    };

    /// Toy payload whose quote is `usize::MAX`, so the next admission overflows.
    struct OverflowPayload {
        value: Box<u8>,
    }
    // SAFETY: `self.value` is a heap-allocated `Box<u8>`, so its address stays
    // valid and stable across moves of `OverflowPayload` until drop.
    unsafe impl PublicationPayload for OverflowPayload {
        type Error = TableError;
        type Generation = u64;

        fn published_address(&self) -> *mut u8 {
            (&raw const *self.value).cast_mut().cast()
        }

        fn retained_bytes(&self) -> RetainedBytes {
            RetainedBytes::new(usize::MAX)
        }

        fn generation(&self) -> u64 {
            0
        }

        fn prepare(current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
            let _ = current;
            Ok(PayloadPlan::Replacement(OverflowPayload {
                value: Box::new(2),
            }))
        }
    }

    let store = PublicationLedger::create(
        OverflowPayload { value: Box::new(1) },
        RetentionLimits::new(RetainedBytes::new(usize::MAX), RetiredCount::new(4)),
    )
    .unwrap();
    let Err(refused) = store.prepare() else {
        panic!("usize::MAX quote prepared a transaction");
    };
    assert_eq!(
        map_publication_error(refused),
        TablePublicationError::Table(TableError::Capacity)
    );

    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let accounted = tables.ledger.account(|admission| {
        admission.admit(RetainedBytes::new(usize::MAX))?;
        Ok(())
    });
    let Err(refused) = accounted.result else {
        panic!("usize::MAX admission prepared a table");
    };
    assert_eq!(
        map_publication_error(refused),
        TablePublicationError::Table(TableError::Capacity)
    );
    // Reserve runs before the overflowing add, so the capacity charge stays
    // and nothing is published.
    assert!(accounted.charged.get() >= std::mem::size_of::<OwnedSymbolTable>());
    assert_eq!(tables.view().table().chunk_count().get(), 1);
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        table_bytes(shape(1)).unwrap() + accounted.charged.get()
    );
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

#[test]
fn e7p0_symbol_table_need_grace_reclaim_unblocks_growth_for_both_caps() {
    // (a) Retiree-count cap (`retired = 1`).
    let backing = Backing::new();
    let limits = TableLimits::new(TableLimits::DEFAULT_BYTES, RetiredTableCount::new(1));
    let tables = PublishedSymbolTables::new(backing.initial(), limits).unwrap();
    let first = tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(2))
        .unwrap();
    assert!(matches!(first, PublicationChange::Published { .. }));
    assert_eq!(tables.stats().unwrap().retired.get(), 1);
    let refused = tables.try_prepare(shape(3));
    assert!(matches!(
        refused.result,
        Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables))
    ));
    let current_ptr_before = tables.view().table() as *const PublishedSymbolTable as usize;
    let current_gen_before = tables.stats().unwrap().generation;

    // SAFETY: unit test owns `tables` exclusively and no view is held across the call.
    let report = unsafe { tables.reclaim_retired_after_grace() }.unwrap();
    assert_eq!(report.reclaimed.get(), 1);
    assert!(report.freed_bytes.get() >= table_bytes(shape(1)).unwrap());
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        table_bytes(shape(2)).unwrap()
    );
    assert_eq!(
        tables.view().table() as *const PublishedSymbolTable as usize,
        current_ptr_before
    );
    assert_eq!(tables.stats().unwrap().generation, current_gen_before);
    let second = tables
        .try_prepare(shape(3))
        .result
        .unwrap()
        .publish(backing.addresses(3))
        .unwrap();
    assert!(matches!(second, PublicationChange::Published { .. }));
    assert_eq!(tables.view().table().chunk_count().get(), 3);

    // (b) Retained-bytes cap (`current + one successor + one slot`, tight after first growth).
    let b1 = table_bytes(shape(1)).unwrap();
    let b2 = table_bytes(shape(2)).unwrap();
    let b3 = table_bytes(shape(3)).unwrap();
    let byte_cap = b2 + b3 + 8 * std::mem::size_of::<OwnedSymbolTable>();
    let byte_tables = PublishedSymbolTables::new(
        backing.initial(),
        TableLimits::new(
            RetainedTableBytes::new(byte_cap),
            TableLimits::DEFAULT_RETIRED,
        ),
    )
    .unwrap();
    byte_tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(2))
        .unwrap();
    // Tighten the byte cap to fit shape(2) + shape(3) only once retired shape(1)
    // is reclaimed, forcing NeedGrace(RetainedBytes) before grace and succeeding after.
    let cap_without_s1 = b2 + b3 + 4 * std::mem::size_of::<OwnedSymbolTable>();
    assert!(b1 + b2 + b3 > cap_without_s1);
    byte_tables
        .set_limits_for_test(TestTableLimits::Override(TableLimits::new(
            RetainedTableBytes::new(cap_without_s1),
            TableLimits::DEFAULT_RETIRED,
        )))
        .unwrap();
    let refused_bytes = byte_tables.try_prepare(shape(3));
    assert!(matches!(
        refused_bytes.result,
        Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes))
    ));
    // SAFETY: unit test owns `byte_tables` exclusively with zero live views.
    let byte_report = unsafe { byte_tables.reclaim_retired_after_grace() }.unwrap();
    assert_eq!(byte_report.reclaimed.get(), 1);
    assert_eq!(byte_tables.stats().unwrap().retired.get(), 0);
    assert_eq!(byte_tables.retained_bytes().unwrap().get(), b2);
    let unblocked = byte_tables
        .try_prepare(shape(3))
        .result
        .unwrap()
        .publish(backing.addresses(3))
        .unwrap();
    assert!(matches!(unblocked, PublicationChange::Published { .. }));
    assert_eq!(byte_tables.view().table().chunk_count().get(), 3);
}

#[test]
fn e7p0_symbol_table_reclaim_is_idempotent_when_no_predecessors() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let ptr_before = tables.view().table() as *const PublishedSymbolTable as usize;
    // SAFETY: unit test owns `tables` exclusively with zero live views.
    let report = unsafe { tables.reclaim_retired_after_grace() }.unwrap();
    assert_eq!(report.reclaimed.get(), 0);
    assert_eq!(report.freed_bytes.get(), 0);
    assert_eq!(
        tables.view().table() as *const PublishedSymbolTable as usize,
        ptr_before
    );
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
    assert_eq!(
        tables.retained_bytes().unwrap().get(),
        table_bytes(shape(1)).unwrap()
    );
}

#[test]
fn e7p0_symbol_table_reclaim_refuses_busy_while_table_view_or_prepare_is_live() {
    let backing = Backing::new();
    let tables = PublishedSymbolTables::new(backing.initial(), TableLimits::default()).unwrap();
    let old_view = tables.view();
    let old_ptr = old_view.table() as *const PublishedSymbolTable as usize;
    tables
        .try_prepare(shape(2))
        .result
        .unwrap()
        .publish(backing.addresses(2))
        .unwrap();
    assert_eq!(tables.stats().unwrap().retired.get(), 1);

    // A live `TableView` obtained via `tables.view()` tracks `view_depth` and
    // must block grace reclamation with `Busy` while retaining the old descriptor.
    // SAFETY: testing fail-closed `Busy` refusal while `old_view` is live.
    assert_eq!(
        unsafe { tables.reclaim_retired_after_grace() },
        Err(TablePublicationError::Busy)
    );
    assert_eq!(
        old_view.table() as *const PublishedSymbolTable as usize,
        old_ptr
    );
    assert_eq!(old_view.table().chunk_count().get(), 1);
    assert_eq!(tables.stats().unwrap().retired.get(), 1);
    drop(old_view);

    // An open `PreparedTablePublication` holds the metadata mutex and must also
    // block grace reclamation with `Busy`.
    let held = tables.try_prepare(shape(3)).result.unwrap();
    // SAFETY: testing fail-closed `Busy` refusal while metadata mutex is held.
    assert_eq!(
        unsafe { tables.reclaim_retired_after_grace() },
        Err(TablePublicationError::Busy)
    );
    drop(held);
    assert_eq!(tables.stats().unwrap().retired.get(), 1);

    // SAFETY: both `old_view` and `held` are dropped; test owns `tables` exclusively.
    let report = unsafe { tables.reclaim_retired_after_grace() }.unwrap();
    assert_eq!(report.reclaimed.get(), 1);
    assert_eq!(tables.stats().unwrap().retired.get(), 0);
}
