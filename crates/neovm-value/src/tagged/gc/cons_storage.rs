//! Cold cons catalog ownership for admitted mutators.
//!
//! The serial heap is an inert shell while this catalog owns its cons spine
//! and residual-Box list/catalog (the latter admits hash-table shells only).
//! Only disjoint raw cell grants leave the gate: neither a heap reference nor
//! a vector/header reference is retained by a running owner. Collection first
//! closes every region and restores the spine under stopped authority.

use super::card_table::CardSlots;
use super::chunk_map::{ChunkClass, ChunkEntry, ChunkMap};
use super::cons_allocator::{ConsAllocatorOwner, ConsBlockBase};
use super::cons_block_trailer::CONS_BLOCK_SIZE;
use super::cons_blocks::ConsBlock;
use super::mutator_registration::{MutatorKey, RegistrationError};
use super::young_registry::YoungBlockLog;
use super::{ConsCell, HeapIdentity, TaggedHeap};
use crate::tagged::header::{GcHeader, HashTableObj, MarkParity};
use crate::tagged::value::TaggedValue;
use rustc_hash::{FxHashMap, FxHashSet};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const MAX_GRANT: usize = 128;

/// Whole-tail batch lending target: two cons blocks per catalog acquisition.
/// Publication at ACK/native stop returns the unhanded suffix, so a batch is
/// handed back at the latest at the next stop; between stops a batch only
/// refills when its owner actually exhausted every lent tail.
pub(super) const TAIL_BATCH_CELLS: usize = 2 * CONS_BLOCK_SIZE;

/// Free-list fragmentation bound: one batch gate takes at most this many
/// runs, bounding the critical section on a fragmented heap.
const TAIL_BATCH_MAX_RUNS: usize = 64;

struct CatalogIdentity {
    abandoned: AtomicBool,
}

/// Distinguishes refill diagnostics without exposing a catalog index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CatalogSource {
    FreeList,
    BlockTail,
    FreshBlock,
}

/// Confined region; forgetting it leaves the catalog's active count charged.
/// Dropping it unclosed marks the catalog terminal and retains its backing.
#[must_use = "close the region through its original catalog and owner"]
pub(super) struct CatalogRegion {
    identity: Arc<CatalogIdentity>,
    owner: MutatorKey,
    first: NonNull<ConsCell>,
    total: usize,
    used: usize,
    source: CatalogSource,
    active: bool,
    _thread: PhantomData<Rc<()>>,
}
impl std::fmt::Debug for CatalogRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogRegion")
            .field("owner", &self.owner)
            .field("total", &self.total)
            .field("used", &self.used)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
impl CatalogRegion {
    pub(super) fn total(&self) -> usize {
        self.total
    }
    pub(super) fn unused(&self) -> usize {
        self.total - self.used
    }
    pub(super) fn empty(&self) -> bool {
        self.used == self.total
    }
    pub(super) fn source(&self) -> CatalogSource {
        self.source
    }

    #[cfg(test)]
    pub(super) fn bounds_for_test(&self) -> (usize, usize, usize) {
        // SAFETY: the exclusive run includes every cell through total.
        let end = unsafe { self.first.as_ptr().add(self.total) };
        (self.first.as_ptr().addr(), end.addr(), self.unused())
    }

    /// Admit a finite exclusive run once at slice entry/refill. Poison retains
    /// backing; it never revokes a region already lent to a running slice.
    pub(super) fn slice_bounds(
        &self,
        catalog: &ConsCatalog,
        owner: MutatorKey,
    ) -> Result<(*mut ConsCell, *mut ConsCell), RegistrationError> {
        catalog.check_live()?;
        if !Arc::ptr_eq(&self.identity, &catalog.identity) || self.owner != owner {
            return Err(RegistrationError::StaleRegistration);
        }
        if !self.active || self.used > self.total {
            return Err(RegistrationError::OutstandingLoan);
        }
        // SAFETY: both offsets are within this retained, exclusive allocation.
        Ok(unsafe {
            (
                self.first.as_ptr().add(self.used),
                self.first.as_ptr().add(self.total),
            )
        })
    }

    /// # Safety
    /// cursor comes from this run and includes only fully initialized/rooted
    /// cells. Commit before any close, refund or publication. No heap borrow.
    pub(super) unsafe fn commit_slice_cursor(&mut self, cursor: *mut ConsCell) {
        // SAFETY: slice admission preserves this allocation's provenance.
        let used = unsafe { cursor.offset_from(self.first.as_ptr()) } as usize;
        debug_assert!(used >= self.used && used <= self.total);
        self.used = used;
    }

    /// Retire a region closed through its owner's local tail rewind: the
    /// lending tail already owns the unused suffix, so dropping the region
    /// must not mark the catalog abandoned.
    pub(super) fn retire(&mut self) {
        self.active = false;
    }
}
impl Drop for CatalogRegion {
    fn drop(&mut self) {
        if self.active {
            self.identity.abandoned.store(true, Ordering::Release);
        }
    }
}
static_assertions::assert_not_impl_any!(CatalogRegion: Send, Sync, Clone, Copy);

