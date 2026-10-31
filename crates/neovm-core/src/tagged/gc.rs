//! Mark-sweep garbage collector for the tagged pointer value system.
//!
//! # Design
//!
//! - **Cons cells**: GNU-shaped aligned block allocator.
//!   Each `ConsBlock` stores a fixed-size array of `ConsCell` at the front of
//!   a 64KB-aligned block, followed by packed mark bits. This lets the GC
//!   derive a cons's owning block/index directly from the pointer, matching the
//!   structure GNU Emacs uses in `alloc.c`.
//!
//! - **Floats, strings, vectors**: SIZE-CLASS OBJECT ARENA PAGES (the
//!   non-cons allocator modernization, stage 3) — 64KB-aligned pages of
//!   fixed-stride slots (Float 32B, String 64B, Vector 64B) with a per-page
//!   allocation bitmap and free list. Page objects keep their `GcHeader`;
//!   ownership is the PAGE-SPAN ORACLE (per-class page-base registry +
//!   stride + alloc bit), NOT the addr-set, and they never join the
//!   intrusive lists; dedicated page sweeps reclaim them.
//!
//! - **All other heap objects** (non-Vector vectorlikes): allocated
//!   via the system allocator, linked via intrusive `GcHeader.next` list
//!   for sweeping, with an address index for O(1) ownership checks during
//!   marking.
//!
//! - **Mark phase**: walk from roots, decode tags, follow heap pointers.
//! - **Sweep phase**: walk cons blocks (bitmap), object arena pages
//!   (bitmap), and the intrusive list (GcHeader chain), freeing unmarked
//!   objects.
//!
//! Generational collection (GEN1) is enabled by default. Set
//! `NEOVM_GC_GENERATIONAL=0` before heap construction to use full-heap-only
//! collection (GEN0); an unset or empty value keeps GEN1 enabled.

use super::header::*;
use super::value::TaggedValue;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::HashTableWeakness;
use crate::heap_types::LispStringStorageKind;
use crate::tagged::symbol_marks::SymbolMarkBits;
use malachite::integer::Integer;
use rustc_hash::{FxHashMap, FxHashSet};
use std::alloc::{self, Layout};
use std::cell::Cell;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) mod symbol_table;

mod cache_root_scan;
pub(crate) use cache_root_scan::CacheRootScan;

/// Optional heap-write observation, used by tests/introspection to inspect which
/// owners (and optionally which individual writes) were mutated since the last
/// reset. This is NOT a GC marking barrier — the concurrent collector's barrier
/// is the SATB log keyed on `concurrent_mark_running`. The dump remembered set is
/// maintained unconditionally in `record_heap_write` regardless of this mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteTrackingMode {
    Disabled,
    OwnersAndRecords,
}

/// Classifies the kind of heap mutation that occurred.
///
/// GNU Emacs performs direct object/cell writes (`XSETCAR`, `XSETCDR`, `ASET`,
/// symbol value writes, etc.).  Neomacs keeps the same Lisp-visible semantics,
/// but records mutation metadata here so generational and concurrent
/// collectors have a single write-barrier surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapWriteKind {
    ConsCar,
    ConsCdr,
    VectorSlot,
    VectorBulk,
    RecordSlot,
    RecordBulk,
    ClosureSlot,
    ClosureBulk,
    StringTextProps,
    StringData,
    HashTableData,
    ByteCodeData,
    LispMarker,
    OverlayData,
    XwidgetData,
    XwidgetViewData,
    /// Mutation of a char-table object (default/parent/ascii/contents/extras).
    /// Char-tables are dumped (syntax/category/case tables) and mutated in
    /// place post-load, so this barrier is required for the dump partition's
    /// remembered set to catch dumped char-table → heap edges.
    CharTableData,
    /// Mutation of a sub-char-table object's contents.
    SubCharTableData,
    /// Mutation of an obarray object (buckets/count). Obarrays are dumped and
    /// mutated post-load by `intern`, so the remembered set must observe
    /// dumped-obarray → heap edges through this chokepoint.
    ObarrayData,
    /// Mutation of a module-function object's `interactive_form` slot
    /// (`module_make_interactive`) — the one traced non-cons slot written
    /// outside a `mutate.rs` wrapper. `record_heap_write` is owner-driven, so
    /// this variant carries no dispatch behaviour; it exists so the write site
    /// names its kind like every other traced veclike, and the barrier logs the
    /// pre-overwrite `interactive_form` (covered by `collect_veclike_children`).
    ModuleFunction,
}

/// A single heap mutation event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapWriteRecord {
    pub owner: TaggedValue,
    pub kind: HeapWriteKind,
    pub slot: Option<usize>,
    pub value: Option<TaggedValue>,
}

pub(crate) const MEMORY_USE_COUNT_LEN: usize = 7;

#[derive(Clone, Copy, Debug)]
pub(crate) enum MemoryUseCountSlot {
    ConsCells = 0,
    Floats = 1,
    VectorCells = 2,
    Symbols = 3,
    StringChars = 4,
    Intervals = 5,
    Strings = 6,
}

impl MemoryUseCountSlot {
    #[inline]
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

impl HeapWriteRecord {
    pub const fn bulk(owner: TaggedValue, kind: HeapWriteKind) -> Self {
        Self {
            owner,
            kind,
            slot: None,
            value: None,
        }
    }

