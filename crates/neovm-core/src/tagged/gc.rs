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
//! No ObjId. No generations. No stale references.

use super::header::*;
use super::value::TaggedValue;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::{HashKey, HashTableWeakness};
use crate::heap_types::LispStringStorageKind;
use crate::tagged::symbol_marks::SymbolMarkBits;
use malachite::integer::Integer;
use rustc_hash::{FxHashMap, FxHashSet};
use std::alloc::{self, Layout};
use std::cell::Cell;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

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
/// but records mutation metadata here so future generational or incremental
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
    /// Mirrors `TaggedHeap::{dump_addr_lo, dump_addr_hi}` so the write
    /// barrier's partition-only path can span-test a cons owner without
    /// dereferencing the heap. `(usize::MAX, 0)` = empty span.
    static TAGGED_HEAP_DUMP_SPAN: Cell<(usize, usize)> = const { Cell::new((usize::MAX, 0)) };
    /// The owner most recently inserted into `mapped_remembered`. That set is
    /// append-only for the life of the heap ("permanent root"), so a repeat
    /// write by the same owner has nothing to add on the partition-only path
    /// (owner tracking Disabled, no concurrent mark — both re-checked before
    /// this cache is consulted). Reset whenever a heap is (re)installed.
    static TAGGED_HEAP_LAST_REMEMBERED: Cell<usize> = const { Cell::new(0) };
    /// Auto-allocated heap for tests that construct Values without a Context.
    #[cfg(test)]
    static TEST_FALLBACK_TAGGED_HEAP: std::cell::RefCell<Option<Box<TaggedHeap>>> =
        const { std::cell::RefCell::new(None) };
}

static NEXT_TAGGED_HEAP_ID: AtomicUsize = AtomicUsize::new(1);

fn next_tagged_heap_identity() -> usize {
    NEXT_TAGGED_HEAP_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Background GC thread (concurrent collector, Phase 4)
// ---------------------------------------------------------------------------

/// A raw `*mut TaggedHeap` that can cross to the GC thread. The heap is `!Send`
/// (raw pointers), but during a handshake the mutator is BLOCKED waiting for the
/// GC thread, so the two threads never touch the heap at the same time — the GC
/// thread has exclusive access for the duration. (Phase 5 makes access genuinely
/// concurrent via the atomic slots + SATB built in Phases 1-3.)
struct HeapPtr(*mut TaggedHeap);
unsafe impl Send for HeapPtr {}

/// A non-blocking concurrent-mark job (Phase 5). Carries everything the GC
/// thread needs WITHOUT a `&mut TaggedHeap` — two threads holding `&mut` to the
/// same heap is UB in Rust's model even with atomic fields. The GC thread marks
/// only conses (fixed 16B; car/cdr + mark bits are atomic) and DEFERS every
/// non-cons (and any non-owned cons) to `deferred`, traced at the stop-the-world
/// termination. So it touches no growable/reallocatable heap structure.
struct ConcurrentMarkJob {
    /// Root snapshot, moved out of the heap's gray queue at the start handshake.
    gray: Vec<TaggedValue>,
    /// Base addresses of every owned cons block at the snapshot (immutable,
    /// read-only on the GC thread). A cons whose block base is here is markable
    /// via block arithmetic; others (mapped/dump, or new blocks) are deferred.
    owned_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT CLAIM DISPATCHER state (per-kind page-base snapshots,
    /// cycle parity, dump span, claim counters) for
    /// `concurrent_try_mark_owned`. Grouped in a sub-struct so the scan
    /// closures below — which mutably borrow `gray` — can borrow it
    /// disjointly. The cons arm reads the dump span from here too.
    claims: ConcurrentClaimJob,
    /// Overwritten children appended by the mutator's SATB barrier; drained here.
    satb: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// Non-cons / non-owned-cons values to trace at the STW termination.
    deferred: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// Set when gray + SATB are drained (tentatively done); polled by the mutator.
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by the mutator to ask this loop to exit.
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Task #7 stage 2a (Fix B): idle-nap wakeup latch. The mutator's
    /// `join_concurrent_mark` notifies it after setting `stop`, so the idle
    /// wait below wakes immediately instead of finishing a fixed sleep.
    wake: std::sync::Arc<(std::sync::Mutex<()>, std::sync::Condvar)>,
    /// Signalled when the loop exits, so the mutator can take over the gray queue.
    exited: std::sync::mpsc::Sender<()>,
    /// Stage 1b CONCURRENT OBARRAY SCAN: a start-captured snapshot of the obarray's
    /// chunked symbol store. When `Some`, the GC thread scans these symbol cells
    /// ONCE per cycle, feeding each symbol's heap children into `gray` (conses) /
    /// `deferred` (non-cons) like the gray-drain cons branch. Always `Some` for a
    /// concurrent mark — the start handshake captures it.
    obarray: Option<crate::emacs_core::symbol::ObarrayScanSnapshot>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: a start-captured snapshot of every
    /// OWNED/Mapped vector backing (base ptr + len). When `Some`, the GC thread
    /// traces these backings ONCE per cycle, feeding each slot's heap children into
    /// `gray` (conses) / `deferred` (non-cons) like the gray-drain cons branch, so
    /// vectors are marked concurrently instead of deferred to the STW termination.
    /// Always `Some` for a concurrent mark — the start handshake captures it.
    vectors: Option<crate::tagged::header::VectorScanSnapshot>,
    /// FIRST PARTITION CYCLE: the mapped (pdump) cons ranges, staged by
    /// `begin_collection` and moved here by `launch_concurrent_mark`, as
    /// `(start_addr, len)` pairs. Scanned on the GC thread BEFORE the drain
    /// (same load-bearing order as the obarray/vector snapshots): the ranges
    /// address immutable process-lifetime mappings, the cons slots are the
    /// Phase-1 atomic slots (`load_car`/`load_cdr`), and racing mutator
    /// writes are covered by the SATB barrier exactly as for young conses.
    /// `None` for every later cycle (the image is black; young children come
    /// from the remembered set).
    mapped_cons_ranges: Option<Vec<(usize, usize)>>,
    /// FIRST PARTITION CYCLE: mapped veclike header addresses, staged like
    /// the cons ranges. Scanned on the GC thread by
    /// [`concurrent_trace_mapped_veclike`]: every arm reads slots through
    /// the Phase-1 atomic loads (`iter_atomic`/`load_value_atomic` — the
    /// same accessors `trace_veclike` uses), mapped `LispValueVec` backings
    /// are retire-on-write (immutable in place), and any kind the
    /// GC-thread tracer does not port (hash tables: mutator-side weak
    /// registry + non-atomic map iteration) defers the OBJECT to the
    /// termination's full `mark_value`.
    mapped_veclikes: Option<Vec<usize>>,
}

/// CONCURRENT CLAIM DISPATCHER (task 01) per-cycle state: everything
/// `concurrent_try_mark_owned` needs to classify + claim a discovered value
/// on the GC thread. All snapshots are captured at the world-stopped start
/// handshake (immutable, read-only on the GC thread — the live registries /
/// bitmaps belong to the mutator) and published through the same
/// `Arc`/channel happens-before as the cons `owned_bases`.
struct ConcurrentClaimJob {
    /// THIS cycle's young non-cons mark parity, captured at launch. The GC
    /// thread's claims must mark to the CURRENT parity ("marked" ≡ bit
    /// == parity); the heap cannot flip mid-cycle (`begin_collection` is the
    /// only flip point and the next one cannot run before this mark joins),
    /// so the captured value is valid for the job's whole lifetime.
    parity: bool,
    /// CONCURRENT STRING MARKING claim oracle (stage 3): the base address of
    /// every STRING ARENA PAGE at the world-stopped start handshake.
    /// Snapshot-hit ⇒ an owned page string ⇒ claim-eligible; MISS ⇒ DEFER,
    /// which is fail-safe for everything else: pages created mid-cycle
    /// (their strings are born-at-parity anyway), mapped (pdump) strings
    /// (marked via the side-table bool — claiming their `GcHeader` bit would
    /// skip the mapped mark + interval trace at termination, a UAF of their
    /// interval children), and any residual `Box` string (none are allocated
    /// anymore, but a miss merely defers). A page base can never collide
    /// with non-page memory: a page owns its whole 64KB span exclusively.
    string_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT FLOAT CLAIMS (task 01): the base address of every FLOAT
    /// ARENA PAGE at the world-stopped start handshake (retired pages
    /// included — their tenured floats short-circuit to drop). Same
    /// discipline as `string_page_bases`: HIT ⇒ owned page float ⇒
    /// claim-eligible; MISS ⇒ DEFER (fail-safe for mid-cycle pages, mapped
    /// (pdump) floats — which mark via the heap's `mapped_float_ranges` side
    /// bitmaps only the mutator may touch — and any residual `Box` float).
    float_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): the base address of every
    /// VECTOR ARENA PAGE at the world-stopped start handshake (retired pages
    /// included). A page is homogeneous (`VectorObj` slots only), so a HIT
    /// both proves ownership AND classifies the veclike as a plain Vector
    /// without reading its header. MISS ⇒ DEFER: mapped (pdump) vectors
    /// (side-table marks + termination `trace_veclike`), any residual `Box`
    /// vector (none are constructible today — `alloc_vector` is the single
    /// Vector chokepoint — but a miss merely defers), and vectors in pages
    /// created mid-cycle. See the claim arm for why page-hit vectors may be
    /// claimed without deferring their CURRENT backing to the termination.
    vector_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT BYTECODE CLAIMS (task 01, finishing arm): the base address
    /// of every BYTECODE ARENA PAGE at the world-stopped start handshake
    /// (retired pages included — their tenured bytecode short-circuits to
    /// drop at the arm). Same discipline as `vector_page_bases`: a page is
    /// homogeneous (384-byte `ByteCodeObj` slots only), so a HIT both proves
    /// ownership AND classifies the veclike as ByteCode without reading its
    /// header. MISS ⇒ DEFER (fail-safe): mapped/dump-span bytecode (marks
    /// live in mutator-only side tables; termination `trace_veclike`), any
    /// residual `Box` bytecode (none are constructible — `alloc_bytecode` is
    /// the single ByteCode chokepoint — but a miss merely defers), and
    /// bytecode in pages created mid-cycle. Unlike vectors (children covered
    /// by the Tier-B backing scan), a claimed bytecode's children are
    /// GRAY-PUSHED by the claim arm itself — see the load-bearing
    /// immutability comment there.
    bytecode_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// Dump (pdump mmap) address span. The cons arm skips conses inside
    /// (permanent-black; young children come from the remembered set); the
    /// subr arm defers span-inside veclikes (every MAPPED veclike
    /// registration extends this span, so span-inside covers the whole
    /// mapped-veclike population whose marks live in mutator-only side
    /// tables).
    dump_lo: usize,
    dump_hi: usize,
    /// FIRST PARTITION CYCLE (concurrent bootstrap): span-inside children are
    /// DROPPED at the dispatcher instead of deferred. Sound because the flat
    /// mapped scans (veclike+string children at the start handshake, the
    /// staged cons-range scan on this thread) enumerate EVERY mapped object's
    /// children — no reachability through the image is needed, and the whole
    /// image is blackened wholesale at cycle completion
    /// (`finish_first_partition_cycle`). Load-bearing: with plain deferral
    /// the STW termination's `mark_value` would TRACE THROUGH the
    /// un-blackened image transitively — the whole bootstrap cost moved into
    /// the pause.
    drop_dump_children: bool,
    /// CONCURRENT STRING MARKING: count of owned interval-free strings this
    /// cycle's GC thread claimed via `concurrent_try_mark_string` (one per
    /// successful `mark_claim_at`, Relaxed — single writer). Read by
    /// `join_concurrent_mark` (after the exit handshake's happens-before) into
    /// the cycle stats; sizes how much string work left the STW drain.
    str_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT FLOAT CLAIMS: same pattern as `str_claimed` — owned young
    /// page floats claimed this cycle (one per successful `mark_claim_at`).
    float_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT VECTOR-HEADER CLAIMS: same pattern — owned young page
    /// vectors whose header this cycle's GC thread claimed.
    vec_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT BYTECODE CLAIMS: same pattern — owned young page bytecode
    /// this cycle's GC thread claimed (and gray-pushed the children of).
    bc_claimed: std::sync::Arc<AtomicUsize>,
    /// SUBR RECOGNIZE-AND-DROP: how many times the GC thread dropped a
    /// leaked-static subr from the defer path this cycle. Counts drop
    /// EVENTS, not unique subrs (dropping is stateless, so a subr
    /// re-discovered through many edges counts once per edge) — a
    /// diagnostic for how much parked-buffer traffic the drop removes.
    subr_dropped: std::sync::Arc<AtomicUsize>,
}

/// A unit of work handed to the GC thread, plus a oneshot done-channel the GC
/// thread signals when finished so the mutator can resume.
///
/// The variant sizes differ (the mark job carries the per-cycle claim
/// snapshots), but exactly one request is in flight per GC cycle, so boxing
/// the large variant would buy nothing.
#[allow(clippy::large_enum_variant)]
enum GcRequest {
    /// Drain the gray queue (mark to a fixpoint) on the GC thread.
    MarkAll(HeapPtr, std::sync::mpsc::Sender<()>),
    /// Non-blocking concurrent mark (Phase 5): mark conses while the mutator
    /// runs; defer everything else to the termination handshake.
    ConcurrentMark(ConcurrentMarkJob),
}

static GC_THREAD: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<GcRequest>>> =
    std::sync::OnceLock::new();

/// Lazily spawn the process-global GC thread and return its request channel.
/// The thread lives for the process; it loops draining requests.
fn gc_thread() -> std::sync::MutexGuard<'static, std::sync::mpsc::Sender<GcRequest>> {
    GC_THREAD
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<GcRequest>();
            std::thread::Builder::new()
                .name("neovm-gc".to_string())
                .spawn(move || {
                    while let Ok(req) = rx.recv() {
                        match req {
                            GcRequest::MarkAll(HeapPtr(p), done) => {
                                // Exclusive access: the mutator is blocked on
                                // `done` until we signal.
                                unsafe { (*p).mark_all() };
                                let _ = done.send(());
                            }
                            GcRequest::ConcurrentMark(job) => {
                                run_concurrent_mark(job);
                            }
                        }
                    }
                })
                .expect("spawn neovm-gc thread");
            std::sync::Mutex::new(tx)
        })
        .lock()
        .expect("gc thread channel poisoned")
}

