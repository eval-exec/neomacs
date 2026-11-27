//! Immutable pointer publication with uniquely owned, grace-reclaimed predecessors.
//!
//! This file is the N3/N5 adapter. The retention owner, its metadata lock,
//! view-depth counter, and predecessor vector live in the generic publication
//! ledger. This lock does not replace the symbol authority required by the
//! backing transaction. Predecessors stay owned until either this owner is
//! dropped or a stop-bound `EraGraceReceipt` invokes
//! `reclaim_retired_after_grace` while zero generated activations and zero
//! tracked `TableView` borrows are live.
use super::sync::{AtomicPtr, Ordering};
use super::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, PublishedSymbolTable,
    SymbolChunkAddress, SymbolTableShape, TableError,
};
use crate::tagged::gc::publication::{
    Commit, OpenPublication, PublicationError, PublicationLedger,
    PublicationView as LedgerPublicationView, RetainedBytes, RetentionLimit, RetentionLimits,
    RetiredCount,
};
use std::marker::PhantomData;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedTableBytes(usize);
impl RetainedTableBytes {
    pub(crate) const fn new(bytes: usize) -> Self {
        Self(bytes)
    }
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetiredTableCount(usize);
impl RetiredTableCount {
    pub(crate) const fn new(count: usize) -> Self {
        Self(count)
    }
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableLimits {
    bytes: RetainedTableBytes,
    retired: RetiredTableCount,
}

/// Test-only limit selection belongs to one ledger, never process state.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy)]
pub(crate) enum TestTableLimits {
    Production,
    Override(TableLimits),
}
impl TableLimits {
    pub(crate) const DEFAULT_BYTES: RetainedTableBytes = RetainedTableBytes::new(64 * 1024 * 1024);
    pub(crate) const DEFAULT_RETIRED: RetiredTableCount = RetiredTableCount::new(1024);