/// One whole block tail or free-list run lent to a single owner for lock-free
/// local region carving. Only the owning thread moves `cursor`; the unhanded
/// suffix `[cursor, limit)` returns under the catalog gate at publication
/// (ACK/native stop/retirement). Cells below `cursor` stay exclusively the
/// owner's until then, so no other grant can overlap them.
#[must_use = "publish every tail through its original catalog before stopping"]
pub(super) struct OwnedConsTail {
    identity: Arc<CatalogIdentity>,
    /// Base address of the lending block; the lent range never spans blocks.
    base: usize,
    /// First index of the lent range.
    start: u16,
    /// First index never handed to a region.
    cursor: u16,
    /// End of the lent range (exclusive).
    limit: u16,
    /// The range came from the block's bump top: publication may rewind
    /// `next_index` over the suffix instead of free-listing it.
    from_tail: bool,
    source: CatalogSource,
    /// Only the owning thread moves `cursor`; this keeps the auto traits
    /// from claiming otherwise, exactly like an open region.
    _thread: PhantomData<Rc<()>>,
}
impl OwnedConsTail {
    pub(super) fn exhausted(&self) -> bool {
        self.cursor == self.limit
    }

    pub(super) fn source(&self) -> CatalogSource {
        self.source
    }

    /// The front carving position and the exclusive end, in cell indexes.
    pub(super) fn bounds(&self) -> (u16, u16) {
        (self.cursor, self.limit)
    }

    /// Carve up to `want` cells as one exclusive region, advancing the local
    /// cursor. No shared lock is taken: the caller owns this tail.
    pub(super) fn carve(&mut self, owner: MutatorKey, want: usize) -> CatalogRegion {
        let avail = self.limit - self.cursor;
        let count = want.min(avail as usize);
        debug_assert_ne!(count, 0, "caller refills on exhaustion");
        let start = self.cursor;
        self.cursor = start + count as u16;
        // SAFETY: this tail's block is registered and live for this era, and
        // the carved range is exclusively this owner's until publication.
        let first = unsafe { ConsBlock::cell_ptr_at(self.base, start as usize) };
        CatalogRegion {
            identity: Arc::clone(&self.identity),
            owner,
            first: NonNull::new(first).expect("nonempty cons run"),
            total: count,
            used: 0,
            source: self.source,
            active: true,
            _thread: PhantomData,
        }
    }

    /// Match a region carved from this tail and rewind the cursor over its
    /// unused suffix, answering that suffix's cell count. The match is exact:
    /// the region's range must end at this tail's cursor and begin at or
    /// after this tail's start, which only the carving tail satisfies
    /// because lent ranges of one block are disjoint.
    pub(super) fn rewind_region(&mut self, region: &CatalogRegion) -> Option<usize> {
        let base = ConsBlock::block_base_for_ptr(region.first.as_ptr());
        if self.base != base {
            return None;
        }
        let start = ConsBlock::index_of_ptr(region.first.as_ptr());
        let end = start + region.total;
        if self.cursor as usize != end || start < self.start as usize {
            return None;
        }
        let unused = region.unused();
        self.cursor = (start + region.used) as u16;
        Some(unused)
    }
}
impl Drop for OwnedConsTail {
    fn drop(&mut self) {
        if !self.exhausted() {
            // An un-published tail leaves the catalog's bump tops and free
            // lists incoherent; only abandonment retains a sound picture.
            self.identity.abandoned.store(true, Ordering::Release);
        }
    }
}
static_assertions::assert_not_impl_any!(OwnedConsTail: Send, Sync, Clone, Copy);

struct CatalogState {
    allocator: ConsAllocatorOwner,
    blocks: Vec<ConsBlock>,
    by_base: FxHashMap<usize, usize>,
    cards: Option<CardSlots>,
    chunk_map: Option<Arc<ChunkMap>>,
    live_count: usize,
    active_regions: usize,
    generational: bool,
    /// Detached together with the cons spine, before the whole-heap loan is
    /// suspended. Only new, unpublished hash-table headers are prepended.
    all_objects: *mut GcHeader,
    non_cons_object_addrs: FxHashSet<usize>,
    /// No collection or marker runs while installed. Every resume snapshots
    /// the collector's updated parity before owners acquire another run.
    mark_parity: MarkParity,
    /// Detect replacement of the inert shell's unique placeholder allocator.
    placeholder: usize,
    #[cfg(test)]
    source_grants: [usize; 3],
}
// SAFETY: ownership of block allocations moves with this state; raw pointers
// are not Rust references into another owner's cells. The catalog mutex guards
// every spine/cursor mutation and the admitted allocator guards free links.
// Outside the gate only exclusive CatalogRegions can touch their own cells.
// The raw residual-Box list also moves as owned storage; only unpublished
// headers are changed, under the same gate. No heap/Box reference is retained.
// Storage is never freed until stopped restoration or is retained on failure.
unsafe impl Send for CatalogState {}