/// Atomically set an OWNED cons cell's mark bit using only its pointer. The mark
/// bitmap lives at `block_base + CONS_MARKS_OFFSET`, derivable from the pointer
/// with no `&TaggedHeap`, so the concurrent GC thread marks conses without an
/// aliasing `&mut`. Returns true if this call set the bit (was unmarked).
///
/// # Safety
/// `ptr` must be a cell-aligned cons in an owned `ConsBlock` (verified by the
/// caller against the start-of-cycle owned-base set). Passing a dump/mapped cons
/// would scribble a mark bit into the wrong region.
#[inline]
unsafe fn atomic_mark_owned_cons_ptr(ptr: *const ConsCell) -> bool {
    let addr = ptr as usize;
    let base = addr & !(CONS_BLOCK_ALIGN - 1);
    let index = (addr - base) / size_of::<ConsCell>();
    let word_index = index / CONS_MARK_BITS_PER_WORD;
    let mask = 1usize << (index % CONS_MARK_BITS_PER_WORD);
    let word = unsafe { &*((base + CONS_MARKS_OFFSET) as *const AtomicUsize).add(word_index) };
    (word.fetch_or(mask, Ordering::Relaxed) & mask) == 0
}

/// CONCURRENT STRING MARKING: try to mark one discovered string on the GC
/// thread. Returns `true` when fully handled here (claimed now, or already
/// marked); `false` means the caller must park the value in `deferred` for the
/// STW termination exactly as before. Called at all three discovery sinks
/// (gray drain, obarray scan, vector-backing scan).
///
/// OWNERSHIP — a START-HANDSHAKE IMMUTABLE PAGE-BASE SNAPSHOT (stage 3;
/// replaces float-v1's dump-span test): all owned strings live in STRING
/// ARENA PAGES, and `string_page_bases` captures every string page base at
/// the world-stopped launch (same `Arc` publication as the cons
/// `owned_bases`). Snapshot-hit ⇒ this is an owned page string (a 64KB page
/// owns its whole span exclusively, so no mapped or foreign address can mask
/// to a registered base) ⇒ claim-eligible. MISS ⇒ DEFER — fail-safe for
/// every other population: pages created mid-cycle (their strings are
/// born-at-parity and need no claim), mapped (pdump) strings (marked via the
/// SEPARATE `MappedStringObject::marked` side bool that sweep/verify
/// consult — claiming their `GcHeader` bit here would let the termination's
/// `mark_value` skip the mapped mark and the interval trace, a use-after-free
/// of their interval children), and residual `Box` strings (none are
/// allocated anymore; any root-reachable stragglers simply keep deferring —
/// the bounded permanent drain tail measured at the cutover). No alloc-bit
/// or registry read happens here: those live structures belong to the
/// mutator (which allocates into snapshot pages mid-cycle); the snapshot is
/// the GC thread's only ownership authority.
///
/// INTERVALS — the hard boundary: the GC thread reads ONLY the interval
/// pointer WORD (`intervals_ptr`), NEVER the table behind it. The mutator can
/// free the table at any instant via `clear_intervals`, so calling
/// `intervals()` / `is_empty()` / `for_each_root()` here is a use-after-free.
/// Any future "trace small interval trees concurrently" extension needs a
/// retire/snapshot scheme like the Tier B vector backings — do not shortcut.
///
/// The null-check runs BEFORE the claim: claiming an interval-BEARING string
/// and then deferring it would make the termination's `mark_value` see the
/// mark bit and return without tracing the intervals. Staleness is safe both
/// ways: a stale non-null word only defers spuriously; a "stale null" can
/// only follow a real `clear_intervals`, whose SATB barrier (enforced inside
/// the `LispString` mutators) logged the dropped children first.
///
/// SATB ARGUMENT for a table installed AFTER the claim (equivalently: for a
/// string ALLOCATED DURING this mark that gains intervals): claiming, then
/// never re-visiting, is sound because every value the mutator can store
/// into that table was obtained from a snapshot-reachable home (whose
/// reachability the snapshot roots + the deletion barriers preserve: an
/// overwrite of the child's original home logs the pre-image to the SATB
/// buffer before the store) or was allocated black this cycle. Either way
/// the child survives THIS cycle without the claimed string being traced,
/// and the NEXT cycle re-traces the string's intervals against fresh marks.
#[inline]
fn concurrent_try_mark_string(
    val: TaggedValue,
    string_page_bases: &FxHashSet<usize>,
    parity: bool,
    str_claimed: &AtomicUsize,
) -> bool {
    debug_assert!(val.is_string());
    let Some(ptr) = val.as_string_ptr() else {
        return false; // malformed value — let the termination's mark_value decide
    };
    let base = (ptr as usize) & !(OBJECT_PAGE_ALIGN - 1);
    if !string_page_bases.contains(&base) {
        return false; // snapshot MISS: mid-cycle page / mapped / residual — defer
    }
    // Owned page string. Read the interval pointer WORD only (see doc above).
    if !unsafe { (*ptr).data.intervals_ptr() }.is_null() {
        return false; // interval-bearing: defer so mark_value traces the children
    }
    // Interval-free owned string: zero Lisp children, so claiming the mark bit
    // IS the complete trace. The claim swaps in the CYCLE parity (carried into
    // the job at launch): a string marked LAST cycle holds the old parity and
    // is correctly claimable again this cycle. A failed claim means someone
    // already marked it at this parity (allocate-black, or an earlier edge
    // this cycle) — equally done, and a lost race leaves bit == parity either
    // way. A TENURED string can arrive here too (e.g. via an obarray symbol
    // cell or root edge): the swap scribbles its frozen bit, which is benign —
    // every tenured reader short-circuits on the `tenured` flag before
    // interpreting the bit, and "handled, no children" is exactly the tenured
    // (permanent-black) semantics for an interval-free string.
    if unsafe { (*ptr).header.mark_claim_at(parity) } {
        str_claimed.fetch_add(1, Ordering::Relaxed);
    }
    true
}

/// CONCURRENT CLAIM DISPATCHER (task 01): try to fully handle one discovered
/// non-cons heap value on the GC thread. Returns `true` when handled here
/// (claimed now, or already marked — nothing further owed this cycle);
/// `false` means the caller must park the value in `deferred` for the STW
/// termination exactly as before. Called at all three GC-thread discovery
/// sinks (gray drain, obarray scan, vector-backing scan). `gray` is the GC
/// thread's local worklist: the bytecode arm pushes a newly-claimed
/// object's children there, so the drain traces them to the fixpoint (a
/// mid-drain stop hands residual gray to the termination like `deferred`).
///
/// Per-kind arms are added one commit at a time. Every arm carries its own
/// snapshot-classify + claim step and REFUSES (→ defer, fail-safe) anything
/// not provably its case — a classification MISS must always defer, never
/// "miss ⇒ mapped" (a mid-cycle heap object misclassified as mapped would be
/// a dropped mark = UAF). Arms wired so far:
///
/// - strings: `concurrent_try_mark_string` (owned interval-free pages);
/// - floats: page-snapshot claim (zero Lisp children — `mark_value`'s float
///   arm is mark-only);
/// - subrs: recognize-and-drop (leaked statics — not a claim at all);
/// - vectors: page-snapshot header claim (children covered by the Tier-B
///   backing scan + SATB — see the load-bearing comment at the arm);
/// - bytecode: page-snapshot header claim + GC-thread gray-push of the
///   children (sound only because published bytecode is immutable — see the
///   load-bearing comment at the arm).
///
/// Arm-internal ordering is mandated (H4/H5): any inspection that can still
/// send the value to `deferred` runs BEFORE the claim (a claimed-then-
/// deferred object whose termination trace early-returns on the mark bit
/// would drop its children), and the TENURED check runs before the parity
/// claim (tenured ≡ permanently black; the flag froze at promotion, which
/// only runs world-stopped, so the read is stable on this thread).
/// Alpha-1/2 exponentially-weighted moving average step for the mark-start
/// pacer's per-cycle samples. Seeds directly from the first nonzero sample.
#[inline]
fn ewma_half(prev: u64, sample: u64) -> u64 {
    if prev == 0 {
        sample
    } else {
        (prev / 2).saturating_add(sample / 2)
    }
}