    pub const fn slot(
        owner: TaggedValue,
        kind: HeapWriteKind,
        slot: usize,
        value: TaggedValue,
    ) -> Self {
        Self {
            owner,
            kind,
            slot: Some(slot),
            value: Some(value),
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-local heap access
// ---------------------------------------------------------------------------

thread_local! {
    static TAGGED_HEAP: Cell<*mut TaggedHeap> = const { Cell::new(std::ptr::null_mut()) };
    // Read ownership metadata without dereferencing a displaced heap pointer.
    static TAGGED_HEAP_ID: Cell<Option<usize>> = const { Cell::new(None) };
    static TAGGED_HEAP_WRITE_TRACKING_MODE: Cell<WriteTrackingMode> =
        const { Cell::new(WriteTrackingMode::Disabled) };
    /// Mirrors `TaggedHeap::partition_dump` so the write-barrier hot path can
    /// decide whether to run without dereferencing the heap.
    static TAGGED_HEAP_PARTITION_ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Mirrors `TaggedHeap::concurrent_mark_running` so the write-barrier hot
    /// path keeps reaching `record_heap_write` (for the concurrent SATB log)
    /// even when owner-tracking is Disabled and the partition is inactive.
    ///
    /// PROTOCOL STATE, NOT SCOPE STATE — deliberately not wrapped in a Drop
    /// guard. The set(true)/set(false) pair lives in `launch_concurrent_mark`
    /// / `join_concurrent_mark`: the true-window spans those two calls across
    /// arbitrarily many mutator frames, so no lexical scope contains it, and a
    /// guard that restored the previous value on unwind would disarm the SATB
    /// barrier while the GC thread is still marking (lost pre-images => live
    /// objects collected). The two writes are kept adjacent to the
    /// `concurrent_mark_running` transitions they mirror (no panic point can
    /// split them), and `set_tagged_heap` re-derives the mirror from the heap
    /// bool whenever a heap is (re)installed on a thread — that resync, not a
    /// guard, is the panic-recovery point.
    static TAGGED_HEAP_CONCURRENT_ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// The installed mutator's Tier-H activation, independently of the general
    /// SATB window: claims-OFF cycles still mark concurrently. Derived from
    /// running + claims + snapshot presence at launch and heap installation;
    /// cleared at join/uninstall, before retained snapshot storage is freed.
    /// Like the general window, this is protocol state, never scope-restored.
    static TAGGED_HEAP_CONCURRENT_HASH_ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Mirrors `TaggedHeap::{dump_addr_lo, dump_addr_hi}` so the write
    /// barrier's partition-only path can span-test a cons owner without
    /// dereferencing the heap. `(usize::MAX, 0)` = empty span.
    static TAGGED_HEAP_DUMP_SPAN: Cell<(usize, usize)> = const { Cell::new((usize::MAX, 0)) };
    /// The write barrier's owner window (`barrier_window.rs`): every owner
    /// it covers takes the out-of-line barrier, every other store is plain
    /// unless its owner is a tenured non-cons the remembered set has not
    /// recorded. Rust non-cons stores test it; compiled GEN0 observed stores
    /// additionally cover certificate owners in their JitHeapState window.
    /// Published by
    /// `TaggedHeap::publish_barrier_window` at every writer of its inputs
    /// and re-derived whenever a heap is (re)installed.
    static TAGGED_HEAP_BARRIER_WINDOW: Cell<BarrierWindow> =
        const { Cell::new(BarrierWindow::NONE) };
    /// Rust cons stores use the same window when generations are disabled,
    /// or ALL when enabled. This protocol mirror folds mode selection into
    /// publication, so a disabled cons store never reads the heap or its mode.
    static TAGGED_HEAP_CONS_BARRIER_WINDOW: Cell<BarrierWindow> =
        const { Cell::new(BarrierWindow::NONE) };
    /// Non-cons owners already in this cycle's `satb_snapshotted_owners`,
    /// direct-mapped like the remembered cache. During a concurrent mark a
    /// write by such an owner has nothing to add: its pre-image was logged at
    /// its first write this cycle, and that same `record_heap_write` made any
    /// remembered-set insert (tenure cannot change inside a mark). Cleared
    /// with the set (`begin_collection`, the termination's take) and whenever
    /// a heap is (re)installed.
    static TAGGED_HEAP_SATB_CACHE: [Cell<usize>; BARRIER_CACHE_SLOTS] =
        const { [const { Cell::new(0) }; BARRIER_CACHE_SLOTS] };
    /// Auto-allocated heap for tests that construct Values without a Context.
    /// The owner keeps the allocation's single provenance root, so the raw
    /// pointer installed in `TAGGED_HEAP` is never derived from a borrow that
    /// a later `borrow_mut` would invalidate.
    #[cfg(test)]
    static TEST_FALLBACK_TAGGED_HEAP: std::cell::RefCell<Option<AliasableBox<TaggedHeap>>> =
        const { std::cell::RefCell::new(None) };
}

/// Query the installed mutator's existing write-tracking protocol mirror.
/// Context installation and mode changes refresh this scalar; no heap pointer
/// or Lisp identity is cached or dereferenced, and each mutator owns its mode.
#[cfg(feature = "jit")]
#[inline]
pub(crate) fn current_write_tracking_enabled() -> bool {
    TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.get() != WriteTrackingMode::Disabled)
}

const BARRIER_CACHE_SLOTS: usize = 64;

#[cfg(test)]
thread_local! {
    /// Writes the barrier's thread-local rejects passed on to the heap.
    static RECORD_HEAP_WRITE_CALLS: Cell<usize> = const { Cell::new(0) };
    /// Writes the barrier's inline gate sent to its outlined part.
    static BARRIER_SLOW_CALLS: Cell<usize> = const { Cell::new(0) };
}

/// The write-barrier caches' slot for an owner's bits (heap addresses are
/// 8-aligned, so the low tag bits carry nothing).
#[inline(always)]
fn barrier_cache_slot(bits: usize) -> usize {
    ((bits >> 4) ^ (bits >> 12)) & (BARRIER_CACHE_SLOTS - 1)
}

fn clear_barrier_cache(cache: &'static std::thread::LocalKey<[Cell<usize>; BARRIER_CACHE_SLOTS]>) {
    cache.with(|slots| slots.iter().for_each(|slot| slot.set(0)));
}

// ---------------------------------------------------------------------------
// TaggedHeap — the main GC-managed heap
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
enum CanonicalEmptyString {
    #[default]
    Missing,
    Owned(TaggedValue),
    Mapped(TaggedValue),
}

impl CanonicalEmptyString {
    fn value(self) -> Option<TaggedValue> {
        match self {
            Self::Missing => None,
            Self::Owned(value) | Self::Mapped(value) => Some(value),
        }
    }

    fn install_owned(&mut self, value: TaggedValue) -> TaggedValue {
        match *self {
            Self::Missing => {
                *self = Self::Owned(value);
                value
            }
            Self::Owned(existing) | Self::Mapped(existing) => existing,
        }
    }

    fn install_mapped(&mut self, value: TaggedValue) -> TaggedValue {
        match *self {
            // A restored dump is authoritative over any temporary object
            // allocated while constructing its destination Context.
            Self::Missing | Self::Owned(_) => {
                *self = Self::Mapped(value);
                value
            }
            // A current dump contains one canonical object per storage kind.
            // Keeping the first also makes old non-canonical dumps deterministic.
            Self::Mapped(existing) => existing,
        }
    }
}

#[derive(Default)]
struct CanonicalEmptyStrings {
    unibyte: CanonicalEmptyString,
    multibyte: CanonicalEmptyString,
}

impl CanonicalEmptyStrings {
    fn slot(&self, kind: LispStringStorageKind) -> CanonicalEmptyString {
        match kind {
            LispStringStorageKind::Unibyte => self.unibyte,
            LispStringStorageKind::Multibyte => self.multibyte,
        }
    }

    fn slot_mut(&mut self, kind: LispStringStorageKind) -> &mut CanonicalEmptyString {
        match kind {
            LispStringStorageKind::Unibyte => &mut self.unibyte,
            LispStringStorageKind::Multibyte => &mut self.multibyte,
        }
    }

    fn get(&self, kind: LispStringStorageKind) -> Option<TaggedValue> {
        self.slot(kind).value()
    }

    fn install_owned(&mut self, kind: LispStringStorageKind, value: TaggedValue) -> TaggedValue {
        self.slot_mut(kind).install_owned(value)
    }

    fn install_mapped(&mut self, kind: LispStringStorageKind, value: TaggedValue) -> TaggedValue {
        self.slot_mut(kind).install_mapped(value)
    }

    fn values(&self) -> impl Iterator<Item = TaggedValue> {
        [self.unibyte.value(), self.multibyte.value()]
            .into_iter()
            .flatten()
    }
}

/// The tagged pointer heap. Owns all heap-allocated Lisp objects.
// Freeze the existing physical field order before appending the store ABI.
// The shipping offsets are pinned against the pre-storefast DWARF table.
#[repr(C)]
pub struct TaggedHeap {
    /// Last ordinary cons block used by the mark phase.
    ///
    /// GNU's cons marker derives the block directly from the pointer and has a
    /// special fast path for successive list cells.  Keep Neomacs's explicit
    /// ownership map, but avoid probing it repeatedly while the mark queue is
    /// walking cells from the same block.
    mark_cons_block_cache: Option<ConsBlockCacheEntry>,

    /// The single mutator's allocation accounting and barrier buffers.
    mutator_gc: MutatorGcState,
    /// Stage 1b: the obarray slot count captured at the start handshake, retained
    /// across the cycle (the snapshot itself is moved into the GC job). At the STW
    /// termination, the residual re-seed covers new symbols in slots `>= this`
    /// (interned mid-cycle, never scanned by the GC thread). `None` outside a
    /// concurrent mark.
    concurrent_obarray_start_slots: Option<usize>,
    /// Where the open allocation regions came from and their color
    /// (`alloc_region.rs`); Rust-only bookkeeping.
    region_book: RegionBook,
    /// GNU `empty_unibyte_string` / `empty_multibyte_string`, modeled per heap.
    /// These handles are permanent runtime roots and mapped dump objects replace
    /// temporary pre-restore owned values.
    canonical_empty_strings: CanonicalEmptyStrings,
    /// Receives when the GC thread has exited its mark loop (so the mutator's
    /// termination can safely take over the gray queue). Set at start.
    gc_exited: Option<std::sync::mpsc::Receiver<ConcurrentMarkResult>>,
    /// Collector-owned generation state; mutator logs live in MutatorGcState.
    generational: generational::GenState,
    /// Cons cell block allocator.
    cons_blocks: Vec<ConsBlock>,

    /// Gray worklist for mark phase.
    gray_queue: Vec<TaggedValue>,
    /// Per-cycle mark bits for symbols. GNU symbols are GC-managed objects, so
    /// weak hash tables decide symbol-key survival from the symbol mark bit.
    /// Neomacs stores symbols as immediate `SymId`s, so the collector mirrors
    /// that mark bit here for weak-table semantics.
    marked_symbols: SymbolMarkBits,
    /// Weak hash tables discovered during this cycle's mark. Their entries are
    /// NOT traced inline (so a weak key/value does not keep its entry alive);
    /// `mark_and_sweep_weak_tables` instead processes them at the stop-the-world
    /// `complete_collection`, after the main mark drains (GNU
    /// `mark_and_sweep_weak_table_contents`). Holds raw object pointers, valid
    /// only within a single collection; cleared each cycle.
    weak_hash_tables: Vec<*mut HashTableObj>,
    /// Weak hash tables that must be re-swept on EVERY collection without
    /// relying on the mark walk to re-register them: image residents (mapped
    /// dump objects and loader-reconstructed permanent rows) and tenured
    /// runtime tables. The main mark never re-runs `trace_veclike` on a
    /// permanent-black object, and a tenured runtime table is black during a
    /// minor, so any of them would otherwise never re-register itself for the
    /// weak sweep and its entries would be pinned forever (a weak-table leak:
    /// GNU re-sweeps every weak table on every GC). Populated by
    /// `promote_and_blacken` and `promote_arena_pages` (every weak-table
    /// survivor) and at mapped-dump registration; seeded into
    /// `weak_hash_tables` at the start of every `mark_and_sweep_weak_tables`
    /// so these tables are swept against the CURRENT cycle's marks exactly
    /// like young ones. Image/mapped pointers never dangle; a freed runtime
    /// table is purged from this vector and its membership set in
    /// `free_gc_object`.
    always_swept_weak_tables: Vec<*mut HashTableObj>,
    /// Every live finalizer object, registered at allocation — the Rust-side
    /// equivalent of GNU's intrusive `finalizers` list (alloc.c). Scanned at
    /// mark termination by `mark_and_queue_doomed_finalizers`: unmarked
    /// entries leave the registry (the object is swept normally) and their
    /// `function` moves to `doomed_finalizer_functions`. Entries stay valid
    /// because every sweep that could free an unmarked finalizer is preceded
    /// by that scan, which removes it first.
    finalizer_registry: Vec<*mut FinalizerObj>,
    /// Functions of finalizer objects found unreachable, waiting to run —
    /// GNU's `doomed_finalizers` list (we queue only the function; the
    /// finalizer object itself is swept). Re-marked transitively when queued
    /// so the imminent sweep keeps them, and seeded as runtime roots every
    /// cycle so a batch that survives across cycles (e.g. queued during a
    /// finalizer run) stays live. Drained by the evaluator's cycle-completed
    /// block, which calls each with zero args, errors ignored.
    doomed_finalizer_functions: Vec<TaggedValue>,
    /// Host surface ids of `SurfaceObj` handles the sweep reclaimed, waiting
    /// for a best-effort `DisplayHost::destroy_shader_surface`. The sweep
    /// (`free_gc_object`) only records the id — it has no display-host access
    /// — and the evaluator's cycle-completed block drains the batch
    /// (`take_pending_surface_destroys`). Plain data (u32), so entries never
    /// need marking; a double destroy is harmless (the render-thread free of
    /// a missing id is a no-op).
    pending_surface_destroys: Vec<u32>,
    /// Stable video ids of `VideoObj` handles reclaimed by the sweep. The
    /// evaluator drains these after collection through `DisplayHost`.
    pending_video_destroys: Vec<neomacs_display_protocol::VideoId>,
    /// SIZE-CLASS OBJECT ARENAS (non-cons allocator modernization stage 3 +
    /// task 03/3a): every heap float/string/vector/bytecode lives in a
    /// 64KB-aligned `ObjectPage`
    /// slot instead of its own `Box`. Page objects are OWNED via the
    /// page-span oracle (`ObjectArena::owns` — registry + stride + alloc
    /// bit), NOT via `non_cons_object_addrs`, and are NEVER on
    /// `all_objects`/`tenured_objects` — the page sweeps are their only
    /// reclaimer, and `free_gc_object` stays Box-only. Empty pages are
    /// retained for reuse; pages are freed only at heap teardown via these
    /// vectors' drops (`ObjectPage: Drop` — drops live payloads in place).
    float_arena: ObjectArena<FloatObj>,
    string_arena: ObjectArena<StringObj>,
    vector_arena: ObjectArena<VectorObj>,
    bytecode_arena: ObjectArena<ByteCodeObj>,
    /// Interpreted closures (task 03/3b): 128B class, own arena. Page
    /// lambdas are owned via the page-span oracle (routed by
    /// `owns_veclike_object`), never on the intrusive lists / addr-set.
    lambda_arena: ObjectArena<LambdaObj>,
    /// Macros (task 03/3b): shares the 128B stride in its OWN arena.
    macro_arena: ObjectArena<MacroObj>,
    /// Records (task 03/3b): 64B class, own arena — backs both the Record and
    /// WindowConfiguration type tags (same `RecordObj`, distinct tag).
    record_arena: ObjectArena<RecordObj>,
    /// Symbols-with-position (task 03/3b): 64B class, own arena. POD-like
    /// ({sym, pos} Values, `needs_drop` == false — no payload to free).
    symbol_with_pos_arena: ObjectArena<SymbolWithPosObj>,
    /// Markers: 128B class, own arena. The highest-churn editor object
    /// (`save-excursion` allocates and frees one per call); GNU's
    /// `marker_block` equivalent. POD payload; the buffer-chain link is
    /// detached by `unchain_dead_markers` before the sweep frees a slot.
    marker_arena: ObjectArena<MarkerObj>,
    /// Bignums: 64B class, own arena — GNU's `make_bignum_bits` takes a
    /// vector-block slot, never a malloc of its own. Childless; the slot's
    /// malachite `Integer` owns the limb vector, freed by the page sweep's
    /// `drop_in_place`.
    bignum_arena: ObjectArena<BignumObj>,
    /// Cons cells loaded directly from a mapped pdump image.  GNU's pdumper
    /// uses external mark bits for dumped objects rather than writing mark
    /// state into malloc/GC allocation headers; mirror that for mapped conses.
    mapped_cons_ranges: Vec<MappedConsRange>,
    /// Float objects loaded directly from a mapped pdump image.  Like GNU
    /// pdumper dump objects, their mark state lives outside the mapped bytes.
    mapped_float_ranges: Vec<MappedFloatRange>,
    /// Vectorlike objects loaded directly from a mapped pdump image.  Their
    /// object headers are in the mapped image, but mark state remains external.
    mapped_veclike_objects: Vec<MappedVecLikeObject>,
    /// String objects loaded directly from a mapped pdump image.  Their text
    /// properties can contain Lisp roots, so mark state must be external too.
    mapped_string_objects: Vec<MappedStringObject>,

    /// Raw pointers to the `markers_head` slot of every live buffer's
    /// `BufferText`. Populated by the caller immediately before
    /// `complete_collection` via `set_marker_chain_head_slots`; drained
    /// by `unchain_dead_markers` between the mark and sweep phases so
    /// unmarked markers are spliced out of the intrusive per-buffer
    /// chain before `sweep_objects` frees them. Mirrors GNU
    /// `sweep_buffer → unchain_dead_markers` (`alloc.c`).
    ///
    /// Empty for GC cycles that don't go through a `Context` (raw-heap
    /// tests in `tagged/tests.rs`), which is fine because those never
    /// create chain-linked markers.
    marker_chain_head_slots: Vec<*mut *mut MarkerObj>,

    /// Canonical runtime handle wrappers keyed by their underlying object id.
    /// Buffer object per `BufferId`, indexed by the id (ids are small slab
    /// indices): `Value::make_buffer` runs ~9K times per org font-lock op
    /// and paid a hash probe per call.
    buffer_registry: Vec<Option<TaggedValue>>,
    dirty_owners: Vec<TaggedValue>,
    dirty_writes: Vec<HeapWriteRecord>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: retired vector backings — the ORIGINAL
    /// `Vec` of each OWNED vector whose backing was clone-on-write replaced during
    /// this concurrent mark (`with_vector_data_mut`). The GC thread's snapshot still
    /// points at these immutable buffers, so they must stay alive until the GC thread
    /// joins. Drained + dropped in `join_concurrent_mark` (the GC thread has provably
    /// exited — the only safe free point). Empty unless a clone-on-write fired.
    retired_vector_buffers: Vec<Vec<super::header::AtomicTaggedValue>>,
    /// Handshake-pause decomposition (per phase, per root group, size probes).
    /// Heap-side phases are written where they run; the evaluator fills the
    /// context-root breakdowns + context-side probes via `handshake_stats_mut`.
    handshake: HandshakeStats,
    /// Mapped cons ranges staged by `begin_collection` for the concurrent
    /// first cycle; `launch_concurrent_mark` moves them into the job.
    staged_mapped_cons_scan: Option<Vec<(usize, usize)>>,
    /// Stopped-start mapped veclike backing descriptors (see the job field).
    staged_mapped_veclikes: Option<MappedVeclikeScanSnapshot>,
    /// Stage 1b CONCURRENT OBARRAY SCAN: a start-captured obarray chunk snapshot
    /// staged by the start handshake (`start_concurrent_mark`) just before
    /// `launch_concurrent_mark`, which moves it into the `ConcurrentMarkJob`. The
    /// heap cannot reach the Context-side obarray itself, so the snapshot is built
    /// Context-side and parked here for the launch to consume. `None` except
    /// between a start handshake and the launch consuming it.
    pending_obarray_scan: Option<crate::emacs_core::symbol::ObarrayScanSnapshot>,
    /// Wall-clock start of the in-flight concurrent mark (stamped at
    /// `launch_concurrent_mark`, consumed at `incremental_finish`).
    pace_mark_start: Option<std::time::Instant>,
    /// Base-address lookup for O(1) cons block ownership and marking.
    cons_block_index_by_base: FxHashMap<usize, usize>,
    /// Exact address set for ordinary non-cons object headers.
    ///
    /// GNU's GC reaches ordinary heap ownership through allocator metadata and
    /// dumped-object ownership through `pdumper_object_p` range metadata. Keep
    /// the same fast-path split here: mark-time checks must not scan
    /// `all_objects`.
    non_cons_object_addrs: FxHashSet<usize>,
    /// Task #7 stage 2a (Fix A) INCREMENTAL VECTOR REGISTRY: the exact
    /// `VecLikeType::Vector` subset of `non_cons_object_addrs`, maintained
    /// incrementally at the link chokepoint (`link_veclike`) and the sweep
    /// free sites (`unregister_vector_object`), so `launch_concurrent_mark`
    /// builds the Tier-B `VectorScanSnapshot` by iterating only the live
    /// vectors instead of filtering the whole non-cons set (~94K entries)
    /// inside the world-stopped start handshake. INVARIANT (asserted at every
    /// launch under `cfg(test)` / `NEOVM_GC_VERIFY_PARTITION=1`): equals the
    /// set of live owned Vector objects at every handshake.
    vector_object_addrs: FxHashSet<usize>,
    /// Membership shadow for `weak_hash_tables`: registration used to dedup
    /// with a linear contains per table, O(T^2) across a cycle. The vector
    /// stays authoritative for deterministic sweep order.
    weak_hash_tables_set: rustc_hash::FxHashSet<*mut HashTableObj>,
    /// Membership shadow for `always_swept_weak_tables` (same pattern).
    always_swept_weak_tables_set: rustc_hash::FxHashSet<*mut HashTableObj>,
    mapped_veclike_index_by_addr: FxHashMap<usize, usize>,
    mapped_string_index_by_addr: FxHashMap<usize, usize>,
    window_registry: FxHashMap<u64, TaggedValue>,
    frame_registry: FxHashMap<u64, TaggedValue>,
    timer_registry: FxHashMap<u64, TaggedValue>,
    process_registry: FxHashMap<crate::emacs_core::process::ProcessId, TaggedValue>,
    dirty_owner_bits: FxHashSet<usize>,
    /// Persistent remembered set: bits of dumped objects that have been
    /// mutated and may now hold heap children. Seeded as roots every cycle so
    /// those heap children stay live. Fed by the write barrier
    /// (`record_heap_write`). Tiny in practice (few dumped objects are ever
    /// mutated). Never cleared (conservative retention).
    mapped_remembered: FxHashSet<usize>,
    /// Completed owner-local SATB chunks; the marker polls without locking
    /// when empty. The owning mutator's partial stays in its cold extension.
    satb_handoff: std::sync::Arc<SatbHandoff>,
    /// Inert physical reserve for the former heap-wide owner-dedup set.
    /// Semantic state belongs to each paired SatbMutatorState. Keeping the
    /// exact field type and position protects GEN0/JIT-visible heap offsets.
    _satb_owner_layout_reserve: FxHashSet<usize>,
    /// Owner-confined SATB cold extension reuses the old pointer-sized slot.
    /// Its Box retains the original NonNull size/alignment/niche; allocation
    /// and inactive barriers never follow this pointer.
    _satb_layout_reserve: mutator_gc::MutatorSatbSlot,
    /// GC thread sets this (Release) when gray + SATB are drained; the mutator
    /// polls it (Acquire) at safe points to decide when to terminate.
    gc_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// CONCURRENT STRING MARKING: shared claim counter for the in-flight cycle
    /// (see `ConcurrentClaimJob::str_claimed`). Reset at `launch_concurrent_mark`,
    /// folded into `last_concurrent_str_claimed` at `join_concurrent_mark`.
    concurrent_str_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT FLOAT CLAIMS (task 01): shared claim counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::float_claimed`) + its
    /// last-completed-cycle fold. Same reset/fold seams as the string pair.
    concurrent_float_claimed: std::sync::Arc<AtomicUsize>,
    /// SUBR RECOGNIZE-AND-DROP (task 01): shared drop counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::subr_dropped`) + its fold.
    concurrent_subr_dropped: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): shared claim counter for
    /// the in-flight cycle (see `ConcurrentClaimJob::vec_claimed`) + fold.
    concurrent_vec_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT BYTECODE CLAIMS (task 01): shared claim counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::bc_claimed`) + fold.
    concurrent_bc_claimed: std::sync::Arc<AtomicUsize>,
    /// Inert physical reserve for former heap-wide string preimage dedup.
    /// Owner-local sets live in SatbMutatorState; this set stays empty.
    _satb_string_layout_reserve: FxHashSet<usize>,
    /// Mutator sets this (Release) to ask the GC thread to finish and exit.
    gc_stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Task #7 stage 2a (Fix B): wakeup latch for the GC thread's idle nap.
    /// `join_concurrent_mark` notifies it AFTER setting `gc_stop`, so a stop
    /// request interrupts the nap immediately instead of burning the
    /// remainder of a fixed 100us sleep (the measured bulk of the
    /// stop-signal -> thread-exit latency in the termination handshake).
    gc_wake: std::sync::Arc<(std::sync::Mutex<()>, std::sync::Condvar)>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: per-cycle clone-on-write dedup set,
    /// keyed on each vector owner's `TaggedValue` bits. On an owner's FIRST bulk
    /// mutation this cycle we clone+retire its OWNED backing once; later mutations of
    /// the same owner skip the clone (they touch the already-cloned live backing the
    /// GC's snapshot does NOT point at). Cleared at every mark start
    /// (`concurrent_begin`/`begin_collection`). Empty unless a clone-on-write fired.
    concurrent_cloned_vectors: FxHashSet<usize>,
    /// State compiled code reads and writes in place (`jit_state.rs`): the
    /// allocation region cursors and the barrier window. Reached as
    /// `vmctx -> Context.tagged_heap -> jit`.
    jit: JitHeapState,
    /// Region refill statistics since the last finished collection.
    region_stats: RegionStats,
    /// Process-unique heap identity used by side tables that carry GC-managed
    /// Lisp values.  It deliberately does not use this heap's address: boxed
    /// heaps are routinely dropped and recreated by snapshot-based tests, and
    /// the allocator may reuse an address for a different heap lifetime.
    identity: HeapIdentity,

    /// The O(1) page directory (`chunk_map.rs`) when `NEOVM_GC_CHUNK_MAP`
    /// is on (read once, at construction): every cons block and arena page
    /// has an entry, and the ownership oracles and the GC thread classify
    /// through it instead of the per-class registries. Shared with the
    /// arenas (their page writers) and each concurrent mark's job.
    chunk_map: Option<HeapChunkMap>,

    /// Intrusive linked list of YOUNG non-cons heap objects (the nursery).
    /// Points to the GcHeader of the first object; follow `next` to traverse.
    /// Every cycle clears+sweeps only this list, so its length bounds the
    /// per-GC clear/sweep cost. FLOATS ARE ABSENT: they live in the float
    /// arena pages (also young, swept by the page sweep) and must never be
    /// linked here — the list sweeps free with `Box::from_raw`.
    all_objects: *mut GcHeader,
    /// Intrusive linked list of TENURED non-cons heap objects (the old
    /// generation). Filled at first-cycle promotion (`promote_and_blacken`);
    /// these are permanently black and are NEVER cleared or swept, so the
    /// minor-GC walk skips them entirely. Freed only at heap teardown.
    tenured_objects: *mut GcHeader,

    /// GC threshold in approximate Lisp heap bytes.
    gc_threshold: usize,
    /// Approximate bytes retained by the live heap after the last sweep.
    live_bytes: usize,

    /// Mark-start pacing state — INSTRUMENTATION ONLY. The reactive
    /// `must_finish` cap (`bytes_since_gc > gc_threshold*4`, checked by the
    /// evaluator while a concurrent mark runs) force-terminates a mark
    /// synchronously — a full STW residual drain. Each terminated mark
    /// measures its window's allocation rate and wall duration into EWMAs;
    /// `pace_lead_bytes` (rate x duration) projects the next window's
    /// allocation, i.e. how close the workload runs to that cap. A trigger
    /// that started marks early at `cap - lead` was built and then REVERTED
    /// after measurement: on the replay-storm recipes the lead never
    /// exceeded ~2% of threshold in debug and ~10.2% in release (313
    /// concurrent starts probed, 0 activations, 0 must_finish — release
    /// marking outruns allocation 40-50x, structural ceiling ~4-10% of the
    /// 300% activation bar). Reintroducing it is a two-line swap in
    /// `gc_safe_point_exact_should_collect` (see the ladder task-3/5
    /// reports); the go-criterion is a real workload whose traced
    /// `mark_window` lead approaches `3x threshold` or any nonzero
    /// `must_finish_count` from this always-on field detector.
    /// Lifetime count of forced (cap-hit) mark terminations.
    must_finish_count: u64,
    /// `bytes_since_gc` at the in-flight mark's start handshake.
    pace_mark_start_bytes: usize,
    /// EWMA (alpha 1/2) of bytes/sec allocated during recent mark windows.
    pace_alloc_rate_bps: u64,
    /// EWMA (alpha 1/2) of recent concurrent-mark wall durations, in µs.
    pace_mark_dur_us: u64,
    /// Projected allocation during the next mark window (rate x duration),
    /// recomputed at each clean termination; doubled on a forced one.
    pace_lead_bytes: usize,

    /// One intrusive free list per block. Before sticky thread admission,
    /// metadata is mutator-exclusive; afterward one mutex serializes every
    /// head/count and available-block access. Refill is cold.
    /// Boxed to preserve the inline heap's JIT-visible layout.
    cons_allocator: ConsAllocatorOwner,
    /// Number of live cons cells currently included in `allocated_count`.
    cons_live_count: usize,

    /// Cumulative GC statistics.
    gc_collections: usize,
    gc_total_elapsed_us: u64,

    /// Time (µs) spent in the `begin_collection` mark-clear pass of the most
    /// recent collection. Part of the clear/mark/sweep split used to size the
    /// dump-partition opportunity (the clear pass and the dump re-mark are the
    /// non-fundamental costs a "dump as permanent tenured region" would remove).
    last_clear_us: u64,
    /// Three-way split of `last_clear_us` (task #7 stage 2a diagnostics rider;
    /// it decided — and now gauges — the parity mark-bit design): the
    /// cons-block bitmap memset, the young non-cons segment (formerly the
    /// `all_objects` pointer-chase walk at ~98% of the clear; now the O(1)
    /// parity flip, expected ~0), and the mapped (pdump) mark-state resets
    /// (zero once partitioned).
    last_clear_cons_us: u64,
    last_clear_noncons_us: u64,
    last_clear_mapped_us: u64,
    /// Address span `[lo, hi)` covering every mapped object, for an O(1) "is
    /// this owner a dumped object?" test in the write-barrier hot path.
    dump_addr_lo: usize,
    dump_addr_hi: usize,
    /// Accumulated marking time (slices + final drain) for the in-flight
    /// incremental cycle, reported as `mark_us` at termination. Reset at start.
    incremental_mark_us: u64,
    /// Strings the GC thread claimed concurrently in the last completed cycle
    /// (diagnostics; the concurrent counterpart of `last_termination_kinds.string`).
    last_concurrent_str_claimed: usize,
    last_concurrent_float_claimed: usize,
    last_concurrent_subr_dropped: usize,
    last_concurrent_vec_claimed: usize,
    last_concurrent_bc_claimed: usize,
    /// Next heap cons-block index the deferred sweep will reclaim.
    sweep_cons_cursor: usize,
    /// Next float/string/vector arena page the deferred sweep will visit
    /// (mirrors `sweep_cons_cursor`; reset when the sweep is armed).
    sweep_float_page_cursor: usize,
    sweep_string_page_cursor: usize,
    sweep_vector_page_cursor: usize,
    sweep_bytecode_page_cursor: usize,
    sweep_lambda_page_cursor: usize,
    sweep_macro_page_cursor: usize,
    sweep_record_page_cursor: usize,
    sweep_symbol_with_pos_page_cursor: usize,
    sweep_marker_page_cursor: usize,
    sweep_bignum_page_cursor: usize,
    /// Where the deferred sweep stops: the cons blocks and arena pages that
    /// existed at mark termination. Blocks and pages created during the sweep
    /// hold only objects born marked, which it would count and not free, so
    /// bounding it by the live `len()` let a mutator that fills pages faster
    /// than one slice visits them keep the sweep from ever finishing (elb
    /// nbody under the JIT: two collections in the whole run, 2.9 GB).
    sweep_cons_end: usize,
    sweep_float_page_end: usize,
    sweep_string_page_end: usize,
    sweep_vector_page_end: usize,
    sweep_bytecode_page_end: usize,
    sweep_lambda_page_end: usize,
    sweep_macro_page_end: usize,
    sweep_record_page_end: usize,
    sweep_symbol_with_pos_page_end: usize,
    sweep_marker_page_end: usize,
    sweep_bignum_page_end: usize,
    /// Cons cells the deferred sweep found marked in the blocks it visited.
    sweep_cons_live_cells: usize,
    /// Non-cons objects detached from `all_objects` at sweep start, reclaimed
    /// incrementally. New non-cons allocations link onto a fresh `all_objects`
    /// and are not swept this cycle.
    sweep_noncons_pending: *mut GcHeader,
    /// Live bytes accumulated from the non-cons objects swept so far this cycle.
    sweep_noncons_live_bytes: usize,
    /// Carried from mark termination for the completion trace/accounting.
    sweep_mark_us: u64,
    sweep_bytes_before: usize,
    /// Bytes allocated during the terminated concurrent mark window. Those
    /// objects were born marked, so the deferred sweep counts them as
    /// survivors; `finish_incremental_sweep` takes them back out of
    /// `live_bytes`.
    sweep_mark_window_alloc_bytes: usize,
    /// Per-cycle deferred-sweep cost accumulators (reset when the sweep is
    /// armed at `incremental_finish`) + lifetime totals, and the
    /// concurrent-termination drain probe. Snapshot via `sweep_stats`.
    sweep_slice_us_total: u64,
    sweep_slice_count: usize,
    sweep_cons_blocks_swept: usize,
    sweep_noncons_freed: usize,
    sweep_lifetime_us: u64,
    sweep_lifetime_slices: usize,
    sweep_lifetime_cons_blocks_swept: usize,
    sweep_lifetime_noncons_freed: usize,
    last_termination_deferred: usize,
    max_termination_deferred: usize,
    last_termination_satb: usize,
    last_termination_kinds: DrainKinds,
    max_termination_kinds: DrainKinds,
    last_termination_fold_us: u64,
    termination_count: usize,
    /// Scratch: last `seed_internal_runtime_roots` cost/volume. Written every
    /// call; routed to the start or termination slot by `concurrent_begin` /
    /// `reseed_runtime_and_remembered_roots` (which know which handshake ran).
    last_runtime_seed_us: u64,
    last_runtime_seed_roots: usize,
    /// Scratch: last `seed_mapped_remembered` cost/volume (owners re-scanned).
    last_remembered_seed_us: u64,
    last_remembered_seed_roots: usize,
    /// The original census pointer also carries optional U35 cold state.
    /// A claims-only carrier has disabled measurement; the two facilities
    /// retain independent history, snapshot and per-mutator log lifetimes.
    census: Option<Box<GenCensus>>,
    /// TEST-ONLY: mapped veclike traces this heap ran (the first-cycle seed
    /// and the mark's mapped arm).
    #[cfg(test)]
    mapped_veclike_traces: usize,
    /// When true, `gc_threshold` was explicitly overridden by tests or host
    /// code and should not be recomputed from Lisp-visible GC variables.
    gc_threshold_overridden: bool,
    /// Set by `note_must_finish` when the in-flight mark is being cap-forced;
    /// consumed by `incremental_finish` (skip the biased EWMA sample, escalate
    /// the lead instead).
    forced_termination_pending: bool,

    /// Owners mutated since the last full collection.
    ///
    /// Optional write-observation metadata for tests and introspection.
    /// Generational remembered sets and concurrent SATB logs are maintained
    /// separately, independently of this tracking mode.
    write_tracking_mode: WriteTrackingMode,
    /// FIRST-CYCLE-CONCURRENT: armed by the driver (`arm_first_cycle_concurrent`)
    /// before the first partition cycle's `concurrent_begin`; makes
    /// `begin_collection` stage the mapped cons ranges instead of enumerating
    /// them in the handshake and makes the claim job DROP span-inside children.
    /// Cleared when the cycle completes (`finish_first_partition_cycle`), or by
    /// a stop-the-world cycle that disarms it (`begin_stw_collection`) and then
    /// finishes the bootstrap itself in `complete_collection`.
    first_cycle_concurrent: bool,
    /// Set while a stop-the-world FIRST partition cycle runs with the image
    /// pre-marked (`premark_mapped_image`): every mapped object is marked in
    /// the side tables and the flat seed pushes all their heap children, so
    /// a root inside the image needs no push. Cleared when the cycle ends.
    image_premarked: bool,

    // --- Dump-partition state (treat the immutable pdump image as a permanent
    // black/tenured region: never clear, re-trace, or sweep it). Gated by
    // `partition_dump`; default off => identical to the full-trace collector.
    /// When true, mapped (pdump) objects are born black and never re-traced;
    /// only mutated dumped objects (`mapped_remembered`) are re-scanned.
    partition_dump: bool,
    /// One-time flag: the mapped image has been blackened (all marks set).
    dump_blackened: bool,
    /// One-time flag: this heap has completed a full stop-the-world collection
    /// (its bootstrap cycle). A dump-less heap runs the concurrent collector
    /// from its second cycle on — the same one-STW-bootstrap-then-concurrent
    /// shape as the dump path; see `should_run_concurrent`.
    bootstrap_collected: bool,

    // --- Young non-cons PARITY MARKS (task #7 stage 2b; tri-state since
    // P3.1 C2.2). "Marked this cycle" for a YOUNG non-cons `GcHeader` ≡
    // (mark byte == `mark_parity`), the parity alternating `One ↔ Two`.
    // `begin_collection` flips the parity instead of pointer-chasing
    // `all_objects` to clear marks (the walk measured ~98% of the clear
    // phase). A byte of 0 is unmarked at rest under either parity (new,
    // mapped and static headers). Cons block bitmaps keep their memset
    // clear (their `fetch_or` marking is set-only and `count_marked`
    // popcounts 1-bits, so parity is structurally impossible there); mapped
    // (pdump) side-table mark state is untouched; tenured objects freeze
    // their byte at promotion, and every reader that can see one asks
    // `GcHeader::black_by_generation` BEFORE interpreting the byte
    // (mark_value owned arms, is_value_marked, unchain_dead_markers,
    // doomed-finalizer scan, the sweep). Mapped (image) objects never read
    // their header mark at all: their mark is the side table's, and
    // `unchain_dead_markers`, which walks header marks, tests the dump span
    // first.
    /// Current cycle's mark parity. INIT `Two` so the FIRST
    /// `begin_collection` flip yields `One`: objects born before any
    /// collection (at `Two`) and headers at rest (0) both read unmarked in
    /// the bootstrap cycle, which must trace everything.
    mark_parity: MarkParity,

    // --- Incremental marking state (step 7). Active on every partitioned cycle
    // (after the first-cycle promotion); the first cycle and no-dump heaps stay
    // stop-the-world. Marking is sliced across evaluator safe points using an
    // incremental-update (Steele) write barrier: dirty owners (written during
    // marking) are re-traced so no black->white edge survives, and the COMPLETE
    // root set is re-snapshotted at mark termination.
    /// True between the start of an incremental mark and its termination/sweep.
    /// While set, every safe point advances marking by one bounded slice.
    mark_in_progress: bool,
    /// True between a concurrent mark's start and termination handshakes — the
    /// mutator runs while the GC thread marks.
    concurrent_mark_running: bool,

    // --- Incremental sweep state (step 8). After a mark terminates, the sweep
    // is deferred and drained in bounded slices at later safe points, so the
    // reclaim is no longer part of the stop-the-world pause. The next mark and
    // any forced GC finish the sweep first (marks must stay intact until then).
    /// True while the deferred sweep is draining.
    sweep_in_progress: bool,
    /// How the concurrent marker handles page vectors (`NEOVM_GC_VEC_SCAN`,
    /// read once, here at construction): the Tier-B snapshot (the default),
    /// or, for the F-G measurement only, deferral to the termination.
    vec_scan: knobs::VecScanMode,
    /// Native GEN0's exclusion, exact-cons admission and vector/record window
    /// words, encoded in separate
    /// one-byte Cells. A single size-16 field sorts ahead of the old byte tail
    /// in Rust's layout; these size-one fields retain the old field offsets.
    /// Contiguity is asserted in barrier_window before generated code reads
    /// each unaligned word. Only this heap's executing mutator publishes;
    /// the GC worker never reads these Cells, and publication has no callback
    /// or safepoint, matching the JitHeapState Cells' contract.
    compiled_observation_exclusion_00: Cell<u8>,
    compiled_observation_exclusion_01: Cell<u8>,
    compiled_observation_exclusion_02: Cell<u8>,
    compiled_observation_exclusion_03: Cell<u8>,
    compiled_observation_exclusion_04: Cell<u8>,
    compiled_observation_exclusion_05: Cell<u8>,
    compiled_observation_exclusion_06: Cell<u8>,
    compiled_observation_exclusion_07: Cell<u8>,
    compiled_observation_exclusion_08: Cell<u8>,
    compiled_observation_exclusion_09: Cell<u8>,
    compiled_observation_exclusion_10: Cell<u8>,
    compiled_observation_exclusion_11: Cell<u8>,
    compiled_observation_exclusion_12: Cell<u8>,
    compiled_observation_exclusion_13: Cell<u8>,
    compiled_observation_exclusion_14: Cell<u8>,
    compiled_observation_exclusion_15: Cell<u8>,
    compiled_observation_exclusion_16: Cell<u8>,
    compiled_observation_exclusion_17: Cell<u8>,
    compiled_observation_exclusion_18: Cell<u8>,
    compiled_observation_exclusion_19: Cell<u8>,
    compiled_observation_exclusion_20: Cell<u8>,
    compiled_observation_exclusion_21: Cell<u8>,
    compiled_observation_exclusion_22: Cell<u8>,
    compiled_observation_exclusion_23: Cell<u8>,
    compiled_observation_exclusion_24: Cell<u8>,
    compiled_observation_exclusion_25: Cell<u8>,
    compiled_observation_exclusion_26: Cell<u8>,
    compiled_observation_exclusion_27: Cell<u8>,
    compiled_observation_exclusion_28: Cell<u8>,
    compiled_observation_exclusion_29: Cell<u8>,
    compiled_observation_exclusion_30: Cell<u8>,
    compiled_observation_exclusion_31: Cell<u8>,
    compiled_observation_exclusion_32: Cell<u8>,
    compiled_observation_exclusion_33: Cell<u8>,
    compiled_observation_exclusion_34: Cell<u8>,
    compiled_observation_exclusion_35: Cell<u8>,
    compiled_observation_exclusion_36: Cell<u8>,
    compiled_observation_exclusion_37: Cell<u8>,
    compiled_observation_exclusion_38: Cell<u8>,
    compiled_observation_exclusion_39: Cell<u8>,
    /// GEN1 card-table base address, one native-endian word in byte Cells
    /// after the exclusion words for the same reason: compiled cons stores
    /// read it (`HEAP_JIT_CARD_TABLE`) without moving any older offset.
    /// Published once, by `install_card_table`, before any GEN1 leaf runs.
    card_table_base_0: Cell<u8>,
    card_table_base_1: Cell<u8>,
    card_table_base_2: Cell<u8>,
    card_table_base_3: Cell<u8>,
    card_table_base_4: Cell<u8>,
    card_table_base_5: Cell<u8>,
    card_table_base_6: Cell<u8>,
    card_table_base_7: Cell<u8>,
    /// Executing mutator's protocol followed by this heap's stopped-handshake
    /// window mirrors. The journal allocation stays with ObservationWorld.
    store: std::mem::MaybeUninit<super::collection_reads::StoreState>,

    /// Creation-order identities for the final overlay precedence tie-break
    /// (`OverlaySerial`). One counter per World's heap; atomic so the World
    /// is N-mutator ready. Appended after the frozen store ABI fields, so no
    /// pinned offset moves.
    overlay_serial: crate::heap_types::OverlaySerialCounter,
}

impl Default for TaggedHeap {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Post-mark ownership verification gate (DIVERGENCES.md 162)
// ---------------------------------------------------------------------------
//
// `verify_marked_objects_owned` was written for the missing-root class and
// then never called ("dead code written for exactly this failure", 161's own
// residual list). It is O(live objects) per collection, so it stays off by
// default and is turned on either process-wide with `NEOVM_GC_VERIFY_MARKED=1`
// — the companion to `NEOVM_GC_STRESS=1`, which is what makes a missing root
// deterministic — or per-thread from a test.

#[cfg(debug_assertions)]
thread_local! {
    static VERIFY_MARKED_OBJECTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(debug_assertions)]
fn verify_marked_objects_enabled() -> bool {
    static FROM_ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let from_env =
        *FROM_ENV.get_or_init(|| std::env::var("NEOVM_GC_VERIFY_MARKED").as_deref() == Ok("1"));
    from_env || VERIFY_MARKED_OBJECTS.with(|flag| flag.get())
}

/// Turn post-mark ownership verification on for THIS thread.
#[cfg(all(debug_assertions, test))]
pub(crate) fn set_verify_marked_objects_for_test(on: bool) {
    VERIFY_MARKED_OBJECTS.with(|flag| flag.set(on));
}

static_assertions::assert_not_impl_any!(TaggedHeap: Send, Sync);

/// Outcome of [`TaggedHeap::note_image_object`]: whether the object now carries
/// the loader's image-residency stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageStamp {
    /// The header now carries `HeaderFlags::image_resident`.
    Stamped,
    /// The object cannot carry the stamp: a cons (no header), a mapped image
    /// object (residency lives in the mapped side tables), or an object this
    /// heap does not own.
    NotStamped,
}

impl TaggedHeap {
    pub fn new() -> Self {
        super::collection_reads::initialize();
        let chunk_map = knobs::chunk_map_on().then(|| std::sync::Arc::new(ChunkMap::new()));
        // One existing lifetime identity binds the heap, canonical cold
        // registry and allocator work indexes; no second identity is issued.
        let identity = HeapIdentity::issue();
        let mut heap = Self {
            store: std::mem::MaybeUninit::uninit(),
            generational: generational::GenState::new(
                std::env::var("NEOVM_GC_GENERATIONAL").as_deref() != Ok("0"),
            ),
            jit: JitHeapState::new(),
            region_book: RegionBook::new(),
            region_stats: RegionStats::default(),
            identity,
            chunk_map: chunk_map.clone().map(HeapChunkMap::new),
            cons_blocks: Vec::new(),
            cons_block_index_by_base: FxHashMap::default(),
            mark_cons_block_cache: None,
            all_objects: std::ptr::null_mut(),
            tenured_objects: std::ptr::null_mut(),
            non_cons_object_addrs: FxHashSet::default(),
            vector_object_addrs: FxHashSet::default(),
            mutator_gc: MutatorGcState::new(),
            gc_threshold: 1_000_000 * size_of::<usize>(),
            gc_threshold_overridden: false,
            live_bytes: 0,
            must_finish_count: 0,
            forced_termination_pending: false,
            pace_mark_start: None,
            pace_mark_start_bytes: 0,
            pace_alloc_rate_bps: 0,
            pace_mark_dur_us: 0,
            pace_lead_bytes: 0,
            gray_queue: Vec::new(),
            marked_symbols: SymbolMarkBits::default(),
            weak_hash_tables: Vec::new(),
            weak_hash_tables_set: rustc_hash::FxHashSet::default(),
            always_swept_weak_tables: Vec::new(),
            always_swept_weak_tables_set: rustc_hash::FxHashSet::default(),
            finalizer_registry: Vec::new(),
            doomed_finalizer_functions: Vec::new(),
            pending_surface_destroys: Vec::new(),
            pending_video_destroys: Vec::new(),
            cons_allocator: ConsAllocatorOwner::new(identity),
            float_arena: ObjectArena::new(chunk_map.clone()),
            string_arena: ObjectArena::new(chunk_map.clone()),
            canonical_empty_strings: CanonicalEmptyStrings::default(),
            vector_arena: ObjectArena::new(chunk_map.clone()),
            bytecode_arena: ObjectArena::new(chunk_map.clone()),
            lambda_arena: ObjectArena::new(chunk_map.clone()),
            macro_arena: ObjectArena::new(chunk_map.clone()),
            record_arena: ObjectArena::new(chunk_map.clone()),
            symbol_with_pos_arena: ObjectArena::new(chunk_map.clone()),
            marker_arena: ObjectArena::new(chunk_map.clone()),
            bignum_arena: ObjectArena::new(chunk_map),
            mapped_cons_ranges: Vec::new(),
            mapped_float_ranges: Vec::new(),
            mapped_veclike_objects: Vec::new(),
            mapped_veclike_index_by_addr: FxHashMap::default(),
            mapped_string_objects: Vec::new(),
            mapped_string_index_by_addr: FxHashMap::default(),
            cons_live_count: 0,
            marker_chain_head_slots: Vec::new(),
            buffer_registry: Vec::new(),
            window_registry: FxHashMap::default(),
            frame_registry: FxHashMap::default(),
            timer_registry: FxHashMap::default(),
            process_registry: FxHashMap::default(),
            write_tracking_mode: WriteTrackingMode::Disabled,
            dirty_owners: Vec::new(),
            first_cycle_concurrent: false,
            staged_mapped_cons_scan: None,
            staged_mapped_veclikes: None,
            image_premarked: false,
            #[cfg(test)]
            mapped_veclike_traces: 0,
            overlay_serial: crate::heap_types::OverlaySerialCounter::new(),
            dirty_owner_bits: FxHashSet::default(),
            dirty_writes: Vec::new(),
            gc_collections: 0,
            gc_total_elapsed_us: 0,
            last_clear_us: 0,
            last_clear_cons_us: 0,
            last_clear_noncons_us: 0,
            last_clear_mapped_us: 0,
            // Activated automatically when a pdump is registered
            // (`extend_dump_span`); a bare/no-dump heap stays on full mark-sweep.
            partition_dump: false,
            dump_blackened: false,
            bootstrap_collected: false,
            mapped_remembered: FxHashSet::default(),
            // Parity invariant: must start `Two` (see the field doc) so the
            // first flip reads pre-existing marks as unmarked.
            mark_parity: MarkParity::Two,
            mark_in_progress: false,
            incremental_mark_us: 0,
            concurrent_mark_running: false,
            satb_handoff: std::sync::Arc::new(SatbHandoff::new()),
            _satb_owner_layout_reserve: FxHashSet::default(),
            _satb_layout_reserve: mutator_gc::MutatorSatbSlot::new(identity),
            gc_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            concurrent_str_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            last_concurrent_str_claimed: 0,
            concurrent_float_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            last_concurrent_float_claimed: 0,
            concurrent_subr_dropped: std::sync::Arc::new(AtomicUsize::new(0)),
            last_concurrent_subr_dropped: 0,
            concurrent_vec_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            last_concurrent_vec_claimed: 0,
            concurrent_bc_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            last_concurrent_bc_claimed: 0,
            _satb_string_layout_reserve: FxHashSet::default(),
            gc_stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            gc_wake: std::sync::Arc::new((std::sync::Mutex::new(()), std::sync::Condvar::new())),
            gc_exited: None,
            pending_obarray_scan: None,
            concurrent_obarray_start_slots: None,
            retired_vector_buffers: Vec::new(),
            concurrent_cloned_vectors: FxHashSet::default(),
            sweep_in_progress: false,
            sweep_cons_cursor: 0,
            sweep_float_page_cursor: 0,
            sweep_string_page_cursor: 0,
            sweep_vector_page_cursor: 0,
            sweep_bytecode_page_cursor: 0,
            sweep_lambda_page_cursor: 0,
            sweep_macro_page_cursor: 0,
            sweep_record_page_cursor: 0,
            sweep_symbol_with_pos_page_cursor: 0,
            sweep_marker_page_cursor: 0,
            sweep_bignum_page_cursor: 0,
            sweep_cons_end: 0,
            sweep_float_page_end: 0,
            sweep_string_page_end: 0,
            sweep_vector_page_end: 0,
            sweep_bytecode_page_end: 0,
            sweep_lambda_page_end: 0,
            sweep_macro_page_end: 0,
            sweep_record_page_end: 0,
            sweep_symbol_with_pos_page_end: 0,
            sweep_marker_page_end: 0,
            sweep_bignum_page_end: 0,
            sweep_cons_live_cells: 0,
            sweep_noncons_pending: std::ptr::null_mut(),
            sweep_noncons_live_bytes: 0,
            sweep_mark_us: 0,
            sweep_bytes_before: 0,
            sweep_mark_window_alloc_bytes: 0,
            sweep_slice_us_total: 0,
            sweep_slice_count: 0,
            sweep_cons_blocks_swept: 0,
            sweep_noncons_freed: 0,
            sweep_lifetime_us: 0,
            sweep_lifetime_slices: 0,
            sweep_lifetime_cons_blocks_swept: 0,
            sweep_lifetime_noncons_freed: 0,
            last_termination_deferred: 0,
            max_termination_deferred: 0,
            last_termination_satb: 0,
            last_termination_kinds: DrainKinds::default(),
            max_termination_kinds: DrainKinds::default(),
            last_termination_fold_us: 0,
            termination_count: 0,
            handshake: HandshakeStats::default(),
            last_runtime_seed_us: 0,
            last_runtime_seed_roots: 0,
            last_remembered_seed_us: 0,
            last_remembered_seed_roots: 0,
            dump_addr_lo: usize::MAX,
            dump_addr_hi: 0,
            vec_scan: knobs::vec_scan_mode(),
            census: GenCensus::from_knob(),
            compiled_observation_exclusion_00: Cell::new(0),
            compiled_observation_exclusion_01: Cell::new(0),
            compiled_observation_exclusion_02: Cell::new(0),
            compiled_observation_exclusion_03: Cell::new(0),
            compiled_observation_exclusion_04: Cell::new(0),
            compiled_observation_exclusion_05: Cell::new(0),
            compiled_observation_exclusion_06: Cell::new(0),
            compiled_observation_exclusion_07: Cell::new(0),
            compiled_observation_exclusion_08: Cell::new(0),
            compiled_observation_exclusion_09: Cell::new(0),
            compiled_observation_exclusion_10: Cell::new(0),
            compiled_observation_exclusion_11: Cell::new(0),
            compiled_observation_exclusion_12: Cell::new(0),
            compiled_observation_exclusion_13: Cell::new(0),
            compiled_observation_exclusion_14: Cell::new(0),
            compiled_observation_exclusion_15: Cell::new(0),
            compiled_observation_exclusion_16: Cell::new(0),
            compiled_observation_exclusion_17: Cell::new(0),
            compiled_observation_exclusion_18: Cell::new(0),
            compiled_observation_exclusion_19: Cell::new(0),
            compiled_observation_exclusion_20: Cell::new(0),
            compiled_observation_exclusion_21: Cell::new(0),
            compiled_observation_exclusion_22: Cell::new(0),
            compiled_observation_exclusion_23: Cell::new(0),
            compiled_observation_exclusion_24: Cell::new(0),
            compiled_observation_exclusion_25: Cell::new(0),
            compiled_observation_exclusion_26: Cell::new(0),
            compiled_observation_exclusion_27: Cell::new(0),
            compiled_observation_exclusion_28: Cell::new(0),
            compiled_observation_exclusion_29: Cell::new(0),
            compiled_observation_exclusion_30: Cell::new(0),
            compiled_observation_exclusion_31: Cell::new(0),
            compiled_observation_exclusion_32: Cell::new(0),
            compiled_observation_exclusion_33: Cell::new(0),
            compiled_observation_exclusion_34: Cell::new(0),
            compiled_observation_exclusion_35: Cell::new(0),
            compiled_observation_exclusion_36: Cell::new(0),
            compiled_observation_exclusion_37: Cell::new(0),
            compiled_observation_exclusion_38: Cell::new(0),
            compiled_observation_exclusion_39: Cell::new(0),
            card_table_base_0: Cell::new(0),
            card_table_base_1: Cell::new(0),
            card_table_base_2: Cell::new(0),
            card_table_base_3: Cell::new(0),
            card_table_base_4: Cell::new(0),
            card_table_base_5: Cell::new(0),
            card_table_base_6: Cell::new(0),
            card_table_base_7: Cell::new(0),
        };
        if knobs::concurrent_claims_on() {
            heap.install_concurrent_claims();
        }
        if heap.generational.enabled {
            heap.install_card_table();
        }
        // The census's remembered-set probe widens the window compiled code
        // tests from the start (the thread-local mirror follows when the heap
        // is installed, `set_tagged_heap`).
        heap.jit.set_barrier_window(heap.barrier_window());
        heap
    }

    pub(crate) fn identity(&self) -> usize {
        self.identity.get()
    }

    /// This heap lifetime's typed identity.
    #[inline]
    pub fn heap_identity(&self) -> HeapIdentity {
        self.identity
    }

    /// The scope of the collection now running or next to run: which
    /// generations it treats as black (`GcHeader::black_by_generation`).
    /// Majors retain Full scope until their last sweep cursor completes.
    #[inline(always)]
    pub(crate) const fn collection_scope(&self) -> CollectionScope {
        if self.generational.major_in_progress {
            CollectionScope::Full
        } else {
            CollectionScope::Young
        }
    }

    pub fn set_write_tracking_mode(&mut self, mode: WriteTrackingMode) {
        self.write_tracking_mode = mode;
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|current| current.set(mode));
        self.publish_barrier_window();
        if mode == WriteTrackingMode::Disabled {
            self.clear_dirty_owners();
            self.clear_dirty_writes();
        }
    }

    pub fn write_tracking_mode(&self) -> WriteTrackingMode {
        self.write_tracking_mode
    }

    #[inline(always)]
    pub fn should_collect(&self) -> bool {
        self.bytes_since_gc() >= self.gc_threshold
    }

    /// Record that the in-flight concurrent mark is being force-terminated by
    /// the allocation cap (`bytes_since_gc > gc_threshold*4`). Called by the
    /// evaluator right before the forced `terminate_concurrent_mark`, so
    /// `incremental_finish` can treat the truncated mark window accordingly.
    pub(crate) fn note_must_finish(&mut self) {
        self.must_finish_count += 1;
        self.forced_termination_pending = true;
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC must_finish#{} bytes_since_gc={} threshold={} lead={}",
                self.must_finish_count,
                self.bytes_since_gc(),
                self.gc_threshold,
                self.pace_lead_bytes,
            );
        }
    }

    /// Lifetime count of cap-forced concurrent-mark terminations.
    pub fn must_finish_count(&self) -> u64 {
        self.must_finish_count
    }

    /// Trace probe: the projected mark-window allocation in bytes (the
    /// cap-pressure field detector). For the `NEOVM_GC concurrent_start`
    /// line; informational since the paced trigger was reverted.
    pub(crate) fn pace_probe(&self) -> usize {
        self.pace_lead_bytes
    }

    /// Fold one terminated mark window into the pacing instrumentation. A
    /// cap-forced termination is a truncated (biased-low) window: skip the
    /// EWMA sample and escalate the lead instead — repeated cap hits keep
    /// the reported pressure honest, while the next clean cycle's full
    /// recompute drops the lead right back. Zero-wall windows (no stamp /
    /// sub-µs) contribute nothing.
    fn pace_close_mark_window(&mut self, wall_us: u64, alloc_bytes: usize, forced: bool) {
        if forced {
            self.pace_lead_bytes = self
                .pace_lead_bytes
                .saturating_mul(2)
                .max(alloc_bytes)
                .min(usize::MAX / 4);
        } else if wall_us > 0 {
            let rate_sample = ((alloc_bytes as u128).saturating_mul(1_000_000) / wall_us as u128)
                .min(u64::MAX as u128) as u64;
            self.pace_alloc_rate_bps = ewma_half(self.pace_alloc_rate_bps, rate_sample);
            self.pace_mark_dur_us = ewma_half(self.pace_mark_dur_us, wall_us);
            self.pace_lead_bytes = ((self.pace_alloc_rate_bps as u128)
                .saturating_mul(self.pace_mark_dur_us as u128)
                / 1_000_000)
                .min((usize::MAX / 4) as u128) as usize;
        }
    }

    pub fn gc_threshold(&self) -> usize {
        self.gc_threshold
    }

    pub fn set_gc_threshold(&mut self, threshold: usize) {
        self.gc_threshold = threshold.max(1);
        self.gc_threshold_overridden = true;
    }

    pub fn set_gc_threshold_from_runtime(&mut self, threshold: usize) {
        if !self.gc_threshold_overridden {
            self.gc_threshold = threshold.max(1);
        }
    }

    pub fn clear_gc_threshold_override(&mut self) {
        self.gc_threshold_overridden = false;
    }

    pub fn gc_threshold_is_overridden(&self) -> bool {
        self.gc_threshold_overridden
    }

    /// Allocated objects (cons + non-cons), exact: the open allocation
    /// regions' unused cells are not counted.
    #[inline]
    pub fn allocated_count(&self) -> usize {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| state.exact_allocated_count(delta))
            .sum()
    }

    /// Total number of completed GC collection cycles since this heap was
    /// created. Used by allocation benchmarks to measure GC frequency.
    pub fn gc_collections(&self) -> usize {
        self.gc_collections
    }

    /// Deferred-sweep cost + termination-drain instrumentation snapshot.
    /// Read the collector's mark bit without exposing collector internals to
    /// evaluator regression fixtures. Call only while the fixture excludes writes.
    #[cfg(test)]
    pub(crate) fn value_is_marked_for_test(&self, value: TaggedValue) -> bool {
        self.is_value_marked(value)
    }

    pub(crate) fn sweep_stats(&self) -> SweepStats {
        SweepStats {
            sweep_us: self.sweep_slice_us_total,
            slice_count: self.sweep_slice_count,
            cons_blocks_swept: self.sweep_cons_blocks_swept,
            noncons_freed: self.sweep_noncons_freed,
            lifetime_sweep_us: self.sweep_lifetime_us,
            lifetime_slices: self.sweep_lifetime_slices,
            lifetime_cons_blocks_swept: self.sweep_lifetime_cons_blocks_swept,
            lifetime_noncons_freed: self.sweep_lifetime_noncons_freed,
            last_termination_deferred: self.last_termination_deferred,
            max_termination_deferred: self.max_termination_deferred,
            last_termination_satb: self.last_termination_satb,
            last_termination_partial_words: self.last_termination_partial_words(),
            last_termination_kinds: self.last_termination_kinds,
            max_termination_kinds: self.max_termination_kinds,
            last_concurrent_str_claimed: self.last_concurrent_str_claimed,
            last_concurrent_float_claimed: self.last_concurrent_float_claimed,
            last_concurrent_subr_dropped: self.last_concurrent_subr_dropped,
            last_concurrent_vec_claimed: self.last_concurrent_vec_claimed,
            last_concurrent_bc_claimed: self.last_concurrent_bc_claimed,
            last_termination_fold_us: self.last_termination_fold_us,
            termination_count: self.termination_count,
            mark_us: self.sweep_mark_us,
        }
    }

    /// Handshake-pause instrumentation snapshot (per phase, per root group,
    /// size probes). Sibling of `sweep_stats`.
    pub(crate) fn handshake_stats(&self) -> HandshakeStats {
        self.handshake.clone()
    }

    /// Mutable access for the evaluator to record the context-side handshake
    /// parts (root-group breakdowns, whole-pause totals, context probes).
    pub(crate) fn handshake_stats_mut(&mut self) -> &mut HandshakeStats {
        &mut self.handshake
    }

    #[inline]
    pub(crate) fn add_memory_use_count(&mut self, slot: MemoryUseCountSlot, delta: u64) {
        let index = slot.index();
        let counts = &mut self.current_mutator_gc_mut().memory_use_counts;
        counts[index] = counts[index].wrapping_add(delta);
    }

    /// The Lisp-visible allocation counts (`memory-use-counts`), exact: one
    /// per object handed out, the open allocation regions' unused cells
    /// excluded (they were charged when the region was granted).
    #[inline]
    pub(crate) fn memory_use_counts_snapshot(&self) -> [u64; MEMORY_USE_COUNT_LEN] {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| state.exact_memory_use_counts(delta))
            .fold([0; MEMORY_USE_COUNT_LEN], |mut total, counts| {
                for (sum, count) in total.iter_mut().zip(counts) {
                    *sum = sum.wrapping_add(count);
                }
                total
            })
    }

    /// Bytes allocated since the last collection, as CHARGED: an open
    /// allocation region counts whole. Never below the exact count, and a
    /// region is never granted past the threshold
    /// (`TaggedHeap::region_budget`), so the pacing gates that read this
    /// collect when they did before — at most one region early, never late.
    #[inline(always)]
    pub fn bytes_since_gc(&self) -> usize {
        self.mutators().map(|state| state.bytes_since_gc).sum()
    }

    /// Bytes allocated since the last collection, exact (GNU's `since_gc`):
    /// the open allocation regions' unused cells excluded. What
    /// `garbage-collect-maybe`'s FACTOR test and the memory profiler read.
    #[inline]
    pub fn bytes_since_gc_exact(&self) -> usize {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| state.exact_bytes_since_gc(delta))
            .sum()
    }

    /// The one place `bytes_since_gc` returns to zero.
    ///
    /// It banks what is being cleared, so that [`Self::total_allocated_bytes`]
    /// stays exact without the allocation path counting it a second time.
    /// Every collector site resets through here for that reason.
    pub(crate) fn reset_bytes_since_gc(&mut self) {
        // Close first, so what is banked is exactly what was handed out.
        self.close_alloc_regions();
        for state in self.mutators_mut() {
            state.bytes_banked_at_resets = state
                .bytes_banked_at_resets
                .saturating_add(state.bytes_since_gc as u64);
            state.bytes_since_gc = 0;
        }
    }

    pub fn live_bytes(&self) -> usize {
        self.live_bytes
    }

    pub fn buffer_value(&self, id: crate::buffer::BufferId) -> Option<TaggedValue> {
        self.buffer_registry.get(id.0 as usize).copied().flatten()
    }

    pub fn register_buffer_value(&mut self, id: crate::buffer::BufferId, value: TaggedValue) {
        let idx = id.0 as usize;
        if self.buffer_registry.len() <= idx {
            self.buffer_registry.resize(idx + 1, None);
        }
        self.buffer_registry[idx] = Some(value);
    }

    pub fn window_value(&self, id: u64) -> Option<TaggedValue> {
        self.window_registry.get(&id).copied()
    }

    pub fn register_window_value(&mut self, id: u64, value: TaggedValue) {
        self.window_registry.insert(id, value);
    }

    pub fn frame_value(&self, id: u64) -> Option<TaggedValue> {
        self.frame_registry.get(&id).copied()
    }

    pub fn register_frame_value(&mut self, id: u64, value: TaggedValue) {
        self.frame_registry.insert(id, value);
    }

    pub fn timer_value(&self, id: u64) -> Option<TaggedValue> {
        self.timer_registry.get(&id).copied()
    }

    pub fn register_timer_value(&mut self, id: u64, value: TaggedValue) {
        self.timer_registry.insert(id, value);
    }

    pub fn process_value(&self, id: crate::emacs_core::process::ProcessId) -> Option<TaggedValue> {
        self.process_registry.get(&id).copied()
    }

    pub fn register_process_value(
        &mut self,
        id: crate::emacs_core::process::ProcessId,
        value: TaggedValue,
    ) {
        self.process_registry.insert(id, value);
    }

    /// Register cons cells whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `start..start+len` must remain mapped and writable for the lifetime of
    /// this heap.  The range must contain aligned `ConsCell` objects.
    pub(crate) unsafe fn register_mapped_cons_range(&mut self, start: *mut ConsCell, len: usize) {
        if len == 0 {
            return;
        }
        debug_assert_eq!(start as usize % std::mem::align_of::<ConsCell>(), 0);
        self.extend_dump_span(start as usize, len.saturating_mul(size_of::<ConsCell>()));
        self.mapped_cons_ranges
            .push(MappedConsRange::new(start, len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(len);
        self.live_bytes = self
            .live_bytes
            .saturating_add(len.saturating_mul(size_of::<ConsCell>()));
    }

    /// Register float objects whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `start..start+len` must remain mapped and writable for the lifetime of
    /// this heap.  The range must contain aligned `FloatObj` objects.
    pub(crate) unsafe fn register_mapped_float_range(&mut self, start: *mut FloatObj, len: usize) {
        if len == 0 {
            return;
        }
        debug_assert_eq!(start as usize % std::mem::align_of::<FloatObj>(), 0);
        self.extend_dump_span(start as usize, len.saturating_mul(size_of::<FloatObj>()));
        self.mapped_float_ranges
            .push(MappedFloatRange::new(start, len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(len);
        self.live_bytes = self
            .live_bytes
            .saturating_add(len.saturating_mul(size_of::<FloatObj>()));
    }

    /// Register a vectorlike object whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `header` must point at a complete, aligned vectorlike object that remains
    /// mapped and writable for the lifetime of this heap.
    /// Pre-size the mapped-object registries for a load about to register
    /// `veclikes` + `strings` objects (a 12K-entry FxHashMap grown by
    /// rehashing costs several M Ir across a pdump load).
    pub fn reserve_mapped_object_capacity(&mut self, veclikes: usize, strings: usize) {
        self.mapped_veclike_objects.reserve(veclikes);
        self.mapped_veclike_index_by_addr.reserve(veclikes);
        self.mapped_string_objects.reserve(strings);
        self.mapped_string_index_by_addr.reserve(strings);
    }

    pub(crate) unsafe fn register_mapped_veclike_object(
        &mut self,
        header: *mut VecLikeHeader,
        byte_len: usize,
    ) {
        if byte_len == 0 {
            return;
        }
        debug_assert_eq!(header as usize % std::mem::align_of::<VecLikeHeader>(), 0);
        self.extend_dump_span(header as usize, byte_len);
        let index = self.mapped_veclike_objects.len();
        let prev = self
            .mapped_veclike_index_by_addr
            .insert(header as usize, index);
        debug_assert!(prev.is_none(), "mapped vectorlike object registered twice");
        self.mapped_veclike_objects
            .push(MappedVecLikeObject::new(header, byte_len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(1);
        self.live_bytes = self.live_bytes.saturating_add(byte_len);
    }

    /// Register a string object whose storage is owned by the loaded pdump image.
    ///
    /// # Safety
    /// `ptr` must point at a complete, aligned string object that remains
    /// mapped and writable for the lifetime of this heap.
    pub(crate) unsafe fn register_mapped_string_object(
        &mut self,
        ptr: *mut StringObj,
        byte_len: usize,
    ) {
        if byte_len == 0 {
            return;
        }
        debug_assert_eq!(ptr as usize % std::mem::align_of::<StringObj>(), 0);
        self.extend_dump_span(ptr as usize, byte_len);
        let index = self.mapped_string_objects.len();
        let prev = self.mapped_string_index_by_addr.insert(ptr as usize, index);
        debug_assert!(prev.is_none(), "mapped string object registered twice");
        self.mapped_string_objects
            .push(MappedStringObject::new(ptr, byte_len));
        let state = self.current_mutator_gc_mut();
        state.allocated_count = state.allocated_count.saturating_add(1);
        self.live_bytes = self.live_bytes.saturating_add(byte_len);

        let string = unsafe { &(*ptr).data };
        if string.sbytes() == 0 {
            let value = unsafe { TaggedValue::from_string_ptr(ptr) };
            self.canonical_empty_strings
                .install_mapped(string.storage_kind(), value);
        }
    }

    /// Stamp VALUE, a heap object the pdump loader reconstructed from a
    /// dump-image row, as an image resident.
    ///
    /// The first partition cycle's promotion makes exactly these objects
    /// permanent; every other survivor is tenured only. Mapped image objects
    /// need no stamp (their residency is the mapped mark side tables), and
    /// conses have no header: both are ignored, as is anything this heap does
    /// not own.
    ///
    /// The loader calls this once per reconstructed image row before the
    /// image is published, so the byte is written before any mutator or
    /// collector can observe the object.
    pub(crate) fn note_image_object(&mut self, value: TaggedValue) -> ImageStamp {
        if value.is_cons() {
            return ImageStamp::NotStamped;
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return ImageStamp::NotStamped;
        };
        if self.owner_is_mapped(value) || !self.owns_heap_value_object(value, addr) {
            return ImageStamp::NotStamped;
        }
        // SAFETY: the ownership oracle just proved this is a live initialized
        // non-cons heap object, and the loader runs single-threaded before the
        // image is published, so no mark or mutator reads the byte.
        unsafe {
            let header = &mut *(addr as *mut GcHeader);
            header.flags = header.flags.with_image_resident();
        }
        ImageStamp::Stamped
    }

    pub fn dirty_owner_count(&self) -> usize {
        self.dirty_owners.len()
    }

    pub fn is_dirty_owner(&self, owner: TaggedValue) -> bool {
        self.dirty_owner_bits.contains(&owner.bits())
    }

    pub fn take_dirty_owners(&mut self) -> Vec<TaggedValue> {
        self.dirty_owner_bits.clear();
        std::mem::take(&mut self.dirty_owners)
    }

    pub fn clear_dirty_owners(&mut self) {
        self.dirty_owners.clear();
        self.dirty_owner_bits.clear();
    }

    pub fn dirty_write_count(&self) -> usize {
        self.dirty_writes.len()
    }

    pub fn dirty_writes(&self) -> &[HeapWriteRecord] {
        &self.dirty_writes
    }

    pub fn take_dirty_writes(&mut self) -> Vec<HeapWriteRecord> {
        std::mem::take(&mut self.dirty_writes)
    }

    pub fn clear_dirty_writes(&mut self) {
        self.dirty_writes.clear();
    }

    fn record_heap_write(&mut self, record: HeapWriteRecord) {
        #[cfg(test)]
        RECORD_HEAP_WRITE_CALLS.with(|calls| calls.set(calls.get() + 1));
        // Dump partition: a mutated dumped object may now hold heap children,
        // so remember it as a permanent root. Conservative — a false positive
        // (a heap owner inside the dump address span) just adds a redundant
        // root; a false negative would be a use-after-free, so the span test
        // must cover every mapped object (see `register_mapped_*`).
        debug_assert!(
            self.generational.enabled
                || self.partition_dump
                || !self.value_is_tenured(record.owner),
            "tenured owner without partition or generations: {:?}",
            record.owner,
        );
        #[cfg(debug_assertions)]
        self.debug_assert_remembered_membership(record.owner);
        if self.generational.enabled {
            if self.concurrent_mark_running && self.generational.major_in_progress {
                if record.owner.is_cons() {
                    // A claimed cons can gain an existing white old object;
                    // SATB preimages and allocate-black births do not cover it.
                    // Deduplicate owners at the stopped-world join, then trace
                    // their current children before any weak decision or free.
                    self.current_mutator_gc_mut()
                        .major_cons_writes
                        .push(record.owner);
                }
                if let Some(value) = record.value
                    && let crate::tagged::value::ValueKind::Symbol(id) = value.gc_kind()
                {
                    // Bare symbols have no born-black heap header. Keep the
                    // inserted-symbol fact for roots and non-cons owners too.
                    self.current_mutator_gc_mut()
                        .major_symbol_preimages
                        .push(id);
                }
            }
            // Concurrent majors use SATB plus P-all. Persistent image-owner
            // additions still log locally; ordinary R resumes after termination.
            if self.concurrent_mark_running || record.value.is_none_or(|value| !value.is_fixnum()) {
                self.remember_owner(record.owner);
            }
        } else if self.partition_dump
            && (self.owner_is_mapped(record.owner) || self.value_is_tenured(record.owner))
        {
            let bits = record.owner.bits();
            self.remember_owner(record.owner);
            self.current_mutator_gc_mut().remembered_cache[barrier_cache_slot(bits)] = bits;
        }
        // SATB (snapshot-at-the-beginning) barrier. Runs BEFORE the store, so the
        // owner's current children are its PRE-overwrite values; logging them
        // keeps the start-of-cycle snapshot live. Nothing is re-read later, so
        // the concurrent GC thread never touches a reallocated owner.
        if self.concurrent_mark_running {
            // The background GC thread is marking — log overwritten children to
            // the shared buffer it drains (not the local gray queue, which
            // belongs to the GC thread for the duration). This SATB barrier keeps
            // the start-of-cycle snapshot live without re-reading a mutated owner.
            self.push_value_children_to_satb(record.owner);
        }
        if self.write_tracking_mode == WriteTrackingMode::Disabled {
            return;
        }
        if self.dirty_owner_bits.insert(record.owner.bits()) {
            self.dirty_owners.push(record.owner);
        }
        if self.write_tracking_mode == WriteTrackingMode::OwnersAndRecords {
            self.dirty_writes.push(record);
        }
    }

    /// Single setter of logged state: remembered implies membership in a
    /// mutator R log, collector R_seed, or the persistent mapped set.
    /// Atomic claims deduplicate owned headers/conses across mutators;
    /// mapped dedup is per-mutator and duplicates are merged at the drain.
    /// The stop-all boundary must wait for claims to finish appending.
    pub(super) fn remember_owner(&mut self, owner: TaggedValue) {
        if self.generational.enabled {
            let mapped = self.owner_is_mapped(owner);
            let major_mark = self.is_generational_major_marking();
            if mapped {
                if self
                    .current_mutator_gc_mut()
                    .r_mapped_seen
                    .insert(owner.bits())
                    && !major_mark
                {
                    self.current_mutator_gc_mut().remset.push(owner);
                }
            } else if owner.is_cons() {
                // An old owned cons is remembered by its card alone, as
                // compiled stores do; the minor traces its dirty cards.
                if self
                    .old_cons_trailer(owner)
                    .is_some_and(|(trailer, index)| trailer.age_is_old(index))
                {
                    self.mark_owner_card(owner.gc_cons_ptr() as usize);
                }
            } else if let Some(addr) = Self::value_heap_addr(owner) {
                let header = unsafe { &*(addr as *const GcHeader) };
                if !header.tenured {
                    return;
                }
                let permanent = header.generation.permanent();
                if permanent {
                    self.current_mutator_gc_mut()
                        .r_mapped_seen
                        .insert(owner.bits());
                }
                if !major_mark && unsafe { &*(addr as *const GcHeader) }.claim_remembered() {
                    self.current_mutator_gc_mut().remset.push(owner);
                }
            }
            return;
        }
        self.mapped_remembered.insert(owner.bits());
        if owner.is_cons() || self.owner_is_mapped(owner) {
            return;
        }
        if let Some(addr) = Self::value_heap_addr(owner) {
            let header = unsafe { &*(addr as *const GcHeader) };
            debug_assert!(header.tenured, "only tenured owners are remembered");
            header
                .remembered
                .store(RememberedState::Logged as u8, Ordering::Relaxed);
        }
    }

    /// Enter the mapped (dump-image) cons CONS into the dump remembered set
    /// before anything writes it -- exactly what `record_heap_write` does at
    /// its first write -- so that later writes by compiled code may be plain
    /// stores while neither a concurrent mark nor owner tracking is on
    /// (P1.4 Stage B: a buffer-local variable's dumped default cell, which
    /// sits in the barrier window's dump span). The set is append-only for
    /// the heap's life and mapped objects are never freed, so the entry
    /// outlives every leaf baked against it (the JIT cache is dropped with
    /// the heap). A spurious entry only re-scans one cons per collection.
    /// `false`, doing nothing, for anything but a mapped cons of a heap with
    /// the dump partition on.
    pub(crate) fn remember_mapped_cons_ahead_of_writes(&mut self, cons: TaggedValue) -> bool {
        if self.generational.enabled
            || !cons.is_cons()
            || !self.partition_dump
            || !self.owner_is_mapped(cons)
        {
            return false;
        }
        let bits = cons.bits();
        self.remember_owner(cons);
        self.current_mutator_gc_mut().remembered_cache[barrier_cache_slot(bits)] = bits;
        true
    }

    /// Raw object address for a heap-tagged value (cons/veclike/string/float),
    /// used for the dump-partition address-span test.
    fn value_heap_addr(value: TaggedValue) -> Option<usize> {
        value.gc_heap_ptr().map(|ptr| ptr as usize)
    }

    /// True if `value` is a mapped (pdump) object, via the address span that
    /// `register_mapped_*` keeps over every mapped object.
    fn owner_is_mapped(&self, value: TaggedValue) -> bool {
        match Self::value_heap_addr(value) {
            Some(addr) => addr >= self.dump_addr_lo && addr < self.dump_addr_hi,
            None => false,
        }
    }

    /// Extend the mapped-object address span to cover `[start, start+len)`.
    ///
    /// The first registered mapped object activates the dump partition (and its
    /// generational/incremental collector): a heap with a loaded pdump runs the
    /// low-pause collector, while a bare heap with no dump (unit tests, the
    /// pre-dump bootstrap loader) stays on the simple full mark-sweep path. This
    /// is intrinsic to whether there is anything to partition — not a tunable.
    fn extend_dump_span(&mut self, start: usize, len_bytes: usize) {
        if len_bytes == 0 {
            return;
        }
        self.dump_addr_lo = self.dump_addr_lo.min(start);
        self.dump_addr_hi = self.dump_addr_hi.max(start.saturating_add(len_bytes));
        TAGGED_HEAP_DUMP_SPAN.with(|s| s.set((self.dump_addr_lo, self.dump_addr_hi)));
        if !self.partition_dump {
            self.partition_dump = true;
            // Keep the write-barrier hot-path mirror in sync so the dump
            // remembered set starts being maintained immediately.
            TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.set(true));
        }
        self.publish_barrier_window();
    }

    /// True when a registered mapped span (a loaded pdump) has activated the
    /// dump-partitioned collector. Diagnostics: lets the drain-kind profiling
    /// probe verify which collector configuration it is measuring.
    #[cfg(test)]
    pub(crate) fn dump_partition_active(&self) -> bool {
        self.partition_dump
    }

    /// Charge an allocation to the consing counter.
    ///
    /// This deliberately does NOT advance `live_bytes`.  `live_bytes` is what
    /// the last sweep actually counted, and both adaptive pacing terms
    /// multiply it, so charging every allocation to it made the collection
    /// threshold chase the consing counter: the threshold grew with each
    /// allocation, `should_collect` could never become true, and the sweep
    /// that would have corrected `live_bytes` was exactly the thing being
    /// prevented.  In `--batch`, where nothing else forces a collection, that
    /// meant none ever ran -- GNU performed 78 collections on a loop that
    /// neomacs completed with 0.  The pacer compensates for recent allocation
    /// on its own, adding `bytes_since_gc / 2` to the live estimate, which is
    /// GNU's `total_bytes_of_live_objects () + since_gc` shape
    /// (`consing_threshold`, `src/alloc.c`).
    fn note_allocation_bytes(&mut self, bytes: usize) {
        // GNU charges an allocation to ONE counter here (`consing_until_gc`,
        // `src/alloc.c`) and totals the rest at collection time.  This charged
        // three, and the third carried no information the other two do not:
        // the lifetime total is what has been banked at the resets plus what
        // has accumulated since.  Every cons in the engine paid for that
        // extra saturating add.
        // A plain add: a usize of allocated bytes cannot overflow.
        self.current_mutator_gc_mut().bytes_since_gc += bytes;
    }

    /// Every byte this heap has ever allocated.
    ///
    /// Derived rather than counted: `bytes_banked_at_resets` is advanced by
    /// [`Self::reset_bytes_since_gc`], the one place `bytes_since_gc` returns
    /// to zero, so the sum is exact by construction.
    pub(crate) fn total_allocated_bytes(&self) -> u64 {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| {
                state
                    .bytes_banked_at_resets
                    .saturating_add(state.exact_bytes_since_gc(delta) as u64)
            })
            .fold(0, u64::saturating_add)
    }

    fn vector_storage_bytes<T>(values: &Vec<T>) -> usize {
        values.capacity().saturating_mul(size_of::<T>())
    }

    fn lisp_value_vec_storage_bytes(values: &LispValueVec) -> usize {
        values
            .owned_capacity()
            .saturating_mul(size_of::<TaggedValue>())
    }

    fn string_object_bytes(obj: &StringObj) -> usize {
        size_of::<StringObj>().saturating_add(obj.data.byte_len())
    }

    fn hash_table_object_bytes(obj: &HashTableObj) -> usize {
        size_of::<HashTableObj>().saturating_add(obj.table.data.known_storage_bytes())
    }

    fn lambda_object_bytes(obj: &LambdaObj) -> usize {
        size_of::<LambdaObj>().saturating_add(Self::lisp_value_vec_storage_bytes(&obj.data))
    }

    fn macro_object_bytes(obj: &MacroObj) -> usize {
        size_of::<MacroObj>().saturating_add(Self::lisp_value_vec_storage_bytes(&obj.data))
    }

    fn bytecode_object_bytes(obj: &ByteCodeObj) -> usize {
        let data = &obj.data;
        size_of::<ByteCodeObj>()
            .saturating_add(data.resident_ops_capacity().saturating_mul(size_of::<Op>()))
            .saturating_add(
                data.constants
                    .owned_capacity()
                    .saturating_mul(size_of::<TaggedValue>()),
            )
            .saturating_add(data.params.named().map_or(0, |params| {
                params
                    .required
                    .capacity()
                    .saturating_add(params.optional.capacity())
                    .saturating_mul(size_of::<SymId>())
            }))
            .saturating_add(
                data.resident_gnu_byte_offset_map_capacity()
                    .saturating_mul(size_of::<GnuByteOffsetMapEntry>()),
            )
            .saturating_add(
                data.gnu_bytecode_bytes
                    .as_ref()
                    .map_or(0, |bytes| bytes.owned_bytes()),
            )
            .saturating_add(Self::vector_storage_bytes(&data.extra_slots))
            .saturating_add(data.docstring.as_ref().map_or(0, |doc| doc.sbytes()))
    }

    fn record_object_bytes(obj: &RecordObj) -> usize {
        size_of::<RecordObj>().saturating_add(Self::lisp_value_vec_storage_bytes(&obj.data))
    }

    fn font_object_bytes(obj: &FontObj) -> usize {
        let identity = &obj.data.identity;
        size_of::<FontObj>()
            .saturating_add(Self::lisp_value_vec_storage_bytes(&obj.data.fields))
            .saturating_add(identity.stable_key.capacity())
            .saturating_add(identity.file_path.as_ref().map_or(0, String::capacity))
            .saturating_add(
                identity
                    .postscript_name
                    .as_ref()
                    .map_or(0, String::capacity),
            )
            .saturating_add(
                identity
                    .variation_coords
                    .capacity()
                    .saturating_mul(
                        size_of::<neomacs_display_protocol::font::FontVariationCoord>(),
                    ),
            )
    }

    fn obarray_object_bytes(obj: &ObarrayObj) -> usize {
        size_of::<ObarrayObj>().saturating_add(Self::lisp_value_vec_storage_bytes(&obj.buckets))
    }

    fn object_bytes_from_header(header: *const GcHeader) -> usize {
        unsafe {
            match (*header).kind {
                HeapObjectKind::String => Self::string_object_bytes(&*(header as *const StringObj)),
                HeapObjectKind::Float => size_of::<FloatObj>(),
                HeapObjectKind::VecLike => {
                    let ptr = header as *const VecLikeHeader;
                    match (*ptr).type_tag {
                        VecLikeType::Vector => {
                            let obj = &*(ptr as *const VectorObj);
                            size_of::<VectorObj>()
                                .saturating_add(Self::lisp_value_vec_storage_bytes(&obj.data))
                        }
                        VecLikeType::CharTable => {
                            let obj = &*(ptr as *const CharTableObj);
                            size_of::<CharTableObj>()
                                .saturating_add(Self::lisp_value_vec_storage_bytes(&obj.extras))
                        }
                        VecLikeType::SubCharTable => {
                            let obj = &*(ptr as *const SubCharTableObj);
                            size_of::<SubCharTableObj>()
                                .saturating_add(Self::lisp_value_vec_storage_bytes(&obj.contents))
                        }
                        VecLikeType::HashTable => {
                            Self::hash_table_object_bytes(&*(ptr as *const HashTableObj))
                        }
                        VecLikeType::Obarray => {
                            Self::obarray_object_bytes(&*(ptr as *const ObarrayObj))
                        }
                        VecLikeType::Lambda => {
                            Self::lambda_object_bytes(&*(ptr as *const LambdaObj))
                        }
                        VecLikeType::Macro => Self::macro_object_bytes(&*(ptr as *const MacroObj)),
                        VecLikeType::ByteCode => {
                            Self::bytecode_object_bytes(&*(ptr as *const ByteCodeObj))
                        }
                        VecLikeType::Record | VecLikeType::WindowConfiguration => {
                            Self::record_object_bytes(&*(ptr as *const RecordObj))
                        }
                        VecLikeType::Font => Self::font_object_bytes(&*(ptr as *const FontObj)),
                        VecLikeType::Overlay => size_of::<OverlayObj>(),
                        VecLikeType::Marker => size_of::<MarkerObj>(),
                        VecLikeType::Buffer => size_of::<BufferObj>(),
                        VecLikeType::Window => size_of::<WindowObj>(),
                        VecLikeType::Frame => size_of::<FrameObj>(),
                        VecLikeType::Timer => size_of::<TimerObj>(),
                        VecLikeType::Process => size_of::<ProcessObj>(),
                        VecLikeType::Terminal => size_of::<TerminalObj>(),
                        VecLikeType::Xwidget => size_of::<XwidgetObj>(),
                        VecLikeType::XwidgetView => size_of::<XwidgetViewObj>(),
                        VecLikeType::SurfaceHandle => size_of::<SurfaceObj>(),
                        VecLikeType::VideoHandle => size_of::<VideoObj>(),
                        VecLikeType::BoolVector => {
                            let obj = &*(ptr as *const BoolVectorObj);
                            size_of::<BoolVectorObj>().saturating_add(size_of_val(obj.words()))
                        }
                        VecLikeType::Subr => size_of::<SubrObj>(),
                        VecLikeType::Bignum => size_of::<BignumObj>(),
                        VecLikeType::SymbolWithPos => size_of::<SymbolWithPosObj>(),
                        VecLikeType::Finalizer => size_of::<FinalizerObj>(),
                        VecLikeType::Sqlite => size_of::<SqliteObj>(),
                        VecLikeType::Thread | VecLikeType::Mutex | VecLikeType::CondVar => {
                            size_of::<ThreadingHandleObj>()
                        }
                        VecLikeType::UserPtr => size_of::<UserPtrObj>(),
                        VecLikeType::ModuleFunction => size_of::<ModuleFunctionObj>(),
                    }
                }
            }
        }
    }

    fn string_payload_layout(string: &crate::heap_types::LispString) -> PayloadLayout {
        let logical_bytes = string.byte_len().saturating_add(1);
        let capacity_bytes = string.owned_capacity();
        PayloadLayout {
            logical_bytes,
            capacity_bytes,
            owned: string.has_owned_storage(),
            mapped: !string.has_owned_storage(),
        }
    }

    /// A bignum's limb vector (GNU: the `mpz_t` limbs GMP mallocs). A value
    /// below 2^64 is malachite `Small` and owns no heap limbs. malachite does
    /// not expose the `Vec` capacity, so the capacity reported is the limb
    /// count: a lower bound (kernel results reserve one carry limb).
    fn bignum_payload_layout(value: &Integer) -> PayloadLayout {
        let limbs = value.unsigned_abs_ref().as_limbs_asc().len();
        if limbs <= 1 {
            return PayloadLayout::default();
        }
        let bytes = limbs.saturating_mul(size_of::<u64>());
        PayloadLayout {
            logical_bytes: bytes,
            capacity_bytes: bytes,
            owned: true,
            mapped: false,
        }
    }

    fn value_vec_payload_layout(values: &LispValueVec) -> PayloadLayout {
        PayloadLayout {
            logical_bytes: values
                .as_slice()
                .len()
                .saturating_mul(size_of::<TaggedValue>()),
            capacity_bytes: Self::lisp_value_vec_storage_bytes(values),
            owned: values.is_owned(),
            mapped: !values.is_owned(),
        }
    }

    fn lambda_params_payload_layout(
        params: &crate::emacs_core::value::LambdaParams,
    ) -> PayloadLayout {
        PayloadLayout {
            logical_bytes: (params.required.len() + params.optional.len())
                .saturating_mul(size_of::<SymId>()),
            capacity_bytes: (params.required.capacity() + params.optional.capacity())
                .saturating_mul(size_of::<SymId>()),
            owned: params.required.capacity() > 0 || params.optional.capacity() > 0,
            mapped: false,
        }
    }

    fn bytecode_payload_layout(obj: &ByteCodeObj) -> PayloadLayout {
        let data = &obj.data;
        let resident_ops = data.resident_ops();
        let mut stats = PayloadLayout {
            logical_bytes: std::mem::size_of_val(resident_ops),
            capacity_bytes: data.resident_ops_capacity().saturating_mul(size_of::<Op>()),
            owned: !resident_ops.is_empty(),
            mapped: false,
        };
        stats = stats.add(PayloadLayout {
            logical_bytes: data
                .constants
                .len()
                .saturating_mul(size_of::<TaggedValue>()),
            capacity_bytes: data
                .constants
                .owned_capacity()
                .saturating_mul(size_of::<TaggedValue>()),
            owned: data.constants.owned_capacity() > 0,
            mapped: false,
        });
        if let Some(params) = data.params.named() {
            stats = stats.add(Self::lambda_params_payload_layout(params));
        }
        if let Some(offsets) = data.resident_gnu_byte_offset_map() {
            stats = stats.add(PayloadLayout {
                logical_bytes: std::mem::size_of_val(offsets),
                capacity_bytes: data
                    .resident_gnu_byte_offset_map_capacity()
                    .saturating_mul(size_of::<GnuByteOffsetMapEntry>()),
                owned: !offsets.is_empty(),
                mapped: false,
            });
        }
        if let Some(bytes) = &data.gnu_bytecode_bytes {
            stats = stats.add(PayloadLayout {
                logical_bytes: bytes.len(),
                capacity_bytes: bytes.owned_bytes(),
                owned: bytes.owned_bytes() > 0,
                mapped: bytes.owned_bytes() == 0 && !bytes.is_empty(),
            });
        }
        stats = stats.add(PayloadLayout {
            logical_bytes: data
                .extra_slots
                .len()
                .saturating_mul(size_of::<TaggedValue>()),
            capacity_bytes: Self::vector_storage_bytes(&data.extra_slots),
            owned: data.extra_slots.capacity() > 0,
            mapped: false,
        });
        if let Some(docstring) = &data.docstring {
            stats = stats.add(Self::string_payload_layout(docstring));
        }
        stats
    }

    fn closure_payload_layout(
        data: &LispValueVec,
        params: Option<&crate::emacs_core::value::LambdaParams>,
    ) -> PayloadLayout {
        let mut stats = Self::value_vec_payload_layout(data);
        if let Some(params) = params {
            stats = stats.add(Self::lambda_params_payload_layout(params));
        }
        stats
    }

    fn veclike_payload_layout(header: *const VecLikeHeader) -> PayloadLayout {
        unsafe {
            match (*header).type_tag {
                VecLikeType::Vector => {
                    Self::value_vec_payload_layout(&(*(header as *const VectorObj)).data)
                }
                VecLikeType::Lambda => {
                    let object = &*(header as *const LambdaObj);
                    Self::closure_payload_layout(&object.data, object.parsed_params.get())
                }
                VecLikeType::Macro => {
                    let object = &*(header as *const MacroObj);
                    Self::closure_payload_layout(&object.data, object.parsed_params.get())
                }
                VecLikeType::ByteCode => {
                    Self::bytecode_payload_layout(&*(header as *const ByteCodeObj))
                }
                VecLikeType::Record | VecLikeType::WindowConfiguration => {
                    Self::value_vec_payload_layout(&(*(header as *const RecordObj)).data)
                }
                VecLikeType::Font => {
                    Self::value_vec_payload_layout(&(*(header as *const FontObj)).data.fields)
                }
                VecLikeType::CharTable => {
                    Self::value_vec_payload_layout(&(*(header as *const CharTableObj)).extras)
                }
                VecLikeType::SubCharTable => {
                    Self::value_vec_payload_layout(&(*(header as *const SubCharTableObj)).contents)
                }
                VecLikeType::Obarray => {
                    Self::value_vec_payload_layout(&(*(header as *const ObarrayObj)).buckets)
                }
                _ => PayloadLayout::default(),
            }
        }
    }

    fn boxed_class(header: *const GcHeader) -> &'static str {
        unsafe {
            match (*header).kind {
                HeapObjectKind::String => "string",
                HeapObjectKind::Float => "float",
                HeapObjectKind::VecLike => match (*(header as *const VecLikeHeader)).type_tag {
                    VecLikeType::Vector => "vector",
                    VecLikeType::Bignum => "bignum",
                    VecLikeType::Marker => "marker",
                    VecLikeType::Overlay => "overlay",
                    VecLikeType::Finalizer => "finalizer",
                    VecLikeType::BoolVector => "bool-vector",
                    VecLikeType::SymbolWithPos => "symbol-with-pos",
                    VecLikeType::UserPtr => "user-ptr",
                    VecLikeType::Process => "process",
                    VecLikeType::Frame => "frame",
                    VecLikeType::Window => "window",
                    VecLikeType::Buffer => "buffer",
                    VecLikeType::HashTable => "hash-table",
                    VecLikeType::Obarray => "obarray",
                    VecLikeType::Terminal => "terminal",
                    VecLikeType::WindowConfiguration => "window-configuration",
                    VecLikeType::Subr => "subr",
                    VecLikeType::Xwidget => "xwidget",
                    VecLikeType::XwidgetView => "xwidget-view",
                    VecLikeType::ModuleFunction => "module-function",
                    VecLikeType::Sqlite => "sqlite",
                    VecLikeType::Thread => "thread",
                    VecLikeType::Mutex => "mutex",
                    VecLikeType::CondVar => "condvar",
                    VecLikeType::Lambda => "lambda",
                    VecLikeType::CharTable => "char-table",
                    VecLikeType::SubCharTable => "sub-char-table",
                    VecLikeType::Record => "record",
                    VecLikeType::Font => "font",
                    VecLikeType::Macro => "macro",
                    VecLikeType::ByteCode => "bytecode",
                    VecLikeType::Timer => "timer",
                    VecLikeType::SurfaceHandle => "surface-handle",
                    VecLikeType::VideoHandle => "video-handle",
                },
            }
        }
    }

    fn note_boxed_list_layout(
        &self,
        mut header: *const GcHeader,
        stats: &mut Vec<BoxedKindLayoutStats>,
    ) {
        while !header.is_null() {
            let class = Self::boxed_class(header);
            let index = stats
                .iter()
                .position(|item| item.class == class)
                .unwrap_or_else(|| {
                    stats.push(BoxedKindLayoutStats {
                        class,
                        ..BoxedKindLayoutStats::default()
                    });
                    stats.len() - 1
                });
            let item = &mut stats[index];
            item.objects += 1;
            item.known_bytes = item
                .known_bytes
                .saturating_add(self.collector_object_bytes(header));
            unsafe {
                item.tenured_objects += usize::from((*header).tenured);
                header = (*header).gc_link();
            }
        }
    }

    /// Snapshot allocator-backed GC page occupancy and the directly-owned
    /// payload capacities of live objects. This does not attempt to reproduce
    /// process RSS: symbol registries, evaluator stacks, display caches,
    /// allocator metadata, and nested hash-key allocations live outside this
    /// accounting and are intentionally exposed as the RSS remainder.
    pub(crate) fn layout_stats(&self) -> HeapLayoutStats {
        // The free-list walk below would miss an open region's cells and
        // `cons_live_count` would count them: callers close first.
        debug_assert!(
            !self.alloc_regions_open(),
            "layout_stats with an open allocation region"
        );
        let mut free_cells_by_block = vec![0usize; self.cons_blocks.len()];
        let bumped_cons_slots: usize = self
            .cons_blocks
            .iter()
            .map(|block| block.next_index as usize)
            .sum();
        let mut free_count = 0usize;
        for (base, count) in self.cons_allocator.free_counts() {
            if let Some(&block_index) = self.cons_block_index_by_base.get(&base.addr()) {
                free_cells_by_block[block_index] = count;
            }
            free_count += count;
        }
        debug_assert!(free_count <= bumped_cons_slots);

        let mut cons = ConsLayoutStats {
            pages: self.cons_blocks.len(),
            page_bytes: CONS_BLOCK_BYTES,
            capacity_slots: self.cons_blocks.len().saturating_mul(CONS_BLOCK_SIZE),
            bumped_slots: bumped_cons_slots,
            live_slots: bumped_cons_slots.saturating_sub(free_count),
            reclaimed_slots: free_count,
            never_used_slots: self
                .cons_blocks
                .len()
                .saturating_mul(CONS_BLOCK_SIZE)
                .saturating_sub(bumped_cons_slots),
            ..ConsLayoutStats::default()
        };
        for (block, reclaimed) in self.cons_blocks.iter().zip(free_cells_by_block) {
            let live = (block.next_index as usize).saturating_sub(reclaimed);
            if live == 0 {
                cons.empty_pages += 1;
            } else if live == CONS_BLOCK_SIZE {
                cons.full_pages += 1;
            } else {
                cons.partial_pages += 1;
            }
        }
        cons.occupied_bytes = cons.live_slots.saturating_mul(size_of::<ConsCell>());
        debug_assert_eq!(cons.live_slots, self.cons_live_count);

        let arenas = vec![
            self.float_arena.layout_stats(|_| PayloadLayout::default()),
            self.string_arena
                .layout_stats(|object| Self::string_payload_layout(&object.data)),
            self.vector_arena
                .layout_stats(|object| Self::value_vec_payload_layout(&object.data)),
            self.bytecode_arena
                .layout_stats(Self::bytecode_payload_layout),
            self.lambda_arena.layout_stats(|object| {
                Self::closure_payload_layout(&object.data, object.parsed_params.get())
            }),
            self.macro_arena.layout_stats(|object| {
                Self::closure_payload_layout(&object.data, object.parsed_params.get())
            }),
            self.record_arena
                .layout_stats(|object| Self::value_vec_payload_layout(&object.data)),
            self.symbol_with_pos_arena
                .layout_stats(|_| PayloadLayout::default()),
            self.marker_arena.layout_stats(|_| PayloadLayout::default()),
            self.bignum_arena
                .layout_stats(|object| Self::bignum_payload_layout(&object.value)),
        ];

        let mapped_conses = self.mapped_cons_ranges.iter().map(|range| range.len).sum();
        let mapped_floats = self.mapped_float_ranges.iter().map(|range| range.len).sum();
        let mut mapped = MappedLayoutStats {
            conses: mapped_conses,
            floats: mapped_floats,
            strings: self.mapped_string_objects.len(),
            veclikes: self.mapped_veclike_objects.len(),
            object_image_bytes: mapped_conses
                .saturating_mul(size_of::<ConsCell>())
                .saturating_add(mapped_floats.saturating_mul(size_of::<FloatObj>()))
                .saturating_add(
                    self.mapped_string_objects
                        .iter()
                        .map(|object| object.byte_len)
                        .sum::<usize>(),
                )
                .saturating_add(
                    self.mapped_veclike_objects
                        .iter()
                        .map(|object| object.byte_len)
                        .sum::<usize>(),
                ),
            ..MappedLayoutStats::default()
        };
        for object in &self.mapped_string_objects {
            let payload = unsafe { Self::string_payload_layout(&(*object.ptr).data) };
            if payload.owned {
                mapped.copied_string_payloads += 1;
                mapped.copied_string_capacity_bytes = mapped
                    .copied_string_capacity_bytes
                    .saturating_add(payload.capacity_bytes);
            }
        }
        for object in &self.mapped_veclike_objects {
            let payload = Self::veclike_payload_layout(object.header);
            if payload.owned {
                mapped.copied_veclike_payloads += 1;
                mapped.copied_veclike_capacity_bytes = mapped
                    .copied_veclike_capacity_bytes
                    .saturating_add(payload.capacity_bytes);
            }
        }

        let mut boxed = Vec::new();
        self.note_boxed_list_layout(self.all_objects, &mut boxed);
        self.note_boxed_list_layout(self.tenured_objects, &mut boxed);
        self.note_boxed_list_layout(self.generational.old_objects, &mut boxed);
        self.note_boxed_list_layout(self.generational.old_sweep_pending, &mut boxed);
        boxed.sort_by_key(|layout| std::cmp::Reverse(layout.known_bytes));

        let page_backing_bytes = cons
            .pages
            .saturating_mul(cons.page_bytes)
            .saturating_add(arenas.iter().map(|arena| arena.page_bytes).sum::<usize>());
        let known_payload_capacity_bytes = arenas
            .iter()
            .map(|arena| arena.payload_capacity_bytes)
            .sum::<usize>()
            .saturating_add(mapped.copied_string_capacity_bytes)
            .saturating_add(mapped.copied_veclike_capacity_bytes);

        HeapLayoutStats {
            allocated_objects: self.mutators().map(|state| state.allocated_count).sum(),
            // `live_bytes` is what the last sweep counted, so add what has been
            // allocated since to keep reporting the current managed size.
            managed_live_bytes: self.live_bytes.saturating_add(self.bytes_since_gc()),
            page_backing_bytes,
            known_payload_capacity_bytes,
            cons,
            arenas,
            mapped,
            boxed,
        }
    }

    // -----------------------------------------------------------------------
    // Marker operations
    // -----------------------------------------------------------------------

    // `find_marker_by_id_during_load` was retired in T11. Pdump load now
    // builds an O(1) `marker_id` → `MarkerObj*` index in
    // `TaggedLoadState::markers_by_id` during `preload_tagged_heap`, so the
    // O(N·M) heap scan is no longer needed.

    /// Install the raw chain-head slots the next `complete_collection`
    /// cycle should walk when unlinking dead markers. Caller (typically
    /// `Context::gc_collect_from_current_roots`) passes one slot per
    /// live `BufferText`. The vec is consumed and cleared by
    /// `unchain_dead_markers` so successive cycles must re-install.
    ///
    /// # Safety
    ///
    /// Each slot must point to a valid `*mut MarkerObj` living inside a live
    /// `BufferText`'s storage and must remain valid for the duration of the GC
    /// cycle. The caller must hold exclusive access to the heap and the buffer
    /// manager during the cycle.
    pub unsafe fn set_marker_chain_head_slots(&mut self, slots: Vec<*mut *mut MarkerObj>) {
        self.marker_chain_head_slots = slots;
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    // NOTE: `link_object` (the bare-`GcHeader` intrusive-list link) is gone —
    // both bare-header classes (Float, String) allocate from arena pages now.
    // `link_veclike` below carries the canonical BORN-AT-PARITY comment; the
    // page alloc paths apply the identical store inline.

    /// Task #7 stage 2a (Fix A): drop a dying non-cons object from the
    /// incremental vector registry. Called with the header still live,
    /// immediately before `free_gc_object`, so reading the kind/tag here is
    /// valid and skips the hash probe for the (majority) non-vector kinds.
    ///
    /// # Safety
    /// `header` must point at a still-allocated non-cons object header.
    #[inline]
    unsafe fn unregister_vector_object(&mut self, header: *mut GcHeader) {
        unsafe {
            if (*header).kind == HeapObjectKind::VecLike
                && (*(header as *const VecLikeHeader)).type_tag == VecLikeType::Vector
            {
                let removed = self.vector_object_addrs.remove(&(header as usize));
                debug_assert!(removed, "freed vector was not in the registry");
            }
        }
    }

    /// Link a veclike object into the all_objects list.
    fn link_veclike(&mut self, header: *mut VecLikeHeader) {
        unsafe {
            (*header).gc.next = self.all_objects;
            (*header).gc.flags = (*header).gc.flags.with_boxed();
            // BORN-AT-PARITY, unconditionally (see `link_object`): during a
            // concurrent mark this is allocate-black; otherwise it pre-arms
            // the bit so the next begin_collection flip reads it as white.
            (*header).gc.set_marked(self.mark_parity);
            // `addr_of_mut!`, not `&mut (*header).gc as *mut GcHeader`: a
            // reference to the field is retagged with just the GcHeader's
            // extent, while the pointer names the whole VecLikeHeader and is
            // read beyond that extent (the test probe reads `type_tag` at
            // offset 0x10). The raw place projection keeps the allocation's
            // full provenance.
            let gc_header = std::ptr::addr_of_mut!((*header).gc);
            let inserted = self.non_cons_object_addrs.insert(gc_header as usize);
            debug_assert!(inserted, "veclike object linked twice");
            // Task #7 stage 2a (Fix A): maintain the incremental vector
            // registry at the veclike link chokepoint. UNREACHABLE for
            // Vector since stage 3 (alloc_vector allocates from pages and
            // registers there); kept as the residual-Box seam so any future
            // Box-vector producer stays registry-correct by construction.
            if (*header).type_tag == VecLikeType::Vector {
                let registered = self.vector_object_addrs.insert(gc_header as usize);
                debug_assert!(registered, "vector linked twice into the registry");
            }
            self.all_objects = gc_header;
            self.note_black_born(gc_header);
            #[cfg(test)]
            alloc_probe::record(gc_header, self.non_cons_object_addrs.len());
        }
    }
}

/// TEST-ONLY allocation-profiling counters for the non-cons allocator
/// modernization probes (size-class arena design inputs): per-kind allocation
/// counts, a size-class histogram over TOTAL object bytes (fixed struct +
/// separately-allocated payload storage, via `object_bytes_from_header`),
/// per-kind byte totals, and the peak `non_cons_object_addrs` population.
/// Compiled ONLY under `cfg(test)` (the consuming probes are behind the
/// `perf-probes` feature), so production builds carry zero instrumentation.
/// Global statics are correct here because nextest runs each probe in its own
/// process, so the counters observe exactly one workload.
#[cfg(test)]
pub(crate) mod alloc_probe;

#[cfg(test)]
#[path = "gc/tests/layout_stats_test.rs"]
mod layout_stats_tests;

#[cfg(test)]
#[path = "gc/tests/pacer_test.rs"]
mod pacer_tests;

#[cfg(test)]
#[path = "gc/tests/ownership_test.rs"]
mod ownership_tests;

#[cfg(test)]
#[path = "gc/tests/thread_local_ownership_test.rs"]
mod thread_local_ownership_tests;

/// FLOAT ARENA PAGES test suite. Every scenario runs twice: plain and with
/// `NEOVM_GC_VERIFY_PARTITION=1` (which also arms the partition via a fake
/// dump span + a bootstrap cycle where the flow allows, so the dump-partition
/// and tricolor verifiers actually engage at each termination). The suite
/// relies on nextest's process-per-test model for the env var and the global
/// `LIVE_FLOAT_PAGES` counter.
#[cfg(test)]
#[path = "gc/tests/float_arena_test.rs"]
mod float_arena_tests;

/// ARENA PROMOTION + RETIREMENT test suite (stage 3, commit 4): the
/// promotion page walk, full-page retirement, mixed-page tenured survival
/// across parities, page-span-oracle exactness, payload-bearing teardown,
/// variable-size live-bytes accounting, and the tenured-page-owner
/// remembered-set scan. Scenarios run plain and (where the partition
/// verifiers add coverage) with `NEOVM_GC_VERIFY_PARTITION=1`.
#[cfg(test)]
#[path = "gc/tests/arena_promotion_test.rs"]
mod arena_promotion_tests;

/// BYTECODE ARENA test suite (task 03/3a): page-span oracle exactness for the
/// first non-power-of-two stride (384B — including the never-allocated page
/// TAIL), alloc/free/reuse + ownership-tracks-sweep, two-cycle parity
/// survival/reclaim, the deferred-at-termination resolution through
/// `mark_value`'s page-oracle-routed veclike arm (TRAP A coverage),
/// adversarial freed-slot staleness, variable-size live-bytes accounting on
/// both recompute sites, loadup-shaped tenure + FULL-page retirement (the
/// first class where retirement meaningfully fires), mixed-page parity
/// survival, the C1 retired-page write-barrier edge, payload-bearing
/// teardown counters, and the test-only constants-mutation seam. Scenarios
/// run plain and (where the partition matters) VERIFY_PARTITION-armed.
#[cfg(test)]
#[path = "gc/tests/bytecode_arena_test.rs"]
mod bytecode_arena_tests;

/// LAMBDA + MACRO ARENA test suite (task 03/3b): the 128B power-of-two class
/// (512 slots/page, no page tail) shared by TWO distinct payload types in
/// SEPARATE arenas. Covers page-span oracle exactness, alloc/free/reuse +
/// ownership-tracks-sweep, two-cycle parity survival/reclaim, the
/// deferred-at-termination resolution through `mark_value`'s
/// page-oracle-routed veclike arm (TRAP A — closures stay DEFERRED for
/// marking; concurrent claiming is a future task), adversarial freed-slot
/// staleness, `drop_in_place` of the closure slot `Vec` (variable-size
/// live-bytes on both recompute sites + payload teardown counters),
/// loadup-shaped tenure + FULL-page retirement (C1), and mixed-page parity
/// survival. Lambda gets the full battery; Macro gets an independent
/// exactness/sweep/tenure/teardown battery proving its own arena. Scenarios
/// run plain and (where the partition matters) VERIFY_PARTITION-armed.
#[cfg(test)]
#[path = "gc/tests/lambda_macro_arena_test.rs"]
mod lambda_macro_arena_tests;

/// RECORD ARENA test suite (task 03/3b): the 64B class (1024 slots/page,
/// shared stride, OWN arena) backing BOTH the `Record` and
/// `WindowConfiguration` type tags. Covers page-span oracle exactness,
/// ownership-tracks-sweep, two-cycle parity survival/reclaim, the
/// deferred-at-termination resolution (TRAP A — records stay DEFERRED for
/// marking), adversarial freed-slot staleness, `drop_in_place` of the slot
/// `Vec` (variable-size live-bytes on both recompute sites + teardown
/// counters), loadup-shaped tenure + FULL-page retirement (C1), mixed-page
/// parity survival, and the WindowConfiguration dual-tag sharing the arena.
/// Scenarios run plain and (where the partition matters) VERIFY_PARTITION.
#[cfg(test)]
#[path = "gc/tests/record_arena_test.rs"]
mod record_arena_tests;

/// SYMBOL-WITH-POS ARENA test suite (task 03/3b): the 64B class (1024
/// slots/page, own arena) for a POD-like fixed `{sym, pos}` type
/// (`needs_drop` == false — the sweep/teardown `drop_in_place` walk compiles
/// out, exactly like FloatObj). Covers page-span oracle exactness,
/// ownership-tracks-sweep, two-cycle parity survival/reclaim, the
/// deferred-at-termination resolution (TRAP A — SymbolWithPos parks in the
/// `other` drain bucket, marking unchanged), adversarial freed-slot staleness
/// (the full-header rewrite + allocated-bit-first still matter for a POD type
/// — a stale header would misread the parity/tenured bits and byte size),
/// fixed-size live-bytes on both recompute sites, loadup-shaped tenure +
/// FULL-page retirement (C1), mixed-page parity survival, teardown page
/// counters, and the promotion-scan young-child edge (both `sym` and `pos`
/// are traced children). Scenarios run plain and (where the partition
/// matters) VERIFY_PARTITION.
mod allocation;

mod collector_tables;
mod mark_sweep;

#[cfg(feature = "gc-memory-telemetry")]
mod memory_inventory;
#[cfg(feature = "gc-memory-telemetry")]
pub mod memory_telemetry;

mod birth_logs;
mod concurrent;
mod generational;
mod old_sweep;
mod pacing;
pub use concurrent::MarkFinishError;
pub(crate) use concurrent::MarkPhase;
mod heap_identity;
pub use heap_identity::HeapIdentity;
mod mark_word;
use mark_word::{MarkStack, MarkWord};
mod satb;
use satb::SatbHandoff;
pub(crate) mod mapped_veclike_scan;
#[cfg(test)]
#[path = "gc/tests/mutator_registry_test.rs"]
mod mutator_registry_tests;
pub(crate) mod scan_contract;
use mapped_veclike_scan::MappedVeclikeScanSnapshot;
#[cfg(test)]
#[path = "gc/tests/shutdown_tests.rs"]
mod shutdown_tests;
pub(crate) use concurrent::{concurrent_hash_snapshot, prepare_concurrent_hash_write};
pub(crate) mod concurrent_hash;

mod rows_observation;
pub(crate) use rows_observation::RowsOwnedDomain;

mod incremental;
mod reclamation;
use reclamation::ReclamationMode;

mod card_table;
#[cfg_attr(not(feature = "jit"), allow(unused_imports))]
pub(crate) use card_table::{
    CARD_CHUNK_SHIFT, CARD_DIRTY, CARD_INDEX_MASK, CARD_SHIFT, CHUNK_SUMMARY_OFFSET,
    HEAP_JIT_CARD_TABLE,
};
use card_table::{CardSlots, CardTable, ChunkCards};
mod cons_block_trailer;
use cons_block_trailer::*;
mod collection_observed;
pub(crate) use collection_observed::{
    advance_collection_observation_epoch, clear_noncons_collection_observed_metadata,
    collection_observation_epoch, collection_observed, collection_observed_metadata,
    cons_collection_observed_word, has_collection_observations, mark_collection_observed,
};
use collection_observed::{
    clear_cons_observed_block, clear_cons_observed_dead, has_noncons_collection_observations,
};
mod cons_allocator;
mod cons_storage;
use cons_allocator::{ConsAllocatorOwner, ConsBlockBase};
mod cons_blocks;
/// The cons-block trailer's shape, for `jit_layout::heap`.
#[cfg_attr(not(feature = "jit"), allow(unused_imports))]
pub(crate) use cons_block_trailer::{
    CONS_AGE_OFFSET, CONS_BLOCK_BYTES, CONS_BLOCK_SIZE as CONS_BLOCK_CELLS, CONS_MARK_WORDS,
    CONS_MARKS_OFFSET, ConsAge,
};
use cons_blocks::*;

mod aliasable;
mod collector_reads;
pub(crate) use aliasable::AliasableBox;
#[cfg(test)]
#[path = "gc/tests/test_heap.rs"]
mod test_heap;
#[cfg(test)]
pub(crate) use test_heap::{SyntheticConcurrentBarrier, TestHeap};

mod arena_pages;
pub(crate) use arena_pages::*;

mod gc_thread;
pub use gc_thread::*;

mod barrier_window;
#[cfg(feature = "jit")]
pub(crate) use barrier_window::neovm_jit_unobserved_collection_owner;
pub(crate) use barrier_window::{
    BarrierWindow, CompiledObservationGate, HEAP_JIT_EXCLUSION_LEN, HEAP_JIT_EXCLUSION_LO,
    HEAP_JIT_SINGLE_CONS, HEAP_JIT_SLOT_BARRIER_LEN, HEAP_JIT_SLOT_BARRIER_LO,
    current_collection_dump_window, publish_collection_observation_window,
};

mod jit_state;
mod store_layout;

mod mutator_gc;
use mutator_gc::MutatorGcState;

mod knobs;
#[cfg_attr(not(feature = "jit"), allow(unused_imports))]
pub(crate) use knobs::card_tag_filter_on;
#[cfg(test)]
pub(crate) use knobs::with_card_tag_filter_for_test;

mod chunk_map;
use chunk_map::{CHUNK_CLASS_COUNT, ChunkClass, ChunkEntry, ChunkMap, HeapChunkMap, PageSnapshot};

mod census;
mod cold_gc;
mod facade_mark;
mod gen1_admission;
pub use gen1_admission::{Gen1Admission, Gen1AdmissionError};
mod mutator_registration;
#[cfg(test)]
use census::CensusRecord;
use census::{CensusCycleKind, GenCensus, census_remset_probe_on};
pub use facade_mark::{
    CollectorQuiescenceError, ConcurrentMarkAdmissionError, ConcurrentMarkCapture,
    ConcurrentMarkPermit, FacadeEpochRetention, FacadeMarkExclusion, FacadeMarkExclusionError,
    QuiescentCollector,
};
#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
pub(crate) use mutator_registration::symbol_functions::boundary as function_boundary_testkit;
pub use mutator_registration::{
    AllStopped, AllStoppedFailure, CollectionError, CollectionReport, FunctionActivation,
    FunctionCallCache, FunctionError, FunctionStep, GlobalError, InitialInternError, KeeperSymbols,
    KeeperTableMut, MinorCollectionError, Mode, Mutator, MutatorEntry, MutatorFinishFailure,
    MutatorKey, MutatorPermit, MutatorStartFailure, NativeReturn, NativeRoots, OwnerPublication,
    OwnerRootSlot, ParallelHeapEra, Phase, PlainSymbolBacking, RegistrationCensus,
    RegistrationError, Rendezvous, RendezvousError, Request, ResumeGcImage, ResumeGcImageFailure,
    RootedRetireFailure, RunAccess, RunSlice, SealedTableAccess, SliceRoot, Snapshot,
    StoppedSummaries, SymbolHandle, SymbolMut, SymbolRef, TableMut, TableRef, YoungWorkerLoan,
};

#[cfg(test)]
pub(crate) use mutator_registration::ProofEra;

pub(crate) use mutator_registration::{
    OwnerRootAdmission, StoppedRootAdmission, SymbolReadToken, SymbolWriteToken,
};

mod alloc_region;
#[cfg(test)]
use alloc_region::{CONS_REGION_MAX_CELLS, ConsRegionSource, FLOAT_REGION_MAX_SLOTS};
use alloc_region::{RegionBook, RegionStats};
#[cfg(test)]
pub(crate) use barrier_window::published_barrier_window;
#[cfg_attr(not(feature = "jit"), allow(unused_imports))]
pub(crate) use jit_state::{
    FLOAT_SLOT_BYTES, HEAP_JIT_BARRIER_LEN, HEAP_JIT_BARRIER_LO, HEAP_JIT_CONS_CUR,
    HEAP_JIT_CONS_LIM, HEAP_JIT_FLOAT_CUR, HEAP_JIT_FLOAT_LIM, JitHeapState,
};
/// Allocation regions: sources, give-back, exact counters, black across
/// the phase flags, and the close at every collector entry.
#[cfg(test)]
#[path = "gc/tests/alloc_region_test.rs"]
mod alloc_region_tests;
#[cfg(test)]
#[path = "gc/tests/barrier_window_generational_test.rs"]
mod barrier_window_generational_tests;
/// The write barrier's owner window against the gate it replaced, state by
/// state and owner by owner, and its republication at every input writer.
#[cfg(test)]
#[path = "gc/tests/barrier_window_test.rs"]
mod barrier_window_tests;
/// BIGNUM ARENA test suite (lever P0.11): the 64B payload-bearing class
/// (1024 slots/page, own arena). Covers the slot fit, page-span oracle
/// exactness, the registry-untouched claim, two-cycle parity survival and
/// reclaim, deferred-at-termination resolution (bignums park in the `other`
/// drain bucket), SATB survival, adversarial freed-slot staleness,
/// cooperative-window reuse, tenure + full-page retirement, teardown page
/// counters and exact values after slot reuse. Scenarios run plain and
/// (where the partition matters) VERIFY_PARTITION-armed.
#[cfg(test)]
#[path = "gc/tests/bignum_arena_test.rs"]
mod bignum_arena_tests;
/// The generation census: survivor classes across cycles on both
/// termination paths, the remembered-set probe, and that it measures without
/// changing anything.
#[cfg(test)]
#[path = "gc/tests/census_test.rs"]
mod census_tests;
/// The chunk map: the radix, its entries through page and block creation,
/// release and re-indexing, the oracles against the registries, the GC
/// thread's snapshot semantics, and concurrent cycles classified through it.
#[cfg(test)]
#[path = "gc/tests/chunk_map_test.rs"]
mod chunk_map_tests;
#[cfg(test)]
#[path = "gc/tests/cons_alloc_test.rs"]
mod cons_alloc_tests;
/// A fake pdump image in one allocation (see the module doc for why one).
#[cfg(test)]
#[path = "gc/tests/fake_image.rs"]
pub(crate) mod fake_image;
#[cfg(test)]
#[path = "gc/tests/generational_verifier_test.rs"]
mod generational_verifier_tests;
#[cfg(test)]
#[path = "gc/tests/marker_arena_test.rs"]
mod marker_arena_tests;
/// Record and closure slot stores are atomic: race-free against an atomic
/// reader, same semantics, same barrier through a concurrent mark.
#[cfg(test)]
#[path = "gc/tests/slot_store_test.rs"]
mod slot_store_tests;
#[cfg(test)]
#[path = "gc/tests/symbol_with_pos_arena_test.rs"]
mod symbol_with_pos_arena_tests;
/// `NEOVM_GC_VEC_SCAN=defer` (F-G): no Tier-B snapshot, page vectors traced
/// by reachability at the termination, with and without the chunk map.
#[cfg(test)]
#[path = "gc/tests/vec_scan_test.rs"]
mod vec_scan_tests;

#[cfg(test)]
#[path = "gc/tests/age_mirror_test.rs"]
mod age_mirror_tests;
#[cfg(test)]
#[path = "gc/tests/card_table_test.rs"]
mod card_table_tests;
#[cfg(test)]
#[path = "gc/tests/generational_test.rs"]
mod generational_tests;

#[cfg(test)]
#[path = "gc/tests/birth_logs_test.rs"]
mod birth_logs_tests;

#[cfg(test)]
#[path = "gc/tests/generational_major_test.rs"]
mod generational_major_tests;

#[cfg(test)]
#[path = "gc/tests/generational_pacing_test.rs"]
mod generational_pacing_tests;

#[cfg(test)]
#[path = "gc/tests/bytecode_parameter_roots.rs"]
mod bytecode_parameter_roots_tests;
#[cfg(test)]
#[path = "gc/tests/major_symbol_preimage_test.rs"]
mod major_symbol_preimage_tests;
#[cfg(test)]
#[path = "gc/tests/symbol_barrier_test.rs"]
mod symbol_barrier_tests;

#[cfg(test)]
#[path = "gc/tests/chartable_contract_test.rs"]
mod chartable_contract_tests;

#[cfg(test)]
#[path = "gc/tests/satb_gc_stress_test.rs"]
mod satb_gc_stress_tests;

/// Young-block tracking, stopped-world flag roles, and verifier mutations.
#[cfg(test)]
#[path = "gc/tests/cons_young_tracking_test.rs"]
mod cons_young_tracking_tests;

#[path = "gc/young_registry.rs"]
mod young_registry;

#[cfg(test)]
#[path = "gc/tests/canonical_young_registry_test.rs"]
mod young_registry_tests;

#[cfg(test)]
#[path = "gc/tests/cons_young_refill_refund_test.rs"]
mod cons_young_refill_refund_tests;

#[cfg(test)]
#[path = "gc/tests/drain_young_preflight_test.rs"]
mod drain_young_preflight_tests;

/// Minor worklist bounds and untouched old-only mark words.
#[cfg(test)]
#[path = "gc/tests/cons_young_minor_work_test.rs"]
mod cons_young_minor_work_tests;

/// Shared GEN1 allocator stress and exit-retained young logs.
#[cfg(test)]
#[path = "gc/tests/cons_young_shared_test.rs"]
mod cons_young_shared_tests;

/// Exact GEN1 sweep deltas, late births, conditional release and verifier fault.
#[cfg(test)]
#[path = "gc/tests/cons_sweep_accounting_test.rs"]
mod cons_sweep_accounting_tests;
