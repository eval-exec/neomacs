//! The per-heap table of roots held by [`SharedRoot`](super::SharedRoot)s.
//!
//! A root cell is one atomic word. Registration (on the heap's mutator, at the
//! sharing boundary) and the collector's root scan hold the table's cell lock.
//! Retirement, when the last clone of a shared root drops on any thread, is one
//! atomic store: it never locks, blocks or panics. The next registration that
//! finds no free cell recycles retired ones.
//!
//! The collector seeds every live cell together with the heap's other runtime
//! roots, at cycle start and again at the concurrent termination's re-seed, so
//! a root registered or retired while a mark runs is handled like any other
//! runtime root: a retired object floats at most one cycle.
//!
//! Tables live in a process-wide registry keyed by [`HeapIdentity`] rather than
//! inside the heap or its Context, whose layouts compiled code addresses. The
//! registry holds only weak references; leases own their table, so a table
//! (and its chunks) is freed after its last root retires, and identities are
//! never reused, so a table can outlive its heap without aliasing a later one.
//! A counted registration lives with every table, including a table retained
//! temporarily by a collector's Arc. When that count is zero, the root walk
//! can skip both lazy registry initialization and its mutex.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, TryLockError, Weak};

use rustc_hash::FxHashMap;

use crate::tagged::gc::HeapIdentity;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;

use super::root_batch::BatchSlots;

/// Cells per chunk. Chunks are boxed, so a cell never moves once handed out.
const CHUNK_CELLS: usize = 64;

/// Number of tables whose storage is still owned. Only registration tokens
/// can change it, so a live table (and therefore every live root lease) keeps
/// it nonzero. An empty count never requires consulting the registry.
#[derive(Debug, Default)]
#[repr(transparent)]
struct LiveTableCount(AtomicUsize);

static_assertions::assert_impl_all!(LiveTableCount: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(LiveTableCount: Clone, Copy);
static_assertions::assert_eq_size!(LiveTableCount, AtomicUsize);
const _: () = {
    assert!(std::mem::align_of::<LiveTableCount>() == std::mem::align_of::<AtomicUsize>());
    assert!(std::mem::offset_of!(LiveTableCount, 0) == 0);
};

impl LiveTableCount {
    const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    fn register(&self) -> TableRegistration<'_> {
        // Publish the count before a table can enter the registry or hand out
        // a lease. The count cannot wrap: each registration owns a distinct
        // allocated table, whose storage exhausts the address space first.
        self.0.fetch_add(1, Ordering::Release);
        TableRegistration(self)
    }

    fn is_empty(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }

    /// Keep registry initialization and locking behind the absence check.
    /// The closure is monomorphized and inlined into the root walker.
    #[inline(always)]
    fn with_registered_tables(&self, scan: impl FnOnce()) {
        if !self.is_empty() {
            scan();
        }
    }
}

/// One counted table lifetime. Moving the token transfers that lifetime;
/// cloning it would make retirement decrement twice and is forbidden.
#[must_use = "dropping a table registration ends its counted lifetime"]
#[derive(Debug)]
#[repr(transparent)]
struct TableRegistration<'a>(&'a LiveTableCount);