#[inline]
fn concurrent_try_mark_owned(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    gray: &mut Vec<TaggedValue>,
) -> bool {
    // FIRST PARTITION CYCLE: a child inside the dump span is fully handled
    // (see `ConcurrentClaimJob::drop_dump_children`) — nothing owed.
    if job.drop_dump_children
        && let Some(addr) = TaggedHeap::value_heap_addr(val)
        && addr >= job.dump_lo
        && addr < job.dump_hi
    {
        return true;
    }
    if val.is_string() {
        return concurrent_try_mark_string(
            val,
            &job.string_page_bases,
            job.parity,
            &job.str_claimed,
        );
    }
    if val.is_float() {
        // CONCURRENT FLOAT CLAIMS (task 01): a float has ZERO Lisp children
        // (`mark_value`'s float arm is mark-only), so claiming the mark bit
        // IS the complete trace. Ownership via the same start-handshake
        // page-base snapshot discipline as strings — no dereference before
        // the page-base hit proves this is a live float-arena slot.
        let Some(ptr) = val.as_float_ptr() else {
            return false; // malformed value — let the termination decide
        };
        let base = (ptr as usize) & !(OBJECT_PAGE_ALIGN - 1);
        if !job.float_page_bases.contains(&base) {
            // Snapshot MISS: mid-cycle page (born-at-parity anyway), mapped
            // (pdump) float (marks via the mutator-only side ranges), or a
            // residual Box float — DEFER, fail-safe.
            return false;
        }
        // TENURED short-circuit BEFORE the claim (H5): tenured ≡ permanently
        // black, never re-traced/re-swept — "handled, nothing owed" without
        // touching the frozen mark bit.
        if unsafe { (*ptr).header.tenured } {
            return true;
        }
        // Young owned page float: claim at THIS cycle's parity. A failed
        // claim means it is already black (allocate-black or an earlier edge
        // this cycle) — equally done.
        if unsafe { (*ptr).header.mark_claim_at(job.parity) } {
            job.float_claimed.fetch_add(1, Ordering::Relaxed);
        }
        return true;
    }
    if val.is_veclike() {
        let Some(ptr) = val.as_veclike_ptr() else {
            return false; // malformed value — let the termination decide
        };
        let addr = ptr as usize;
        // CONCURRENT VECTOR-HEADER CLAIMS (task 01). Page-base hit FIRST,
        // before any dereference: vector-arena pages are homogeneous
        // `VectorObj` slots, so a hit is simultaneously the ownership proof
        // and the type classification. CLAIM ONLY ON PAGE-HIT: page-resident
        // vectors are exactly the Tier-B-registered population
        // ({page vectors} ⊆ `vector_object_addrs` — the launch-time debug
        // cross-check asserts this inclusion from this arm's perspective),
        // so a claimed vector's backing is in this cycle's Tier-B snapshot
        // and its children trace concurrently. Box-residual/mapped vectors
        // MISS and keep the STW defer path (termination `mark_value` marks
        // them and runs `trace_veclike` on their CURRENT backing).
        //
        // THE LOAD-BEARING SUBTLETY: claiming the header removes the
        // termination's CURRENT-BACKING re-trace backstop — `mark_value`
        // early-returns on the mark bit (`is_marked_at`), so
        // `trace_veclike` never runs for a claimed vector. Its
        // current-backing children are then covered ONLY by
        //   {Tier-B start-snapshot scan of the (possibly retired-on-write)
        //    start backing} + {SATB deletion barrier on every slot/bulk
        //    overwrite} + {allocate-black for mid-cycle values} + {the
        //    termination root reseed} + {the termination INSERTION-COVERAGE
        //    re-trace of every owner mutated this cycle —
        //    `satb_snapshotted_owners` at `join_concurrent_mark`}.
        // The last leg is NOT optional: the SATB deletion barrier preserves
        // only SNAPSHOT-time children, so a pre-existing value INSERTED
        // mid-cycle from a mutator register (root→heap motion; e.g.
        // `set_vector_slot` after the Tier-B scan already ran) has no other
        // covered home once its register/root copies are gone. Before the
        // claims, the STW termination re-traced every deferred vector's
        // CURRENT backing, which silently covered such insertions; the
        // dirty-owner re-trace restores exactly that, scoped to mutated
        // owners. Every write path into a VectorObj backing MUST therefore
        // fire the `mutate.rs` barriers (`set_vector_slot`'s pre-image log +
        // atomic store; `with_vector_data_mut`'s bulk pre-image log +
        // clone-on-write retire) — an unbarriered vector-slot writer would
        // now be a dropped mark (UAF), not just a duplicated trace.
        //
        // Mid-cycle vectors in REUSED SLOTS of snapshotted pages do NOT
        // defer (their page base IS in the snapshot): they are born-at-
        // parity, so `mark_claim_at` returns "already marked" ⇒ handled ⇒
        // their constructor contents are covered by the born-black/SATB
        // argument — they came from snapshot-reachable homes (whose
        // overwrites the deletion barrier logs), are themselves born-black,
        // or were in the world-stopped start root snapshot; post-
        // construction insertions are covered by the dirty-owner re-trace
        // like any other write. The NEXT cycle re-traces against fresh
        // marks. Their backing is absent from this cycle's Tier-B snapshot,
        // which is exactly the allocate-black story vectors already had.
        let base = addr & !(OBJECT_PAGE_ALIGN - 1);
        if job.vector_page_bases.contains(&base) {
            // Page vectors are 64-byte slots; a page-hit veclike value must
            // decode to a slot boundary (page-homogeneity argument above).
            debug_assert_eq!(
                (addr - base) % <VectorObj as PagedObject>::SLOT_BYTES,
                0,
                "page-hit veclike value does not address a vector slot",
            );
            // TENURED short-circuit BEFORE the claim (H5): permanently
            // black, never re-traced; frozen at the world-stopped promotion.
            if unsafe { (*ptr).gc.tenured } {
                return true;
            }
            if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                job.vec_claimed.fetch_add(1, Ordering::Relaxed);
            }
            return true;
        }
        // CONCURRENT BYTECODE CLAIMS (task 01, finishing arm). Page-base hit
        // FIRST, before any dereference: bytecode-arena pages are
        // homogeneous 384-byte `ByteCodeObj` slots, so a hit is
        // simultaneously the ownership proof and the type classification
        // (and rules out mapped: a page owns its 64KB span exclusively).
        // MISS ⇒ DEFER, fail-safe: mapped/dump bytecode (side-table marks +
        // termination `trace_veclike`), mid-cycle pages (their bytecode is
        // born-at-parity anyway), and any residual Box bytecode keep the STW
        // path unchanged.
        //
        // THE LOAD-BEARING IMMUTABILITY ARGUMENT: on a fresh claim this arm
        // reads the object's `ByteCodeFunction` fields — `constants:
        // Vec<Value>` / `extra_slots` through plain non-atomic loads on the
        // GC thread. That is sound ONLY because post-publish bytecode
        // immutability is COMPILE-TIME ENFORCED (task 03/3a): the one
        // mutation seam is `#[cfg(test)] with_bytecode_data_mut_for_test`
        // (`mutate.rs` — gated out of production builds, with the
        // hard-invariant doc), `aset` has no ByteCode arm, and the pdump
        // restore (`install_restored_bytecode_data`) initializes a fresh
        // placeholder PRE-PUBLISH, like `alloc_bytecode`'s own constructor
        // write. Pre-publish writes happen-before the world-stopped start
        // handshake that snapshotted the page, which happens-before this
        // job's Arc/channel publication — so every claimable (= unmarked at
        // this parity, i.e. pre-cycle) bytecode's fields are stable and
        // race-free here. Any new mutation path must first add vector-style
        // clone-on-write (see `with_vector_data_mut`) — do NOT just read.
        //
        // COVERAGE ARGUMENT (why a claimed bytecode never being re-traced at
        // the termination drain — `mark_value` early-returns on the mark
        // bit — drops no children):
        //  (a) fresh claim: THIS arm gray-pushes exactly the fields
        //      `trace_veclike`'s ByteCode arm traces (arglist, constants,
        //      env, doc_form, interactive, extra_slots; `params` carries
        //      only SymIds — untraced by design), and the drain traces them
        //      to the fixpoint (a mid-drain stop hands residual gray to the
        //      termination).
        //  (b) mid-cycle-ALLOCATED bytecode in a NEW page: not in the
        //      snapshot ⇒ deferred ⇒ the termination marks it and traces
        //      its current children in full.
        //  (c) mid-cycle bytecode in a REUSED SLOT of a snapshotted page
        //      (page-hit, born-at-parity ⇒ `mark_claim_at` fails ⇒ handled
        //      WITHOUT a children push — and without reading its fields,
        //      which its constructor may still be racing): its children
        //      were installed PRE-PUBLISH during construction from values
        //      live/reachable at that moment — each child is
        //      snapshot-reachable at its source home (deletions of that
        //      source are SATB-barriered) or born-black this cycle;
        //      register-moved insertions into OTHER owners are covered by
        //      the termination's dirty-owner re-gray
        //      (`satb_snapshotted_owners`). The NEXT cycle re-traces the
        //      bytecode against fresh marks. This is the vector arm's
        //      reused-slot argument verbatim, minus post-publish insertions
        //      into the bytecode itself — immutability rules those out.
        if job.bytecode_page_bases.contains(&base) {
            // Page bytecode is 384-byte slots; a page-hit veclike value
            // must decode to a slot boundary (page-homogeneity argument).
            debug_assert_eq!(
                (addr - base) % <ByteCodeObj as PagedObject>::SLOT_BYTES,
                0,
                "page-hit veclike value does not address a bytecode slot",
            );
            // TENURED short-circuit BEFORE the claim (H5): permanently
            // black, never re-traced/re-swept; frozen bit untouched. Its
            // young children are the promotion-time page-tenured
            // remembered-set scan's job, exactly as on the defer path.
            if unsafe { (*ptr).gc.tenured } {
                return true;
            }
            if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                job.bc_claimed.fetch_add(1, Ordering::Relaxed);
                // Fresh claim: gray-push the children (coverage leg (a)).
                // Field reads are race-free per the immutability argument
                // above (a fresh claim proves the object pre-dates the
                // cycle, so construction completed before the snapshot).
                let data = unsafe { &(*(ptr as *const ByteCodeObj)).data };
                // Lazy pdump stubs are confined to the MAPPED image (the
                // arena/descriptor load fallback stays eager): this arm
                // reads fields with plain loads on the GC thread, which a
                // mid-materialize ~350-byte overwrite would tear.
                debug_assert!(
                    !data.is_pdump_stub(),
                    "arena bytecode must never be a lazy pdump stub"
                );
                if data.arglist.is_heap_object() {
                    gray.push(data.arglist);
                }
                for &c in &data.constants {
                    if c.is_heap_object() {
                        gray.push(c);
                    }
                }
                if let Some(env) = data.env
                    && env.is_heap_object()
                {
                    gray.push(env);
                }
                if let Some(doc_form) = data.doc_form
                    && doc_form.is_heap_object()
                {
                    gray.push(doc_form);
                }
                if let Some(interactive) = data.interactive
                    && interactive.is_heap_object()
                {
                    gray.push(interactive);
                }
                for &s in &data.extra_slots {
                    if s.is_heap_object() {
                        gray.push(s);
                    }
                }
            }
            // Already marked (lost race, earlier edge, or born-at-parity —
            // coverage leg (c)): equally handled, nothing further owed.
            return true;
        }
        // MAPPED (pdump) veclikes mark via the heap's side table
        // (`mapped_veclike_objects[..].marked`), which only the mutator may
        // touch → always DEFER. The range check runs before any header
        // read (the vector page-hit above cannot be a mapped object: a
        // page owns its 64KB span exclusively): every mapped-veclike
        // registration extends the dump span
        // (`register_mapped_veclike_object`), so span-inside covers the
        // entire mapped population. Recognizing a mapped subr as "leaked"
        // (or claiming its header) would leave its side-table mark unset
        // and panic the tricolor/partition verifiers — the mis-claim UAF
        // shape.
        if addr >= job.dump_lo && addr < job.dump_hi {
            return false;
        }
        // SUBR RECOGNIZE-AND-DROP (task 01 — NOT a claim). SubrObjs are
        // `Box::leak`ed statics (`allocate_static_subr_object`): never
        // page-allocated, never linked into `all_objects`/
        // `non_cons_object_addrs`, never swept — permanently live by
        // construction. The header mark bit of a leaked subr is DEAD STATE
        // nobody reads: `is_value_marked` answers an unconditional `true`
        // for not-owned/not-mapped veclikes, and the termination's
        // `mark_value` is a no-op for them (`owns_veclike_object` false,
        // mapped-lookup miss). Deferring one is pure parked-buffer waste;
        // "handled, nothing owed" is exact. We do NOT write the header —
        // no claim — and a subr has no Lisp children to trace
        // (`trace_veclike`'s Subr arm is empty; `name`/`sym_id` are interner
        // ids, not Values; `update_static_subr_object_entry`'s in-place
        // rewrites touch function/arity metadata only). The `type_tag` read
        // is construction-immutable, same read discipline as the string
        // arm's interval word.
        if unsafe { (*ptr).type_tag } == VecLikeType::Subr {
            job.subr_dropped.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        return false;
    }
    false
}

