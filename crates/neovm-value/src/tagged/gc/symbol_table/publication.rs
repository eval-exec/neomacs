//! Immutable pointer publication with uniquely owned, unreclaimed predecessors.
//!
//! This metadata lock does not replace the symbol authority required by the
//! backing transaction. No table is reclaimed here: a later stop-bound receipt
//! must establish that every generated activation has returned first.
use super::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, PublishedSymbolTable,
    SymbolChunkAddress, SymbolTableShape, TableError,
};
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};

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
pub(crate) struct TableStatistics {
    pub(crate) generation: PointerTableGeneration,
    pub(crate) retired: RetiredTableCount,
    pub(crate) retained_bytes: RetainedTableBytes,
}

struct TableLedger {
    current: OwnedSymbolTable,
    retired: Vec<OwnedSymbolTable>,
    #[cfg(any(test, feature = "test-support"))]
    test_limits: TestTableLimits,
}

fn table_bytes(shape: SymbolTableShape) -> Result<usize, TableError> {
    shape
        .chunks
        .0
        .checked_mul(size_of::<SymbolChunkAddress>())
        .and_then(|bytes| bytes.checked_add(size_of::<PublishedSymbolTable>()))
        .ok_or(TableError::Capacity)
}

impl TableLedger {
    fn capacity_bytes(&self) -> RetainedTableBytes {
        // Vec's allocated capacity is bounded by isize::MAX bytes. Saturation
        // keeps this accessor total even if that allocation invariant changes.
        RetainedTableBytes(
            self.retired
                .capacity()
                .saturating_mul(size_of::<OwnedSymbolTable>()),
        )
    }