/// Era-local catalog identity. This is not a cloneable heap facade.
/// The canonical registry validates keys/log binding before calling grant.
/// No method takes the registry, root-table, World, or worker-registry lock.
pub(super) struct ConsCatalog {
    heap: HeapIdentity,
    identity: Arc<CatalogIdentity>,
    state: Mutex<Option<CatalogState>>,
    /// Installation flag for slice admission, set under the gate at install
    /// and cleared under the gate at restore. A live RunAccess loan excludes
    /// stopped restoration of this catalog, so an acquire load inside a slice
    /// can never observe a torn install/restore transition: no mutex needed.
    installed: AtomicBool,
    /// Card-table base snapshotted at install. Outside the catalog mutex so a
    /// cons field store can mark a card without the heap loan or the gate.
    /// Zero means this era has no generational table. Not cleared on restore:
    /// owners are stopped then, and the next install refreshes it.
    card_base: AtomicUsize,
}
impl std::fmt::Debug for ConsCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsCatalog")
            .field("heap", &self.heap)
            .field(
                "abandoned",
                &self.identity.abandoned.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}
static_assertions::assert_impl_all!(ConsCatalog: Send, Sync);
static_assertions::assert_not_impl_any!(ConsCatalog: Clone, Copy);

/// Test-only accounting of catalog-mutex wait time on the cold grant/close
/// refill paths, for the A8 contention rows. Never compiled into images.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct CatalogWaits {
    pub(crate) grant_calls: std::sync::atomic::AtomicU64,
    pub(crate) grant_ns: std::sync::atomic::AtomicU64,
    pub(crate) grant_max_ns: std::sync::atomic::AtomicU64,
    pub(crate) close_calls: std::sync::atomic::AtomicU64,
    pub(crate) close_ns: std::sync::atomic::AtomicU64,
    pub(crate) close_max_ns: std::sync::atomic::AtomicU64,
}

#[cfg(test)]
pub(crate) static CATALOG_WAITS: CatalogWaits = CatalogWaits {
    grant_calls: std::sync::atomic::AtomicU64::new(0),
    grant_ns: std::sync::atomic::AtomicU64::new(0),
    grant_max_ns: std::sync::atomic::AtomicU64::new(0),
    close_calls: std::sync::atomic::AtomicU64::new(0),
    close_ns: std::sync::atomic::AtomicU64::new(0),
    close_max_ns: std::sync::atomic::AtomicU64::new(0),
};

#[cfg(test)]
pub(crate) enum CatalogOp {
    Grant,
    Close,
}

#[cfg(test)]
fn test_catalog_wait(start: std::time::Instant, op: CatalogOp) {
    use std::sync::atomic::Ordering::Relaxed;
    let nanos = start.elapsed().as_nanos() as u64;
    match op {
        CatalogOp::Grant => {
            CATALOG_WAITS.grant_calls.fetch_add(1, Relaxed);
            CATALOG_WAITS.grant_ns.fetch_add(nanos, Relaxed);
            CATALOG_WAITS.grant_max_ns.fetch_max(nanos, Relaxed);
        }
        CatalogOp::Close => {
            CATALOG_WAITS.close_calls.fetch_add(1, Relaxed);
            CATALOG_WAITS.close_ns.fetch_add(nanos, Relaxed);
            CATALOG_WAITS.close_max_ns.fetch_max(nanos, Relaxed);
        }
    }
}

impl ConsCatalog {
    #[cold]
    #[inline(never)]
    pub(super) fn new(heap: HeapIdentity) -> Self {
        Self {
            heap,
            identity: Arc::new(CatalogIdentity {
                abandoned: AtomicBool::new(false),
            }),
            state: Mutex::new(None),
            installed: AtomicBool::new(false),
            card_base: AtomicUsize::new(0),
        }
    }

    /// Two Relaxed card stores from the base captured while the heap was
    /// still exclusively owned. No catalog lock and no heap TLS.
    pub(super) fn mark_published_cons_card(&self, owner_addr: usize) {
        let base = self.card_base.load(Ordering::Acquire);
        super::card_table::mark_published(base, owner_addr);
    }

    fn publish_card_base(&self, heap: &TaggedHeap) {
        let base = if heap.generational_enabled() {
            usize::from_ne_bytes([
                heap.card_table_base_0.get(),
                heap.card_table_base_1.get(),
                heap.card_table_base_2.get(),
                heap.card_table_base_3.get(),
                heap.card_table_base_4.get(),
                heap.card_table_base_5.get(),
                heap.card_table_base_6.get(),
                heap.card_table_base_7.get(),
            ])
        } else {
            0
        };
        self.card_base.store(base, Ordering::Release);
    }

    /// Cold admission confirms installation before a slice callback can run.
    /// The live RunAccess loan excludes stopped restoration of this catalog,
    /// so the installed flag load needs no mutex.
    #[cold]
    #[inline(never)]
    pub(super) fn admit_slice(&self, owner: MutatorKey) -> Result<(), RegistrationError> {
        if owner.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if !self.installed.load(Ordering::Acquire) {
            return Err(RegistrationError::Publication);
        }
        self.check_live()
    }