static_assertions::assert_impl_all!(TableRegistration<'static>: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(TableRegistration<'static>: Clone, Copy);
static_assertions::assert_eq_size!(TableRegistration<'static>, &LiveTableCount);
const _: () = {
    assert!(
        std::mem::align_of::<TableRegistration<'static>>()
            == std::mem::align_of::<&LiveTableCount>()
    );
    assert!(std::mem::offset_of!(TableRegistration<'static>, 0) == 0);
};

impl Drop for TableRegistration<'_> {
    fn drop(&mut self) {
        // Exactly one nonblocking atomic operation: no mutex, allocation,
        // callback or panic. The table's last Arc owner ends this lifetime.
        self.0.0.fetch_sub(1, Ordering::Release);
    }
}

static LIVE_TABLES: LiveTableCount = LiveTableCount::new();

/// The word of a value the collector traces from a root: a heap object, or a
/// symbol other than `nil` and `t` (an uninterned symbol's cells survive only
/// while something marks it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TracedWord(usize);

static_assertions::assert_impl_all!(TracedWord: Send, Sync, Copy, std::fmt::Debug);
const _: () = {
    assert!(std::mem::size_of::<TracedWord>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<TracedWord>() == std::mem::align_of::<usize>());
    assert!(std::mem::offset_of!(TracedWord, 0) == 0);
};

impl TracedWord {
    /// `value`'s word, when the collector traces it from a root.
    pub(super) fn of(value: TaggedValue) -> Option<Self> {
        let traced =
            value.is_heap_object() || (value.is_symbol() && !value.is_nil() && !value.is_t());
        traced.then_some(Self(value.bits()))
    }

    /// The local value. The caller is the owning heap's mutator or collector.
    pub(super) fn value(self) -> TaggedValue {
        TaggedValue::from_bits(self.0)
    }
}

/// What one root cell holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellState {
    /// Never handed out, or recycled into the free list.
    Vacant,
    /// Its lease ended; recycled by the next registration that needs a cell.
    Retired,
    /// Owned by a live lease.
    Live(TracedWord),
}

impl CellState {
    // A traced word is never `nil` or `t`, so their words encode the two
    // states that hold no root.
    const VACANT: usize = TaggedValue::NIL.0;
    const RETIRED: usize = TaggedValue::T.0;

    fn decode(word: usize) -> Self {
        match word {
            Self::VACANT => Self::Vacant,
            Self::RETIRED => Self::Retired,
            live => Self::Live(TracedWord(live)),
        }
    }

    fn encode(self) -> usize {
        match self {
            Self::Vacant => Self::VACANT,
            Self::Retired => Self::RETIRED,
            Self::Live(word) => word.0,
        }
    }
}

#[repr(transparent)]
struct RootCell(AtomicUsize);

impl RootCell {
    fn vacant() -> Self {
        Self(AtomicUsize::new(CellState::VACANT))
    }

    fn load(&self) -> CellState {
        CellState::decode(self.0.load(Ordering::Acquire))
    }

    fn store(&self, state: CellState) {
        self.0.store(state.encode(), Ordering::Release);
    }

    /// Turn a retired cell vacant; false when it holds anything else.
    fn reclaim(&self) -> bool {
        self.0
            .compare_exchange(
                CellState::RETIRED,
                CellState::VACANT,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    }
}

type Chunk = [RootCell; CHUNK_CELLS];

/// Position of a cell: `chunk * CHUNK_CELLS + offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CellIndex(usize);

/// The cell storage, guarded by the table lock.
#[derive(Default)]
pub(super) struct CellAllocator {
    chunks: Vec<Box<Chunk>>,
    /// Vacant cells below `used`, ready for reuse.
    free: Vec<CellIndex>,
    /// Cells handed out from chunk space at least once.
    used: usize,
    /// Grouped roots are dormant until the first explicit batch reservation.
    /// They share this allocator's lock and collector walk with single cells.
    pub(super) batches: Option<BatchSlots>,
}

impl CellAllocator {
    fn cell(&self, index: CellIndex) -> &RootCell {
        &self.chunks[index.0 / CHUNK_CELLS][index.0 % CHUNK_CELLS]
    }

    fn cells(&self) -> impl Iterator<Item = &RootCell> {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.iter())
            .take(self.used)
    }

    /// A vacant cell, recycling retired cells first when the free list is
    /// empty and some lease has retired since the last recycling pass.
    fn take_vacant(&mut self, retired: &AtomicUsize) -> CellIndex {
        if self.free.is_empty() && retired.swap(0, Ordering::Acquire) > 0 {
            let reclaimed: Vec<CellIndex> = self
                .cells()
                .enumerate()
                .filter(|(_, cell)| cell.reclaim())
                .map(|(index, _)| CellIndex(index))
                .collect();
            self.free = reclaimed;
        }
        if let Some(index) = self.free.pop() {
            return index;
        }
        if self.used == self.chunks.len() * CHUNK_CELLS {
            self.chunks
                .push(Box::new(std::array::from_fn(|_| RootCell::vacant())));
        }
        let index = CellIndex(self.used);
        self.used += 1;
        index
    }
}

/// One heap's shared roots.
pub(super) struct RootTable {
    pub(super) heap: HeapIdentity,
    cells: Mutex<CellAllocator>,
    /// Retirements since the last recycling pass: a hint that one may find
    /// cells, never a count of them.
    retired: AtomicUsize,
    /// Count the table through its last Arc owner, including a collector
    /// retaining it after the final root lease retires. Not JIT-visible.
    _registration: TableRegistration<'static>,
}

static_assertions::assert_impl_all!(RootTable: Send, Sync, std::fmt::Debug);

impl RootTable {
    fn new(heap: HeapIdentity, registration: TableRegistration<'static>) -> Self {
        Self {
            heap,
            cells: Mutex::new(CellAllocator::default()),
            retired: AtomicUsize::new(0),
            _registration: registration,
        }
    }

    /// The cell lock. Single-cell allocation failure leaves at worst a vacant
    /// cell outside the free list. Batch allocation/checks precede reservation
    /// installation; installed payloads are immutable and cancellation only
    /// changes typed atomic metadata. Every intermediate state remains a valid
    /// root walk, so poison does not justify discarding any registered roots.
    pub(super) fn lock(&self) -> MutexGuard<'_, CellAllocator> {
        self.cells.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Nonblocking retirement admission. Poison retains the same structurally
    /// valid storage as `lock`; contention leaves every registration intact.
    pub(super) fn try_lock(&self) -> Option<MutexGuard<'_, CellAllocator>> {
        match self.cells.try_lock() {
            Ok(cells) => Some(cells),
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }
}

impl std::fmt::Debug for RootTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootTable")
            .field("heap", &self.heap)
            .field("retired", &self.retired.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// Ownership of one live root cell; dropping it retires the cell.
pub(super) struct RootLease {
    table: Arc<RootTable>,
    cell: NonNull<RootCell>,
}

// SAFETY: `cell` points into a boxed chunk of `table`'s allocator. Chunks are
// freed only when the table drops, which the `table` Arc prevents for the
// lease's whole life, and they never move. The lease touches the cell only
// through atomic loads and stores, so sharing or sending it is sound.
unsafe impl Send for RootLease {}
// SAFETY: see `Send`; `&RootLease` exposes only atomic loads of the cell.
unsafe impl Sync for RootLease {}

static_assertions::assert_impl_all!(RootLease: Send, Sync);

impl RootLease {
    /// Root `word` in `heap`'s table.
    pub(super) fn register(heap: HeapIdentity, word: TracedWord) -> Self {
        let table = table_for(heap);
        let cell = {
            let mut cells = table.lock();
            let index = cells.take_vacant(&table.retired);
            let cell = cells.cell(index);
            cell.store(CellState::Live(word));
            NonNull::from(cell)
        };
        Self { table, cell }
    }

    fn cell(&self) -> &RootCell {
        // SAFETY: the chunk outlives `self` (see the `Send` impl).
        unsafe { self.cell.as_ref() }
    }

    /// The rooted word. Only this lease's drop changes the cell, so it holds
    /// the registered word for as long as `&self` lives.
    pub(super) fn word(&self) -> TracedWord {
        TracedWord(self.cell().0.load(Ordering::Acquire))
    }
}

impl Drop for RootLease {
    fn drop(&mut self) {
        // One store and one counter bump: no lock, no allocation, no panic.
        self.cell().store(CellState::Retired);
        self.table.retired.fetch_add(1, Ordering::Release);
    }
}

impl std::fmt::Debug for RootLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootLease")
            .field("heap", &self.table.heap)
            .field("word", &format_args!("{:#x}", self.word().0))
            .finish()
    }
}