    fn retained_bytes(&self) -> Result<RetainedTableBytes, TableError> {
        let mut bytes = self
            .retired
            .capacity()
            .checked_mul(size_of::<OwnedSymbolTable>())
            .and_then(|bytes| bytes.checked_add(table_bytes(self.current.table().shape).ok()?))
            .ok_or(TableError::Capacity)?;
        for table in &self.retired {
            bytes = bytes
                .checked_add(table_bytes(table.table().shape)?)
                .ok_or(TableError::Capacity)?;
        }
        Ok(RetainedTableBytes(bytes))
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
    published: AtomicPtr<PublishedSymbolTable>,
    ledger: Mutex<TableLedger>,
    limits: TableLimits,
}

// SAFETY: the unique raw owners name immutable allocation metadata only. Chunk
// entries are nonowning addresses and are never dereferenced here. The mutex
// serializes ownership changes, and none of the allocations is reclaimed while
// this owner lives. Views borrow this owner and cannot cross threads themselves.
unsafe impl Send for PublishedSymbolTables {}
// SAFETY: immutable descriptor reads use Acquire publication; owner changes are
// mutex-exclusive and retain every predecessor. Shared access cannot mutate a
// table, dereference chunk contents, or reclaim an allocation behind a view.
unsafe impl Sync for PublishedSymbolTables {}

impl PublishedSymbolTables {
    pub(crate) fn new(
        initial: OwnedSymbolTable,
        limits: TableLimits,
    ) -> Result<Self, TablePublicationError> {
        if table_bytes(initial.table().shape)? > limits.bytes.0 {
            return Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes));
        }
        let pointer = initial.as_ptr();
        let owner = Self {
            published: AtomicPtr::new(std::ptr::null_mut()),
            ledger: Mutex::new(TableLedger {
                current: initial,
                retired: Vec::new(),
                #[cfg(any(test, feature = "test-support"))]
                test_limits: TestTableLimits::Production,
            }),
            limits,
        };
        // Ownership and initialization precede even the initial publication.
        owner.published.store(pointer, Ordering::Release);
        Ok(owner)
    }

    /// Borrow the publication slot while the era retains all predecessors.
    /// The view cannot be sent away from the bounded activation that owns it.
    pub(crate) fn publication(&self) -> PublicationView<'_> {
        PublicationView {
            slot: &self.published,
            owner: PhantomData,
        }
    }

    pub(crate) fn view(&self) -> TableView<'_> {
        let pointer = self.published.load(Ordering::Acquire);
        // SAFETY: construction publishes a fully initialized owned allocation
        // before returning Self. Every replacement is ledger-owned before its
        // Release store; predecessors stay owned until Self is dropped. This
        // borrow prevents that Drop, including while the metadata lock poisons.
        let table = unsafe { &*pointer };
        TableView {
            table,
            owner: PhantomData,
        }
    }

    pub(crate) fn stats(&self) -> Result<TableStatistics, TablePublicationError> {
        let ledger = match self.ledger.try_lock() {
            Ok(ledger) => ledger,
            Err(TryLockError::WouldBlock) => return Err(TablePublicationError::Busy),
            Err(TryLockError::Poisoned(_)) => return Err(TablePublicationError::Poisoned),
        };
        Ok(TableStatistics {
            generation: ledger.current.table().generation,
            retired: RetiredTableCount(ledger.retired.len()),
            retained_bytes: ledger.retained_bytes()?,
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
        let mut ledger = match self.ledger.try_lock() {
            Ok(ledger) => ledger,
            Err(TryLockError::WouldBlock) => return Err(TablePublicationError::Busy),
            Err(TryLockError::Poisoned(_)) => return Err(TablePublicationError::Poisoned),
        };
        ledger.test_limits = limits;
        Ok(())
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
        let ledger = match self.ledger.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let owned = std::iter::once(&ledger.current)
            .chain(ledger.retired.iter())
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
    }

    pub(crate) fn try_prepare(&self, shape: SymbolTableShape) -> TablePreparationAccounting<'_> {
        let ledger = match self.ledger.try_lock() {
            Ok(ledger) => ledger,
            Err(TryLockError::WouldBlock) => {
                return TablePreparationAccounting {
                    result: Err(TablePublicationError::Busy),
                    charged: RetainedTableBytes(0),
                };
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                drop(poisoned.into_inner());
                return TablePreparationAccounting {
                    result: Err(TablePublicationError::Poisoned),
                    charged: RetainedTableBytes(0),
                };
            }
        };
        let initial_capacity = ledger.capacity_bytes();
        let prepared = self.prepare_locked(ledger, shape);
        match prepared {
            Ok(prepared) => {
                let charged = RetainedTableBytes(
                    prepared
                        .ledger
                        .capacity_bytes()
                        .0
                        .saturating_sub(initial_capacity.0),
                );
                TablePreparationAccounting {
                    result: Ok(prepared),
                    charged,
                }
            }
            Err((error, capacity)) => TablePreparationAccounting {
                result: Err(error),
                charged: RetainedTableBytes(capacity.0.saturating_sub(initial_capacity.0)),
            },
        }
    }

    fn prepare_locked<'owner>(
        &'owner self,
        mut ledger: MutexGuard<'owner, TableLedger>,
        shape: SymbolTableShape,
    ) -> Result<PreparedTablePublication<'owner>, (TablePublicationError, RetainedTableBytes)> {
        let limits = self.limits;
        #[cfg(any(test, feature = "test-support"))]
        let limits = match ledger.test_limits {
            TestTableLimits::Production => limits,
            TestTableLimits::Override(limits) => limits,
        };
        let prepare = (|| {
            let current = ledger.current.table();
            if shape.chunk_slots != current.shape.chunk_slots
                || shape.chunks.0 < current.shape.chunks.0
                || shape.slots.0 < current.shape.slots.0
            {
                return Err(TablePublicationError::Table(TableError::Shape));
            }
            if shape == current.shape {
                return Ok(Replacement::Unchanged);
            }
            let generation = current.generation.checked_successor()?;
            if ledger.retired.len() >= limits.retired.0 {
                return Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables));
            }
            // Reserve BEFORE the backing may mutate. A successful reserve is
            // retained and charged even if a later byte limit refuses growth.
            ledger
                .retired
                .try_reserve(1)
                .map_err(|_| TablePublicationError::Table(TableError::Capacity))?;
            let needed = ledger
                .retained_bytes()?
                .0
                .checked_add(table_bytes(shape)?)
                .ok_or(TablePublicationError::Table(TableError::Capacity))?;
            if needed > limits.bytes.0 {
                return Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes));
            }
            Ok(Replacement::Prepared(PreparedSymbolTable::new(
                shape, generation,
            )?))
        })();
        match prepare {
            Ok(replacement) => Ok(PreparedTablePublication {
                owner: self,
                ledger,
                replacement,
            }),
            Err(error) => Err((error, ledger.capacity_bytes())),
        }
    }
}