/// The background concurrent-mark loop (Phase 5). Runs on the "neovm-gc" thread
/// with no `&mut TaggedHeap`: it marks conses via atomic block-bitmap ops +
/// atomic car/cdr loads, claims the kinds the claim dispatcher recognizes
/// (`concurrent_try_mark_owned` — e.g. owned interval-free strings, mark-only,
/// zero children) via their atomic header mark bit,
/// and defers every other non-cons (and non-owned conses) to the mutator's
/// stop-the-world termination. Loops draining its local gray queue and the
/// shared SATB buffer until both are empty and the mutator asks it to stop.
/// GC-thread child enumeration for ONE mapped veclike (first partition
/// cycle). Mirrors `trace_veclike`'s atomic reads, routing children like the
/// obarray/cons scans: span-inside children drop (the flat scans cover every
/// mapped object), symbols dedup into `deferred`, young heap values go
/// through the claim dispatcher. Kinds with mutator-only side effects
/// (hash tables) defer the whole OBJECT to the termination.
fn concurrent_trace_mapped_veclike(
    ptr: *mut VecLikeHeader,
    job: &mut ConcurrentMarkJob,
    seen_symbols: &mut FxHashSet<usize>,
) {
    let mut route =
        |child: TaggedValue, job: &mut ConcurrentMarkJob, seen_symbols: &mut FxHashSet<usize>| {
            if child.is_cons() {
                let addr = child.xcons_ptr() as usize;
                if addr < job.claims.dump_lo || addr >= job.claims.dump_hi {
                    job.gray.push(child);
                }
            } else if child.is_symbol() {
                if seen_symbols.insert(child.bits() as usize) {
                    job.deferred.lock().unwrap().push(child);
                }
            } else if child.is_heap_object() {
                if !concurrent_try_mark_owned(child, &job.claims, &mut job.gray) {
                    job.deferred.lock().unwrap().push(child);
                }
            }
        };
    match unsafe { (*ptr).type_tag } {
        VecLikeType::Vector => {
            let obj = ptr as *const VectorObj;
            for val in unsafe { (*obj).data.iter_atomic() } {
                route(val, job, seen_symbols);
            }
        }
        VecLikeType::Record | VecLikeType::WindowConfiguration => {
            let obj = ptr as *const RecordObj;
            for val in unsafe { (*obj).data.iter_atomic() } {
                route(val, job, seen_symbols);
            }
        }
        VecLikeType::SubCharTable => {
            let obj = unsafe { &*(ptr as *const SubCharTableObj) };
            for val in obj.contents.iter_atomic() {
                route(val, job, seen_symbols);
            }
        }
        VecLikeType::CharTable => {
            let obj = unsafe { &*(ptr as *const CharTableObj) };
            for value in [
                load_value_atomic(&obj.defalt),
                load_value_atomic(&obj.parent),
                load_value_atomic(&obj.purpose),
                load_value_atomic(&obj.ascii),
            ] {
                route(value, job, seen_symbols);
            }
            for slot in &obj.contents {
                route(load_value_atomic(slot), job, seen_symbols);
            }
            for val in obj.extras.iter_atomic() {
                route(val, job, seen_symbols);
            }
        }
        _ => {
            // Not ported (hash tables, anything exotic): the whole object
            // goes to the termination's `mark_value`, which marks the side
            // table and runs the mutator-side `trace_veclike`. Direct push
            // bypasses the dispatcher so `drop_dump_children` cannot eat it.
            //
            // TRIPWIRE: porting ByteCode into concurrent mapped tracing is
            // FORBIDDEN while lazy pdump stubs exist without atomic payload
            // publication — the mutator materializes a stub with a plain
            // whole-data write, safe today only because this arm defers all
            // mapped bytecode to the mutator side.
            job.deferred
                .lock()
                .unwrap()
                .push(unsafe { TaggedValue::from_veclike_ptr(ptr) });
        }
    }
}

fn run_concurrent_mark(mut job: ConcurrentMarkJob) {
    use std::sync::atomic::Ordering;
    // LOAD-BEARING ORDER (task 01, vector-header claims): both start-snapshot
    // scans run TO COMPLETION *before* the stop-interruptible gray drain.
    //
    // The claim arm handles a page vector entirely on this thread, so the
    // termination's `mark_value` never re-traces its CURRENT backing — a
    // claimed vector's children are covered ONLY IF the Tier-B backing scan
    // actually enumerated the snapshot backings this cycle. Under the old
    // defer-everything design the scans could be skipped on an early stop
    // (aggressive pacing joins the mark after a few drain quanta — e.g.
    // gc_threshold=1) because every discovered veclike was re-traced at the
    // STW termination anyway; with claims that safety net is gone, and a
    // skipped scan is a swept-while-live child (the vm_mapatoms SIGSEGV:
    // the compat ObarrayObj living only in a claimed obarray-vector's slot).
    // Scanning first guarantees the enumeration for every cycle that can
    // claim; the added stop latency is the O(entries) scan itself (tens of
    // µs at profiled sizes), comparable to a few drain quanta. The obarray
    // scan is hoisted with it for the same reason: symbol cells can hold
    // claimable values whose children only the scan would surface.
    //
    // Stage 1b CONCURRENT OBARRAY SCAN: scan the start-captured symbol
    // cells ONCE per cycle, feeding each heap child into `gray` (conses) /
    // the claim dispatcher / `deferred`, exactly like the cons-drain branch
    // below; the drain then walks the transitive children to a fixpoint.
    if let Some(snap) = job.obarray.take() {
        // Safety: `snap` was captured at this cycle's world-stopped start
        // handshake; its chunk + seq pointers address the live, non-moving
        // obarray storage, and we are on the GC thread.
        unsafe {
            snap.scan(|child| {
                if child.is_cons() {
                    job.gray.push(child);
                } else if !concurrent_try_mark_owned(child, &job.claims, &mut job.gray) {
                    job.deferred.lock().unwrap().push(child);
                }
            });
        }
    }
    // Stage 2 Tier B CONCURRENT VECTOR SCAN: trace the snapshotted vector
    // backings ONCE per cycle, routing children exactly as above.
    if let Some(snap) = job.vectors.take() {
        // Safety: `snap` was captured at this cycle's world-stopped start
        // handshake; each entry's base/len addresses a live, immutable backing
        // (Mapped dump or retired-on-write Owned buffer), and we are on the GC
        // thread.
        unsafe {
            snap.scan(|child| {
                if child.is_cons() {
                    job.gray.push(child);
                } else if !concurrent_try_mark_owned(child, &job.claims, &mut job.gray) {
                    job.deferred.lock().unwrap().push(child);
                }
            });
        }
    }
    // FIRST PARTITION CYCLE: flat scan of the mapped veclike headers (see
    // the job-field doc for the safety envelope; unported kinds defer).
    if let Some(addrs) = job.mapped_veclikes.take() {
        let mut seen_symbols: FxHashSet<usize> = FxHashSet::default();
        for addr in addrs {
            concurrent_trace_mapped_veclike(
                addr as *mut VecLikeHeader,
                &mut job,
                &mut seen_symbols,
            );
        }
    }
    // FIRST PARTITION CYCLE: flat scan of the mapped cons ranges — the
    // concurrent replacement for `seed_all_mapped_children`'s cons half (the
    // 76%-of-image bulk). Children route exactly like the obarray/vector
    // scans; span-inside children drop at the dispatcher / the drain's cons
    // arm. Runs to completion before the stop-interruptible drain for the
    // same claim-coverage reason as the snapshots above.
    if let Some(ranges) = job.mapped_cons_ranges.take() {
        // Symbols route through `deferred` (mutator-side `mark_symbol`), but
        // undeduplicated the image floods it — nil alone is half the cdrs.
        // The mutator's `marked_symbols` set IS the dedup; mirror it locally.
        let mut seen_symbols: FxHashSet<usize> = FxHashSet::default();
        for (start_addr, len) in ranges {
            let start = start_addr as *const ConsCell;
            for i in 0..len {
                // Safety: the range addresses a live, immutable, process-
                // lifetime pdump mapping; cons slots are atomic (Phase 1).
                let cell = unsafe { start.add(i) };
                let car = unsafe { (*cell).load_car() };
                let cdr = unsafe { (*cell).load_cdr() };
                for child in [car, cdr] {
                    if child.is_cons() {
                        // Most children of image conses are image conses;
                        // dropping them here (the drain arm would skip them
                        // anyway) keeps ~2x the image size out of the queue.
                        let addr = child.xcons_ptr() as usize;
                        if addr < job.claims.dump_lo || addr >= job.claims.dump_hi {
                            job.gray.push(child);
                        }
                    } else if child.is_symbol() {
                        // Deduped symbol hand-off: `mark_symbol` is
                        // mutator-only, and an uninterned dumped symbol is
                        // reachable only through image data, so each UNIQUE
                        // symbol must reach the termination exactly once.
                        if seen_symbols.insert(child.bits() as usize) {
                            job.deferred.lock().unwrap().push(child);
                        }
                    } else if child.is_heap_object() {
                        // Same filter as `mark_or_push_child`: immediates
                        // (fixnums, chars) carry nothing to mark — routing
                        // them into `deferred` flooded the first termination
                        // with ~118K no-op entries.
                        if !concurrent_try_mark_owned(child, &job.claims, &mut job.gray) {
                            job.deferred.lock().unwrap().push(child);
                        }
                    }
                }
            }
        }
    }
    // Task #7 stage 2a (Fix B): how many gray items are processed between
    // `stop` polls. Small enough that a stop request interrupts a long drain
    // within ~tens of µs; large enough that the Acquire load is amortized to
    // nothing against the per-item marking work.
    const STOP_CHECK_QUANTUM: usize = 512;
    let mut since_stop_check = 0usize;
    'mark: loop {
        // Drain the local gray worklist (GC-thread-owned; no sharing).
        while let Some(val) = job.gray.pop() {
            // Fix B: react to a stop request at a bounded quantum instead of
            // only between full drains. Any remaining gray work is handed to
            // the mutator below exactly like `deferred`: the termination fold
            // pushes it into the STW gray queue, whose full `mark_value` drain
            // handles every value kind (it already receives non-owned conses
            // and every non-cons via `deferred`), so no marking is lost — the
            // residual work moves to the already-stopped-and-waiting mutator.
            since_stop_check += 1;
            if since_stop_check >= STOP_CHECK_QUANTUM {
                since_stop_check = 0;
                if job.stop.load(Ordering::Acquire) {
                    job.gray.push(val); // not processed yet — hand it back too
                    break 'mark;
                }
            }
            if val.is_cons() {
                let ptr = val.xcons_ptr();
                let addr = ptr as usize;
                if addr >= job.claims.dump_lo && addr < job.claims.dump_hi {
                    continue; // dump cons: permanent black, children via remembered set
                }
                let base = addr & !(CONS_BLOCK_ALIGN - 1);
                if !job.owned_bases.contains(&base) {
                    // Mapped (non-dump) or new-block cons — let the mutator's
                    // termination mark it through the full `mark_value` path.
                    job.deferred.lock().unwrap().push(val);
                    continue;
                }
                if unsafe { atomic_mark_owned_cons_ptr(ptr) } {
                    // cdr-chasing loop (GNU `mark_object`): mark a list spine
                    // inline instead of round-tripping every cell through the
                    // gray worklist. Each chased cell counts toward the stop
                    // quantum; on quantum the unmarked tail goes back to gray
                    // so the outer loop's stop check stays bounded.
                    let mut ptr = ptr;
                    loop {
                        let car = unsafe { (*ptr).load_car() };
                        let cdr = unsafe { (*ptr).load_cdr() };
                        if car.is_heap_object() {
                            job.gray.push(car);
                        }
                        if !cdr.is_cons() {
                            if cdr.is_heap_object() {
                                job.gray.push(cdr);
                            }
                            break;
                        }
                        since_stop_check += 1;
                        if since_stop_check >= STOP_CHECK_QUANTUM {
                            job.gray.push(cdr);
                            break;
                        }
                        let cptr = cdr.xcons_ptr();
                        let caddr = cptr as usize;
                        if caddr >= job.claims.dump_lo && caddr < job.claims.dump_hi {
                            break; // dump cons: permanent black
                        }
                        let cbase = caddr & !(CONS_BLOCK_ALIGN - 1);
                        if !job.owned_bases.contains(&cbase) {
                            job.deferred.lock().unwrap().push(cdr);
                            break;
                        }
                        if !unsafe { atomic_mark_owned_cons_ptr(cptr) } {
                            break; // already marked (shared tail)
                        }
                        ptr = cptr;
                    }
                }
            } else if val.is_heap_object() {
                // Claim dispatcher first: the kinds it recognizes (e.g. an
                // owned, interval-free string — mark-only, zero Lisp
                // children; a page bytecode — claim + children gray-pushed
                // right back onto this worklist) are fully handled right
                // here. Everything it refuses — veclikes whose backing the
                // mutator may reallocate, and interval-bearing or mapped
                // strings, which need the mutator's `mark_value` — is
                // deferred to the STW termination.
                if concurrent_try_mark_owned(val, &job.claims, &mut job.gray) {
                    continue;
                }
                job.deferred.lock().unwrap().push(val);
            }
        }
        // Fold the mutator's SATB log (overwritten children) into gray.
        let batch = { std::mem::take(&mut *job.satb.lock().unwrap()) };
        if batch.is_empty() {
            // Tentatively drained. Advertise done; exit if the mutator asked.
            job.done.store(true, Ordering::Release);
            if job.stop.load(Ordering::Acquire) {
                break 'mark;
            }
            // Idle wait — short enough to react to new SATB quickly, long
            // enough not to peg a core. Fix B: interruptible — re-check `stop`
            // UNDER the wake lock before waiting: `join_concurrent_mark`
            // stores `stop` first and only then locks+notifies, so either the
            // flag is visible here (skip the wait) or this thread is already
            // waiting when the notify lands — a wakeup cannot be lost. The
            // timeout keeps the old 100us cadence as the SATB pickup backstop
            // (SATB pushes do not notify).
            let (lock, cvar) = &*job.wake;
            let guard = lock.lock().unwrap();
            if !job.stop.load(Ordering::Acquire) {
                let _ = cvar
                    .wait_timeout(guard, std::time::Duration::from_micros(100))
                    .unwrap();
            }
        } else {
            job.done.store(false, Ordering::Release);
            job.gray.extend(batch);
        }
    }
    // Fix B: residual local gray (a mid-drain stop) joins the deferred
    // handoff; the termination fold routes both through the STW `mark_value`
    // drain. Empty on the normal (idle-stop) path.
    if !job.gray.is_empty() {
        job.deferred.lock().unwrap().extend(job.gray.drain(..));
    }
    let _ = job.exited.send(());
}

