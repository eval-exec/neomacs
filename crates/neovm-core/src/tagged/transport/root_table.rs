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

use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, Weak};

use rustc_hash::FxHashMap;

use crate::tagged::gc::HeapIdentity;
use crate::tagged::value::TaggedValue;

/// Cells per chunk. Chunks are boxed, so a cell never moves once handed out.
const CHUNK_CELLS: usize = 64;

/// The word of a value the collector traces from a root: a heap object, or a
/// symbol other than `nil` and `t` (an uninterned symbol's cells survive only
/// while something marks it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TracedWord(usize);

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
struct CellAllocator {
    chunks: Vec<Box<Chunk>>,
    /// Vacant cells below `used`, ready for reuse.
    free: Vec<CellIndex>,
    /// Cells handed out from chunk space at least once.
    used: usize,
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
    heap: HeapIdentity,
    cells: Mutex<CellAllocator>,
    /// Retirements since the last recycling pass: a hint that one may find
    /// cells, never a count of them.
    retired: AtomicUsize,
}

impl RootTable {
    fn new(heap: HeapIdentity) -> Self {
        Self {
            heap,
            cells: Mutex::new(CellAllocator::default()),
            retired: AtomicUsize::new(0),
        }
    }

    /// The cell lock. A panic while it was held (allocation failure) leaves
    /// at worst one vacant cell outside the free list, so poison is ignored.
    fn lock(&self) -> MutexGuard<'_, CellAllocator> {
        self.cells.lock().unwrap_or_else(PoisonError::into_inner)
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

fn table_for(heap: HeapIdentity) -> Arc<RootTable> {
    let mut tables = root_tables();
    if let Some(table) = tables.get(&heap).and_then(Weak::upgrade) {
        return table;
    }
    tables.retain(|_, table| table.strong_count() > 0);
    let table = Arc::new(RootTable::new(heap));
    tables.insert(heap, Arc::downgrade(&table));
    table
}

fn existing_table(heap: HeapIdentity) -> Option<Arc<RootTable>> {
    root_tables().get(&heap).and_then(Weak::upgrade)
}

/// Append every live shared root of `heap` to `out`, for the collector's root
/// walk on that heap's mutator.
pub(crate) fn collect_shared_root_gc_roots(heap: HeapIdentity, out: &mut Vec<TaggedValue>) {
    let Some(table) = existing_table(heap) else {
        return;
    };
    let cells = table.lock();
    out.extend(cells.cells().filter_map(|cell| match cell.load() {
        CellState::Live(word) => Some(word.value()),
        CellState::Vacant | CellState::Retired => None,
    }));
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