/// Transparent borrowed atomic slot for consumers of the explicit descriptor
/// ABI. Its representation is one reference; no mutable slot or chunk owner
/// escapes. Generated consumers must use an Acquire-or-stronger atomic load.
#[repr(transparent)]
#[must_use = "retain this borrow until every descriptor use returns"]
#[doc(hidden)]
pub struct PublicationView<'owner> {
    slot: &'owner AtomicPtr<PublishedSymbolTable>,
    owner: PhantomData<std::rc::Rc<()>>,
}
impl PublicationView<'_> {
    pub(crate) fn load(&self) -> TableView<'_> {
        let pointer = self.slot.load(Ordering::Acquire);
        // SAFETY: only PublishedSymbolTables constructs this borrow. It
        // publishes initialized owners before Release and retains every
        // predecessor until all view/activation borrows have ended.
        TableView {
            table: unsafe { &*pointer },
            owner: PhantomData,
        }
    }
}
const _: () = assert!(size_of::<PublicationView<'static>>() == size_of::<*const ()>());
static_assertions::assert_not_impl_any!(PublicationView<'static>: Send, Sync, Clone, Copy);

/// A view never outlives the owner and never loans chunk ownership.
pub(crate) struct TableView<'owner> {
    table: &'owner PublishedSymbolTable,
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
    owner: &'owner PublishedSymbolTables,
    ledger: MutexGuard<'owner, TableLedger>,
    replacement: Replacement,
}

impl PreparedTablePublication<'_> {
    /// Address conversion and initialization allocate nothing. The caller must
    /// already hold the independent backing WRITER authority for this commit.
    pub(crate) fn publish(
        mut self,
        addresses: impl IntoIterator<Item = SymbolChunkAddress>,
    ) -> Result<PublicationChange, TablePublicationError> {
        let next = match self.replacement {
            Replacement::Unchanged => {
                let mut addresses = addresses.into_iter();
                for old in self.ledger.current.table().chunks.as_slice() {
                    if addresses.next() != Some(*old) {
                        return Err(TablePublicationError::Table(TableError::Shape));
                    }
                }
                if addresses.next().is_some() {
                    return Err(TablePublicationError::Table(TableError::Shape));
                }
                return Ok(PublicationChange::Unchanged {
                    generation: self.ledger.current.table().generation,
                });
            }
            Replacement::Prepared(prepared) => prepared.initialize(addresses)?,
        };
        if !next
            .table()
            .chunks
            .as_slice()
            .starts_with(self.ledger.current.table().chunks.as_slice())
        {
            return Err(TablePublicationError::Table(TableError::Shape));
        }
        let charged = RetainedTableBytes(table_bytes(next.table().shape)?);
        let pointer = next.as_ptr();
        let generation = next.table().generation;
        let previous = std::mem::replace(&mut self.ledger.current, next);
        // Preparation reserved this slot. No owner is dropped or allocated on
        // this path, and both allocations are owned before Release publication.
        self.ledger.retired.push(previous);
        self.owner.published.store(pointer, Ordering::Release);
        Ok(PublicationChange::Published {
            generation,
            charged,
        })
    }
}

static_assertions::assert_impl_all!(PublishedSymbolTables: Send, Sync);
static_assertions::assert_not_impl_any!(TableView<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PreparedTablePublication<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