/// Set the thread-local tagged heap pointer.
pub fn set_tagged_heap(heap: &mut TaggedHeap) {
    TAGGED_HEAP.with(|h| h.set(heap as *mut TaggedHeap));
    TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.set(heap.write_tracking_mode()));
    TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.set(heap.partition_dump));
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(heap.concurrent_mark_running));
    TAGGED_HEAP_DUMP_SPAN.with(|s| s.set((heap.dump_addr_lo, heap.dump_addr_hi)));
    // Owner bits are heap-specific: a different heap invalidates the cache.
    TAGGED_HEAP_LAST_REMEMBERED.with(|l| l.set(0));
}

/// Uninstall `heap` from this thread's allocation slot, if it is the heap
/// currently installed there.
///
/// `set_tagged_heap` stores a RAW pointer with no lifetime relationship to the
/// storage it names, so whoever owns that storage must uninstall it before
/// freeing it. Leaving a stale pointer behind is not merely untidy: the next
/// `with_tagged_heap` sees a non-null slot, skips the fallback path, and
/// allocates into freed memory — the object it hands back is already a
/// use-after-free, and the next heap to reuse that storage turns its header
/// into garbage.
///
/// The pointer identity check makes this safe to call unconditionally from a
/// drop hook: an owner whose heap was already displaced by a later
/// `set_tagged_heap` leaves the newer installation alone.
pub fn clear_tagged_heap_if_installed(heap: &TaggedHeap) {
    let owned = heap as *const TaggedHeap as *mut TaggedHeap;
    TAGGED_HEAP.with(|h| {
        if h.get() == owned {
            h.set(std::ptr::null_mut());
            TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.set(WriteTrackingMode::Disabled));
            TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.set(false));
            TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(false));
            TAGGED_HEAP_DUMP_SPAN.with(|s| s.set((usize::MAX, 0)));
            TAGGED_HEAP_LAST_REMEMBERED.with(|l| l.set(0));
        }
    });
}

/// True when this thread has a tagged heap installed for allocation.
pub fn tagged_heap_is_installed() -> bool {
    TAGGED_HEAP.with(|h| !h.get().is_null())
}

/// Return the current thread's tagged heap identity, if one is installed.
///
/// This is used only for runtime side tables that must avoid retaining Lisp
/// objects from a different evaluator heap. GNU keeps those object references
/// inside ordinary GC-managed structures; the heap identity preserves that
/// ownership boundary for Neomacs side tables.
pub(crate) fn current_tagged_heap_identity() -> Option<usize> {
    TAGGED_HEAP.with(|h| {
        let ptr = h.get();
        (!ptr.is_null()).then(|| unsafe { (*ptr).identity() })
    })
}

/// Access the thread-local tagged heap.
///
/// In test mode, auto-creates a fallback heap if none is set.
/// In production, panics if no heap is set.
#[inline]
pub fn with_tagged_heap<R>(f: impl FnOnce(&mut TaggedHeap) -> R) -> R {
    TAGGED_HEAP.with(|h| {
        let ptr = h.get();
        if !ptr.is_null() {
            return f(unsafe { &mut *ptr });
        }
        #[cfg(test)]
        {
            TEST_FALLBACK_TAGGED_HEAP.with(|fb| {
                let mut borrow = fb.borrow_mut();
                if borrow.is_none() {
                    *borrow = Some(Box::new(TaggedHeap::new()));
                }
                let heap_ref: &mut TaggedHeap = borrow.as_mut().unwrap();
                let ptr = heap_ref as *mut TaggedHeap;
                h.set(ptr);
                f(unsafe { &mut *ptr })
            })
        }
        #[cfg(not(test))]
        {
            panic!("no TaggedHeap set for this thread");
        }
    })
}

/// Central mutation hook for bulk writes to the tagged heap.
#[inline]
pub fn note_heap_write(owner: TaggedValue, kind: HeapWriteKind) {
    note_heap_write_record(HeapWriteRecord::bulk(owner, kind));
}

/// Central mutation hook for slot writes to the tagged heap.
#[inline]
pub fn note_heap_slot_write(
    owner: TaggedValue,
    kind: HeapWriteKind,
    slot: usize,
    value: TaggedValue,
) {
    note_heap_write_record(HeapWriteRecord::slot(owner, kind, slot, value));
}

#[inline]
fn note_heap_write_record(record: HeapWriteRecord) {
    if !record.owner.is_heap_object() {
        return;
    }
    let disabled =
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.get()) == WriteTrackingMode::Disabled;
    // The dump partition needs the barrier even when owner-tracking is off, to
    // record mutations of dumped objects into the remembered set.
    let partition = TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.get());
    // The concurrent collector needs the barrier (its SATB log) regardless of
    // owner-tracking / partition state.
    let concurrent = TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get());
    if disabled && !partition && !concurrent {
        return;
    }
    if disabled && !concurrent {
        // Partition-only path: the barrier's sole job is the append-only dump
        // remembered set (see `record_heap_write`), so two cheap thread-local
        // rejects apply. (1) A repeat of the last-inserted owner has nothing
        // to add — its entry is permanent. (2) A cons owner's decision is the
        // dump-span test alone (`value_is_tenured` is always false for cons,
        // and neither an address nor the span ever changes), so a cons outside
        // the span never needs the heap at all.
        let bits = record.owner.bits();
        if TAGGED_HEAP_LAST_REMEMBERED.with(|l| l.get()) == bits {
            return;
        }
        if record.owner.is_cons()
            && let Some(addr) = TaggedHeap::value_heap_addr(record.owner)
        {
            let (lo, hi) = TAGGED_HEAP_DUMP_SPAN.with(|s| s.get());
            if addr < lo || addr >= hi {
                return;
            }
        }
    }
    with_tagged_heap(|heap| heap.record_heap_write(record));
}

/// SATB deletion barrier for ROOT-slot overwrites — specifically a symbol's
/// value / function / plist cell. A symbol `TaggedValue` is a `SymId`, not a heap
/// pointer, so symbol-cell writes are ROOT writes that bypass `note_heap_write`
/// (which gates on `owner.is_heap_object()`). Without logging them, the
/// concurrent mark must re-scan the whole obarray at termination to catch any
/// object that became reachable only through a symbol cell.
///
/// Call with the OLD value of the cell BEFORE the store (Yuasa snapshot-at-the-
/// beginning: the value being deleted from the root must be retained for this
/// cycle). No-ops outside a concurrent mark — a single thread-local load + branch,
/// no heap touch — and for non-heap pre-images (fixnum / UNBOUND / nil /
/// symbol-id), so cold-path callers pay essentially nothing when GC is idle.
/// Feed live mutator stack roots to an active concurrent mark (see
/// [`TaggedHeap::feed_satb_roots`]). No-op (one thread-local load) when no
/// concurrent mark is running.
#[inline]
pub(crate) fn feed_concurrent_roots(values: &[TaggedValue]) {
    if values.is_empty() || !TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get()) {
        return;
    }
    with_tagged_heap(|heap| heap.feed_satb_roots(values));
}

#[inline]
pub(crate) fn note_root_overwrite(pre_image: TaggedValue) {
    if !pre_image.is_heap_object() {
        return;
    }
    if !TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get()) {
        return;
    }
    with_tagged_heap(|heap| heap.note_root_overwrite_value(pre_image));
}

/// Whether a concurrent mark is active on this (mutator) thread — the gate the
/// Stage 1b symbol-cell seqlock uses to bracket value-cell ARM changes only
/// while the GC thread might be scanning the obarray. A thread-local load;
/// false (zero cost) off the concurrent path.
#[inline]
pub(crate) fn concurrent_mark_active() -> bool {
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get())
}