/// Weak references to every live table, by heap.
static ROOT_TABLES: LazyLock<Mutex<FxHashMap<HeapIdentity, Weak<RootTable>>>> =
    LazyLock::new(Default::default);

/// The registry lock. Its map is consistent between statements, so poison
/// from an unrelated panic is ignored.
fn root_tables() -> MutexGuard<'static, FxHashMap<HeapIdentity, Weak<RootTable>>> {
    ROOT_TABLES.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn table_for(heap: HeapIdentity) -> Arc<RootTable> {
    let mut tables = root_tables();
    if let Some(table) = tables.get(&heap).and_then(Weak::upgrade) {
        return table;
    }
    tables.retain(|_, table| table.strong_count() > 0);
    let table = Arc::new(RootTable::new(heap, LIVE_TABLES.register()));
    tables.insert(heap, Arc::downgrade(&table));
    table
}

fn existing_table(heap: HeapIdentity) -> Option<Arc<RootTable>> {
    root_tables().get(&heap).and_then(Weak::upgrade)
}

/// Append every live shared root of `heap` to `out`, for the collector's root
/// walk on that heap's mutator.
pub(crate) fn collect_shared_root_gc_roots(heap: &TaggedHeap, out: &mut Vec<TaggedValue>) {
    // Completed admission precedes this heap's world-stopped root capture;
    // its table registration keeps the count nonzero. A concurrent admission
    // after this observation is governed by the same start-root/SATB/birth
    // and termination re-seed requirements as admission after the locked
    // scan below. This is an absence check, not a new root snapshot protocol.
    LIVE_TABLES.with_registered_tables(|| {
        let Some(table) = existing_table(heap.heap_identity()) else {
            return;
        };
        let cells = table.lock();
        out.extend(cells.cells().filter_map(|cell| match cell.load() {
            CellState::Live(word) => Some(word.value()),
            CellState::Vacant | CellState::Retired => None,
        }));
        if let Some(batches) = &cells.batches {
            batches.collect_roots(heap, out);
        }
    });
}

/// Live and retired-but-unrecycled cells of `heap`'s table.
#[cfg(test)]
pub(super) fn cell_census(heap: HeapIdentity) -> (usize, usize) {
    let Some(table) = existing_table(heap) else {
        return (0, 0);
    };
    let cells = table.lock();
    cells
        .cells()
        .fold((0, 0), |(live, retired), cell| match cell.load() {
            CellState::Live(_) => (live + 1, retired),
            CellState::Retired => (live, retired + 1),
            CellState::Vacant => (live, retired),
        })
}

#[cfg(test)]
#[path = "root_table_tests.rs"]
mod tests;