    pub(super) fn check_live(&self) -> Result<(), RegistrationError> {
        if self.identity.abandoned.load(Ordering::Acquire) {
            Err(RegistrationError::Poisoned)
        } else {
            Ok(())
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Option<CatalogState>>, RegistrationError> {
        if self.identity.abandoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        let guard = match self.state.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.identity.abandoned.store(true, Ordering::Release);
                return Err(RegistrationError::Poisoned);
            }
        };
        if self.identity.abandoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        Ok(guard)
    }

    #[cold]
    #[inline(never)]
    pub(super) fn active_regions(&self) -> Result<usize, RegistrationError> {
        Ok(self
            .lock()?
            .as_ref()
            .map_or(0, |state| state.active_regions))
    }

    /// # Safety
    /// The caller owns this exact quiescent serial heap, has stopped every
    /// participant with all regions closed, excludes marker launches/reentry,
    /// and suspends the whole-heap loan before any owner runs. While installed
    /// no other code may read/write the heap's inert cons, boxed-list or
    /// non-cons address-set placeholder fields.
    #[cold]
    #[inline(never)]
    pub(super) unsafe fn install(&self, heap: &mut TaggedHeap) -> Result<(), RegistrationError> {
        if heap.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        let mut gate = self.lock()?;
        if gate.is_some() {
            return Err(RegistrationError::Busy);
        }
        if heap.alloc_regions_open() {
            return Err(RegistrationError::OpenRegion);
        }
        if heap.concurrent_mark_running
            || heap.mark_in_progress
            || heap.sweep_in_progress
            || heap.generational.major_in_progress
        {
            return Err(RegistrationError::Busy);
        }
        let placeholder = ConsAllocatorOwner::new(self.heap);
        if !heap.cons_allocator.threads_admitted() {
            // SAFETY: install already verified closed regions and stopped,
            // quiescent authority. Admit this allocator directly: no second
            // whole-heap close/admission operation is needed here.
            unsafe { heap.cons_allocator.stopped_world() }.admit_threads();
        }
        let allocator = std::mem::replace(&mut heap.cons_allocator, placeholder);
        let placeholder =
            (&*heap.cons_allocator as *const super::cons_allocator::ConsAllocator) as usize;
        let state = CatalogState {
            allocator,
            blocks: std::mem::take(&mut heap.cons_blocks),
            by_base: std::mem::take(&mut heap.cons_block_index_by_base),
            cards: heap.card_slots_mut().map(std::mem::take),
            chunk_map: heap.chunk_map.as_ref().map(|map| Arc::clone(map.shared())),
            live_count: std::mem::take(&mut heap.cons_live_count),
            active_regions: 0,
            generational: heap.generational.enabled,
            all_objects: std::mem::replace(&mut heap.all_objects, std::ptr::null_mut()),
            non_cons_object_addrs: std::mem::take(&mut heap.non_cons_object_addrs),
            mark_parity: heap.mark_parity,
            placeholder,
            #[cfg(test)]
            source_grants: [0; 3],
        };
        *gate = Some(state);
        self.installed.store(true, Ordering::Release);
        self.publish_card_base(heap);
        Ok(())
    }