/// SATB pre-image sink for STRING interval-table mutations, called from inside
/// the `LispString` interval mutators themselves (`ensure_intervals` /
/// `clear_intervals` in heap_types.rs) so the barrier is enforced at the only
/// mutation choke points — no call site, wrapper or raw, can drop a string's
/// interval children unlogged while the concurrent GC thread may have claimed
/// the string as interval-free. Logs the table's current child VALUES (not an
/// owner) to the shared SATB buffer, deduped once per string address per cycle
/// (`satb_string_preimage_addrs`, cleared at `begin_collection`): the first
/// pre-image is a superset of the start-of-cycle children — the same argument
/// as `push_value_children_to_satb_shared`'s owner dedup. The caller has
/// already checked `concurrent_mark_active()`.
pub(crate) fn note_string_interval_preimage(
    string_addr: usize,
    table: &crate::buffer::text_props::TextPropertyTable,
) {
    with_tagged_heap(|heap| {
        if !heap.satb_string_preimage_addrs.insert(string_addr) {
            return; // this string's full pre-image was already logged this cycle
        }
        let mut shared = heap.satb_shared.lock().unwrap();
        table.for_each_root(|value| {
            if value.is_heap_object() {
                shared.push(value);
            }
        });
    });
}

// ---------------------------------------------------------------------------
// TaggedHeap — the main GC-managed heap
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum CanonicalEmptyString {
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

impl Default for CanonicalEmptyString {
    fn default() -> Self {
        Self::Missing
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
pub struct TaggedHeap {
    /// Process-unique heap identity used by side tables that carry GC-managed
    /// Lisp values.  It deliberately does not use this heap's address: boxed
    /// heaps are routinely dropped and recreated by snapshot-based tests, and
    /// the allocator may reuse an address for a different heap lifetime.
    identity: usize,

    /// Cons cell block allocator.
    cons_blocks: Vec<ConsBlock>,
    /// Base-address lookup for O(1) cons block ownership and marking.
    cons_block_index_by_base: FxHashMap<usize, usize>,
    /// Last ordinary cons block used by the mark phase.
    ///
    /// GNU's cons marker derives the block directly from the pointer and has a
    /// special fast path for successive list cells.  Keep Neomacs's explicit
    /// ownership map, but avoid probing it repeatedly while the mark queue is
    /// walking cells from the same block.
    mark_cons_block_cache: Option<ConsBlockCacheEntry>,

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

    /// Total number of allocated objects (cons + non-cons).
    pub allocated_count: usize,
    /// Lisp-visible allocation statistics backing `memory-use-counts`.
    memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],

    /// GC threshold in approximate Lisp heap bytes.
    gc_threshold: usize,
    /// When true, `gc_threshold` was explicitly overridden by tests or host
    /// code and should not be recomputed from Lisp-visible GC variables.
    gc_threshold_overridden: bool,
    /// Approximate Lisp heap bytes allocated since the last full collection.
    bytes_since_gc: usize,
    /// Monotonic managed allocation bytes used by the Lisp memory profiler.
    total_allocated_bytes: u64,
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
    /// Set by `note_must_finish` when the in-flight mark is being cap-forced;
    /// consumed by `incremental_finish` (skip the biased EWMA sample, escalate
    /// the lead instead).
    forced_termination_pending: bool,
    /// Wall-clock start of the in-flight concurrent mark (stamped at
    /// `launch_concurrent_mark`, consumed at `incremental_finish`).
    pace_mark_start: Option<std::time::Instant>,
    /// `bytes_since_gc` at the in-flight mark's start handshake.
    pace_mark_start_bytes: usize,
    /// EWMA (alpha 1/2) of bytes/sec allocated during recent mark windows.
    pace_alloc_rate_bps: u64,
    /// EWMA (alpha 1/2) of recent concurrent-mark wall durations, in µs.
    pace_mark_dur_us: u64,
    /// Projected allocation during the next mark window (rate x duration),
    /// recomputed at each clean termination; doubled on a forced one.
    pace_lead_bytes: usize,

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
    /// Membership shadow for `weak_hash_tables`: registration used to dedup
    /// with a linear contains per table, O(T^2) across a cycle. The vector
    /// stays authoritative for deterministic sweep order.
    weak_hash_tables_set: rustc_hash::FxHashSet<*mut HashTableObj>,
    /// Weak hash tables that have become PERMANENT (tenured old generation or
    /// mapped pdump image). The main mark never re-runs `trace_veclike` on a
    /// permanent-black object, so such a table would otherwise never re-register
    /// itself for the weak sweep and its entries would be pinned forever (a
    /// weak-table leak: GNU re-sweeps every weak table on every GC). Populated
    /// at `promote_and_blacken` (tenuring) and at mapped-dump registration;
    /// seeded into `weak_hash_tables` at the start of every `mark_and_sweep_
    /// weak_tables` so permanent weak tables are swept against the CURRENT cycle's
    /// marks exactly like young ones. Permanent, so its pointers never dangle.
    permanent_weak_hash_tables: Vec<*mut HashTableObj>,
    /// Membership shadow for `permanent_weak_hash_tables` (same pattern).
    permanent_weak_hash_tables_set: rustc_hash::FxHashSet<*mut HashTableObj>,
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

    /// Reclaimed cons cells threaded through the dead cells themselves,
    /// matching GNU alloc.c's `cons_free_list`.
    cons_free_list: *mut ConsCell,
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
    /// GNU `empty_unibyte_string` / `empty_multibyte_string`, modeled per heap.
    /// These handles are permanent runtime roots and mapped dump objects replace
    /// temporary pre-restore owned values.
    canonical_empty_strings: CanonicalEmptyStrings,
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
    mapped_veclike_index_by_addr: FxHashMap<usize, usize>,
    /// String objects loaded directly from a mapped pdump image.  Their text
    /// properties can contain Lisp roots, so mark state must be external too.
    mapped_string_objects: Vec<MappedStringObject>,
    mapped_string_index_by_addr: FxHashMap<usize, usize>,
    /// Number of live cons cells currently included in `allocated_count`.
    cons_live_count: usize,

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
    buffer_registry: FxHashMap<crate::buffer::BufferId, TaggedValue>,
    window_registry: FxHashMap<u64, TaggedValue>,
    frame_registry: FxHashMap<u64, TaggedValue>,
    timer_registry: FxHashMap<u64, TaggedValue>,
    process_registry: FxHashMap<crate::emacs_core::process::ProcessId, TaggedValue>,

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

    /// Owners mutated since the last full collection.
    ///
    /// This is the minimal remembered-set precursor for future generational
    /// or incremental GC. We keep owner identity, not child edges, because the
    /// current collector is still full-heap mark-sweep.
    write_tracking_mode: WriteTrackingMode,
    dirty_owners: Vec<TaggedValue>,
    /// FIRST-CYCLE-CONCURRENT: armed by the driver (`arm_first_cycle_concurrent`)
    /// before the first partition cycle's `concurrent_begin`; makes
    /// `begin_collection` stage the mapped cons ranges instead of enumerating
    /// them in the handshake and makes the claim job DROP span-inside children.
    /// Cleared when the cycle completes (`finish_first_partition_cycle`) or by
    /// an STW `complete_collection` finishing the bootstrap first.
    first_cycle_concurrent: bool,
    /// Mapped cons ranges staged by `begin_collection` for the concurrent
    /// first cycle; `launch_concurrent_mark` moves them into the job.
    staged_mapped_cons_scan: Option<Vec<(usize, usize)>>,
    /// Mapped veclike header addresses staged alongside (see the job field).
    staged_mapped_veclikes: Option<Vec<usize>>,
    dirty_owner_bits: FxHashSet<usize>,
    dirty_writes: Vec<HeapWriteRecord>,

    // --- Dump-partition state (treat the immutable pdump image as a permanent
    // black/tenured region: never clear, re-trace, or sweep it). Gated by
    // `partition_dump`; default off => identical to the full-trace collector.
    /// When true, mapped (pdump) objects are born black and never re-traced;
    /// only mutated dumped objects (`mapped_remembered`) are re-scanned.
    partition_dump: bool,
    /// One-time flag: the mapped image has been blackened (all marks set).
    dump_blackened: bool,
    /// Persistent remembered set: bits of dumped objects that have been
    /// mutated and may now hold heap children. Seeded as roots every cycle so
    /// those heap children stay live. Fed by the write barrier
    /// (`record_heap_write`). Tiny in practice (few dumped objects are ever
    /// mutated). Never cleared (conservative retention).
    mapped_remembered: FxHashSet<usize>,
    /// Address span `[lo, hi)` covering every mapped object, for an O(1) "is
    /// this owner a dumped object?" test in the write-barrier hot path.
    dump_addr_lo: usize,
    dump_addr_hi: usize,
    /// One-time flag: this heap has completed a full stop-the-world collection
    /// (its bootstrap cycle). A dump-less heap runs the concurrent collector
    /// from its second cycle on — the same one-STW-bootstrap-then-concurrent
    /// shape as the dump path; see `should_run_concurrent`.
    bootstrap_collected: bool,

    // --- Young non-cons PARITY MARK BITS (task #7 stage 2b). "Marked this
    // cycle" for a YOUNG non-cons `GcHeader` ≡ (raw bit == `mark_parity`).
    // `begin_collection` flips the parity instead of pointer-chasing
    // `all_objects` to clear bits (the walk measured ~98% of the clear phase).
    // Cons block bitmaps keep their memset clear (their `fetch_or` marking is
    // set-only and `count_marked` popcounts 1-bits, so parity is structurally
    // impossible there); mapped (pdump) side-table mark state is untouched;
    // tenured objects freeze their bit at promotion, and every reader that can
    // see a tenured object short-circuits on `tenured` BEFORE interpreting the
    // bit (mark_value owned arms, is_value_marked, unchain_dead_markers,
    // doomed-finalizer scan).
    /// Current cycle's mark parity. INIT `false` so the FIRST
    /// `begin_collection` flip yields `true` — opposite the zeroed/`false`
    /// bits of freshly created and pdump-loaded headers (`GcHeader::new`) —
    /// otherwise the bootstrap cycle would read everything as marked and
    /// trace nothing.
    mark_parity: bool,

    // --- Incremental marking state (step 7). Active on every partitioned cycle
    // (after the first-cycle promotion); the first cycle and no-dump heaps stay
    // stop-the-world. Marking is sliced across evaluator safe points using an
    // incremental-update (Steele) write barrier: dirty owners (written during
    // marking) are re-traced so no black->white edge survives, and the COMPLETE
    // root set is re-snapshotted at mark termination.
    /// True between the start of an incremental mark and its termination/sweep.
    /// While set, every safe point advances marking by one bounded slice.
    mark_in_progress: bool,
    /// Accumulated marking time (slices + final drain) for the in-flight
    /// incremental cycle, reported as `mark_us` at termination. Reset at start.
    incremental_mark_us: u64,
    /// True between a concurrent mark's start and termination handshakes — the
    /// mutator runs while the GC thread marks.
    concurrent_mark_running: bool,
    /// Mutator->GC channel (Phase 5): the SATB barrier appends the overwritten
    /// children here (locked); the GC thread drains them into its gray worklist.
    satb_shared: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// Per-cycle dedup for the COARSE (bulk) SATB barrier. A bulk mutator
    /// (`with_hash_table_mut`, `with_vector_data_mut`, char-table, …) hands a
    /// `&mut` to an arbitrary closure, so the barrier — which runs BEFORE the
    /// store and cannot know which slot the closure will touch — conservatively
    /// snapshots the owner's WHOLE pre-image. Doing that on every write is O(n)
    /// per write => O(n²) to build an n-element container (the `(ucs-names)` OOM).
    /// SATB only needs each owner's start-of-cycle child set logged ONCE: at the
    /// owner's FIRST mutation this cycle, all its snapshot-time children are still
    /// present (a child can only be unlinked by a mutation of this owner, which is
    /// itself this first write firing the barrier pre-store), so that single
    /// snapshot is a superset of every child reachable at snapshot time. Later
    /// writes can only overwrite values already logged (or born-black new ones),
    /// so re-snapshotting is pure waste. We record owners snapshotted this cycle
    /// here and skip the re-enumeration. Cleared at every mark start
    /// (`concurrent_begin`/`begin_collection`). Conses (2 children, O(1) barrier)
    /// bypass it; only multi-child veclike/string owners are deduped.
    ///
    /// SECOND ROLE (task 01, load-bearing): this set is exactly "every
    /// multi-child owner MUTATED this cycle", and `join_concurrent_mark`
    /// drains it to re-gray each such owner's CURRENT children at the STW
    /// termination — the INSERTION-COVERAGE re-trace that keeps mid-cycle
    /// insertions (root→heap motion) live now that concurrently-CLAIMED
    /// owners (page vectors; interval-free strings that gained a table) are
    /// no longer re-traced by the termination's `mark_value`.
    satb_snapshotted_owners: FxHashSet<usize>,
    /// Veclikes/strings the GC thread reached but did NOT trace (their backing
    /// can be reallocated by the mutator, so reading it concurrently would be a
    /// UAF). They are marked black and parked here, then traced at the
    /// termination handshake while the mutator is stopped.
    deferred_veclikes: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// GC thread sets this (Release) when gray + SATB are drained; the mutator
    /// polls it (Acquire) at safe points to decide when to terminate.
    gc_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// CONCURRENT STRING MARKING: shared claim counter for the in-flight cycle
    /// (see `ConcurrentClaimJob::str_claimed`). Reset at `launch_concurrent_mark`,
    /// folded into `last_concurrent_str_claimed` at `join_concurrent_mark`.
    concurrent_str_claimed: std::sync::Arc<AtomicUsize>,
    /// Strings the GC thread claimed concurrently in the last completed cycle
    /// (diagnostics; the concurrent counterpart of `last_termination_kinds.string`).
    last_concurrent_str_claimed: usize,
    /// CONCURRENT FLOAT CLAIMS (task 01): shared claim counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::float_claimed`) + its
    /// last-completed-cycle fold. Same reset/fold seams as the string pair.
    concurrent_float_claimed: std::sync::Arc<AtomicUsize>,
    last_concurrent_float_claimed: usize,
    /// SUBR RECOGNIZE-AND-DROP (task 01): shared drop counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::subr_dropped`) + its fold.
    concurrent_subr_dropped: std::sync::Arc<AtomicUsize>,
    last_concurrent_subr_dropped: usize,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): shared claim counter for
    /// the in-flight cycle (see `ConcurrentClaimJob::vec_claimed`) + fold.
    concurrent_vec_claimed: std::sync::Arc<AtomicUsize>,
    last_concurrent_vec_claimed: usize,
    /// CONCURRENT BYTECODE CLAIMS (task 01): shared claim counter for the
    /// in-flight cycle (see `ConcurrentClaimJob::bc_claimed`) + fold.
    concurrent_bc_claimed: std::sync::Arc<AtomicUsize>,
    last_concurrent_bc_claimed: usize,
    /// CONCURRENT STRING MARKING: per-cycle dedup for the ENFORCED in-mutator
    /// string interval SATB barrier (`note_string_interval_preimage`), keyed by
    /// `LispString` address — stable for the whole cycle because nothing is
    /// freed while a mark runs. Cleared at `begin_collection`, like
    /// `satb_snapshotted_owners`.
    satb_string_preimage_addrs: FxHashSet<usize>,
    /// Mutator sets this (Release) to ask the GC thread to finish and exit.
    gc_stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Task #7 stage 2a (Fix B): wakeup latch for the GC thread's idle nap.
    /// `join_concurrent_mark` notifies it AFTER setting `gc_stop`, so a stop
    /// request interrupts the nap immediately instead of burning the
    /// remainder of a fixed 100us sleep (the measured bulk of the
    /// stop-signal -> thread-exit latency in the termination handshake).
    gc_wake: std::sync::Arc<(std::sync::Mutex<()>, std::sync::Condvar)>,
    /// Receives when the GC thread has exited its mark loop (so the mutator's
    /// termination can safely take over the gray queue). Set at start.
    gc_exited: Option<std::sync::mpsc::Receiver<()>>,
    /// Stage 1b CONCURRENT OBARRAY SCAN: a start-captured obarray chunk snapshot
    /// staged by the start handshake (`start_concurrent_mark`) just before
    /// `launch_concurrent_mark`, which moves it into the `ConcurrentMarkJob`. The
    /// heap cannot reach the Context-side obarray itself, so the snapshot is built
    /// Context-side and parked here for the launch to consume. `None` except
    /// between a start handshake and the launch consuming it.
    pending_obarray_scan: Option<crate::emacs_core::symbol::ObarrayScanSnapshot>,
    /// Stage 1b: the obarray slot count captured at the start handshake, retained
    /// across the cycle (the snapshot itself is moved into the GC job). At the STW
    /// termination, the residual re-seed covers new symbols in slots `>= this`
    /// (interned mid-cycle, never scanned by the GC thread). `None` outside a
    /// concurrent mark.
    concurrent_obarray_start_slots: Option<usize>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: retired vector backings — the ORIGINAL
    /// `Vec` of each OWNED vector whose backing was clone-on-write replaced during
    /// this concurrent mark (`with_vector_data_mut`). The GC thread's snapshot still
    /// points at these immutable buffers, so they must stay alive until the GC thread
    /// joins. Drained + dropped in `join_concurrent_mark` (the GC thread has provably
    /// exited — the only safe free point). Empty unless a clone-on-write fired.
    retired_vector_buffers: Vec<Vec<TaggedValue>>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: per-cycle clone-on-write dedup set,
    /// keyed on each vector owner's `TaggedValue` bits. On an owner's FIRST bulk
    /// mutation this cycle we clone+retire its OWNED backing once; later mutations of
    /// the same owner skip the clone (they touch the already-cloned live backing the
    /// GC's snapshot does NOT point at). Cleared at every mark start
    /// (`concurrent_begin`/`begin_collection`). Empty unless a clone-on-write fired.
    concurrent_cloned_vectors: FxHashSet<usize>,

    // --- Incremental sweep state (step 8). After a mark terminates, the sweep
    // is deferred and drained in bounded slices at later safe points, so the
    // reclaim is no longer part of the stop-the-world pause. The next mark and
    // any forced GC finish the sweep first (marks must stay intact until then).
    /// True while the deferred sweep is draining.
    sweep_in_progress: bool,
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
    /// Non-cons objects detached from `all_objects` at sweep start, reclaimed
    /// incrementally. New non-cons allocations link onto a fresh `all_objects`
    /// and are not swept this cycle.
    sweep_noncons_pending: *mut GcHeader,
    /// Live bytes accumulated from the non-cons objects swept so far this cycle.
    sweep_noncons_live_bytes: usize,
    /// Carried from mark termination for the completion trace/accounting.
    sweep_mark_us: u64,
    sweep_bytes_before: usize,
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
    /// Handshake-pause decomposition (per phase, per root group, size probes).
    /// Heap-side phases are written where they run; the evaluator fills the
    /// context-root breakdowns + context-side probes via `handshake_stats_mut`.
    handshake: HandshakeStats,
    /// Scratch: last `seed_internal_runtime_roots` cost/volume. Written every
    /// call; routed to the start or termination slot by `concurrent_begin` /
    /// `reseed_runtime_and_remembered_roots` (which know which handshake ran).
    last_runtime_seed_us: u64,
    last_runtime_seed_roots: usize,
    /// Scratch: last `seed_mapped_remembered` cost/volume (owners re-scanned).
    last_remembered_seed_us: u64,
    last_remembered_seed_roots: usize,
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

impl TaggedHeap {
    pub fn new() -> Self {
        Self {
            identity: next_tagged_heap_identity(),
            cons_blocks: Vec::new(),
            cons_block_index_by_base: FxHashMap::default(),
            mark_cons_block_cache: None,
            all_objects: std::ptr::null_mut(),
            tenured_objects: std::ptr::null_mut(),
            non_cons_object_addrs: FxHashSet::default(),
            vector_object_addrs: FxHashSet::default(),
            allocated_count: 0,
            memory_use_counts: [0; MEMORY_USE_COUNT_LEN],
            gc_threshold: 1_000_000 * size_of::<usize>(),
            gc_threshold_overridden: false,
            bytes_since_gc: 0,
            total_allocated_bytes: 0,
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
            permanent_weak_hash_tables: Vec::new(),
            permanent_weak_hash_tables_set: rustc_hash::FxHashSet::default(),
            finalizer_registry: Vec::new(),
            doomed_finalizer_functions: Vec::new(),
            pending_surface_destroys: Vec::new(),
            pending_video_destroys: Vec::new(),
            cons_free_list: std::ptr::null_mut(),
            float_arena: ObjectArena::new(),
            string_arena: ObjectArena::new(),
            canonical_empty_strings: CanonicalEmptyStrings::default(),
            vector_arena: ObjectArena::new(),
            bytecode_arena: ObjectArena::new(),
            lambda_arena: ObjectArena::new(),
            macro_arena: ObjectArena::new(),
            record_arena: ObjectArena::new(),
            symbol_with_pos_arena: ObjectArena::new(),
            mapped_cons_ranges: Vec::new(),
            mapped_float_ranges: Vec::new(),
            mapped_veclike_objects: Vec::new(),
            mapped_veclike_index_by_addr: FxHashMap::default(),
            mapped_string_objects: Vec::new(),
            mapped_string_index_by_addr: FxHashMap::default(),
            cons_live_count: 0,
            marker_chain_head_slots: Vec::new(),
            buffer_registry: FxHashMap::default(),
            window_registry: FxHashMap::default(),
            frame_registry: FxHashMap::default(),
            timer_registry: FxHashMap::default(),
            process_registry: FxHashMap::default(),
            write_tracking_mode: WriteTrackingMode::Disabled,
            dirty_owners: Vec::new(),
            first_cycle_concurrent: false,
            staged_mapped_cons_scan: None,
            staged_mapped_veclikes: None,
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
            // Parity invariant: must start `false` (see the field doc) so the
            // first flip reads pre-existing `false` bits as unmarked.
            mark_parity: false,
            mark_in_progress: false,
            incremental_mark_us: 0,
            concurrent_mark_running: false,
            satb_shared: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            satb_snapshotted_owners: FxHashSet::default(),
            deferred_veclikes: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
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
            satb_string_preimage_addrs: FxHashSet::default(),
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
            sweep_noncons_pending: std::ptr::null_mut(),
            sweep_noncons_live_bytes: 0,
            sweep_mark_us: 0,
            sweep_bytes_before: 0,
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
        }
    }

    pub(crate) fn identity(&self) -> usize {
        self.identity
    }

    pub fn set_write_tracking_mode(&mut self, mode: WriteTrackingMode) {
        self.write_tracking_mode = mode;
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|current| current.set(mode));
        if mode == WriteTrackingMode::Disabled {
            self.clear_dirty_owners();
            self.clear_dirty_writes();
        }
    }

    pub fn write_tracking_mode(&self) -> WriteTrackingMode {
        self.write_tracking_mode
    }

    pub fn should_collect(&self) -> bool {
        self.bytes_since_gc >= self.gc_threshold
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
                self.bytes_since_gc,
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

    pub fn allocated_count(&self) -> usize {
        self.allocated_count
    }

    /// Total number of completed GC collection cycles since this heap was
    /// created. Used by allocation benchmarks to measure GC frequency.
    pub fn gc_collections(&self) -> usize {
        self.gc_collections
    }

    /// Deferred-sweep cost + termination-drain instrumentation snapshot.
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
        self.memory_use_counts[index] = self.memory_use_counts[index].wrapping_add(delta);
    }

    #[inline]
    pub(crate) fn memory_use_counts_snapshot(&self) -> [u64; MEMORY_USE_COUNT_LEN] {
        self.memory_use_counts
    }

    pub fn bytes_since_gc(&self) -> usize {
        self.bytes_since_gc
    }

    pub(crate) fn reset_bytes_since_gc(&mut self) {
        self.bytes_since_gc = 0;
    }

    pub fn live_bytes(&self) -> usize {
        self.live_bytes
    }

    pub fn buffer_value(&self, id: crate::buffer::BufferId) -> Option<TaggedValue> {
        self.buffer_registry.get(&id).copied()
    }

    pub fn register_buffer_value(&mut self, id: crate::buffer::BufferId, value: TaggedValue) {
        self.buffer_registry.insert(id, value);
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
        self.allocated_count = self.allocated_count.saturating_add(len);
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
        self.allocated_count = self.allocated_count.saturating_add(len);
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
        self.allocated_count = self.allocated_count.saturating_add(1);
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
        self.allocated_count = self.allocated_count.saturating_add(1);
        self.live_bytes = self.live_bytes.saturating_add(byte_len);

        let string = unsafe { &(*ptr).data };
        if string.sbytes() == 0 {
            let value = unsafe { TaggedValue::from_string_ptr(ptr) };
            self.canonical_empty_strings
                .install_mapped(string.storage_kind(), value);
        }
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
        // Dump partition: a mutated dumped object may now hold heap children,
        // so remember it as a permanent root. Conservative — a false positive
        // (a heap owner inside the dump address span) just adds a redundant
        // root; a false negative would be a use-after-free, so the span test
        // must cover every mapped object (see `register_mapped_*`).
        if self.partition_dump
            && (self.owner_is_mapped(record.owner) || self.value_is_tenured(record.owner))
        {
            self.mapped_remembered.insert(record.owner.bits());
            // Arm the barrier's repeat-owner reject: this entry is permanent,
            // so the partition-only path can skip the same owner's next write.
            TAGGED_HEAP_LAST_REMEMBERED.with(|l| l.set(record.owner.bits()));
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
            self.push_value_children_to_satb_shared(record.owner);
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

    /// Raw object address for a heap-tagged value (cons/veclike/string/float),
    /// used for the dump-partition address-span test.
    fn value_heap_addr(value: TaggedValue) -> Option<usize> {
        if value.is_cons() {
            Some(value.xcons_ptr() as usize)
        } else if value.is_veclike() {
            value.as_veclike_ptr().map(|ptr| ptr as usize)
        } else if value.is_string() {
            value.as_string_ptr().map(|ptr| ptr as usize)
        } else if value.is_float() {
            value.as_float_ptr().map(|ptr| ptr as usize)
        } else {
            None
        }
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
    }

    /// True when a registered mapped span (a loaded pdump) has activated the
    /// dump-partitioned collector. Diagnostics: lets the drain-kind profiling
    /// probe verify which collector configuration it is measuring.
    #[cfg(test)]
    pub(crate) fn dump_partition_active(&self) -> bool {
        self.partition_dump
    }

    fn note_allocation_bytes(&mut self, bytes: usize) {
        self.bytes_since_gc = self.bytes_since_gc.saturating_add(bytes);
        self.total_allocated_bytes = self.total_allocated_bytes.saturating_add(bytes as u64);
        self.live_bytes = self.live_bytes.saturating_add(bytes);
    }

    pub(crate) fn total_allocated_bytes(&self) -> u64 {
        self.total_allocated_bytes
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
            .saturating_add(
                data.params
                    .required
                    .capacity()
                    .saturating_mul(size_of::<SymId>()),
            )
            .saturating_add(
                data.params
                    .optional
                    .capacity()
                    .saturating_mul(size_of::<SymId>()),
            )
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
                        VecLikeType::Subr => size_of::<SubrObj>(),
                        VecLikeType::Bignum => size_of::<BignumObj>(),
                        VecLikeType::SymbolWithPos => size_of::<SymbolWithPosObj>(),
                        VecLikeType::Finalizer => size_of::<FinalizerObj>(),
                        VecLikeType::Sqlite => size_of::<SqliteObj>(),
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
        stats = stats.add(Self::lambda_params_payload_layout(&data.params));
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
                mapped: bytes.owned_bytes() == 0 && bytes.len() > 0,
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

    fn note_boxed_list_layout(mut header: *const GcHeader, stats: &mut Vec<BoxedKindLayoutStats>) {
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
                .saturating_add(Self::object_bytes_from_header(header));
            unsafe {
                item.tenured_objects += usize::from((*header).tenured);
                header = (*header).next;
            }
        }
    }

    /// Snapshot allocator-backed GC page occupancy and the directly-owned
    /// payload capacities of live objects. This does not attempt to reproduce
    /// process RSS: symbol registries, evaluator stacks, display caches,
    /// allocator metadata, and nested hash-key allocations live outside this
    /// accounting and are intentionally exposed as the RSS remainder.
    pub(crate) fn layout_stats(&self) -> HeapLayoutStats {
        let mut free_cells_by_block = vec![0usize; self.cons_blocks.len()];
        let bumped_cons_slots: usize = self
            .cons_blocks
            .iter()
            .map(|block| block.next_index as usize)
            .sum();
        let mut free = self.cons_free_list;
        let mut free_count = 0usize;
        while !free.is_null() && free_count < bumped_cons_slots {
            let base = ConsBlock::block_base_for_ptr(free);
            if let Some(&block_index) = self.cons_block_index_by_base.get(&base) {
                free_cells_by_block[block_index] += 1;
            }
            free_count += 1;
            free = unsafe { (*free).free_next() };
        }
        debug_assert!(free.is_null(), "cons free list exceeds bumped cell count");

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
        Self::note_boxed_list_layout(self.all_objects, &mut boxed);
        Self::note_boxed_list_layout(self.tenured_objects, &mut boxed);
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
            allocated_objects: self.allocated_count,
            managed_live_bytes: self.live_bytes,
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

    /// Walk each installed buffer-chain head slot and splice out markers
    /// whose GC mark bit is clear. Runs between `mark_all` and
    /// `sweep_objects` so reading `header.gc.marked` is sound (the
    /// allocation is still live). Mirrors GNU Emacs `sweep_buffer →
    /// unchain_dead_markers` (alloc.c).
    fn unchain_dead_markers(&mut self) {
        // Take the slot list out so we don't alias self while iterating.
        let slots = std::mem::take(&mut self.marker_chain_head_slots);
        let parity = self.mark_parity;
        for slot in slots {
            unsafe {
                let mut prev_slot: *mut *mut MarkerObj = slot;
                while !(*prev_slot).is_null() {
                    let curr = *prev_slot;
                    // Buffer marker chains can hold TENURED markers (promoted
                    // at the first partition cycle): their bit froze at
                    // promotion and must not be interpreted against the
                    // current parity — tenured ≡ permanently live.
                    if (*curr).header.gc.tenured || (*curr).header.gc.is_marked_at(parity) {
                        // Live — advance prev
                        prev_slot = &mut (*curr).data.next_marker;
                    } else {
                        // Dead — splice out. The generic `sweep_objects`
                        // pass frees the allocation.
                        *prev_slot = (*curr).data.next_marker;
                        (*curr).data.next_marker = std::ptr::null_mut();
                    }
                }
            }
        }
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
            // BORN-AT-PARITY, unconditionally (see `link_object`): during a
            // concurrent mark this is allocate-black; otherwise it pre-arms
            // the bit so the next begin_collection flip reads it as white.
            (*header).gc.set_marked(self.mark_parity);
            let gc_header = &mut (*header).gc as *mut GcHeader;
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
            #[cfg(test)]
            alloc_probe::record(gc_header, self.non_cons_object_addrs.len());
        }
    }
}

impl Drop for TaggedHeap {
    fn drop(&mut self) {
        // A live concurrent mark holds start-of-cycle snapshots into this
        // heap (cons blocks + their mark bitmaps, vector backings, the
        // Context obarray) on the GC thread. Reclaim exclusive ownership
        // BEFORE freeing anything it can still read. `tagged_heap` is the
        // first `Context` field, so this join also runs before the obarray
        // drops. No-op when no mark is in flight.
        if self.concurrent_mark_running {
            self.join_concurrent_mark();
        }
        // Free all non-cons objects via every intrusive list: young, tenured,
        // and any objects detached for an in-flight deferred sweep.
        for mut current in [
            self.all_objects,
            self.tenured_objects,
            self.sweep_noncons_pending,
        ] {
            while !current.is_null() {
                unsafe {
                    let next = (*current).next;
                    self.free_gc_object(current);
                    current = next;
                }
            }
        }
        // ConsBlocks are dropped automatically (they implement Drop).
        // Object arena pages likewise: page floats/strings/vectors/bytecode/
        // lambdas/macros/records/symbols-with-pos are on NONE of the lists
        // above (so the walk cannot hand a page pointer to `free_gc_object`'s
        // `Box::from_raw`), and the arena fields drop after this body, freeing
        // every page via `ObjectPage::drop`, which walks the allocated slots
        // and `drop_in_place`s each live object (strings free their byte
        // storage + interval tables, vectors their element `Vec`, bytecode its
        // ops/constants vectors + params + GNU byte maps + docstring,
        // lambdas/macros/records their slot `Vec` + cached params; floats and
        // symbols-with-pos are POD and the walk compiles out) before releasing
        // the page storage —
        // retired pages included. The concurrent-mark join at the top of this
        // body has already reclaimed exclusive ownership, so the GC thread
        // cannot still be reading a page.
    }
}

/// TEST-ONLY allocation-profiling counters for the non-cons allocator
/// modernization probes (size-class arena design inputs): per-kind allocation
/// counts, a size-class histogram over TOTAL object bytes (fixed struct +
/// separately-allocated payload storage, via `object_bytes_from_header`),
/// per-kind byte totals, and the peak `non_cons_object_addrs` population.
/// Compiled ONLY under `cfg(test)` (the consuming probes are in-crate
/// `#[ignore]`d tests), so production builds carry zero instrumentation.
/// Global statics are correct here because nextest runs each probe in its own
/// process, so the counters observe exactly one workload.
#[cfg(test)]
pub(crate) mod alloc_probe;

#[cfg(test)]
mod layout_stats_tests;

#[cfg(test)]
mod pacer_tests;

#[cfg(test)]
mod ownership_tests;

/// FLOAT ARENA PAGES test suite. Every scenario runs twice: plain and with
/// `NEOVM_GC_VERIFY_PARTITION=1` (which also arms the partition via a fake
/// dump span + a bootstrap cycle where the flow allows, so the dump-partition
/// and tricolor verifiers actually engage at each termination). The suite
/// relies on nextest's process-per-test model for the env var and the global
/// `LIVE_FLOAT_PAGES` counter.
#[cfg(test)]
mod float_arena_tests;

/// ARENA PROMOTION + RETIREMENT test suite (stage 3, commit 4): the
/// promotion page walk, full-page retirement, mixed-page tenured survival
/// across parities, page-span-oracle exactness, payload-bearing teardown,
/// variable-size live-bytes accounting, and the tenured-page-owner
/// remembered-set scan. Scenarios run plain and (where the partition
/// verifiers add coverage) with `NEOVM_GC_VERIFY_PARTITION=1`.
#[cfg(test)]
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

mod mark_sweep;

mod concurrent;

mod incremental;

mod cons_blocks;
pub use cons_blocks::*;

mod arena_pages;
pub use arena_pages::*;
#[cfg(test)]
mod symbol_with_pos_arena_tests;

/// Test-only growth helper mirroring the production insert resize policy closely
/// enough to force rehashes during the concurrent-mark stress test.
#[cfg(test)]
fn maybe_resize_for_test(ht: &mut crate::emacs_core::value::LispHashTable) {
    let len = ht.data.len() as i64;
    if len >= ht.size {
        ht.size = if ht.size == 0 { 6 } else { ht.size * 2 };
        ht.data.reserve(ht.size as usize);
    }
}