    pub(crate) const fn new(bytes: RetainedTableBytes, retired: RetiredTableCount) -> Self {
        Self { bytes, retired }
    }
}
impl Default for TableLimits {
    fn default() -> Self {
        Self::new(Self::DEFAULT_BYTES, Self::DEFAULT_RETIRED)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableLimit {
    RetainedBytes,
    RetiredTables,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TablePublicationError {
    Table(TableError),
    NeedGrace(TableLimit),
    Busy,
    Poisoned,
}
impl From<TableError> for TablePublicationError {
    fn from(error: TableError) -> Self {
        Self::Table(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationChange {
    Unchanged {
        generation: PointerTableGeneration,
    },
    Published {
        generation: PointerTableGeneration,
        charged: RetainedTableBytes,
    },
}

impl PublicationChange {
    pub(crate) fn charged(self) -> RetainedTableBytes {
        match self {
            Self::Unchanged { .. } => RetainedTableBytes(0),
            Self::Published { charged, .. } => charged,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableGraceReclaim {
    pub(crate) reclaimed: RetiredTableCount,
    pub(crate) freed_bytes: RetainedTableBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct TableStatistics {
    pub(crate) generation: PointerTableGeneration,
    pub(crate) retired: RetiredTableCount,
    pub(crate) retained_bytes: RetainedTableBytes,
}
impl TableStatistics {
    /// Copied `(generation, retired)` scalars. The newtypes stay crate-private.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn copied_counts_for_test(self) -> (u64, usize) {
        (self.generation.0.get(), self.retired.get())
    }
}

#[cfg(feature = "test-support")]
impl TableStatistics {
    /// Copied read (peer #1482 condition 2/4): the registry generation as a
    /// plain word. The generation type and field stay private.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn generation_word(&self) -> u64 {
        self.generation.get()
    }

    /// Copied read (peer #1482 condition 2/4): the retired-table count as a
    /// plain usize. The count type and field stay private.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn retired_count(&self) -> usize {
        self.retired.get()
    }
}

fn table_bytes(shape: SymbolTableShape) -> Result<usize, TableError> {
    shape
        .chunks
        .0
        .checked_mul(size_of::<SymbolChunkAddress>())
        .and_then(|bytes| bytes.checked_add(size_of::<PublishedSymbolTable>()))
        .ok_or(TableError::Capacity)
}

fn core_limits(limits: TableLimits) -> RetentionLimits {
    RetentionLimits::new(
        RetainedBytes::new(limits.bytes.get()),
        RetiredCount::new(limits.retired.get()),
    )
}

fn map_publication_error(error: PublicationError<TableError>) -> TablePublicationError {
    match error {
        PublicationError::Payload(error) => TablePublicationError::Table(error),
        PublicationError::Capacity => TablePublicationError::Table(TableError::Capacity),
        PublicationError::NeedGrace(RetentionLimit::RetainedBytes) => {
            TablePublicationError::NeedGrace(TableLimit::RetainedBytes)
        }
        PublicationError::NeedGrace(RetentionLimit::Retired) => {
            TablePublicationError::NeedGrace(TableLimit::RetiredTables)
        }
        PublicationError::Busy => TablePublicationError::Busy,
        PublicationError::Poisoned => TablePublicationError::Poisoned,
    }
}

fn map_change(
    change: crate::tagged::gc::publication::PublicationChange<PointerTableGeneration>,
) -> PublicationChange {
    match change {
        crate::tagged::gc::publication::PublicationChange::Unchanged { generation } => {
            PublicationChange::Unchanged { generation }
        }
        crate::tagged::gc::publication::PublicationChange::Published {
            generation,
            charged,
        } => PublicationChange::Published {
            generation,
            charged: RetainedTableBytes::new(charged.get()),
        },
    }
}

/// Sealed payload. The quote is the descriptor plus its chunk-address array.
/// Vector storage is `size_of` of this handle, one slot per reserved predecessor.
struct SymbolPublication {
    table: OwnedSymbolTable,
}

// SAFETY: `self.table` is an `OwnedSymbolTable` heap allocation whose raw
// descriptor pointer remains valid and stable across moves into the
// predecessor vector until `SymbolPublication` is dropped.
unsafe impl crate::tagged::gc::publication::PublicationPayload for SymbolPublication {
    type Error = TableError;
    type Generation = PointerTableGeneration;

    fn published_address(&self) -> *mut u8 {
        self.table.as_ptr().cast()
    }

    fn retained_bytes(&self) -> RetainedBytes {
        // A live table's shape was accepted, so this quote already fit.
        RetainedBytes::new(table_bytes(self.table.table().shape).unwrap_or(usize::MAX))
    }

    fn storage_overhead() -> usize {
        size_of::<OwnedSymbolTable>()
    }

    fn generation(&self) -> Self::Generation {
        self.table.table().generation
    }

    fn prepare(
        _current: &Self,
    ) -> Result<crate::tagged::gc::publication::PayloadPlan<Self>, Self::Error> {
        // Growth goes through `PublicationLedger::account`. The trait's
        // one-step prepare cannot see chunk addresses, which arrive at publish.
        Ok(crate::tagged::gc::publication::PayloadPlan::Unchanged)
    }
}

/// Copied current-table census. Address is the published descriptor pointer.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy)]
pub(crate) struct TableCensus {
    pub(crate) address: usize,
    pub(crate) slots: usize,
    pub(crate) chunk_slots: usize,
    pub(crate) chunk0: Option<usize>,
    pub(crate) generation: u64,
    pub(crate) retired: usize,
    pub(crate) retained_bytes: usize,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy)]
pub(crate) enum TestLimitProbe {
    Bytes,
    Retirees,
    Production,
}

/// One owner of every descriptor ever published during this era.
#[must_use = "retain the publication owner until all table users have returned"]
pub(crate) struct PublishedSymbolTables {
    ledger: PublicationLedger<SymbolPublication>,
}

// SAFETY: the unique raw owners name immutable allocation metadata only. Chunk
// entries are nonowning addresses and are never dereferenced here. The mutex
// serializes ownership changes and predecessor reclamation; predecessors are
// freed only on drop or via `unsafe fn reclaim_retired_after_grace` under a
// stop-bound `EraGraceReceipt` (which proves zero live `ExecutionActivation`
// borrows) and only when `ledger.view_depth == 0` (no live `TableView`).
// Views borrow this owner and cannot cross threads themselves.
unsafe impl Send for PublishedSymbolTables {}
// SAFETY: immutable descriptor reads use Acquire publication; owner changes are
// mutex-exclusive and retain every predecessor until stop-bound grace
// reclamation. `TableView` tracks `ledger.view_depth` atomically so
// `reclaim_retired_after_grace` refuses `Busy` behind any live `TableView`, and
// `PublicationView` is confined to `ExecutionActivation` (`IN_SLICE`) where
// `AllStopped` excludes concurrent grace reclamation. Shared access cannot
// mutate a table, dereference chunk contents, or reclaim behind a live view.
unsafe impl Sync for PublishedSymbolTables {}

impl PublishedSymbolTables {
    pub(crate) fn new(
        initial: OwnedSymbolTable,
        limits: TableLimits,
    ) -> Result<Self, TablePublicationError> {
        if table_bytes(initial.table().shape)? > limits.bytes.get() {
            return Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes));
        }
        let ledger =
            PublicationLedger::create(SymbolPublication { table: initial }, core_limits(limits))
                .map_err(map_publication_error)?;
        Ok(Self { ledger })
    }

    /// Borrow the 1-word ABI publication slot for an `IN_SLICE` activation.
    ///
    /// Does not bump `ledger.view_depth` because `PublicationView` is
    /// `#[repr(transparent)]` over `&AtomicPtr<u8>`; safety relies on the
    /// enclosing `ExecutionActivation` (`IN_SLICE`), which excludes stop-bound
    /// `EraGraceReceipt` reclamation until the activation returns.
    pub(crate) fn publication(&self) -> PublicationView<'_> {
        PublicationView {
            slot: self.ledger.slot(),
            owner: PhantomData,
        }
    }

    /// Borrow the current immutable descriptor while tracking `ledger.view_depth`
    /// so `reclaim_retired_after_grace` refuses `Busy` while this view is live.
    pub(crate) fn view(&self) -> TableView<'_> {
        let ledger_view = self.ledger.view();
        // SAFETY: `ledger_view` loaded an initialized `PublishedSymbolTable`
        // pointer with Acquire ordering and incremented `ledger.view_depth`,
        // preventing `drain_predecessors_after_grace` from reclaiming any
        // predecessor until `ledger_view` is dropped with `TableView`.
        unsafe { published_table_tracked(ledger_view) }
    }

    pub(crate) fn stats(&self) -> Result<TableStatistics, TablePublicationError> {
        let statistics = self.ledger.statistics().map_err(map_publication_error)?;
        Ok(TableStatistics {
            generation: statistics.generation,
            retired: RetiredTableCount::new(statistics.retired.get()),
            retained_bytes: RetainedTableBytes::new(statistics.retained_bytes.get()),
        })
    }

    pub(crate) fn retained_bytes(&self) -> Result<RetainedTableBytes, TablePublicationError> {
        self.stats().map(|statistics| statistics.retained_bytes)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_limits_for_test(
        &self,
        limits: TestTableLimits,
    ) -> Result<(), TablePublicationError> {
        let limits = match limits {
            TestTableLimits::Production => None,
            TestTableLimits::Override(limits) => Some(core_limits(limits)),
        };
        self.ledger
            .set_limit_override(limits)
            .map_err(map_publication_error)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn census_for_test(&self) -> Result<TableCensus, TablePublicationError> {
        let view = self.view();
        let table = view.table();
        let stats = self.stats()?;
        Ok(TableCensus {
            address: table as *const PublishedSymbolTable as usize,
            slots: table.slots().get(),
            chunk_slots: table.chunk_slots().get(),
            chunk0: table
                .chunk(super::SymbolChunkIndex::from(0))
                .map(|chunk| chunk.as_ptr() as usize),
            generation: stats.generation.0.get(),
            retired: stats.retired.get(),
            retained_bytes: stats.retained_bytes.get(),
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn apply_limit_probe_for_test(
        &self,
        probe: TestLimitProbe,
    ) -> Result<(), TablePublicationError> {
        let limits = match probe {
            TestLimitProbe::Production => TestTableLimits::Production,
            TestLimitProbe::Bytes => {
                let retained = self.stats()?.retained_bytes;
                TestTableLimits::Override(TableLimits::new(retained, TableLimits::DEFAULT_RETIRED))
            }
            TestLimitProbe::Retirees => TestTableLimits::Override(TableLimits::new(
                TableLimits::DEFAULT_BYTES,
                RetiredTableCount::new(0),
            )),
        };
        self.set_limits_for_test(limits)
    }

    /// Read one retained cell's name. Searches the current table, then retired
    /// descriptors. The caller holds the matching cell reader, or the era is
    /// already poisoned and no writer remains.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn retained_name_at_for_test(
        &self,
        address: usize,
        slot: usize,
    ) -> crate::intern::NameId {
        self.with_owners_for_test(|current, predecessors| {
            let owned = std::iter::once(current)
                .chain(predecessors.iter())
                .map(|owner| &owner.table)
                .find(|table| table.as_ptr() as usize == address)
                .unwrap_or_else(|| panic!("retained symbol table {address:#x}"));
            let table = owned.table();
            assert!(slot < table.slots().get());
            let span = table.chunk_slots().get();
            let chunk = table
                .chunk(super::SymbolChunkIndex::from(slot / span))
                .unwrap_or_else(|| panic!("symbol table chunk"));
            // SAFETY: the caller retains the matching cell reader, or the era is
            // poisoned and the writer has returned. The slot bound and the chunk
            // come from this same retained descriptor. No symbol reference escapes.
            unsafe {
                (&*chunk
                    .as_ptr()
                    .cast::<crate::symbol::LispSymbol>()
                    .add(slot % span))
                    .name()
            }
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    fn with_owners_for_test<R>(
        &self,
        body: impl FnOnce(&SymbolPublication, &[SymbolPublication]) -> R,
    ) -> R {
        self.ledger.with_retained(body)
    }

    pub(crate) fn try_prepare(&self, shape: SymbolTableShape) -> TablePreparationAccounting<'_> {
        let accounted = self
            .ledger
            .account(|admission| prepare_shape(admission, shape));
        TablePreparationAccounting {
            result: match accounted.result {
                Ok(open) => Ok(PreparedTablePublication { open }),
                Err(error) => Err(map_publication_error(error)),
            },
            charged: RetainedTableBytes::new(accounted.charged.get()),
        }
    }

    /// Reclaim all retired predecessor descriptors (never `current`) once a
    /// stop-bound grace proof guarantees every concurrent activation has exited.
    ///
    /// # Safety
    /// The caller must hold a valid stop-bound grace proof for this era's
    /// `PublishedSymbolTables` establishing that zero threads retain any
    /// descriptor pointer loaded prior to the current published root.
    pub(crate) unsafe fn reclaim_retired_after_grace(
        &self,
    ) -> Result<TableGraceReclaim, TablePublicationError> {
        // SAFETY: caller supplies the stop-bound grace proof above.
        let report = unsafe { self.ledger.drain_predecessors_after_grace() }
            .map_err(map_publication_error)?;
        Ok(TableGraceReclaim {
            reclaimed: RetiredTableCount::new(report.reclaimed.get()),
            freed_bytes: RetainedTableBytes::new(report.freed_bytes.get()),
        })
    }
}

fn prepare_shape(
    admission: &mut crate::tagged::gc::publication::Admission<'_, SymbolPublication>,
    shape: SymbolTableShape,
) -> Result<Replacement, PublicationError<TableError>> {
    let current = admission.current().table.table();
    if shape.chunk_slots != current.shape.chunk_slots
        || shape.chunks.0 < current.shape.chunks.0
        || shape.slots.0 < current.shape.slots.0
    {
        return Err(PublicationError::Payload(TableError::Shape));
    }
    if shape == current.shape {
        return Ok(Replacement::Unchanged);
    }
    let generation = current
        .generation
        .checked_successor()
        .map_err(PublicationError::Payload)?;
    // Reserve BEFORE the backing may mutate. A successful reserve is retained
    // and charged even if a later byte limit refuses growth.
    let extra = table_bytes(shape).map_err(PublicationError::Payload)?;
    admission.admit(RetainedBytes::new(extra))?;
    Ok(Replacement::Prepared(
        PreparedSymbolTable::new(shape, generation).map_err(PublicationError::Payload)?,
    ))
}

/// Transparent borrowed atomic slot for consumers of the explicit descriptor
/// ABI. Its representation is one reference; no mutable slot or chunk owner
/// escapes. Generated consumers must use an Acquire-or-stronger atomic load.
#[repr(transparent)]
#[must_use = "retain this borrow until every descriptor use returns"]
#[doc(hidden)]
pub struct PublicationView<'owner> {
    slot: &'owner AtomicPtr<u8>,
    owner: PhantomData<std::rc::Rc<()>>,
}
impl PublicationView<'_> {
    pub(crate) fn load(&self) -> TableView<'_> {
        // SAFETY: `self` is confined to an `IN_SLICE` `ExecutionActivation`
        // during which no stop-bound `EraGraceReceipt` reclamation can run,
        // so the Acquire-loaded `PublishedSymbolTable` stays alive for `'owner`.
        unsafe { published_table(self.slot.load(Ordering::Acquire)) }
    }
}
const _: () = assert!(size_of::<PublicationView<'static>>() == size_of::<*const ()>());
static_assertions::assert_not_impl_any!(PublicationView<'static>: Send, Sync, Clone, Copy);

/// # Safety
/// `pointer` must point to an initialized [`PublishedSymbolTable`] that remains
/// alive and immutable for at least `'owner` (guaranteed by an enclosing
/// `ExecutionActivation` / `IN_SLICE` excluding stop-bound grace reclamation).
unsafe fn published_table<'owner>(pointer: *mut u8) -> TableView<'owner> {
    // SAFETY: construction publishes a fully initialized owned allocation
    // before returning the owner. Every replacement is retained before its
    // Release store; predecessors stay owned until either owner drop or a
    // stop-bound grace reclamation when zero activations are live. The caller
    // ties this borrow to an activation excluded from grace reclamation,
    // including while the metadata lock is poisoned.
    TableView {
        table: unsafe { &*pointer.cast::<PublishedSymbolTable>() },
        _ledger_view: None,
        owner: PhantomData,
    }
}

/// # Safety
/// `ledger_view.published_address()` must point to an initialized
/// [`PublishedSymbolTable`] owned by `PublishedSymbolTables`.
unsafe fn published_table_tracked<'owner>(
    ledger_view: LedgerPublicationView<'owner, SymbolPublication>,
) -> TableView<'owner> {
    let pointer = ledger_view.published_address();
    // SAFETY: `ledger_view` holds a live `view_depth` increment on the owning
    // `PublicationLedger`, so `drain_predecessors_after_grace` refuses `Busy`
    // and cannot reclaim the loaded descriptor until `TableView` is dropped.
    TableView {
        table: unsafe { &*pointer.cast::<PublishedSymbolTable>() },
        _ledger_view: Some(ledger_view),
        owner: PhantomData,
    }
}

/// A view never outlives the owner and never loans chunk ownership.
/// Views created via `PublishedSymbolTables::view` hold a tracked ledger view
/// that keeps `ledger.view_depth > 0` until dropped.
pub(crate) struct TableView<'owner> {
    table: &'owner PublishedSymbolTable,
    _ledger_view: Option<LedgerPublicationView<'owner, SymbolPublication>>,
    owner: PhantomData<&'owner PublishedSymbolTables>,
}
impl TableView<'_> {
    pub(crate) fn table(&self) -> &PublishedSymbolTable {
        self.table
    }
}

enum Replacement {
    Unchanged,
    Prepared(PreparedSymbolTable),
}

/// Retained capacity growth is returned even on refusal. Detached candidate
/// allocations are charged only by a successful publication, never here.
#[must_use]
pub(crate) struct TablePreparationAccounting<'owner> {
    pub(crate) result: Result<PreparedTablePublication<'owner>, TablePublicationError>,
    pub(crate) charged: RetainedTableBytes,
}

#[must_use = "publish after backing commit, or drop the unexposed replacement"]
pub(crate) struct PreparedTablePublication<'owner> {
    open: OpenPublication<'owner, SymbolPublication, Replacement>,
}

impl PreparedTablePublication<'_> {
    /// Address conversion and initialization allocate nothing. The caller must
    /// already hold the independent backing WRITER authority for this commit.
    pub(crate) fn publish(
        self,
        addresses: impl IntoIterator<Item = SymbolChunkAddress>,
    ) -> Result<PublicationChange, TablePublicationError> {
        let mut addresses = addresses.into_iter();
        self.open
            .finish(|replacement, current| match replacement {
                Replacement::Unchanged => {
                    for old in current.table.table().chunks.as_slice() {
                        if addresses.next() != Some(*old) {
                            return Err(PublicationError::Payload(TableError::Shape));
                        }
                    }
                    if addresses.next().is_some() {
                        return Err(PublicationError::Payload(TableError::Shape));
                    }
                    Ok(Commit::Keep)
                }
                Replacement::Prepared(prepared) => {
                    let next = prepared
                        .initialize(addresses)
                        .map_err(PublicationError::Payload)?;
                    if !next
                        .table()
                        .chunks
                        .as_slice()
                        .starts_with(current.table.table().chunks.as_slice())
                    {
                        return Err(PublicationError::Payload(TableError::Shape));
                    }
                    Ok(Commit::Replace(SymbolPublication { table: next }))
                }
            })
            .map(map_change)
            .map_err(map_publication_error)
    }
}

static_assertions::assert_impl_all!(PublishedSymbolTables: Send, Sync);
static_assertions::assert_not_impl_any!(TableView<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PreparedTablePublication<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