    /// # Safety
    /// The matching all-stopped transition excludes every running/initializing
    /// owner, worker and callback. All owner regions are closed. The inert heap
    /// placeholders have not been accessed or changed since install.
    #[cold]
    #[inline(never)]
    pub(super) unsafe fn restore(&self, heap: &mut TaggedHeap) -> Result<(), RegistrationError> {
        if heap.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        let mut gate = self.lock()?;
        let state = gate.as_ref().ok_or(RegistrationError::Publication)?;
        if state.active_regions != 0 {
            return Err(RegistrationError::OpenRegion);
        }
        let placeholder =
            (&*heap.cons_allocator as *const super::cons_allocator::ConsAllocator) as usize;
        let same_map = match (&state.chunk_map, &heap.chunk_map) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b.shared()),
            _ => false,
        };
        if placeholder != state.placeholder
            || !heap.cons_blocks.is_empty()
            || heap.cons_blocks.capacity() != 0
            || !heap.cons_block_index_by_base.is_empty()
            || heap.cons_block_index_by_base.capacity() != 0
            || heap.cons_live_count != 0
            || !heap.all_objects.is_null()
            || !heap.non_cons_object_addrs.is_empty()
            || heap.non_cons_object_addrs.capacity() != 0
            || heap.mark_parity != state.mark_parity
            || heap.cons_allocator.threads_admitted()
            || !heap.cons_allocator.free_counts().is_empty()
            || heap.generational.enabled != state.generational
            || !same_map
            || heap.card_slots().is_some() != state.cards.is_some()
            || heap.card_slots().is_some_and(|slots| !slots.is_empty())
        {
            return Err(RegistrationError::Publication);
        }
        let state = gate.take().expect("validated installed catalog");
        self.installed.store(false, Ordering::Release);
        heap.cons_allocator = state.allocator;
        heap.cons_blocks = state.blocks;
        heap.cons_block_index_by_base = state.by_base;
        heap.cons_live_count = state.live_count;
        heap.all_objects = state.all_objects;
        heap.non_cons_object_addrs = state.non_cons_object_addrs;
        if let Some(cards) = state.cards {
            *heap.card_slots_mut().expect("validated card carrier") = cards;
        }
        Ok(())
    }

    /// Publish one fully initialized, privately owned table shell into the
    /// detached serial list/catalog. The caller retains a live RunAccess and
    /// immediately installs the returned word into its prevalidated root home;
    /// no callback, safepoint or other heap borrow intervenes.
    #[cold]
    #[inline(never)]
    pub(super) fn link_hash_table(
        &self,
        owner: MutatorKey,
        mut object: Box<HashTableObj>,
    ) -> Result<TaggedValue, RegistrationError> {
        if owner.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        let mut gate = self.lock()?;
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        object.header.gc.next = state.all_objects;
        object.header.gc.flags = object.header.gc.flags.with_boxed();
        object.header.gc.set_marked(state.mark_parity);
        let address = std::ptr::addr_of_mut!(object.header.gc) as usize;
        let inserted = state.non_cons_object_addrs.insert(address);
        assert!(inserted, "hash-table shell linked twice");
        // No fallible action remains. Transfer ownership before publishing the
        // list head; stopped restoration is the only path back to the sweeper.
        let object = Box::into_raw(object);
        // SAFETY: derive the persistent header pointer from the final owned
        // raw allocation, rather than retaining a prior Box field reborrow.
        let header = unsafe { std::ptr::addr_of_mut!((*object).header.gc) };
        state.all_objects = header;
        // SAFETY: constructor and all boxed/list/catalog metadata completed;
        // the live run prevents collector reunion until the root is installed.
        Ok(unsafe { TaggedValue::from_veclike_ptr(object.cast()) })
    }

    /// The caller's canonical registry has validated the full key and its
    /// uniquely borrowed resident young payload. No raw storage is usable
    /// until all catalog metadata, young recording and charging have committed.
    #[cold]
    #[inline(never)]
    pub(super) fn grant(
        &self,
        owner: MutatorKey,
        young: &mut YoungBlockLog,
        want: usize,
    ) -> Result<CatalogRegion, RegistrationError> {
        if owner.heap_identity() != self.heap || young.id().heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if !(1..=MAX_GRANT).contains(&want) {
            return Err(RegistrationError::Capacity);
        }
        #[cfg(test)]
        let catalog_wait = std::time::Instant::now();
        let mut gate = self.lock()?;
        #[cfg(test)]
        test_catalog_wait(catalog_wait, CatalogOp::Grant);
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        state
            .live_count
            .checked_add(want)
            .ok_or(RegistrationError::Exhausted)?;
        let active = state
            .active_regions
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        // SAFETY: this catalog owns every registered block, no sweep overlaps,
        // and the existing admitted allocator removes an exclusive free run.
        let free = unsafe { state.allocator.reserve_run(want) };
        let (first, count, source) = if let Some((first, count)) = free {
            (first, count, CatalogSource::FreeList)
        } else {
            let tail = state
                .blocks
                .last_mut()
                .filter(|block| state.allocator.is_ready(ConsBlockBase::of(block)))
                .and_then(|block| {
                    let (start, count) = block.reserve_tail(want);
                    (count != 0).then(|| {
                        // SAFETY: reserve_tail bounds this run in owned storage.
                        (unsafe { block.cells_ptr().add(start) }, count)
                    })
                });
            if let Some((first, count)) = tail {
                (first, count, CatalogSource::BlockTail)
            } else {
                // Install retained storage ownership before metadata can name
                // it. Any unwind after this point poisons the gate; Drop keeps
                // the entire installed catalog, including this backing alive.
                let mut block = ConsBlock::new();
                let (_, count) = block.reserve_tail(want);
                let first = block.cells_ptr();
                let base = block.base_addr();
                state.blocks.push(block);
                let index = state.blocks.len() - 1;
                state.allocator.register(&state.blocks[index]);
                assert!(
                    state.by_base.insert(base, index).is_none(),
                    "duplicate cons base"
                );
                if state.generational {
                    if let Some(cards) = &mut state.cards {
                        cards.insert_block(base);
                    }
                }
                // Release publication is LAST: a reader must never discover a
                // chunk entry before allocator/vector/base/card registration.
                if let Some(map) = &state.chunk_map {
                    map.set(base, ChunkEntry::new(ChunkClass::Cons, index));
                }
                (first, count, CatalogSource::FreshBlock)
            }
        };
        let base = ConsBlock::block_base_for_ptr(first);
        let index = *state
            .by_base
            .get(&base)
            .expect("grant has canonical base index");
        if state.generational {
            if let Err(error) = state
                .allocator
                .note_carve(ConsBlockBase::of(&state.blocks[index]), young)
            {
                // No marks, initialization, charge or handout occurred. The
                // reserved run remains uniquely ours in retained storage.
                unsafe { state.allocator.return_unused(first, count) };
                return Err(error.into());
            }
        }
        state.live_count += count;
        state.active_regions = active;
        #[cfg(test)]
        {
            let source_index = match source {
                CatalogSource::FreeList => 0,
                CatalogSource::BlockTail => 1,
                CatalogSource::FreshBlock => 2,
            };
            state.source_grants[source_index] += 1;
        }
        Ok(CatalogRegion {
            identity: Arc::clone(&self.identity),
            owner,
            first: NonNull::new(first).expect("nonempty cons run"),
            total: count,
            used: 0,
            source,
            active: true,
            _thread: PhantomData,
        })
    }

    #[cold]
    #[inline(never)]
    pub(super) fn close(
        &self,
        owner: MutatorKey,
        mut region: CatalogRegion,
    ) -> Result<usize, (RegistrationError, CatalogRegion)> {
        let result = (|| {
            if !Arc::ptr_eq(&self.identity, &region.identity) {
                return Err(RegistrationError::StaleRegistration);
            }
            if owner != region.owner {
                return Err(RegistrationError::WrongOwner);
            }
            #[cfg(test)]
            let catalog_wait = std::time::Instant::now();
            let mut gate = self.lock()?;
            #[cfg(test)]
            test_catalog_wait(catalog_wait, CatalogOp::Close);
            let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
            if !region.active || state.active_regions == 0 {
                return Err(RegistrationError::OutstandingLoan);
            }
            let unused = region.unused();
            let base = ConsBlock::block_base_for_ptr(region.first.as_ptr());
            let index = *state
                .by_base
                .get(&base)
                .ok_or(RegistrationError::Publication)?;
            let block = &mut state.blocks[index];
            if block.base_addr() != base || state.live_count < unused {
                return Err(RegistrationError::Publication);
            }
            // SAFETY: only the unhanded suffix is touched; pointer provenance
            // comes from the original grant, and no stopped sweep overlaps.
            let first = unsafe { region.first.as_ptr().add(region.used) };
            let start = ConsBlock::index_of_ptr(first);
            if unused != 0 {
                ConsBlock::mark_run_at(base, start, unused, false);
                if region.source != CatalogSource::FreeList
                    && block.next_index as usize == start + unused
                {
                    block.rewind_tail(start);
                } else {
                    // SAFETY: these exact unused young cells are absent from
                    // every free list and exclusively owned by this region.
                    unsafe { state.allocator.return_unused(first, unused) };
                }
                state.live_count -= unused;
            }
            state.active_regions -= 1;
            region.active = false;
            Ok(unused)
        })();
        result.map_err(|error| (error, region))
    }

    fn note_batch_carve(
        state: &mut CatalogState,
        base: usize,
        young: &mut YoungBlockLog,
    ) -> Result<(), crate::tagged::gc::young_registry::YoungRegistryError> {
        // Non-generational eras keep every owner young log empty exactly as
        // the per-region grant did: capture_young refuses a nonempty GEN0 log.
        if !state.generational {
            return Ok(());
        }
        let index = *state
            .by_base
            .get(&base)
            .expect("batch grant has canonical base index");
        // The carve age certifies the inline store sites' no-old-owner
        // fast path; catalog owners publish their own cards instead, so
        // only the young recording's error escapes to the batch unwind.
        state
            .allocator
            .note_carve(ConsBlockBase::of(&state.blocks[index]), young)
            .map(|_| ())
    }

    /// Unwind a partially built batch after a failed young recording: every
    /// lent run is still exclusively ours, so the same close semantics return
    /// each range before the error escapes.
    fn unwind_batch(state: &mut CatalogState, tails: &mut [OwnedConsTail]) {
        for tail in tails.iter_mut() {
            let unused = (tail.limit - tail.cursor) as usize;
            if tail.from_tail {
                let index = *state
                    .by_base
                    .get(&tail.base)
                    .expect("granted block base index");
                let block = &mut state.blocks[index];
                debug_assert_eq!(block.next_index as usize, tail.limit as usize);
                // Bits above the rewound top were never set.
                block.rewind_tail(tail.start as usize);
            } else {
                ConsBlock::mark_run_at(tail.base, tail.cursor as usize, unused, false);
                // SAFETY: these exact unhanded cells are absent from every
                // free list and exclusively owned by this batch.
                unsafe {
                    state.allocator.return_unused(
                        // SAFETY: the tail names a registered live block.
                        unsafe { ConsBlock::cell_ptr_at(tail.base, tail.cursor as usize) },
                        unused,
                    )
                };
            }
            tail.cursor = tail.limit;
        }
    }

    /// Lend whole tails — free-list runs, the last block's remaining bump
    /// top, and freshly installed blocks — until `want` cells are exclusive
    /// to this owner. One catalog acquisition serves a whole batch; the
    /// per-region carve from a held tail takes no shared lock.
    ///
    /// The caller's canonical registry has validated the full key and its
    /// uniquely borrowed resident young payload, exactly as for `grant`.
    #[cold]
    #[inline(never)]
    pub(super) fn grant_tail_batch(
        &self,
        owner: MutatorKey,
        young: &mut YoungBlockLog,
        want: usize,
    ) -> Result<Vec<OwnedConsTail>, RegistrationError> {
        if owner.heap_identity() != self.heap || young.id().heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if !(1..=TAIL_BATCH_CELLS).contains(&want) {
            return Err(RegistrationError::Capacity);
        }
        #[cfg(test)]
        let catalog_wait = std::time::Instant::now();
        let mut gate = self.lock()?;
        #[cfg(test)]
        test_catalog_wait(catalog_wait, CatalogOp::Grant);
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        state
            .live_count
            .checked_add(want)
            .ok_or(RegistrationError::Exhausted)?;
        let mut tails: Vec<OwnedConsTail> = Vec::new();
        let mut remaining = want;
        let carve_result: Result<(), crate::tagged::gc::young_registry::YoungRegistryError> = 'batch: {
            loop {
                if remaining == 0 || tails.len() == TAIL_BATCH_MAX_RUNS {
                    break 'batch Ok(());
                }
                // (a) A free-list run of previously used cells.
                // SAFETY: this catalog owns every registered block, no sweep
                // overlaps, and the admitted allocator removes an exclusive
                // free run.
                if let Some((first, count)) = unsafe { state.allocator.reserve_run(remaining) } {
                    let base = ConsBlock::block_base_for_ptr(first);
                    if let Err(error) = Self::note_batch_carve(state, base, young) {
                        break 'batch Err(error);
                    }
                    let start = ConsBlock::index_of_ptr(first) as u16;
                    tails.push(OwnedConsTail {
                        identity: Arc::clone(&self.identity),
                        base,
                        start,
                        cursor: start,
                        limit: start + count as u16,
                        from_tail: false,
                        source: CatalogSource::FreeList,
                        _thread: PhantomData,
                    });
                    remaining -= count;
                    continue;
                }
                // (b) The whole remaining bump top of the last ready block.
                let tail = state
                    .blocks
                    .last_mut()
                    .filter(|block| state.allocator.is_ready(ConsBlockBase::of(block)))
                    .and_then(|block| {
                        let (start, count) = block.reserve_tail(remaining);
                        (count != 0).then_some((start, count))
                    });
                if let Some((start, count)) = tail {
                    let base = state.blocks.last().expect("just reserved").base_addr();
                    if let Err(error) = Self::note_batch_carve(state, base, young) {
                        break 'batch Err(error);
                    }
                    tails.push(OwnedConsTail {
                        identity: Arc::clone(&self.identity),
                        base,
                        start: start as u16,
                        cursor: start as u16,
                        limit: (start + count) as u16,
                        from_tail: true,
                        source: CatalogSource::BlockTail,
                        _thread: PhantomData,
                    });
                    remaining -= count;
                    continue;
                }
                // (c) Install retained storage ownership before metadata
                // can name it. Any unwind after this point poisons the
                // gate; Drop keeps the entire installed catalog, including
                // this backing, alive.
                {
                    let mut block = ConsBlock::new();
                    let (start, count) = block.reserve_tail(remaining);
                    let base = block.base_addr();
                    state.blocks.push(block);
                    let index = state.blocks.len() - 1;
                    state.allocator.register(&state.blocks[index]);
                    assert!(
                        state.by_base.insert(base, index).is_none(),
                        "duplicate cons base"
                    );
                    if state.generational {
                        if let Some(cards) = &mut state.cards {
                            cards.insert_block(base);
                        }
                    }
                    // Release publication is LAST: a reader must never
                    // discover a chunk entry before allocator/vector/base/
                    // card registration.
                    if let Some(map) = &state.chunk_map {
                        map.set(base, ChunkEntry::new(ChunkClass::Cons, index));
                    }
                    if let Err(error) = Self::note_batch_carve(state, base, young) {
                        break 'batch Err(error);
                    }
                    tails.push(OwnedConsTail {
                        identity: Arc::clone(&self.identity),
                        base,
                        start: start as u16,
                        cursor: start as u16,
                        limit: (start + count) as u16,
                        from_tail: true,
                        source: CatalogSource::FreshBlock,
                        _thread: PhantomData,
                    });
                    remaining -= count;
                }
            }
        };
        if let Err(error) = carve_result {
            Self::unwind_batch(state, &mut tails);
            return Err(error.into());
        }
        let handed = want - remaining;
        state
            .live_count
            .checked_add(handed)
            .ok_or(RegistrationError::Exhausted)?;
        state
            .active_regions
            .checked_add(tails.len())
            .ok_or(RegistrationError::Exhausted)?;
        state.live_count += handed;
        state.active_regions += tails.len();
        #[cfg(test)]
        {
            for tail in &tails {
                let source_index = match tail.source {
                    CatalogSource::FreeList => 0,
                    CatalogSource::BlockTail => 1,
                    CatalogSource::FreshBlock => 2,
                };
                state.source_grants[source_index] += 1;
            }
        }
        Ok(tails)
    }

    /// The ACK-side publication: return every unhanded tail suffix under one
    /// gate acquisition. A `from_tail` suffix rewinds the block's bump top
    /// only while this tail still owns it; every other suffix returns through
    /// the free list with the defensive mark clear. After this call the
    /// catalog sees exactly the handed cells as live.
    #[cold]
    #[inline(never)]
    pub(super) fn publish_tails(
        &self,
        owner: MutatorKey,
        tails: Vec<OwnedConsTail>,
    ) -> Result<usize, RegistrationError> {
        if owner.heap_identity() != self.heap {
            return Err(RegistrationError::WrongHeap);
        }
        #[cfg(test)]
        let catalog_wait = std::time::Instant::now();
        let mut gate = self.lock()?;
        #[cfg(test)]
        test_catalog_wait(catalog_wait, CatalogOp::Close);
        let state = gate.as_mut().ok_or(RegistrationError::Publication)?;
        let mut refunded = 0;
        for mut tail in tails {
            if !Arc::ptr_eq(&self.identity, &tail.identity) {
                return Err(RegistrationError::StaleRegistration);
            }
            if state.active_regions == 0 {
                return Err(RegistrationError::OutstandingLoan);
            }
            let unused = (tail.limit - tail.cursor) as usize;
            if unused != 0 {
                let index = *state
                    .by_base
                    .get(&tail.base)
                    .ok_or(RegistrationError::Publication)?;
                let block = &mut state.blocks[index];
                if block.base_addr() != tail.base || state.live_count < unused {
                    return Err(RegistrationError::Publication);
                }
                // Mirror `close`: rewind the bump top only while this tail
                // still ends at it — a later grant may own the cells above
                // our limit, and then the suffix returns through the free
                // list with the defensive mark clear instead.
                if tail.from_tail && block.next_index as usize == tail.limit as usize {
                    // Bits above the rewound top were never set.
                    block.rewind_tail(tail.cursor as usize);
                } else {
                    ConsBlock::mark_run_at(tail.base, tail.cursor as usize, unused, false);
                    // SAFETY: the unhanded suffix is exclusively owned here
                    // and absent from every free list until this publication.
                    unsafe {
                        state.allocator.return_unused(
                            // SAFETY: the tail names a registered live block.
                            unsafe { ConsBlock::cell_ptr_at(tail.base, tail.cursor as usize) },
                            unused,
                        )
                    };
                }
                state.live_count -= unused;
                refunded += unused;
            }
            state.active_regions -= 1;
            // Fully published: dropping the spent tail must not abandon.
            tail.cursor = tail.limit;
        }
        Ok(refunded)
    }
}
/// Counts/addresses only; never an unlocked storage borrow.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CatalogSnapshot {
    pub(super) installed: bool,
    pub(super) blocks: Vec<(usize, usize)>,
    pub(super) metadata_coherent: bool,
    pub(super) free_count: usize,
    pub(super) active_regions: usize,
    pub(super) live_count: usize,
    /// Free-list, existing-tail, fresh-block grants, respectively.
    pub(super) source_grants: [usize; 3],
    /// Whether young-log carving records for this installed catalog.
    pub(super) generational: bool,
}
#[cfg(test)]
impl ConsCatalog {
    pub(super) fn snapshot_for_test(&self) -> Result<CatalogSnapshot, RegistrationError> {
        let gate = self.lock()?;
        let Some(state) = gate.as_ref() else {
            return Ok(CatalogSnapshot {
                installed: false,
                blocks: Vec::new(),
                metadata_coherent: true,
                free_count: 0,
                active_regions: 0,
                live_count: 0,
                source_grants: [0; 3],
                generational: false,
            });
        };
        let free = state.allocator.free_counts();
        let coherent = state.blocks.len() == state.by_base.len()
            && state.blocks.len() == free.len()
            && state.blocks.iter().enumerate().all(|(index, block)| {
                let base = block.base_addr();
                state.by_base.get(&base) == Some(&index)
                    && free.iter().any(|(key, count)| {
                        key.addr() == base && *count <= block.next_index as usize
                    })
                    && state.chunk_map.as_ref().is_none_or(|map| {
                        let entry = map.get(base);
                        entry.is(ChunkClass::Cons) && entry.index() == index
                    })
                    && (!state.generational
                        || state.cards.as_ref().is_none_or(|cards| {
                            cards
                                .blocks(super::card_table::ChunkSlot::of(base))
                                .contains(&base)
                        }))
            });
        Ok(CatalogSnapshot {
            installed: true,
            blocks: state
                .blocks
                .iter()
                .enumerate()
                .map(|(i, b)| (b.base_addr(), i))
                .collect(),
            metadata_coherent: coherent,
            free_count: free.iter().map(|(_, n)| n).sum(),
            active_regions: state.active_regions,
            live_count: state.live_count,
            source_grants: state.source_grants,
            generational: state.generational,
        })
    }
}

impl Drop for ConsCatalog {
    fn drop(&mut self) {
        self.identity.abandoned.store(true, Ordering::Release);
        // A forgotten/abandoned era cannot establish serial ownership. Even
        // zero active regions may contain rooted cells. Only explicit restore
        // can release this ownership into the original heap; retain otherwise.
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(state) = state.take() {
            std::mem::forget(state);
        }
    }
}
