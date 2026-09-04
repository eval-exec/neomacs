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
// Cons block allocator
// ---------------------------------------------------------------------------

/// GNU Emacs keeps conses in fixed-size aligned blocks and derives the owning
/// block/index directly from the cons pointer. Keep the same shape here so
/// mark/ownership checks stay O(1) instead of linearly scanning `cons_blocks`.
const CONS_BLOCK_BYTES: usize = 64 * 1024;
const CONS_BLOCK_ALIGN: usize = CONS_BLOCK_BYTES;
const CONS_MARK_BITS_PER_WORD: usize = usize::BITS as usize;

const fn cons_mark_words(cell_count: usize) -> usize {
    cell_count.div_ceil(CONS_MARK_BITS_PER_WORD)
}

const fn cons_block_cell_count() -> usize {
    let cons_size = size_of::<ConsCell>();
    let mark_word_size = size_of::<usize>();
    let mut cells = CONS_BLOCK_BYTES / cons_size;
    while cells > 0 {
        let marks_bytes = cons_mark_words(cells) * mark_word_size;
        if cells * cons_size + marks_bytes <= CONS_BLOCK_BYTES {
            return cells;
        }
        cells -= 1;
    }
    0
}

const CONS_BLOCK_SIZE: usize = cons_block_cell_count();
const CONS_MARK_WORDS: usize = cons_mark_words(CONS_BLOCK_SIZE);
const CONS_CELLS_BYTES: usize = CONS_BLOCK_SIZE * size_of::<ConsCell>();
const CONS_MARKS_OFFSET: usize = CONS_CELLS_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConsMarkBit {
    word_index: usize,
    mask: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConsBlockCacheEntry {
    block_base: usize,
    block_index: usize,
}

impl ConsBlockCacheEntry {
    fn new(block_base: usize, block_index: usize) -> Self {
        Self {
            block_base,
            block_index,
        }
    }
}

/// A GNU-shaped cons block with cells at the front of a fixed-size aligned
/// storage area, followed by packed mark bits.
struct ConsBlock {
    /// Aligned raw storage for cons cells plus mark bits.
    storage: *mut u8,
    /// Index of the first never-allocated cell in this block.
    next_index: u16,
}

impl ConsBlock {
    fn layout() -> Layout {
        Layout::from_size_align(CONS_BLOCK_BYTES, CONS_BLOCK_ALIGN).expect("cons block layout")
    }

    fn new() -> Self {
        let layout = Self::layout();
        let storage = unsafe { alloc::alloc_zeroed(layout) };
        if storage.is_null() {
            alloc::handle_alloc_error(layout);
        }
        Self {
            storage,
            next_index: 0,
        }
    }

    #[inline]
    fn base_addr(&self) -> usize {
        self.storage as usize
    }

    #[inline]
    fn cells_ptr(&self) -> *mut ConsCell {
        self.storage.cast()
    }

    #[inline]
    fn mark_words_ptr(&self) -> *mut usize {
        unsafe { self.storage.add(CONS_MARKS_OFFSET).cast() }
    }

    #[inline]
    fn block_base_for_ptr(ptr: *const ConsCell) -> usize {
        (ptr as usize) & !(CONS_BLOCK_ALIGN - 1)
    }

    #[inline]
    fn ptr_offset(ptr: *const ConsCell) -> usize {
        (ptr as usize).saturating_sub(Self::block_base_for_ptr(ptr))
    }

    #[inline]
    fn ptr_is_cell_aligned(ptr: *const ConsCell) -> bool {
        let offset = Self::ptr_offset(ptr);
        offset < CONS_CELLS_BYTES && offset.is_multiple_of(size_of::<ConsCell>())
    }

    #[inline]
    fn index_of_ptr(ptr: *const ConsCell) -> usize {
        Self::ptr_offset(ptr) / size_of::<ConsCell>()
    }

    #[inline]
    fn mark_bit(index: usize) -> ConsMarkBit {
        let word = index / CONS_MARK_BITS_PER_WORD;
        let bit = index % CONS_MARK_BITS_PER_WORD;
        ConsMarkBit {
            word_index: word,
            mask: 1usize << bit,
        }
    }

    /// View a mark-bitmap word as an atomic. The cons mark bits are accessed
    /// atomically (relaxed) so a future concurrent GC thread can set them while
    /// the mutator allocate-blacks / reads them without a data race; on x86 a
    /// relaxed atomic load/store is a plain mov, so this is free single-threaded.
    #[inline]
    fn mark_word(&self, word_index: usize) -> &AtomicUsize {
        unsafe { &*(self.mark_words_ptr().add(word_index) as *const AtomicUsize) }
    }

    #[inline]
    fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        let index = Self::index_of_ptr(ptr);
        let mark = Self::mark_bit(index);
        debug_assert!(mark.word_index < CONS_MARK_WORDS);
        (self.mark_word(mark.word_index).load(Ordering::Relaxed) & mark.mask) != 0
    }

    #[inline]
    fn mark_ptr(&mut self, ptr: *const ConsCell) {
        let index = Self::index_of_ptr(ptr);
        let mark = Self::mark_bit(index);
        debug_assert!(mark.word_index < CONS_MARK_WORDS);
        self.mark_word(mark.word_index)
            .fetch_or(mark.mask, Ordering::Relaxed);
    }

    /// Allocate a fresh cons cell from this block's bump cursor.
    /// Returns None if the block has no never-used cells left.
    fn alloc_bump(&mut self, car: TaggedValue, cdr: TaggedValue) -> Option<*mut ConsCell> {
        if self.next_index as usize >= CONS_BLOCK_SIZE {
            return None;
        }
        let idx = self.next_index;
        self.next_index += 1;
        let cell = unsafe { self.cells_ptr().add(idx as usize) };
        unsafe {
            (*cell).set_car(car);
            (*cell).set_cdr(cdr);
        }
        Some(cell)
    }

    /// Clear all mark bits used by this block. Runs stop-the-world (at
    /// `begin_collection`), but stores atomically so the representation stays
    /// consistent with the concurrent reads/writes elsewhere.
    fn clear_marks(&mut self) {
        let used_words = cons_mark_words(self.next_index as usize);
        for w in 0..used_words {
            self.mark_word(w).store(0, Ordering::Relaxed);
        }
    }

    /// Count currently-marked (live) cells via mark-bitmap popcount. Bits at or
    /// above `next_index` are never set, so popcounting the used words is exact.
    /// Cheap O(cells/64); used to recompute the live count after an incremental
    /// sweep without a second cell walk.
    fn count_marked(&self) -> usize {
        let used_words = cons_mark_words(self.next_index as usize);
        let mut live = 0usize;
        for w in 0..used_words {
            live += self.mark_word(w).load(Ordering::Relaxed).count_ones() as usize;
        }
        live
    }

    /// Sweep: thread reclaimed cells into the global intrusive free list and
    /// return the number of live cells in this block.
    fn sweep(&mut self, free_list: &mut *mut ConsCell) -> usize {
        let mut live = 0;

        // Match GNU alloc.c: reclaimed conses are linked through the dead
        // cells themselves instead of rebuilding an external index vector.
        for i in (0..self.next_index as usize).rev() {
            let cell = unsafe { self.cells_ptr().add(i) };
            let mark = Self::mark_bit(i);
            let marked = (self.mark_word(mark.word_index).load(Ordering::Relaxed) & mark.mask) != 0;
            if marked {
                live += 1;
            } else {
                unsafe {
                    (*cell).set_free_next(*free_list);
                }
                *free_list = cell;
            }
        }

        live
    }
}

impl Drop for ConsBlock {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.storage, Self::layout()) };
    }
}

// ---------------------------------------------------------------------------
// Size-class object arena pages (non-cons allocator modernization, stage 3)
// ---------------------------------------------------------------------------
//
// Floats (v1), strings and vectors (stage 3), and bytecode (task 03/3a) live
// in size-class arena PAGES:
// fixed 64KB-aligned pages of per-class fixed-stride slots replace the
// per-object `Box` + intrusive-list storage. Page objects keep their
// `GcHeader` (parity/tenured semantics untouched) but are NEVER in
// `non_cons_object_addrs` and NEVER linked onto `all_objects`/
// `tenured_objects`: the intrusive lists sweep with `free_gc_object`, whose
// `Box::from_raw` would corrupt the heap on a page pointer. The OWNERSHIP
// ORACLE for a page object is the page-span test (`ObjectArena::owns`):
// page-base registry hit + stride alignment + ALLOC-BIT-SET. The page sweep
// (`ObjectArena::sweep_range`) is their only reclaimer, wired into both sweep
// entry points (eager `finalize_collection` and the cooperative
// `incremental_sweep_slice`).
//
// GENERATIONAL NOTE — page objects tenure via the promotion PAGE WALK
// (`promote_and_blacken` flips `header.tenured` on every allocated slot at
// the one-time first partition cycle; the intrusive-list splice never sees
// them). The per-object `header.tenured` remains the SOLE mark-path
// authority; no page-level flag is consulted by any mark path. Pages that
// are FULL of tenured slots at promotion are RETIRED: never swept, never
// allocated into, freed only at heap teardown — but they STAY in the
// ownership oracle, because `value_is_tenured` (and through it the
// remembered-set write barrier) gates on ownership: a retired-page tenured
// object that answered "not owned" would miss its first post-retirement
// tenured→young edge and its child would be swept while live (UAF). Pages
// left with a mix of tenured + free/young slots stay in rotation as MIXED
// pages: every later sweep re-skips their tenured slots (a bounded cost —
// the one-time loadup survivor set is the only tenured population).

const OBJECT_PAGE_BYTES: usize = 64 * 1024;
/// Pages are ALIGNED to their size so any slot pointer derives its page base
/// with `addr & !(OBJECT_PAGE_BYTES - 1)` (the cons-block trick). The explicit
/// `Layout` alignment in `ObjectPage::layout` is what makes that mask valid.
const OBJECT_PAGE_ALIGN: usize = OBJECT_PAGE_BYTES;
/// Bitmap capacity for the smallest stride (32B floats → 2048 slots → 32
/// words). Classes with larger strides use a prefix of the array; the unused
/// tail words stay zero forever.
const OBJECT_PAGE_MAX_ALLOC_WORDS: usize = (OBJECT_PAGE_BYTES / 32).div_ceil(usize::BITS as usize);
/// Sentinel: "no slot" (free-list terminator) / "no page" (partial-chain
/// terminator and empty-chain head).
const PAGE_NONE: usize = usize::MAX;

/// Per-class parameters for the size-class arena pages. Implemented by the
/// paged object types (`FloatObj`, `StringObj`, `VectorObj`).
///
/// CONTRACT for implementors: `Self` is `#[repr(C)]` with a `GcHeader` at
/// offset 0, and fits its slot with room for the trailing free-list link
/// word (`size_of::<Self>() + 8 <= SLOT_BYTES`) — const-checked in
/// `ObjectPage::<Self>::LAYOUT_OK`, evaluated at every page creation.
trait PagedObject: Sized {
    /// Slot stride in bytes; a page holds `OBJECT_PAGE_BYTES / SLOT_BYTES`
    /// slots. The trailing 8 bytes of a slot hold the page-local free-list
    /// link while the slot is FREE, so the link never aliases object bytes —
    /// an adversarially scribbled dead header cannot corrupt the free list,
    /// and pushing a slot to the free list does not touch its (stale) header.
    const SLOT_BYTES: usize;
    /// The `GcHeader.kind` every allocated slot of this class must carry
    /// (debug-asserted by the sweep and verifiers).
    const KIND: HeapObjectKind;
    /// Class name for diagnostics.
    const CLASS: &'static str;
    /// TEST-ONLY live page counter (teardown-leak / double-free probe for the
    /// Drop tests): `ObjectPage::new` increments, `ObjectPage::drop` decrements.
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize;
}

// Slot strides. Float: `FloatObj` is exactly 24 bytes (GcHeader 16 + f64 8) →
// 32B slots (24..32 = free link). String: `StringObj` is 56 bytes (GcHeader
// 16 + LispString 40) → 64B slots with the link in bytes 56..64 — ZERO slack,
// const-proven below. Vector: `VectorObj` is 48 bytes (VecLikeHeader 24 +
// LispValueVec 24 — the 24 relies on the Owned(Vec)/Mapped niche packing; the
// const assert below is the compile-time proof) → shares the 64B class, link
// in bytes 56..64.
const _: () = assert!(size_of::<FloatObj>() == 24, "FloatObj must stay 24 bytes");
const _: () = assert!(
    size_of::<StringObj>() <= 56,
    "StringObj must fit a 64-byte slot with its trailing free-list link \
     (bytes 56..64 — zero slack)",
);
const _: () = assert!(
    size_of::<VectorObj>() <= 48,
    "VectorObj must stay <= 48 bytes (VecLikeHeader 24 + niche-packed \
     LispValueVec 24); if this fails the niche packing broke — give Vector \
     its own larger stride instead of silently overlapping the link word",
);
// ByteCode (task 03/3a): `ByteCodeObj` is VecLikeHeader 24 + ByteCodeFunction
// (~336B of vecs/options/params + the jit Runtime) → 384B slots with the
// free-list link in bytes 376..384. 384 is NOT a power of two: a page holds
// floor(64KB / 384) = 170 slots and the trailing 256 bytes are a permanently
// unused tail (never bump-reached, no alloc bit — `ObjectArena::owns` bounds
// the slot index explicitly so a stride-aligned tail address answers
// NOT-owned). If this assert fails the ByteCodeFunction grew — BUMP THE
// STRIDE (and say so in the commit); never squeeze the link into live bytes.
const _: () = assert!(
    size_of::<ByteCodeObj>() <= 376,
    "ByteCodeObj must fit a 384-byte slot with its trailing free-list link \
     (bytes 376..384); bump the bytecode stride if the struct grew",
);
// Lambda/Macro (task 03/3b): each is VecLikeHeader 24 + LispValueVec 24 +
// OnceLock<LambdaParams> (~64) = 112B → a 128B power-of-two class (512
// slots/page, link in bytes 120..128). LambdaObj and MacroObj are
// byte-identical in layout (same fields) but DISTINCT Rust types, so each
// gets its OWN class arena at the shared 128B stride — exactly as
// string/vector share the 64B stride in separate arenas (per-class
// registries never merge; a page hit is never a cross-class collision). If
// either assert fails the struct grew — BUMP THE STRIDE; never squeeze the
// link into live bytes.
const _: () = assert!(
    size_of::<LambdaObj>() <= 120,
    "LambdaObj must fit a 128-byte slot with its trailing free-list link \
     (bytes 120..128); bump the lambda/macro stride if the struct grew",
);
const _: () = assert!(
    size_of::<MacroObj>() <= 120,
    "MacroObj must fit a 128-byte slot with its trailing free-list link \
     (bytes 120..128); bump the lambda/macro stride if the struct grew",
);
// Record (task 03/3b): `RecordObj` is VecLikeHeader 24 + LispValueVec 24 =
// 48B → the 64B class (1024 slots/page, link in bytes 56..64), shared with
// string/vector in its OWN arena. `RecordObj` backs BOTH the `Record` and
// `WindowConfiguration` type tags (`alloc_record` / `alloc_window_configuration`
// — same Rust type, distinct tag), so both funnel to `record_arena`. If this
// assert fails the struct grew — bump the stride; never squeeze the link.
const _: () = assert!(
    size_of::<RecordObj>() <= 56,
    "RecordObj must fit a 64-byte slot with its trailing free-list link \
     (bytes 56..64); bump the record stride if the struct grew",
);
// SymbolWithPos (task 03/3b): `SymbolWithPosObj` is VecLikeHeader 24 + two
// fixed TaggedValue fields (sym, pos) = 40B → the 64B class (1024 slots/page,
// link in bytes 56..64), OWN arena. Both fields are `Copy` immediates
// (TaggedValue), so the struct is POD-like (`needs_drop` == false, like
// FloatObj): the generic sweep/teardown `drop_in_place` walk compiles out —
// no payload to free. 64B (vs a tighter 48B) keeps the class power-of-two /
// page-dividing (no page tail) and leaves comfortable headroom for a
// low-volume type; if this assert fails the struct grew — bump the stride.
const _: () = assert!(
    size_of::<SymbolWithPosObj>() <= 56,
    "SymbolWithPosObj must fit a 64-byte slot with its trailing free-list \
     link (bytes 56..64); bump the symbol-with-pos stride if the struct grew",
);

#[cfg(test)]
pub(crate) static LIVE_FLOAT_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_STRING_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_VECTOR_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_BYTECODE_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_LAMBDA_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_MACRO_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_RECORD_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_SYMBOL_WITH_POS_PAGES: AtomicUsize = AtomicUsize::new(0);

impl PagedObject for FloatObj {
    const SLOT_BYTES: usize = 32;
    const KIND: HeapObjectKind = HeapObjectKind::Float;
    const CLASS: &'static str = "float";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_FLOAT_PAGES
    }
}

impl PagedObject for StringObj {
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::String;
    const CLASS: &'static str = "string";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_STRING_PAGES
    }
}

impl PagedObject for VectorObj {
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "vector";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_VECTOR_PAGES
    }
}

impl PagedObject for ByteCodeObj {
    // First non-power-of-two stride: 170 slots/page + a 256B unused tail
    // (see the ByteCodeObj const assert above).
    const SLOT_BYTES: usize = 384;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "bytecode";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_BYTECODE_PAGES
    }
}

impl PagedObject for LambdaObj {
    // Shared 128B lambda/macro class: 512 slots/page, link in bytes 120..128.
    const SLOT_BYTES: usize = 128;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "lambda";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_LAMBDA_PAGES
    }
}

impl PagedObject for MacroObj {
    // Shares the 128B lambda class stride in its OWN arena (see LambdaObj).
    const SLOT_BYTES: usize = 128;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "macro";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_MACRO_PAGES
    }
}

impl PagedObject for RecordObj {
    // 64B class (shared stride, own arena): 1024 slots/page, link 56..64.
    // Backs both the Record and WindowConfiguration type tags.
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "record";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_RECORD_PAGES
    }
}

impl PagedObject for SymbolWithPosObj {
    // 64B class, own arena: POD-like (no payload, `needs_drop` == false).
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "symbol-with-pos";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_SYMBOL_WITH_POS_PAGES
    }
}

/// Slot count of a float page (test scenarios size their populations off it).
#[cfg(test)]
const FLOAT_PAGE_SLOTS: usize = OBJECT_PAGE_BYTES / <FloatObj as PagedObject>::SLOT_BYTES;
/// Slot count of a bytecode page (170: the 384B stride does not divide 64KB;
/// the 256B page tail is never allocated).
#[cfg(test)]
const BYTECODE_PAGE_SLOTS: usize = OBJECT_PAGE_BYTES / <ByteCodeObj as PagedObject>::SLOT_BYTES;
/// Slot count of a lambda/macro page (512: the 128B stride divides 64KB
/// exactly, no tail). LambdaObj and MacroObj share the stride so this const
/// applies to both arenas.
#[cfg(test)]
const LAMBDA_PAGE_SLOTS: usize = OBJECT_PAGE_BYTES / <LambdaObj as PagedObject>::SLOT_BYTES;
/// Slot count of a record page (1024: 64B stride divides 64KB exactly).
#[cfg(test)]
const RECORD_PAGE_SLOTS: usize = OBJECT_PAGE_BYTES / <RecordObj as PagedObject>::SLOT_BYTES;
/// Slot count of a symbol-with-pos page (1024: 64B stride).
#[cfg(test)]
const SYMBOL_WITH_POS_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <SymbolWithPosObj as PagedObject>::SLOT_BYTES;

/// One 64KB-aligned arena page of fixed-stride `T` slots.
///
/// The ALLOCATION BITMAP (`alloc_bits`) is the sole authority on which slots
/// hold live objects: a clear bit means the slot bytes are GARBAGE (a
/// never-bumped slot is uninitialized; a freed slot's header is stale and may
/// have been scribbled by reuse machinery), so every reader — sweep, verifier,
/// teardown, and the page-span ownership oracle — must test the bit BEFORE
/// any header access (ALLOCATED-BIT-FIRST; the INVERSE of the intrusive-list
/// sweep, whose list membership itself implies a valid header). The alloc-bit
/// test in `ObjectArena::owns` is also what makes the page-span oracle exact:
/// a freed-but-unswept... rather, a freed slot answers NOT-owned the instant
/// its bit clears, replacing float-v1's explicit addr-set evict-before-free.
struct ObjectPage<T: PagedObject> {
    /// 64KB of raw slot storage, aligned to 64KB (`OBJECT_PAGE_ALIGN`).
    storage: *mut u8,
    /// Bump cursor: index of the first never-allocated slot.
    next_index: usize,
    /// Per-slot allocation bitmap (bit set ⇔ slot holds a live `T`). Sized
    /// for the smallest stride; this class uses the first `ALLOC_WORDS`.
    alloc_bits: [usize; OBJECT_PAGE_MAX_ALLOC_WORDS],
    /// Occupancy: number of set bits in `alloc_bits`.
    allocated: usize,
    /// Page-local free list: head slot index, linked through each free slot's
    /// trailing link word (`FREE_LINK_OFFSET`). `PAGE_NONE` = empty.
    free_head: usize,
    /// Class free list ("pages with free slots") chain: index of the next
    /// such page in the arena, `PAGE_NONE` at the tail.
    next_partial: usize,
    /// Whether this page is currently linked on the arena's partial chain.
    on_partial: bool,
    /// RETIRED (promotion, stage-3 commit 4): the page was full of tenured
    /// slots at the one-time promotion. Never swept, never allocated into
    /// (it has no free slots and never gains any), freed at heap teardown —
    /// but it STAYS in the page-base registry so the ownership oracle keeps
    /// answering "owned" for its slots (see the C1 note in the module doc).
    retired: bool,
    _class: std::marker::PhantomData<*mut T>,
}

impl<T: PagedObject> ObjectPage<T> {
    /// Slots per page for this class.
    const SLOTS: usize = OBJECT_PAGE_BYTES / T::SLOT_BYTES;
    /// Bitmap words this class actually uses (prefix of `alloc_bits`).
    const ALLOC_WORDS: usize = Self::SLOTS.div_ceil(usize::BITS as usize);
    /// Offset of the free-list link word inside a FREE slot (past the object).
    const FREE_LINK_OFFSET: usize = T::SLOT_BYTES - size_of::<usize>();
    /// Layout proofs the slot scheme rests on, per class. Referenced in
    /// `new()` so the asserts are evaluated at compile time for every
    /// instantiated class.
    const LAYOUT_OK: () = {
        // The stride need NOT be a power of two or divide the page exactly
        // (bytecode's 384B stride is the first such class): `SLOTS` floors
        // the division and the sub-stride page tail is simply never
        // bump-reached. Everything stride-derived (`slot_ptr` multiply,
        // `owns`'s modulo + explicit `< SLOTS` bound, the bitmap prefix) is
        // exact for any stride that satisfies the asserts below.
        assert!(Self::SLOTS >= 1);
        assert!(Self::SLOTS * T::SLOT_BYTES <= OBJECT_PAGE_BYTES);
        assert!(Self::ALLOC_WORDS <= OBJECT_PAGE_MAX_ALLOC_WORDS);
        // The trailing link word never aliases object bytes.
        assert!(Self::FREE_LINK_OFFSET >= size_of::<T>());
        assert!(Self::FREE_LINK_OFFSET + size_of::<usize>() <= T::SLOT_BYTES);
        assert!(T::SLOT_BYTES.is_multiple_of(std::mem::align_of::<T>()));
        assert!(Self::FREE_LINK_OFFSET.is_multiple_of(std::mem::align_of::<usize>()));
    };

    fn layout() -> Layout {
        Layout::from_size_align(OBJECT_PAGE_BYTES, OBJECT_PAGE_ALIGN).expect("object page layout")
    }

    fn new() -> Self {
        // Force the per-class layout proofs (compile-time).
        #[allow(clippy::let_unit_value)]
        let () = Self::LAYOUT_OK;
        let storage = unsafe { alloc::alloc(Self::layout()) };
        if storage.is_null() {
            alloc::handle_alloc_error(Self::layout());
        }
        #[cfg(test)]
        T::live_page_counter().fetch_add(1, Ordering::Relaxed);
        Self {
            storage,
            next_index: 0,
            alloc_bits: [0; OBJECT_PAGE_MAX_ALLOC_WORDS],
            allocated: 0,
            free_head: PAGE_NONE,
            next_partial: PAGE_NONE,
            on_partial: false,
            retired: false,
            _class: std::marker::PhantomData,
        }
    }

    #[inline]
    fn base_addr(&self) -> usize {
        self.storage as usize
    }

    #[inline]
    fn slot_ptr(&self, index: usize) -> *mut T {
        debug_assert!(index < Self::SLOTS);
        unsafe { self.storage.add(index * T::SLOT_BYTES).cast() }
    }

    /// Page base for any pointer into a page — valid ONLY because pages are
    /// size-aligned (see `OBJECT_PAGE_ALIGN`).
    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn page_base_for_ptr(ptr: *const T) -> usize {
        (ptr as usize) & !(OBJECT_PAGE_ALIGN - 1)
    }

    #[inline]
    fn is_allocated(&self, index: usize) -> bool {
        let word = index / usize::BITS as usize;
        let mask = 1usize << (index % usize::BITS as usize);
        (self.alloc_bits[word] & mask) != 0
    }

    /// Set slot `index`'s alloc bit (it must be clear) and bump occupancy.
    #[inline]
    fn set_allocated(&mut self, index: usize) {
        debug_assert!(!self.is_allocated(index), "arena slot double-allocated");
        self.alloc_bits[index / usize::BITS as usize] |= 1usize << (index % usize::BITS as usize);
        self.allocated += 1;
    }

    /// The free-list link word of slot `index` (meaningful only while free).
    #[inline]
    fn free_link_ptr(&self, index: usize) -> *mut usize {
        debug_assert!(index < Self::SLOTS);
        unsafe {
            self.storage
                .add(index * T::SLOT_BYTES + Self::FREE_LINK_OFFSET)
                .cast()
        }
    }

    /// Pop one slot off the page-local free list. The caller must set the
    /// alloc bit and FULL-HEADER-WRITE the slot before publishing it.
    #[inline]
    fn pop_free(&mut self) -> Option<usize> {
        if self.free_head == PAGE_NONE {
            return None;
        }
        let index = self.free_head;
        debug_assert!(
            !self.is_allocated(index),
            "free-listed arena slot has its alloc bit set",
        );
        self.free_head = unsafe { self.free_link_ptr(index).read() };
        Some(index)
    }

    /// FREE one slot: clear its alloc bit and thread it onto the page-local
    /// free list. For payload-bearing classes (strings own byte storage +
    /// interval tables; vectors own their element `Vec`) the caller MUST
    /// `drop_in_place` the slot BEFORE calling this — a bit-clear-only free
    /// leaks every payload, and once the bit is clear the slot bytes are
    /// garbage that no reader (this fn included) may interpret. Only the
    /// trailing link word is written here — the stale object bytes are left
    /// in place and must never be read again (allocated-bit-first).
    #[inline]
    fn free_slot(&mut self, index: usize) {
        debug_assert!(self.is_allocated(index), "arena slot double-freed");
        debug_assert!(!self.retired, "freed a slot in a retired page");
        self.alloc_bits[index / usize::BITS as usize] &=
            !(1usize << (index % usize::BITS as usize));
        self.allocated -= 1;
        unsafe { self.free_link_ptr(index).write(self.free_head) };
        self.free_head = index;
    }

    /// Bump-allocate the next never-used slot index, if any. The caller must
    /// set the alloc bit and FULL-HEADER-WRITE the slot.
    #[inline]
    fn bump(&mut self) -> Option<usize> {
        if self.next_index >= Self::SLOTS {
            return None;
        }
        let index = self.next_index;
        self.next_index += 1;
        Some(index)
    }
}

impl<T: PagedObject> Drop for ObjectPage<T> {
    fn drop(&mut self) {
        // TEARDOWN OWNS THE PAYLOADS: walk the allocated slots (bit-first —
        // clear-bit slot bytes are garbage) and `drop_in_place` each live
        // object, so strings free their byte storage + interval tables and
        // vectors free their element `Vec`. Float-v1's dealloc-only Drop does
        // NOT generalize; `needs_drop` keeps the float walk compiled out.
        // Reached either when a completed sweep removes an empty young page,
        // or when the owning `TaggedHeap` drops. Both paths run on the mutator
        // after any concurrent marker has joined, so no GC thread can still be
        // reading these slots. Retired pages are freed only at heap teardown.
        if std::mem::needs_drop::<T>() {
            for word_index in 0..Self::ALLOC_WORDS {
                let mut bits = self.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    unsafe { std::ptr::drop_in_place(self.slot_ptr(index)) };
                }
            }
        }
        #[cfg(test)]
        T::live_page_counter().fetch_sub(1, Ordering::Relaxed);
        unsafe { alloc::dealloc(self.storage, Self::layout()) };
    }
}

/// One size class of the object arena: its pages, the page-base registry
/// (page base → `pages` index; mirrors `cons_block_index_by_base` but is a
/// DISTINCT per-class registry — the mark paths dispatch tag-first to the
/// class registry, and the collision analysis for `owns` depends on each
/// registry holding only its own class's pages — never merge them, with each
/// other or with the cons registry), and the class free list.
struct ObjectArena<T: PagedObject> {
    /// Every retained page of this class, retired pages included. Completely
    /// empty young pages are removed after a full sweep; all remaining pages
    /// are freed by this vector's drop at heap teardown (`ObjectPage: Drop`).
    pages: Vec<ObjectPage<T>>,
    /// Page-base → `pages` index: O(1) page lookup from any slot pointer
    /// (pages are size-aligned, so `ObjectPage::page_base_for_ptr` masks the
    /// base out of the pointer). Retired pages STAY registered (C1).
    page_index_by_base: FxHashMap<usize, usize>,
    /// Class free list: index of the first page with free slots
    /// (`PAGE_NONE` = none), chained through `ObjectPage::next_partial`.
    /// Alloc order: partial-page free-slot pop → last-page bump → new page.
    partial_head: usize,
}

impl<T: PagedObject> ObjectArena<T> {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            page_index_by_base: FxHashMap::default(),
            partial_head: PAGE_NONE,
        }
    }

    /// THE PAGE-SPAN OWNERSHIP ORACLE for this class: `ptr` is an owned, LIVE
    /// object of class `T` iff its masked page base is a registered page of
    /// this arena AND the offset is slot-aligned AND the slot's ALLOC BIT IS
    /// SET. The alloc-bit test is load-bearing: bump-cursor/registry bounds
    /// alone would answer "owned" for a freed slot, and every owner of an
    /// owns()→header-read sequence (`is_heap_young`, `value_is_tenured`,
    /// `is_value_marked`, `mark_value`'s owned arms) would then read garbage
    /// bytes. A freed slot answers NOT-owned the instant its bit clears —
    /// this replaces float-v1's explicit addr-set evict-before-free. Retired
    /// pages answer normally (their bits are all set, permanently) — C1.
    ///
    /// Mutator-thread only (like every `&self` heap read); the GC thread's
    /// ownership test is the start-handshake page-base SNAPSHOT, never this
    /// live registry/bitmap.
    #[inline]
    fn owns(&self, ptr: *const u8) -> bool {
        let addr = ptr as usize;
        let base = addr & !(OBJECT_PAGE_ALIGN - 1);
        let Some(&index) = self.page_index_by_base.get(&base) else {
            return false;
        };
        let offset = addr - base;
        if !offset.is_multiple_of(T::SLOT_BYTES) {
            return false;
        }
        let slot = offset / T::SLOT_BYTES;
        // Non-power-of-two strides (bytecode's 384B) leave a sub-stride page
        // TAIL whose first byte is stride-aligned; bound the index so a tail
        // address answers NOT-owned by construction (its bit also can never
        // be set — bump/free never mint indices >= SLOTS — but the oracle
        // must not lean on "never set" for exactness).
        slot < ObjectPage::<T>::SLOTS && self.pages[index].is_allocated(slot)
    }

    /// Grab one raw slot: class free-list pop → current-page bump → new page.
    /// Sets the slot's alloc bit; the caller MUST immediately
    /// full-header-write the slot (its bytes are garbage until then, and the
    /// sweep may legally visit it as soon as the mutator next yields —
    /// allocated-bit ⇒ readable header is the sweep's contract).
    fn alloc_slot(&mut self) -> *mut T {
        // 1. Class free list: pop from the first page with freed slots.
        //    Retired pages are never on this chain (they never gain free
        //    slots: full at retirement and never swept).
        if self.partial_head != PAGE_NONE {
            let page_index = self.partial_head;
            let page = &mut self.pages[page_index];
            let index = page.pop_free().expect("partial page must have free slots");
            page.set_allocated(index);
            if page.free_head == PAGE_NONE {
                // Drained: unlink from the partial chain (head pop — O(1)).
                self.partial_head = page.next_partial;
                page.next_partial = PAGE_NONE;
                page.on_partial = false;
            }
            return page.slot_ptr(index);
        }
        // 2. Current-page bump: only the NEWEST page can have never-used
        //    slots (older pages were bump-exhausted before it was created).
        //    A retired last page is bump-exhausted by construction (retired
        //    ⇒ full), so `bump` correctly falls through to a fresh page.
        if let Some(page) = self.pages.last_mut()
            && let Some(index) = page.bump()
        {
            page.set_allocated(index);
            return page.slot_ptr(index);
        }
        // 3. Fresh 64KB-aligned page.
        let mut page = ObjectPage::<T>::new();
        let index = page.bump().expect("fresh arena page must have space");
        page.set_allocated(index);
        let ptr = page.slot_ptr(index);
        let base = page.base_addr();
        self.pages.push(page);
        let prev = self.page_index_by_base.insert(base, self.pages.len() - 1);
        debug_assert!(prev.is_none(), "arena page base registered twice");
        ptr
    }

    /// Sweep pages `[start, end)` of this class — the page objects' only
    /// reclaimer, wired into BOTH sweep entry points: the eager
    /// `finalize_collection` (whole range in one call) and the cooperative
    /// `incremental_sweep_slice` (page-at-a-time behind a per-class cursor).
    /// Runs on the mutator thread only; the GC thread never sweeps.
    ///
    /// Visit order is ALLOCATED-BIT-FIRST: a clear bit means the slot bytes
    /// are garbage (never-bumped = uninitialized; freed = stale, possibly
    /// scribbled), so ANY header read through it is UB — the inverse of the
    /// intrusive-list sweep, whose list membership implies a valid header.
    /// RETIRED pages are skipped whole (never swept; their slots are all
    /// tenured — permanently live). For allocated slots the order is:
    ///
    /// 1. TENURED-SKIP BEFORE THE PARITY TEST: a tenured slot's mark bit
    ///    froze at promotion; interpreting it against the current parity
    ///    would free a live tenured object on every alternate-parity cycle
    ///    (the float-v1 template's bare `is_marked_at` is exactly that bug
    ///    once page objects can tenure). Tenured slots are skipped (MIXED
    ///    pages carry them forever — bounded by the one-time loadup survivor
    ///    set) and, like tenured LIST objects — which the young-list sweep
    ///    never counts — contribute nothing to the recomputed live bytes,
    ///    keeping `live_bytes` (the adaptive pacer term) on the same
    ///    definition it had before the migration.
    /// 2. Marked-at-parity slots are survivors: their VARIABLE byte size
    ///    (`object_bytes_from_header` — fixed struct + payload storage) is
    ///    summed into the returned live bytes, which both recompute sites
    ///    feed into `live_bytes`.
    /// 3. Dead slots: `on_free(addr)` (registry eviction hook — the vector
    ///    class evicts `vector_object_addrs` here), then
    ///    `drop_in_place::<T>` (strings own byte storage + interval tables,
    ///    vectors own their element `Vec` — a bit-clear-only free leaks them
    ///    all; NEVER `Box::from_raw` on page memory), then the alloc bit
    ///    clears and the slot threads onto the page-local free list.
    ///
    /// Pages that gained free slots join the arena's partial chain, so the
    /// class free list can hand their slots out again — including to a
    /// mutator running BETWEEN cooperative slices. That mid-sweep reuse is
    /// why each visit RE-READS the live bitmap word instead of a sweep-start
    /// snapshot. A slot reallocated mid-sweep re-enters allocated +
    /// born-at-parity ⇒ reads as marked ⇒ survivor.
    ///
    /// Returns `(survivor bytes, slots freed)`.
    fn sweep_range(
        &mut self,
        start: usize,
        end: usize,
        parity: bool,
        mut on_free: impl FnMut(usize),
    ) -> (usize, usize) {
        let mut live_bytes = 0usize;
        let mut freed = 0usize;
        for page_index in start..end.min(self.pages.len()) {
            let page = &mut self.pages[page_index];
            if page.retired {
                continue; // never swept; slots permanently tenured-live
            }
            let mut freed_any = false;
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                // RE-READ the current bitmap word (see the doc above).
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    let slot = page.slot_ptr(index);
                    // Alloc bit set ⇒ the slot holds a fully written live
                    // object — reading its header is sound.
                    let header = unsafe { &*(slot as *const GcHeader) };
                    debug_assert!(
                        header.kind == T::KIND,
                        "wrong-kind header in a {} arena slot",
                        T::CLASS,
                    );
                    // (1) TENURED-SKIP before any parity interpretation.
                    if header.tenured {
                        continue;
                    }
                    if header.is_marked_at(parity) {
                        // (2) Survivor: variable-size byte accounting.
                        live_bytes = live_bytes.saturating_add(
                            TaggedHeap::object_bytes_from_header(slot as *const GcHeader),
                        );
                    } else {
                        // (3) Dead: evict from any class registry, drop the
                        // payload IN PLACE, then clear the bit (the oracle
                        // answers NOT-owned from here on).
                        on_free(slot as usize);
                        unsafe { std::ptr::drop_in_place(slot) };
                        page.free_slot(index);
                        freed += 1;
                        freed_any = true;
                    }
                }
            }
            if freed_any && !page.on_partial {
                page.on_partial = true;
                page.next_partial = self.partial_head;
                self.partial_head = page_index;
            }
        }
        (live_bytes, freed)
    }

    /// Release completely empty young pages after the whole class has been
    /// swept, then rebuild every index-bearing arena structure. This must not
    /// run between cooperative sweep slices: removing a `pages` element shifts
    /// later indices and would invalidate both the sweep cursor and partial
    /// chain. Slot storage has its own stable allocation, so compacting the
    /// `Vec<ObjectPage<T>>` does not move any surviving Lisp object.
    fn release_empty_pages(&mut self) -> usize {
        let old_len = self.pages.len();
        if !self
            .pages
            .iter()
            .any(|page| page.allocated == 0 && !page.retired)
        {
            return 0;
        }

        self.pages
            .retain(|page| page.allocated != 0 || page.retired);
        self.pages.shrink_to_fit();

        self.page_index_by_base =
            FxHashMap::with_capacity_and_hasher(self.pages.len(), Default::default());
        self.partial_head = PAGE_NONE;
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            let previous = self.page_index_by_base.insert(page.base_addr(), page_index);
            debug_assert!(previous.is_none(), "arena page base registered twice");

            page.next_partial = PAGE_NONE;
            page.on_partial = false;
            if page.free_head != PAGE_NONE {
                page.on_partial = true;
                page.next_partial = self.partial_head;
                self.partial_head = page_index;
            }
        }

        old_len - self.pages.len()
    }

    /// Collect raw pointers to every ALLOCATED slot (allocated-bit-first;
    /// retired pages INCLUDED — their slots are live tenured objects).
    /// Snapshot semantics: callers walk the returned vector while calling
    /// arbitrary `&self`/`&mut self` heap methods (verifiers, promotion).
    fn collect_allocated_slots(&self) -> Vec<*mut T> {
        let mut out = Vec::new();
        for page in &self.pages {
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    out.push(page.slot_ptr(word_index * usize::BITS as usize + bit));
                }
            }
        }
        out
    }

    /// Exact page/slot occupancy plus directly-owned payload capacity for
    /// diagnostics. The allocation bitmap is authoritative, just as it is for
    /// sweep and ownership checks; unallocated slot bytes are never read.
    fn layout_stats(&self, payload_layout: impl Fn(&T) -> PayloadLayout) -> ArenaLayoutStats {
        let mut stats = ArenaLayoutStats {
            class: T::CLASS,
            pages: self.pages.len(),
            page_bytes: self.pages.len().saturating_mul(OBJECT_PAGE_BYTES),
            slot_bytes: T::SLOT_BYTES,
            slots_per_page: ObjectPage::<T>::SLOTS,
            ..ArenaLayoutStats::default()
        };

        for page in &self.pages {
            stats.bumped_slots = stats.bumped_slots.saturating_add(page.next_index);
            stats.allocated_slots = stats.allocated_slots.saturating_add(page.allocated);
            stats.reclaimed_slots = stats
                .reclaimed_slots
                .saturating_add(page.next_index.saturating_sub(page.allocated));
            stats.never_used_slots = stats
                .never_used_slots
                .saturating_add(ObjectPage::<T>::SLOTS.saturating_sub(page.next_index));
            stats.retired_pages += usize::from(page.retired);
            if page.allocated == 0 {
                stats.empty_pages += 1;
            } else if page.allocated == ObjectPage::<T>::SLOTS {
                stats.full_pages += 1;
            } else {
                stats.partial_pages += 1;
            }

            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    let object = unsafe { &*page.slot_ptr(index) };
                    let header = unsafe { &*(object as *const T as *const GcHeader) };
                    if header.tenured {
                        stats.tenured_slots += 1;
                    } else {
                        stats.young_slots += 1;
                    }
                    let payload = payload_layout(object);
                    stats.payload_logical_bytes = stats
                        .payload_logical_bytes
                        .saturating_add(payload.logical_bytes);
                    stats.payload_capacity_bytes = stats
                        .payload_capacity_bytes
                        .saturating_add(payload.capacity_bytes);
                    stats.owned_payloads += usize::from(payload.owned);
                    stats.mapped_payloads += usize::from(payload.mapped);
                }
            }
        }

        stats.occupied_slot_bytes = stats.allocated_slots.saturating_mul(T::SLOT_BYTES);
        stats.object_struct_bytes = stats.allocated_slots.saturating_mul(size_of::<T>());
        debug_assert_eq!(
            stats.allocated_slots,
            stats.tenured_slots + stats.young_slots,
            "arena layout accounting lost an allocated slot",
        );
        stats
    }
}

struct MappedConsRange {
    start: *mut ConsCell,
    len: usize,
    mark_bits: Vec<usize>,
}

impl MappedConsRange {
    fn new(start: *mut ConsCell, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    fn contains_ptr(&self, ptr: *const ConsCell) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<ConsCell>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<ConsCell>())
    }

    #[inline]
    fn index_of_ptr(&self, ptr: *const ConsCell) -> usize {
        (ptr as usize - self.start as usize) / size_of::<ConsCell>()
    }

    #[inline]
    fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    fn mark_ptr(&mut self, ptr: *const ConsCell) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    fn live_count(&self) -> usize {
        self.mark_bits
            .iter()
            .enumerate()
            .map(|(word_index, word)| {
                let full_words = self.len / CONS_MARK_BITS_PER_WORD;
                let tail_bits = self.len % CONS_MARK_BITS_PER_WORD;
                if word_index < full_words || tail_bits == 0 {
                    word.count_ones() as usize
                } else {
                    let mask = (1usize << tail_bits) - 1;
                    (word & mask).count_ones() as usize
                }
            })
            .sum()
    }
}

struct MappedFloatRange {
    start: *mut FloatObj,
    len: usize,
    mark_bits: Vec<usize>,
}

impl MappedFloatRange {
    fn new(start: *mut FloatObj, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    fn contains_ptr(&self, ptr: *const FloatObj) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<FloatObj>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<FloatObj>())
    }

    #[inline]
    fn index_of_ptr(&self, ptr: *const FloatObj) -> usize {
        (ptr as usize - self.start as usize) / size_of::<FloatObj>()
    }

    #[inline]
    fn is_marked_ptr(&self, ptr: *const FloatObj) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    fn mark_ptr(&mut self, ptr: *const FloatObj) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    fn live_count(&self) -> usize {
        self.mark_bits
            .iter()
            .enumerate()
            .map(|(word_index, word)| {
                let full_words = self.len / CONS_MARK_BITS_PER_WORD;
                let tail_bits = self.len % CONS_MARK_BITS_PER_WORD;
                if word_index < full_words || tail_bits == 0 {
                    word.count_ones() as usize
                } else {
                    let mask = (1usize << tail_bits) - 1;
                    (word & mask).count_ones() as usize
                }
            })
            .sum()
    }
}

struct MappedVecLikeObject {
    header: *mut VecLikeHeader,
    byte_len: usize,
    marked: bool,
}

impl MappedVecLikeObject {
    fn new(header: *mut VecLikeHeader, byte_len: usize) -> Self {
        Self {
            header,
            byte_len,
            marked: false,
        }
    }
}

struct MappedStringObject {
    ptr: *mut StringObj,
    byte_len: usize,
    marked: bool,
}

impl MappedStringObject {
    fn new(ptr: *mut StringObj, byte_len: usize) -> Self {
        Self {
            ptr,
            byte_len,
            marked: false,
        }
    }
}

/// Per-kind breakdown of the values the GC thread parked in `deferred` for the
/// STW termination drain, taken as `join_concurrent_mark` folds the buffer into
/// gray. Sizes the concurrent-tracing extension: which kind a further
/// concurrent tier should take on first (strings are mark-only + intervals;
/// records/closures need atomic slots + snapshot/clone-on-write; weak/growable
/// hash tables stay deferred regardless). Counts are ENTRIES, not unique
/// objects — the GC thread parks a value once per discovered edge, and the
/// termination's `mark_value` dedups. Diagnostics only.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DrainKinds {
    pub string: usize,
    /// Vectors trace concurrently (Stage 2 Tier B scans their BACKINGS), and
    /// since task 01 the GC thread also CLAIMS owned page vectors' header
    /// marks — so this bucket counts only the still-parked residue
    /// (mapped/Box-residual vectors, and snapshot-missed edge cases).
    pub vector: usize,
    pub record: usize,
    /// Lambda + Macro (interpreted closures).
    pub closure: usize,
    /// Since task 01's bytecode arm the GC thread CLAIMS owned page
    /// bytecode (children gray-pushed at claim time) — this bucket counts
    /// only the still-parked residue (mapped/dump-span bytecode and
    /// mid-cycle-page snapshot misses).
    pub bytecode: usize,
    pub hash_table: usize,
    /// CharTable + SubCharTable.
    pub char_table: usize,
    pub float: usize,
    /// Non-owned conses (new-block or mapped) the GC thread could not mark via
    /// its start-of-cycle block snapshot.
    pub cons: usize,
    /// Built-in functions — a large near-constant population (~1.7k registered
    /// at startup), split out so it does not mask the true `other` residue.
    pub subr: usize,
    /// Every remaining veclike (marker/buffer/overlay/bignum/...).
    pub other: usize,
}

impl DrainKinds {
    /// Classify one parked value into its bucket — the same tag dispatch
    /// `mark_value` uses (cons/string/float, then the veclike `type_tag`).
    ///
    /// # Safety
    /// `val` must be a live heap value: `join_concurrent_mark` runs before the
    /// termination drain and sweep, and nothing is freed during a concurrent
    /// mark (allocate-black; sweeps never overlap marking), so every parked
    /// entry's header is still valid.
    unsafe fn note(&mut self, val: TaggedValue) {
        if val.is_cons() {
            self.cons += 1;
        } else if val.is_string() {
            self.string += 1;
        } else if val.is_float() {
            self.float += 1;
        } else if val.is_veclike() {
            let ptr = val.as_veclike_ptr().unwrap();
            match unsafe { (*ptr).type_tag } {
                VecLikeType::Vector => self.vector += 1,
                VecLikeType::Record => self.record += 1,
                VecLikeType::Lambda | VecLikeType::Macro => self.closure += 1,
                VecLikeType::ByteCode => self.bytecode += 1,
                VecLikeType::HashTable => self.hash_table += 1,
                VecLikeType::CharTable | VecLikeType::SubCharTable => self.char_table += 1,
                VecLikeType::Subr => self.subr += 1,
                _ => self.other += 1,
            }
        } else {
            self.other += 1; // unreachable: only heap objects are parked
        }
    }

    /// Fold `cycle`'s per-kind counts into this lifetime per-kind maximum.
    fn merge_max(&mut self, cycle: &DrainKinds) {
        self.string = self.string.max(cycle.string);
        self.vector = self.vector.max(cycle.vector);
        self.record = self.record.max(cycle.record);
        self.closure = self.closure.max(cycle.closure);
        self.bytecode = self.bytecode.max(cycle.bytecode);
        self.hash_table = self.hash_table.max(cycle.hash_table);
        self.char_table = self.char_table.max(cycle.char_table);
        self.float = self.float.max(cycle.float);
        self.cons = self.cons.max(cycle.cons);
        self.subr = self.subr.max(cycle.subr);
        self.other = self.other.max(cycle.other);
    }

    /// Sum of all buckets — equals the deferred-entry count it was built from.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn total(&self) -> usize {
        self.string
            + self.vector
            + self.record
            + self.closure
            + self.bytecode
            + self.hash_table
            + self.char_table
            + self.float
            + self.cons
            + self.subr
            + self.other
    }
}

impl std::fmt::Display for DrainKinds {
    /// Compact trace-line segment: `str=N vec=N rec=N clo=N bc=N ht=N ct=N f=N
    /// cons=N sub=N other=N`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "str={} vec={} rec={} clo={} bc={} ht={} ct={} f={} cons={} sub={} other={}",
            self.string,
            self.vector,
            self.record,
            self.closure,
            self.bytecode,
            self.hash_table,
            self.char_table,
            self.float,
            self.cons,
            self.subr,
            self.other,
        )
    }
}

/// A point-in-time accounting of one fixed-stride object arena. This is
/// diagnostics-only and intentionally separates the always-resident 64 KiB
/// page backing from payload allocations owned by live objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArenaLayoutStats {
    pub class: &'static str,
    pub pages: usize,
    pub page_bytes: usize,
    pub slot_bytes: usize,
    pub slots_per_page: usize,
    pub allocated_slots: usize,
    pub tenured_slots: usize,
    pub young_slots: usize,
    pub bumped_slots: usize,
    pub reclaimed_slots: usize,
    pub never_used_slots: usize,
    pub empty_pages: usize,
    pub partial_pages: usize,
    pub full_pages: usize,
    pub retired_pages: usize,
    pub occupied_slot_bytes: usize,
    pub object_struct_bytes: usize,
    pub payload_logical_bytes: usize,
    pub payload_capacity_bytes: usize,
    pub owned_payloads: usize,
    pub mapped_payloads: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PayloadLayout {
    logical_bytes: usize,
    capacity_bytes: usize,
    owned: bool,
    mapped: bool,
}

impl PayloadLayout {
    fn add(self, other: Self) -> Self {
        Self {
            logical_bytes: self.logical_bytes.saturating_add(other.logical_bytes),
            capacity_bytes: self.capacity_bytes.saturating_add(other.capacity_bytes),
            owned: self.owned || other.owned,
            mapped: self.mapped || other.mapped,
        }
    }
}

/// Exact ordinary-cons block occupancy. Mapped pdump conses are reported in
/// [`MappedLayoutStats`] because they do not consume allocator-backed blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConsLayoutStats {
    pub pages: usize,
    pub page_bytes: usize,
    pub capacity_slots: usize,
    pub bumped_slots: usize,
    pub live_slots: usize,
    pub reclaimed_slots: usize,
    pub never_used_slots: usize,
    pub empty_pages: usize,
    pub partial_pages: usize,
    pub full_pages: usize,
    pub occupied_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MappedLayoutStats {
    pub conses: usize,
    pub floats: usize,
    pub strings: usize,
    pub veclikes: usize,
    pub object_image_bytes: usize,
    pub copied_string_payloads: usize,
    pub copied_string_capacity_bytes: usize,
    pub copied_veclike_payloads: usize,
    pub copied_veclike_capacity_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoxedKindLayoutStats {
    pub class: &'static str,
    pub objects: usize,
    pub tenured_objects: usize,
    /// Object struct plus directly-owned backing capacities known to the GC.
    /// Nested allocations inside structural hash keys are not included.
    pub known_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeapLayoutStats {
    pub allocated_objects: usize,
    pub managed_live_bytes: usize,
    pub page_backing_bytes: usize,
    pub known_payload_capacity_bytes: usize,
    pub cons: ConsLayoutStats,
    pub arenas: Vec<ArenaLayoutStats>,
    pub mapped: MappedLayoutStats,
    pub boxed: Vec<BoxedKindLayoutStats>,
}

/// Snapshot of the deferred-sweep cost accounting plus the concurrent-mark
/// termination drain probe. Diagnostics only: per-cycle fields hold the most
/// recently completed (or in-flight) deferred sweep; lifetime fields aggregate
/// across the heap's life, with the eager STW sweep feeding `lifetime_sweep_us`
/// too so the two sweep paths are comparable.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SweepStats {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub sweep_us: u64,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub slice_count: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub cons_blocks_swept: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub noncons_freed: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_sweep_us: u64,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_slices: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_cons_blocks_swept: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_noncons_freed: usize,
    /// Values `join_concurrent_mark` folded into the termination gray queue:
    /// the GC thread's parked non-cons buffer and the residual SATB log.
    pub last_termination_deferred: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub max_termination_deferred: usize,
    pub last_termination_satb: usize,
    /// Per-kind breakdown of `last_termination_deferred`, plus the lifetime
    /// per-kind maximum (each bucket's own max across cycles). Populated in
    /// crate tests and under `NEOVM_GC_TRACE=1`; zero otherwise — the
    /// classification's header reads are not free STW time.
    pub last_termination_kinds: DrainKinds,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub max_termination_kinds: DrainKinds,
    /// CONCURRENT STRING MARKING: owned interval-free strings the GC thread
    /// claimed concurrently last cycle — string marks that LEFT the STW drain
    /// (the `kinds.string` bucket keeps counting the strings still parked:
    /// interval-bearing + mapped/dump-span ones). Always populated.
    pub last_concurrent_str_claimed: usize,
    /// CONCURRENT FLOAT CLAIMS (task 01): owned young page floats the GC
    /// thread claimed concurrently last cycle (the `kinds.float` bucket keeps
    /// counting the still-parked ones: snapshot-missed/mapped/Box floats).
    pub last_concurrent_float_claimed: usize,
    /// SUBR RECOGNIZE-AND-DROP (task 01): defer-path drops of leaked-static
    /// subrs last cycle (drop EVENTS — one per discovered edge, not unique
    /// subrs; the `kinds.subr` bucket keeps counting mapped subrs, which
    /// still park).
    pub last_concurrent_subr_dropped: usize,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): owned young page vectors
    /// whose header the GC thread claimed last cycle (the `kinds.vector`
    /// bucket keeps counting the still-parked ones: mapped/Box-residual).
    pub last_concurrent_vec_claimed: usize,
    /// CONCURRENT BYTECODE CLAIMS (task 01): owned young page bytecode the
    /// GC thread claimed (children gray-pushed) last cycle (the
    /// `kinds.bytecode` bucket keeps counting the still-parked residue:
    /// mapped/dump-span and mid-cycle-page bytecode).
    pub last_concurrent_bc_claimed: usize,
    /// Cost of the `join_concurrent_mark` fold itself (taking the SATB +
    /// deferred buffers, classifying, pushing to gray) — the cheap half of the
    /// termination; the mark fixpoint that follows is the trace line's `drain`.
    pub last_termination_fold_us: u64,
    /// Lifetime count of concurrent-mark terminations (`join_concurrent_mark`
    /// calls), so a probe polling between eval chunks can detect a new cycle.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub termination_count: usize,
    /// Mark cost of the most recent cycle at `incremental_finish`. For a
    /// concurrent cycle this is exactly the STW termination drain: the counter
    /// resets at `concurrent_begin` and the termination's
    /// `incremental_drain_all` is the only accumulation.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub mark_us: u64,
}

/// One root group's cost inside a `seed_all_context_roots` call:
/// `(name, wall µs enumerating+seeding the group, values the group visited)`.
/// The count is the enumeration volume (every value fed to the seeding sink,
/// BEFORE the non-heap-object / mapped-root filters), which is what the walk's
/// O() actually scales with.
pub(crate) type RootGroup = (&'static str, u64, usize);

/// Per-group decomposition of one `seed_all_context_roots` call (one root
/// handshake's context-root seeding). Built fresh each call by the evaluator.
#[derive(Clone, Debug, Default)]
pub(crate) struct RootSeedBreakdown {
    /// Whole-call wall time (all groups + thread-local + marker heads).
    pub total_us: u64,
    /// Ordered per-group `(name, µs, values visited)` records.
    pub groups: Vec<RootGroup>,
}

impl RootSeedBreakdown {
    /// Compact `name=USus/COUNT` rendering of the nonzero groups, for the
    /// `NEOVM_GC_TRACE` handshake lines.
    pub(crate) fn format_nonzero(&self) -> String {
        let mut out = String::new();
        for &(name, us, count) in &self.groups {
            if us == 0 && count == 0 {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&format!("{name}={us}us/{count}"));
        }
        out
    }

    /// Count for a named group (0 if absent) — test/probe convenience.
    #[cfg(test)]
    pub(crate) fn group_count(&self, name: &str) -> usize {
        self.groups
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|&(_, _, c)| c)
            .unwrap_or(0)
    }
}

/// STW pause instrumentation for the concurrent collector's TWO handshakes —
/// the START handshake (`start_concurrent_mark`: clear marks, obarray
/// snapshot, root seeding, cons/vector snapshots, job assembly) and the
/// TERMINATION handshake (`terminate_concurrent_mark`: join+fold, root
/// re-seeding, residual drain, weak/finalizer/marker post-passes) — each
/// decomposed per phase and per root GROUP, plus once-per-handshake O() size
/// probes. Sibling of [`SweepStats`]; diagnostics only (no behavior).
/// Heap-side phases are recorded where they run in this file; the evaluator
/// records the context-root breakdown and the context-side probes via
/// `handshake_stats_mut`.
#[derive(Clone, Debug, Default)]
pub(crate) struct HandshakeStats {
    // --- START handshake (once per concurrent cycle) ---
    /// Lifetime count of concurrent start handshakes (`concurrent_begin`).
    pub start_count: usize,
    /// Whole start-handshake pause (µs) and its lifetime max.
    pub last_start_total_us: u64,
    pub max_start_total_us: u64,
    /// `begin_collection` mark-bit clearing (young non-cons + cons blocks).
    pub last_start_clear_us: u64,
    /// The clear's three-way split (task #7 stage 2a diagnostics rider):
    /// cons-block bitmap memset / young non-cons `all_objects` walk (the only
    /// component an epoch/parity mark-bit design would remove) / mapped
    /// (pdump) mark-state resets.
    pub last_start_clear_cons_us: u64,
    pub last_start_clear_noncons_us: u64,
    pub last_start_clear_mapped_us: u64,
    /// `seed_internal_runtime_roots` at start (registries + doomed queue).
    pub last_start_runtime_us: u64,
    pub last_start_runtime_roots: usize,
    /// `seed_mapped_remembered` at start (dump remembered-set re-scan).
    pub last_start_remembered_us: u64,
    pub last_start_remembered_roots: usize,
    /// `obarray.scan_snapshot()` capture.
    pub last_start_obsnap_us: u64,
    /// `seed_all_context_roots` at start, per group.
    pub last_start_roots: RootSeedBreakdown,
    /// `launch_concurrent_mark` phases: cons-block base snapshot, vector
    /// backing snapshot, and the residual job assembly + thread send.
    pub last_start_conssnap_us: u64,
    pub last_start_vecsnap_us: u64,
    /// CONCURRENT FLOAT CLAIMS (task 01): float-arena page-base snapshot
    /// capture — O(pages) only, mirrors `last_start_vecsnap_us`.
    pub last_start_floatsnap_us: u64,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): vector-arena page-BASE
    /// snapshot capture (distinct from the Tier-B backing `vecsnap`).
    pub last_start_vecbasesnap_us: u64,
    /// CONCURRENT BYTECODE CLAIMS (task 01): bytecode-arena page-base
    /// snapshot capture — O(pages) only, mirrors `last_start_vecbasesnap_us`.
    pub last_start_bcsnap_us: u64,
    pub last_start_jobasm_us: u64,

    // --- TERMINATION handshake (once per concurrent cycle) ---
    /// Lifetime count of termination reseeds
    /// (`reseed_runtime_and_remembered_roots`).
    pub term_count: usize,
    /// The whole pre-drain roots lump (join → reseed → ctx roots → new
    /// symbols), as printed by the existing `roots=` trace field, + max.
    pub last_term_roots_total_us: u64,
    pub max_term_roots_total_us: u64,
    /// `join_concurrent_mark` total (stop signal + thread exit wait + the
    /// SATB/deferred fold; the fold alone is `SweepStats::
    /// last_termination_fold_us`).
    pub last_term_join_us: u64,
    /// `seed_internal_runtime_roots` at termination.
    pub last_term_runtime_us: u64,
    pub last_term_runtime_roots: usize,
    /// `seed_mapped_remembered` at termination.
    pub last_term_remembered_us: u64,
    pub last_term_remembered_roots: usize,
    /// `seed_all_context_roots` at termination, per group.
    pub last_term_ctxroots: RootSeedBreakdown,
    /// Stage 1b residual: `trace_new_symbol_cells` over mid-cycle interns.
    pub last_term_newsyms_us: u64,
    pub last_term_newsyms_roots: usize,
    /// `incremental_finish` post-drain passes: doomed-finalizer scan, weak
    /// hash-table sweep, dead-marker unchaining.
    pub last_term_finalizer_us: u64,
    pub last_term_weak_us: u64,
    pub last_term_unchain_us: u64,

    // --- O() size probes (refreshed at each handshake) ---
    /// JIT COMPILED cache: total cached entries / total reloc slots walked.
    pub probe_jit_compiled_entries: usize,
    pub probe_jit_reloc_slots: usize,
    /// `mapped_remembered.len()` — the dump remembered set (never cleared).
    pub probe_mapped_remembered: usize,
    /// Bytecode operand-stack buffer depth (`bc_buf`).
    pub probe_bc_buf_depth: usize,
    /// Binding-stack depth (`specpdl`).
    pub probe_specpdl_depth: usize,
    /// Obarray logical slots + chunk count (start snapshot / current).
    pub probe_obarray_slots: usize,
    pub probe_obarray_chunks: usize,
    /// Vector-backing snapshot length (Tier B, captured at start).
    pub probe_vector_snapshot_len: usize,
    /// Owned cons blocks snapshotted at start.
    pub probe_cons_blocks: usize,
    /// Live buffers (= marker chain-head slots installed).
    pub probe_buffer_count: usize,
}

impl HandshakeStats {
    /// Compact probe rendering for the `NEOVM_GC_TRACE` handshake lines.
    pub(crate) fn format_probes(&self) -> String {
        format!(
            "jit={}/{} rem={} bc={} spec={} obslots={} obchunks={} vecs={} consblk={} bufs={}",
            self.probe_jit_compiled_entries,
            self.probe_jit_reloc_slots,
            self.probe_mapped_remembered,
            self.probe_bc_buf_depth,
            self.probe_specpdl_depth,
            self.probe_obarray_slots,
            self.probe_obarray_chunks,
            self.probe_vector_snapshot_len,
            self.probe_cons_blocks,
            self.probe_buffer_count,
        )
    }
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
    // Allocation
    // -----------------------------------------------------------------------

    /// Allocate a cons cell. Returns a tagged Value.
    pub fn alloc_cons(&mut self, car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::ConsCells, 1);
        // Allocate-black during the deferred sweep OR a concurrent mark: a cons
        // born while a block is unswept must survive that block's reclaim, and a
        // cons born during concurrent marking must survive this cycle's sweep
        // (the GC thread won't reach it, and a black owner may point at it before
        // the next root snapshot). New conses are always live, so this is exact
        // (cleared at the next mark's begin).
        let sweeping = self.sweep_in_progress || self.concurrent_mark_running;
        if !self.cons_free_list.is_null() {
            let cell = self.cons_free_list;
            unsafe {
                self.cons_free_list = (*cell).free_next();
                (*cell).set_car(car);
                (*cell).set_cdr(cdr);
            }
            self.allocated_count += 1;
            self.cons_live_count += 1;
            self.note_allocation_bytes(size_of::<ConsCell>());
            if sweeping {
                self.mark_cons(cell);
            }
            return unsafe { TaggedValue::from_cons_ptr(cell) };
        }

        if let Some(block) = self.cons_blocks.last_mut()
            && let Some(cell) = block.alloc_bump(car, cdr)
        {
            if sweeping {
                block.mark_ptr(cell);
            }
            self.allocated_count += 1;
            self.cons_live_count += 1;
            self.note_allocation_bytes(size_of::<ConsCell>());
            return unsafe { TaggedValue::from_cons_ptr(cell) };
        }

        // All existing blocks are exhausted and there are no reclaimed cells,
        // so allocate a fresh current block and bump from it, matching GNU's
        // cons_block/cons_block_index path.
        let mut block = ConsBlock::new();
        let block_base = block.base_addr();
        let cell = block
            .alloc_bump(car, cdr)
            .expect("fresh block should have space");
        self.cons_blocks.push(block);
        let block_index = self.cons_blocks.len() - 1;
        self.cons_block_index_by_base
            .insert(block_base, block_index);
        self.allocated_count += 1;
        self.cons_live_count += 1;
        self.note_allocation_bytes(size_of::<ConsCell>());
        if sweeping {
            self.mark_cons(cell);
        }
        unsafe { TaggedValue::from_cons_ptr(cell) }
    }

    /// Allocate a string object from the STRING ARENA PAGES.
    ///
    /// Every slot allocation/reuse performs a FULL-header `ptr::write` of the
    /// whole 56-byte `StringObj` — a fresh `GcHeader` (kind=String,
    /// tenured=false, next=null) plus the moved-in `LispString`, whose
    /// `intervals` `AtomicPtr` word overwrites any STALE interval pointer
    /// left by the slot's previous occupant BEFORE the value is published
    /// (for a fresh `LispString` that word is null; a leaked stale non-null
    /// word would be taken for a live table by the GC thread's null-check
    /// and dereferenced by `mark_value`'s interval trace — a UAF). Writing
    /// the atomic word non-atomically inside `ptr::write` is sound: the slot
    /// is unreachable by any other thread until the tagged value escapes.
    /// Then the same unconditional born-at-parity store `link_object`
    /// applies.
    ///
    /// Page strings are OWNED via the page-span oracle: they NEVER touch
    /// `all_objects`, `non_cons_object_addrs`, or `link_object` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// string reclaimer (it `drop_in_place`s dead slots, freeing the byte
    /// storage and interval table the string owns).
    pub fn alloc_string(&mut self, s: crate::heap_types::LispString) -> TaggedValue {
        let empty_kind = (s.sbytes() == 0).then(|| s.storage_kind());
        if let Some(value) = empty_kind.and_then(|kind| self.canonical_empty_strings.get(kind)) {
            return value;
        }

        self.add_memory_use_count(MemoryUseCountSlot::Strings, 1);
        self.add_memory_use_count(MemoryUseCountSlot::StringChars, s.sbytes() as u64);
        let ptr = self.string_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                StringObj {
                    header: GcHeader::new(HeapObjectKind::String),
                    data: s,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_object`): allocate-black during a mark/sweep, pre-armed
            // white for the next `begin_collection` flip otherwise.
            (*ptr).header.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::string_object_bytes(&*ptr) });
        let value = unsafe { TaggedValue::from_string_ptr(ptr) };
        if let Some(kind) = empty_kind {
            self.canonical_empty_strings.install_owned(kind, value)
        } else {
            value
        }
    }

    /// Allocate a float object from the FLOAT ARENA PAGES.
    ///
    /// Every slot allocation/reuse performs a FULL-HEADER WRITE — a complete
    /// `FloatObj` (fresh `GcHeader`: kind=Float, tenured=false, next=null)
    /// followed by the same unconditional born-at-parity store `link_object`
    /// applies. A reused slot's stale bytes must never leak into the new
    /// object: a stale mark bit is a same-cycle-reuse UAF, a stale kind is a
    /// type-confused free, a stale tenured flag is a leak plus child-UAF
    /// (never traced, never swept).
    ///
    /// Page floats are OWNED via the page-span oracle (stage-3 fold-in: they
    /// no longer touch `non_cons_object_addrs` — `mark_value`'s
    /// owned-vs-mapped routing and `is_heap_young` answer through
    /// `float_arena.owns`) and are NOT `link_object`ed — the intrusive lists
    /// sweep with `free_gc_object`/`Box::from_raw`, which would corrupt the
    /// heap on a page pointer. The page sweep is the only float reclaimer.
    pub fn alloc_float(&mut self, value: f64) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::Floats, 1);
        let ptr = self.float_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                FloatObj {
                    header: GcHeader::new(HeapObjectKind::Float),
                    value,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_object`): allocate-black during a mark/sweep, pre-armed
            // white for the next `begin_collection` flip otherwise.
            (*ptr).header.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FloatObj>());
        unsafe { TaggedValue::from_float_ptr(ptr) }
    }

    /// Allocate a vector from the VECTOR ARENA PAGES.
    ///
    /// This is the single `VecLikeType::Vector` allocation chokepoint (every
    /// other veclike stays a `Box` through `link_veclike`). One FULL-header
    /// `ptr::write` of the whole 48-byte `VectorObj` (fresh `VecLikeHeader`
    /// plus the `LispValueVec` built from `items` — a reused slot's stale
    /// bytes never leak), then the unconditional born-at-parity store.
    ///
    /// INCREMENTAL VECTOR REGISTRY (Fix A) at the page chokepoint: page
    /// vectors never pass `link_veclike`, so the registry insert lives HERE
    /// and the matching remove lives in the page sweep's free hook
    /// (`sweep_arena_pages_ranges`) — the Tier-B vecsnap keeps enumerating
    /// every live vector. Page vectors never touch `all_objects` /
    /// `non_cons_object_addrs`; the page sweep is their only reclaimer (its
    /// `drop_in_place` frees the element `Vec` the vector owns).
    pub fn alloc_vector(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        let ptr = self.vector_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                VectorObj {
                    header: VecLikeHeader::new(VecLikeType::Vector),
                    data: items.into(),
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_veclike`).
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        let registered = self.vector_object_addrs.insert(ptr as usize);
        debug_assert!(
            registered,
            "page vector allocated twice (bitmap/registry out of sync)"
        );
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(
            size_of::<VectorObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(unsafe { &(*ptr).data })),
        );
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped char-table.
    pub fn alloc_char_table(
        &mut self,
        purpose: TaggedValue,
        init: TaggedValue,
        n_extras: usize,
    ) -> TaggedValue {
        let contents = [init; CHAR_TABLE_TOP_SLOTS];
        let extras = vec![init; n_extras];
        self.add_memory_use_count(
            MemoryUseCountSlot::VectorCells,
            (4 + CHAR_TABLE_TOP_SLOTS + n_extras) as u64,
        );
        let obj = Box::new(CharTableObj {
            header: VecLikeHeader::new(VecLikeType::CharTable),
            defalt: init,
            parent: TaggedValue::NIL,
            purpose,
            ascii: init,
            contents,
            extras: extras.into(),
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe {
            size_of::<CharTableObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(&(*ptr).extras))
        });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped sub-char-table.
    pub fn alloc_sub_char_table(
        &mut self,
        depth: i32,
        min_char: i32,
        contents: Vec<TaggedValue>,
    ) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, contents.len() as u64);
        let obj = Box::new(SubCharTableObj {
            header: VecLikeHeader::new(VecLikeType::SubCharTable),
            depth,
            min_char,
            contents: contents.into(),
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe {
            size_of::<SubCharTableObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(&(*ptr).contents))
        });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a hash table.
    pub fn alloc_hash_table(
        &mut self,
        table: crate::emacs_core::value::LispHashTable,
    ) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, 1);
        let obj = Box::new(HashTableObj {
            header: VecLikeHeader::new(VecLikeType::HashTable),
            table,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::hash_table_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped obarray object.
    pub fn alloc_obarray(&mut self, buckets: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, buckets.len() as u64);
        let obj = Box::new(ObarrayObj {
            header: VecLikeHeader::new(VecLikeType::Obarray),
            buckets: buckets.into(),
            count: 0,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::obarray_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a lambda.
    /// Allocate a lambda (interpreted closure) as a Value vector.
    /// Matches GNU Emacs's PVEC_CLOSURE: all slots are GC-traced Values.
    ///
    /// Allocated from the LAMBDA ARENA PAGES (task 03/3b): one FULL-header
    /// `ptr::write` of the whole `LambdaObj` (a reused slot's stale bytes
    /// never leak — a stale kind would type-confuse the Drop of garbage
    /// `Vec`/`OnceLock` pointers), then the unconditional born-at-parity
    /// store. Page lambdas are OWNED via the page-span oracle
    /// (`lambda_arena.owns`, routed by `owns_veclike_object`): they NEVER
    /// touch `all_objects` / `non_cons_object_addrs` / `link_veclike` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// lambda reclaimer (its `drop_in_place` frees the closure slot `Vec` +
    /// the cached `LambdaParams`). MARKING IS UNCHANGED — the GC thread still
    /// defers every lambda to the STW termination drain (concurrent claiming
    /// is a future task); `mark_value`'s owned veclike arm traces it as before.
    pub fn alloc_lambda(&mut self, slots: Vec<TaggedValue>) -> TaggedValue {
        let ptr = self.lambda_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                LambdaObj {
                    header: VecLikeHeader::new(VecLikeType::Lambda),
                    data: slots.into(),
                    parsed_params: std::sync::OnceLock::new(),
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::lambda_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a lambda from a LambdaData (bridge for migration).
    /// Converts LambdaData fields to the Value vector layout.
    pub fn alloc_lambda_from_data(
        &mut self,
        data: crate::emacs_core::value::LambdaData,
    ) -> TaggedValue {
        let slots = data.to_closure_slots();
        self.alloc_lambda(slots)
    }

    /// Allocate a macro as a Value vector, from the MACRO ARENA PAGES (task
    /// 03/3b — same discipline as `alloc_lambda`, own arena at the shared
    /// 128B stride; `drop_in_place` frees the slot `Vec` + cached params).
    pub fn alloc_macro(&mut self, slots: Vec<TaggedValue>) -> TaggedValue {
        let ptr = self.macro_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                MacroObj {
                    header: VecLikeHeader::new(VecLikeType::Macro),
                    data: slots.into(),
                    parsed_params: std::sync::OnceLock::new(),
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::macro_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a macro from a LambdaData (bridge for migration).
    pub fn alloc_macro_from_data(
        &mut self,
        data: crate::emacs_core::value::LambdaData,
    ) -> TaggedValue {
        let slots = data.to_closure_slots();
        self.alloc_macro(slots)
    }

    /// Allocate a buffer reference.
    pub fn alloc_buffer(&mut self, id: crate::buffer::BufferId) -> TaggedValue {
        let obj = Box::new(BufferObj {
            header: VecLikeHeader::new(VecLikeType::Buffer),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<BufferObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a window reference.
    pub fn alloc_window(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(WindowObj {
            header: VecLikeHeader::new(VecLikeType::Window),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<WindowObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a frame reference.
    pub fn alloc_frame(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(FrameObj {
            header: VecLikeHeader::new(VecLikeType::Frame),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FrameObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a timer reference.
    pub fn alloc_timer(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(TimerObj {
            header: VecLikeHeader::new(VecLikeType::Timer),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<TimerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a process reference.
    pub fn alloc_process(&mut self, id: crate::emacs_core::process::ProcessId) -> TaggedValue {
        let obj = Box::new(ProcessObj {
            header: VecLikeHeader::new(VecLikeType::Process),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<ProcessObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a display terminal object.
    pub fn alloc_terminal(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(TerminalObj {
            header: VecLikeHeader::new(VecLikeType::Terminal),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<TerminalObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an xwidget model object.
    pub fn alloc_xwidget(
        &mut self,
        type_: TaggedValue,
        title: TaggedValue,
        buffer: TaggedValue,
        width: i32,
        height: i32,
        xwidget_id: u32,
        webview_id: neomacs_display_protocol::WebViewId,
    ) -> TaggedValue {
        let obj = Box::new(XwidgetObj {
            header: VecLikeHeader::new(VecLikeType::Xwidget),
            plist: TaggedValue::NIL,
            type_,
            buffer,
            title,
            script_callbacks: TaggedValue::NIL,
            height,
            width,
            xwidget_id,
            webview_id,
            kill_without_query: false,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<XwidgetObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GC-managed shader-surface handle.
    ///
    /// Deliberately NOT registry-rooted (contrast `alloc_finalizer` /
    /// xwidgets' `internal_xwidget_list`): the handle dies when Lisp drops
    /// it, and `free_gc_object` then queues `surface_id` on
    /// `pending_surface_destroys` for the evaluator's post-collection drain.
    pub fn alloc_surface_handle(&mut self, surface_id: u32) -> TaggedValue {
        let obj = Box::new(SurfaceObj {
            header: VecLikeHeader::new(VecLikeType::SurfaceHandle),
            surface_id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SurfaceObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GC-managed video-session handle.
    pub fn alloc_video_handle(
        &mut self,
        video_id: neomacs_display_protocol::VideoId,
    ) -> TaggedValue {
        let obj = Box::new(VideoObj {
            header: VecLikeHeader::new(VecLikeType::VideoHandle),
            video_id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<VideoObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Take the surface ids of handles the sweep reclaimed since the last
    /// drain. The evaluator's cycle-completed block queues a best-effort
    /// `DisplayHost::destroy_shader_surface` for each.
    pub fn take_pending_surface_destroys(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_surface_destroys)
    }

    /// Take video ids whose last Lisp handle was reclaimed by the sweep.
    pub fn take_pending_video_destroys(&mut self) -> Vec<neomacs_display_protocol::VideoId> {
        std::mem::take(&mut self.pending_video_destroys)
    }

    /// Allocate an xwidget view object.
    pub fn alloc_xwidget_view(&mut self, model: TaggedValue, window: TaggedValue) -> TaggedValue {
        let obj = Box::new(XwidgetViewObj {
            header: VecLikeHeader::new(VecLikeType::XwidgetView),
            model,
            window,
            x: 0,
            y: 0,
            clip_right: 0,
            clip_bottom: 0,
            clip_top: 0,
            clip_left: 0,
            redisplayed: false,
            hidden: false,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<XwidgetViewObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a bytecode function from the BYTECODE ARENA PAGES.
    ///
    /// This is the single `VecLikeType::ByteCode` allocation chokepoint —
    /// every producer (`Value::make_bytecode`, the pdump restore placeholder
    /// at `DumpHeapObject::ByteCode`) funnels here. One FULL-header
    /// `ptr::write` of the whole `ByteCodeObj` (fresh `VecLikeHeader` plus
    /// the moved-in `ByteCodeFunction` — a reused slot's stale bytes never
    /// leak: a stale kind is a type-confused Drop of garbage `Vec` pointers),
    /// then the unconditional born-at-parity store (the `link_veclike` seam's
    /// store).
    ///
    /// Page bytecode is OWNED via the page-span oracle
    /// (`bytecode_arena.owns`, routed by `owns_veclike_object`): it NEVER
    /// touches `all_objects` / `non_cons_object_addrs` / `link_veclike` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// bytecode reclaimer (its `drop_in_place` frees the ops/constants
    /// vectors, params, GNU byte maps, and docstring the function owns).
    ///
    /// MARKING (task 01 bytecode arm): the GC thread CLAIMS page bytecode
    /// discovered during a concurrent mark (page-base snapshot hit +
    /// `mark_claim_at`) and gray-pushes its children right there — sound
    /// because published bytecode is immutable (compile-time enforced; see
    /// the claim arm in `concurrent_try_mark_owned`). Snapshot misses
    /// (mid-cycle pages, mapped/dump residue) still defer to the STW
    /// termination drain, where `mark_value`'s owned veclike arm traces
    /// them exactly as before.
    pub fn alloc_bytecode(
        &mut self,
        data: crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> TaggedValue {
        let ptr = self.bytecode_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                ByteCodeObj {
                    header: VecLikeHeader::new(VecLikeType::ByteCode),
                    data,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_veclike`).
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::bytecode_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a record.
    ///
    /// Allocated from the RECORD ARENA PAGES (task 03/3b): the single
    /// `RecordObj` allocation chokepoint alongside `alloc_window_configuration`
    /// (same Rust type, distinct tag — both funnel to `record_arena`). One
    /// FULL-header `ptr::write` (a stale kind would type-confuse the Drop of
    /// the garbage slot `Vec`), unconditional born-at-parity store; NO
    /// intrusive-list / addr-set entry (owned via the page-span oracle,
    /// routed by `owns_veclike_object`). The page sweep's `drop_in_place`
    /// frees the record's slot `Vec`. Marking is unchanged (deferred).
    pub fn alloc_record(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        self.alloc_record_like(VecLikeType::Record, items)
    }

    /// Allocate a native opened-font pseudovector (`PVEC_FONT`).  Fonts retain
    /// typed metrics and an exact backend identity, so they are residual
    /// boxed objects rather than pretending to be record slots.
    pub fn alloc_font(&mut self, data: FontObjectData) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, data.fields.len() as u64);
        let obj = Box::new(FontObj {
            header: VecLikeHeader::new(VecLikeType::Font),
            data,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::font_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a window configuration. Structurally a record (`{header, data}`)
    /// but tagged `WindowConfiguration` so it is a distinct pseudovector type.
    /// Shares the record arena (same `RecordObj`).
    pub fn alloc_window_configuration(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        self.alloc_record_like(VecLikeType::WindowConfiguration, items)
    }

    /// Shared `RecordObj` page allocator for the `Record` and
    /// `WindowConfiguration` tags. `add_memory_use_count` is the caller's job
    /// (both currently count `VectorCells`).
    fn alloc_record_like(&mut self, tag: VecLikeType, items: Vec<TaggedValue>) -> TaggedValue {
        debug_assert!(matches!(
            tag,
            VecLikeType::Record | VecLikeType::WindowConfiguration
        ));
        let ptr = self.record_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                RecordObj {
                    header: VecLikeHeader::new(tag),
                    data: items.into(),
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::record_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an overlay.
    pub fn alloc_overlay(&mut self, data: crate::heap_types::OverlayData) -> TaggedValue {
        let obj = Box::new(OverlayObj {
            header: VecLikeHeader::new(VecLikeType::Overlay),
            data,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<OverlayObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a marker.
    pub fn alloc_marker(&mut self, data: crate::heap_types::LispMarker) -> TaggedValue {
        let obj = Box::new(MarkerObj {
            header: VecLikeHeader::new(VecLikeType::Marker),
            data,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<MarkerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a bignum (arbitrary-precision integer).
    ///
    /// Mirrors GNU `make_bignum` (`src/bignum.c:113`): the caller is
    /// responsible for ensuring the value is outside fixnum range.
    /// Use `Value::make_integer` for the canonical "fixnum-or-bignum"
    /// constructor that delegates here only when promotion is needed.
    pub fn alloc_bignum(&mut self, value: Integer) -> TaggedValue {
        let obj = Box::new(BignumObj {
            header: VecLikeHeader::new(VecLikeType::Bignum),
            value,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<BignumObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a symbol-with-pos object from the SYMBOL-WITH-POS ARENA PAGES
    /// (task 03/3b). `sym` must be a bare symbol, `pos` must be a fixnum.
    ///
    /// POD-like: `SymbolWithPosObj` is two `Copy` Values (no payload, no
    /// Drop), so the class behaves like FloatObj — the sweep/teardown
    /// `drop_in_place` walk compiles out. Still one FULL-header `ptr::write`
    /// (the ownership oracle and every header read demand fully-initialized
    /// slot bytes) + the born-at-parity store; NO intrusive-list / addr-set
    /// entry (owned via the page-span oracle, routed by owns_veclike_object;
    /// free_gc_object's SymbolWithPos arm stays the residual-Box seam).
    pub fn alloc_symbol_with_pos(&mut self, sym: TaggedValue, pos: TaggedValue) -> TaggedValue {
        let ptr = self.symbol_with_pos_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                SymbolWithPosObj {
                    header: VecLikeHeader::new(VecLikeType::SymbolWithPos),
                    sym,
                    pos,
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SymbolWithPosObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a finalizer object (GNU `Fmake_finalizer`). Registered in
    /// `finalizer_registry` so mark termination can detect when the object
    /// becomes unreachable and queue `function` to run after that cycle.
    /// GNU accepts any object as the function; callers do not validate it.
    pub fn alloc_finalizer(&mut self, function: TaggedValue) -> TaggedValue {
        let obj = Box::new(FinalizerObj {
            header: VecLikeHeader::new(VecLikeType::Finalizer),
            function,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.finalizer_registry.push(ptr);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FinalizerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an SQLite database or statement object.
    pub fn alloc_sqlite(&mut self, is_statement: bool, id: i64) -> TaggedValue {
        let obj = Box::new(SqliteObj {
            header: VecLikeHeader::new(VecLikeType::Sqlite),
            is_statement,
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SqliteObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a user-pointer object for dynamic module API.
    pub fn alloc_user_ptr(
        &mut self,
        ptr: *mut std::ffi::c_void,
        finalizer: EmacsFinalizer,
    ) -> TaggedValue {
        let obj = Box::new(UserPtrObj {
            header: VecLikeHeader::new(VecLikeType::UserPtr),
            ptr,
            finalizer,
        });
        let raw = Box::into_raw(obj);
        self.link_veclike(raw as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<UserPtrObj>());
        unsafe { TaggedValue::from_veclike_ptr(raw as *const VecLikeHeader) }
    }

    /// Allocate a module-function object for dynamic module API.
    pub fn alloc_module_function(
        &mut self,
        min_arity: isize,
        max_arity: isize,
        subr: *const std::ffi::c_void,
        data: *mut std::ffi::c_void,
        documentation: TaggedValue,
        interactive_form: TaggedValue,
    ) -> TaggedValue {
        let obj = Box::new(ModuleFunctionObj {
            header: VecLikeHeader::new(VecLikeType::ModuleFunction),
            min_arity,
            max_arity,
            subr,
            data,
            finalizer: None,
            documentation,
            interactive_form,
        });
        let raw = Box::into_raw(obj);
        self.link_veclike(raw as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<ModuleFunctionObj>());
        unsafe { TaggedValue::from_veclike_ptr(raw as *const VecLikeHeader) }
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

    // -----------------------------------------------------------------------
    // Garbage collection — stop-the-world mark-sweep
    // -----------------------------------------------------------------------

    /// Run a full mark-sweep garbage collection.
    ///
    /// `roots` must yield every reachable `TaggedValue`.
    pub fn collect(&mut self, roots: impl Iterator<Item = TaggedValue>) {
        self.collect_exact(roots);
    }

    /// Run a full mark-sweep collection using only the explicit roots provided.
    pub fn collect_exact(&mut self, roots: impl Iterator<Item = TaggedValue>) {
        self.begin_collection();
        for root in roots {
            self.seed_root(root);
        }
        self.complete_collection();
    }

    pub(crate) fn begin_collection(&mut self) {
        // (Pre-mark verification removed — unmarked objects may have stale data
        //  that will be swept. Only post-mark verification is meaningful.)

        // A mark must never start while a deferred sweep is still draining: the
        // sweep reads the mark bits the parity flip below would re-interpret.
        // The driver finishes any in-flight sweep before getting here
        // (`gc_collect_from_current_roots` checks `sweep_in_progress` on every
        // path). HARD assert (not debug): under parity marks, flipping while
        // the detached sweep list is still draining would make every dead
        // object read as marked — the remainder of the sweep would relink
        // garbage as survivors (a leak), and a later cycle could trace through
        // their stale children (worse).
        assert!(
            !self.sweep_in_progress,
            "begin_collection while a deferred sweep is in progress"
        );
        // YOUNG NON-CONS PARITY FLIP (task #7 stage 2b): this single store
        // un-marks the entire young non-cons generation — everything the last
        // cycle marked has bit == old parity, which the new parity reads as
        // unmarked. It replaces the O(objects) `all_objects` pointer-chase
        // clear walk that measured ~98% of the clear phase. The flip lives in
        // `begin_collection` ONLY (`concurrent_begin` delegates here; no other
        // entry point may flip).
        self.mark_parity = !self.mark_parity;

        let clear_t0 = std::time::Instant::now();
        // The first partition cycle runs a NORMAL full collection (so it traces
        // everything and frees load transients); promotion + blackening happen
        // at the end of that cycle (`complete_collection`). Only once
        // `dump_blackened` is set do the partitioned skips apply.
        let partitioned = self.partition_dump && self.dump_blackened;

        // -- Clear marks (heap cons) --
        for block in &mut self.cons_blocks {
            block.clear_marks();
        }
        let clear_cons_done = std::time::Instant::now();
        // -- Mapped (pdump) marks: permanent black region when partitioned --
        if !partitioned {
            for range in &mut self.mapped_cons_ranges {
                range.clear_marks();
            }
            for range in &mut self.mapped_float_ranges {
                range.clear_marks();
            }
            for object in &mut self.mapped_veclike_objects {
                object.marked = false;
            }
            for object in &mut self.mapped_string_objects {
                object.marked = false;
            }
        }
        let clear_mapped_done = std::time::Instant::now();
        // -- YOUNG non-cons (heap) marks: NO WALK. The parity flip at the top
        //    of this fn already un-marked the whole young `all_objects` list
        //    in O(1). The tenured old generation lives on a separate list
        //    (`tenured_objects`) whose frozen bits are never interpreted —
        //    tenured readers short-circuit on the `tenured` flag, so it stays
        //    permanently black. Before the first-cycle promotion every object
        //    is still on `all_objects` with bit == false, and the first flip
        //    (parity false -> true) reads the full preloaded world as
        //    unmarked, so that one cycle traces everything. --

        // Task #7 stage 2a/2b: the clear split (cons bitmap memset / mapped
        // resets / young non-cons segment) sized the parity mark-bit design;
        // the non-cons segment is now the flip (~0), kept as the regression
        // gauge for the removed pointer-chase walk.
        let clear_end = std::time::Instant::now();
        self.last_clear_cons_us = (clear_cons_done - clear_t0).as_micros() as u64;
        self.last_clear_mapped_us = (clear_mapped_done - clear_cons_done).as_micros() as u64;
        self.last_clear_noncons_us = (clear_end - clear_mapped_done).as_micros() as u64;
        self.last_clear_us = (clear_end - clear_t0).as_micros() as u64;

        // -- Seed gray queue from roots --
        self.gray_queue.clear();
        self.marked_symbols.clear();
        self.weak_hash_tables.clear();
        self.weak_hash_tables_set.clear();
        self.mark_cons_block_cache = None;
        // New mark cycle: the per-cycle SATB pre-image dedup set must start empty
        // so each owner's full pre-image is snapshotted once for THIS cycle's
        // start-of-cycle reachability (a carried-over entry would wrongly suppress
        // the snapshot of an owner whose children differ this cycle).
        self.satb_snapshotted_owners.clear();
        // CONCURRENT STRING MARKING: same per-cycle reset for the enforced
        // in-mutator string interval pre-image dedup (`note_string_interval_preimage`).
        self.satb_string_preimage_addrs.clear();
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: the per-cycle clone-on-write dedup
        // set must start empty so each vector owner is cloned+retired at most once
        // per cycle (a carried-over entry would wrongly suppress this cycle's clone).
        self.concurrent_cloned_vectors.clear();
        // OWNER-TRACKING REMEMBERED-SET PRECURSOR (`dirty_owners` /
        // `dirty_owner_bits` / `dirty_writes`): clear it HERE, at the START of the
        // cycle, on the same per-cycle lifecycle as the SATB dedup sets above —
        // NOT at end-of-collection. A carried-over entry is not merely wasteful;
        // it is an ABA hazard. An owner address recorded before this cycle can be
        // FREED by this cycle's sweep and its slot handed to a NEW same-class
        // object by the arena; the stale `dirty_owner_bits` entry would then dedup
        // (suppress) the new object's barriered write — a missed remembered-write.
        // Clearing at begin makes the tables hold only writes made SINCE this
        // cycle started, so no entry outlives the object it names into a
        // sweep+reuse. This is the exact ABA-safety argument the SATB sets rely
        // on (per-cycle; no free during mark; cleared at begin). The tables are
        // the seam for the future generational remembered set (task 06), whose
        // consumer walks them per cycle and needs no cross-cycle accumulation;
        // every reader today is a test.
        self.clear_dirty_owners();
        self.clear_dirty_writes();
        self.seed_internal_runtime_roots();
        if partitioned {
            // Re-scan dumped/tenured objects mutated to point at young heap
            // objects: those children must be kept live even though the dump and
            // the tenured old generation are black.
            self.seed_mapped_remembered();
        } else if self.partition_dump {
            if self.first_cycle_concurrent {
                // Concurrent first cycle: string intervals seed here
                // (handshake); veclike headers and cons ranges are STAGED
                // for the GC thread (`launch_concurrent_mark` moves them
                // into the job).
                self.seed_mapped_string_children();
                self.staged_mapped_veclikes = Some(
                    self.mapped_veclike_objects
                        .iter()
                        .map(|o| o.header as usize)
                        .collect(),
                );
                self.staged_mapped_cons_scan = Some(
                    self.mapped_cons_ranges
                        .iter()
                        .map(|range| (range.start as usize, range.len))
                        .collect(),
                );
            } else {
                // First partition cycle, STW (explicit garbage-collect /
                // dump-less bootstrap): keep every dump-referenced heap
                // object alive so none is swept and left dangling when the
                // image is blackened at the end of this cycle.
                self.seed_all_mapped_children();
            }
        }
    }

    /// Run once at the END of the first partition cycle (after a full
    /// trace+sweep): promote every survivor to the tenured old generation,
    /// blacken the mapped dump image, and build the initial remembered set.
    /// Thereafter both regions are permanently black and skipped each cycle.
    fn promote_and_blacken(&mut self) {
        // 1. Promote every surviving heap object to tenured (old generation).
        //    The first partition cycle ran a full trace+sweep, so everything
        //    still in `all_objects` is alive = a permanent (the preloaded world
        //    plus whatever the session has retained). They are already marked;
        //    setting `tenured` FREEZES that bit — no later parity flip may be
        //    interpreted against it (every tenured reader short-circuits on
        //    the flag), so these objects are permanently black without ever
        //    being re-touched.
        //    Move the whole young list onto the tenured list and flag each
        //    node so the nursery (`all_objects`) starts empty; from now on only
        //    post-loadup allocations land there and get cleared/swept.
        let mut tail: *mut GcHeader = std::ptr::null_mut();
        let mut obj = self.all_objects;
        while !obj.is_null() {
            unsafe {
                (*obj).tenured = true;
                // A weak hash table being tenured becomes permanent-black and the
                // main mark will never re-touch it; record it so the weak sweep
                // keeps re-evaluating its entries every GC (GNU sweeps every weak
                // table every GC). See `permanent_weak_hash_tables`.
                if (*obj).kind == HeapObjectKind::VecLike {
                    let vptr = obj as *mut VecLikeHeader;
                    if (*vptr).type_tag == VecLikeType::HashTable {
                        let ht_ptr = vptr as *mut HashTableObj;
                        if (*ht_ptr).table.weakness.is_some()
                            && !self.permanent_weak_hash_tables_set.contains(&ht_ptr)
                        {
                            self.permanent_weak_hash_tables_set.insert(ht_ptr);
                            self.permanent_weak_hash_tables.push(ht_ptr);
                        }
                    }
                }
                tail = obj;
                obj = (*obj).next;
            }
        }
        if !tail.is_null() {
            // Splice: [all_objects .. tail] -> front of tenured_objects.
            unsafe {
                (*tail).next = self.tenured_objects;
            }
            self.tenured_objects = self.all_objects;
            self.all_objects = std::ptr::null_mut();
        }
        // 1b. PROMOTION PAGE WALK (stage 3): page objects are on no intrusive
        //     list, so the splice above cannot tenure them — without this
        //     walk the (loadup-sized) paged survivor set would stay young and
        //     be re-seeded + re-traced + re-swept every cycle, defeating
        //     tenuring. Flip `header.tenured` on every ALLOCATED slot (the
        //     sweep has already run, so allocated ≡ survivor); the per-object
        //     header REMAINS the sole mark-path authority — no page-level
        //     flag is consulted by any mark path. No weak-table registration
        //     is needed here (the paged classes are Float/String/Vector;
        //     hash tables stay Box and ride the splice above). Then RETIRE
        //     full pages: a page whose every slot is allocated (hence, after
        //     this walk, tenured) can never free a slot again — the sweep
        //     skips it whole, the allocator never touches it, and it is
        //     freed only at heap teardown, while STAYING in the page-base
        //     registry so the ownership oracle keeps answering "owned"
        //     (`value_is_tenured` gates on ownership — see C1 on the arena
        //     doc). The criterion is deliberately occupancy==SLOTS, NOT "all
        //     allocated slots tenured": right after this walk EVERY page
        //     trivially satisfies the latter, which would retire
        //     nearly-empty pages and strand their free slots forever.
        //     Partial pages stay in rotation as MIXED pages — every later
        //     sweep re-skips their tenured slots, a perpetual per-slot
        //     branch bounded by the one-time loadup survivor set (the only
        //     population this one-shot promotion ever tenures).
        self.promote_arena_pages_and_retire_full();
        // 2. Blacken the mapped image.
        for range in &mut self.mapped_cons_ranges {
            range.mark_all();
        }
        for range in &mut self.mapped_float_ranges {
            range.mark_all();
        }
        for object in &mut self.mapped_veclike_objects {
            object.marked = true;
        }
        for object in &mut self.mapped_string_objects {
            object.marked = true;
        }
        // Mapped (pdump) weak hash tables become permanent-black here too (the
        // preloaded image ships several, e.g. `print-number-table` helpers and
        // internal caches). Like tenured weak tables, they would never be
        // re-traced and their entries would be pinned forever; register them so
        // `mark_and_sweep_weak_tables` re-evaluates them every GC.
        let mapped_weak: Vec<*mut HashTableObj> = self
            .mapped_veclike_objects
            .iter()
            .filter_map(|object| {
                let header = object.header;
                // SAFETY: `header` is a live mapped veclike for the dump's lifetime.
                unsafe {
                    if (*header).type_tag == VecLikeType::HashTable {
                        let ht_ptr = header as *mut HashTableObj;
                        if (*ht_ptr).table.weakness.is_some() {
                            return Some(ht_ptr);
                        }
                    }
                }
                None
            })
            .collect();
        for ht_ptr in mapped_weak {
            if self.permanent_weak_hash_tables_set.insert(ht_ptr) {
                self.permanent_weak_hash_tables.push(ht_ptr);
            }
        }
        // 3. Remember permanents (mapped or tenured) that point at a YOUNG
        //    heap object so its children stay live. After promotion (list
        //    splice + page walk) the only young heap objects are heap CONSES
        //    (header-less, cannot be tenured), so this scan retains exactly
        //    the permanent→cons edges. It covers page-tenured owners too —
        //    see the page walk inside `scan_permanents_for_young_children`.
        self.scan_permanents_for_young_children();
    }

    /// Stage-3 promotion page walk + retirement (see the call site in
    /// `promote_and_blacken` for the full rationale). One-shot: runs only at
    /// the single promotion of the first partition cycle.
    fn promote_arena_pages_and_retire_full(&mut self) {
        fn walk_one<T: PagedObject>(arena: &mut ObjectArena<T>) {
            for page in &mut arena.pages {
                for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                    let mut bits = page.alloc_bits[word_index];
                    while bits != 0 {
                        let bit = bits.trailing_zeros() as usize;
                        bits &= bits - 1;
                        let index = word_index * usize::BITS as usize + bit;
                        let slot = page.slot_ptr(index);
                        // Allocated ⇒ survivor of the just-completed sweep ⇒
                        // a permanent. Plain store: promotion is STW.
                        unsafe { (*(slot as *mut GcHeader)).tenured = true };
                    }
                }
                // RETIREMENT: FULL pages only (occupancy == slots, which
                // after the flip above implies all-tenured). A full page has
                // an empty free list and is off the partial chain by
                // construction.
                if page.allocated == ObjectPage::<T>::SLOTS {
                    debug_assert!(!page.on_partial, "full page on the partial chain");
                    debug_assert_eq!(page.free_head, PAGE_NONE);
                    page.retired = true;
                }
            }
        }
        walk_one(&mut self.float_arena);
        walk_one(&mut self.string_arena);
        walk_one(&mut self.vector_arena);
        // Bytecode is loadup-heavy: most of the population tenures at this
        // one-time promotion, so FULL-page retirement fires for real here
        // (unlike floats) — retired pages stay registered/owned (C1).
        walk_one(&mut self.bytecode_arena);
        // Lambdas/macros likewise tenure at the loadup promotion (interpreted
        // closures are loadup-heavy); their arenas retire full pages too.
        walk_one(&mut self.lambda_arena);
        walk_one(&mut self.macro_arena);
        walk_one(&mut self.record_arena);
        walk_one(&mut self.symbol_with_pos_arena);
    }

    /// Scan every permanent object (mapped dump + tenured old gen) for edges to
    /// young heap objects and add such permanents to the remembered set. Used
    /// at promotion and re-buildable on demand; the result is seeded each cycle.
    fn scan_permanents_for_young_children(&mut self) {
        // -- mapped vectorlike --
        let veclike: Vec<*mut VecLikeHeader> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| o.header)
            .collect();
        for ptr in veclike {
            if self
                .collect_veclike_children(ptr)
                .iter()
                .any(|c| self.is_heap_young(*c))
            {
                let value = unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) };
                self.mapped_remembered.insert(value.bits());
            }
        }
        // -- mapped conses --
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                let car = unsafe { (*cell).load_car() };
                let cdr = unsafe { (*cell).load_cdr() };
                if self.is_heap_young(car) || self.is_heap_young(cdr) {
                    let value = unsafe { TaggedValue::from_cons_ptr(cell) };
                    self.mapped_remembered.insert(value.bits());
                }
            }
        }
        // -- mapped strings (text-prop intervals) --
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            if roots.iter().any(|r| self.is_heap_young(*r)) {
                let value = unsafe { TaggedValue::from_string_ptr(ptr) };
                self.mapped_remembered.insert(value.bits());
            }
        }
        // -- tenured heap objects (old generation) --
        let tenured: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let mut obj = self.tenured_objects;
            while !obj.is_null() {
                unsafe {
                    out.push(obj);
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in tenured {
            self.remember_tenured_owner_if_young_children(header);
        }
        // -- PAGE-TENURED objects (stage 3): the arenas' tenured slots are on
        //    no intrusive list, so the walk above never sees them. They MUST
        //    be scanned here: heap CONSES never tenure, so a page-tenured
        //    vector whose element (or string whose interval plist) references
        //    a cons has a YOUNG child RIGHT AT promotion — the design note
        //    "page-tenured objects have no young children at promotion" is
        //    false for exactly this cons-child case, and skipping the walk
        //    would sweep such a cons on the next cycle while its
        //    permanently-black owner (never re-traced) still points at it: a
        //    UAF (regression-tested by
        //    tenured_page_vector_keeps_young_cons_child_alive). ONGOING
        //    (post-promotion) edges are separately caught by the write
        //    barrier: `value_is_tenured` answers through the page-span
        //    oracle — retired pages included — so `record_heap_write` keeps
        //    remembering mutated page-tenured owners. --
        for header in self.collect_tenured_page_slot_headers() {
            self.remember_tenured_owner_if_young_children(header);
        }
    }

    /// Insert a tenured (list or page) owner into the dump remembered set if
    /// any of its direct heap children is YOUNG. Floats have no children.
    fn remember_tenured_owner_if_young_children(&mut self, header: *mut GcHeader) {
        let kind = unsafe { (*header).kind };
        let has_young = match kind {
            HeapObjectKind::VecLike | HeapObjectKind::String => self
                .heap_object_children(header)
                .iter()
                .any(|c| self.is_heap_young(*c)),
            HeapObjectKind::Float => false,
        };
        if has_young {
            let value = match kind {
                HeapObjectKind::VecLike => unsafe {
                    TaggedValue::from_veclike_ptr(header as *const VecLikeHeader)
                },
                HeapObjectKind::String => unsafe {
                    TaggedValue::from_string_ptr(header as *mut StringObj)
                },
                HeapObjectKind::Float => return,
            };
            self.mapped_remembered.insert(value.bits());
        }
    }

    /// True if `value` is a YOUNG heap object: a real heap allocation that is
    /// neither mapped (dump) nor tenured (old gen) — i.e. it participates in the
    /// normal clear/mark/sweep each cycle. Heap cons cells are always young
    /// (header-less, cannot be tenured).
    fn is_heap_young(&self, value: TaggedValue) -> bool {
        if !value.is_heap_object() || self.owner_is_mapped(value) {
            return false;
        }
        if value.is_cons() {
            return true; // heap cons: header-less, cannot be tenured
        }
        // Non-cons: young iff heap-OWNED and not tenured. Static/untracked
        // objects (e.g. Subrs) are permanently live, never young.
        match Self::value_heap_addr(value) {
            Some(addr) => {
                self.owns_heap_value_object(value, addr)
                    && !unsafe { (*(addr as *const GcHeader)).tenured }
            }
            None => false,
        }
    }

    /// True if `value` is a tenured (old-gen) heap non-cons object.
    ///
    /// Gates on OWNERSHIP first, so the page-span oracle must keep answering
    /// "owned" for RETIRED pages (C1): a retired-page tenured object that
    /// answered not-owned here would read as neither mapped nor tenured, the
    /// write barrier (`record_heap_write`) would skip its first
    /// post-retirement tenured→young edge, the child would never be re-seeded
    /// (`seed_mapped_remembered`) and would be swept while live.
    fn value_is_tenured(&self, value: TaggedValue) -> bool {
        if value.is_cons() {
            return false;
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return false;
        };
        if !self.owns_heap_value_object(value, addr) {
            return false; // mapped, not a tenured heap object
        }
        unsafe { (*(addr as *const GcHeader)).tenured }
    }

    /// First-cycle only: seed the heap children of EVERY mapped object so they
    /// survive the cycle's sweep. Dumped objects are never freed, so a heap
    /// object referenced only by an (otherwise unreachable) dumped object must
    /// still be kept — otherwise it would be swept and the dumped object would
    /// be left holding a dangling pointer once the image is blackened.
    fn seed_all_mapped_children(&mut self) {
        self.seed_mapped_veclike_and_string_children();
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                let car = unsafe { (*cell).load_car() };
                let cdr = unsafe { (*cell).load_cdr() };
                self.mark_or_push_child(car, "first-cycle-mapped-cons-car");
                self.mark_or_push_child(cdr, "first-cycle-mapped-cons-cdr");
            }
        }
    }

    /// The veclike + string-interval half of [`Self::seed_all_mapped_children`]
    /// (STW path).
    fn seed_mapped_veclike_and_string_children(&mut self) {
        let veclike: Vec<*mut VecLikeHeader> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| o.header)
            .collect();
        for ptr in veclike {
            unsafe { self.trace_veclike(ptr) };
        }
        self.seed_mapped_string_children();
    }

    /// String-interval children only: interval trees carry no concurrent-read
    /// guarantee, so the concurrent first cycle keeps THIS part in the start
    /// handshake while staging veclikes (atomic-read slots) and cons ranges
    /// for the GC thread.
    fn seed_mapped_string_children(&mut self) {
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            for root in roots {
                self.mark_or_push_child(root, "first-cycle-mapped-string-interval");
            }
        }
    }

    /// Seed the gray queue with the heap children of every dumped object that
    /// has been mutated since load (the dump remembered set). Because the dump
    /// is black, `mark_value` would otherwise never re-trace these, so we
    /// enqueue their children directly. Mapped children are already black and
    /// are skipped when popped; only heap children get marked.
    fn seed_mapped_remembered(&mut self) {
        // Handshake instrumentation: owners re-scanned + wall cost, routed to
        // the start/termination slot by the caller. The remembered set is
        // append-only (never cleared), so this count is the monotonic-growth
        // probe as well.
        let seed_t0 = std::time::Instant::now();
        self.last_remembered_seed_roots = self.mapped_remembered.len();
        self.handshake.probe_mapped_remembered = self.mapped_remembered.len();
        if self.mapped_remembered.is_empty() {
            self.last_remembered_seed_us = 0;
            return;
        }
        let owners: Vec<TaggedValue> = self
            .mapped_remembered
            .iter()
            .map(|&bits| TaggedValue(bits))
            .collect();
        for owner in owners {
            self.push_value_children_to_gray(owner, "remembered-dump-child");
        }
        self.last_remembered_seed_us = seed_t0.elapsed().as_micros() as u64;
    }

    /// Push every heap child of `owner` onto the gray queue (re-trace its
    /// outgoing references). Unlike `mark_value`, this does NOT consult the
    /// owner's own mark bit, so it re-examines an already-black owner's slots —
    /// exactly what the incremental-update barrier and the dump remembered set
    /// both need. Mirrors `trace_veclike`/cons/string child enumeration.
    fn push_value_children_to_gray(&mut self, owner: TaggedValue, origin: &'static str) {
        if owner.is_cons() {
            let ptr = owner.xcons_ptr();
            let car = unsafe { (*ptr).load_car() };
            let cdr = unsafe { (*ptr).load_cdr() };
            self.mark_or_push_child(car, origin);
            self.mark_or_push_child(cdr, origin);
        } else if owner.is_veclike() {
            if let Some(ptr) = owner.as_veclike_ptr() {
                // A dumped/tenured WEAK hash table is permanent-black, so the main
                // mark never re-runs `trace_veclike` on it and it would otherwise
                // never re-register for the weak sweep. Register it here (the
                // remembered-set / SATB / permanent scan is the ONLY path that
                // reaches such a table) and push only its NON-weak children
                // (custom test/hash closures) strongly. Its weak keys/values are
                // deliberately NOT traced here — `mark_and_sweep_weak_tables`
                // (which runs at every mark termination, before
                // `verify_dump_partition`) decides per-entry survival against the
                // current marks and physically removes the dead entries, so the
                // verifier never sees an unmarked weak child. This mirrors GNU's
                // `mark_object` PVEC_HASH_TABLE (alloc.c): weak tables register
                // themselves and do NOT mark their contents.
                if let Some(weak_children) = self.register_weak_hash_table_for_sweep(ptr) {
                    for child in weak_children {
                        self.mark_or_push_child(child, origin);
                    }
                } else {
                    // STRONG enumeration for every other veclike (and non-weak
                    // hash tables): the remembered-set / SATB paths and the
                    // dump-partition verifier require every heap child of a
                    // permanent owner to be marked, or it is swept while still
                    // referenced (UAF).
                    for child in self.collect_veclike_children(ptr as *mut VecLikeHeader) {
                        self.mark_or_push_child(child, origin);
                    }
                }
            }
        } else if owner.is_string()
            && let Some(ptr) = owner.as_string_ptr()
        {
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| {
                    self.mark_or_push_child(root, origin);
                });
            }
        }
        // Floats have no heap children.
    }

    /// Is `value` currently marked? Covers heap and mapped objects of every
    /// category. Used only by the dump-partition verifier.
    fn is_value_marked(&self, value: TaggedValue) -> bool {
        if let crate::tagged::value::ValueKind::Symbol(id) = value.kind() {
            return crate::emacs_core::intern::is_canonical_id(id)
                || self.marked_symbols.contains(id);
        }
        if value.is_cons() {
            let ptr = value.xcons_ptr();
            if ConsBlock::ptr_is_cell_aligned(ptr) {
                let base = ConsBlock::block_base_for_ptr(ptr);
                if let Some(&idx) = self.cons_block_index_by_base.get(&base) {
                    return self.cons_blocks[idx].is_marked_ptr(ptr);
                }
            }
            return self
                .mapped_cons_ranges
                .iter()
                .find(|range| range.contains_ptr(ptr))
                .map(|range| range.is_marked_ptr(ptr))
                .unwrap_or(false);
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return true;
        };
        if self.owns_heap_value_object(value, addr) {
            // Heap-owned non-cons: every such object starts with a `GcHeader`
            // (`StringObj`/`FloatObj` headers and `VecLikeHeader.gc` are all
            // at offset 0). TENURED SHORT-CIRCUIT before the bit read:
            // `promote_and_blacken` never removes tenured objects from
            // `non_cons_object_addrs`, and a tenured bit froze at promotion,
            // so interpreting it against the current parity would read
            // "unmarked" on every other cycle — spurious partition/tricolor
            // verifier panics and needless old-gen concern. Tenured ≡ marked.
            let header = addr as *const GcHeader;
            if unsafe { (*header).tenured } {
                return true;
            }
            return unsafe { (*header).is_marked_at(self.mark_parity) };
        }
        // A non-cons object that is neither heap-owned nor mapped is a static,
        // never-swept runtime object (e.g. a `Subr`) — permanently live, so
        // treat it as marked (`unwrap_or(true)`). This relies on the dump
        // partition keeping every dump-referenced heap object live, so a
        // not-owned/not-mapped pointer is never a dangling reference.
        if value.is_string() {
            return self
                .mapped_string_index_by_addr
                .get(&addr)
                .map(|&i| self.mapped_string_objects[i].marked)
                .unwrap_or(true);
        }
        if value.is_float() {
            let ptr = addr as *const FloatObj;
            return self
                .mapped_float_ranges
                .iter()
                .find(|range| range.contains_ptr(ptr))
                .map(|range| range.is_marked_ptr(ptr))
                .unwrap_or(true);
        }
        if value.is_veclike() {
            return self
                .mapped_veclike_index_by_addr
                .get(&addr)
                .map(|&i| self.mapped_veclike_objects[i].marked)
                .unwrap_or(true);
        }
        true
    }

    /// Verification gate for the dump partition (env `NEOVM_GC_VERIFY_PARTITION`).
    /// After the partitioned mark, every direct heap child of every dumped
    /// object MUST already be marked — otherwise the write barrier missed a
    /// dumped→heap mutation and the partition is about to free a live object.
    /// Panics on the first violation. Expensive (full dump scan); verification
    /// runs only.
    fn verify_dump_partition(&mut self) {
        let mut violations: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut sample: Option<usize> = None;
        let mut record = |owner: &str, child: TaggedValue| {
            let child_kind = if child.is_cons() {
                "cons".to_string()
            } else if child.is_string() {
                "string".to_string()
            } else if child.is_float() {
                "float".to_string()
            } else if child.is_veclike() {
                format!("{:?}", child.veclike_type())
            } else {
                "other".to_string()
            };
            *violations
                .entry(format!("{owner} -> {child_kind}"))
                .or_insert(0) += 1;
            sample.get_or_insert(child.0);
        };

        // Mapped veclike objects (char-tables etc.), grouped by owner type.
        let veclike: Vec<(*mut VecLikeHeader, VecLikeType)> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| (o.header, unsafe { (*o.header).type_tag }))
            .collect();
        for (ptr, ty) in veclike {
            let owner = format!("{ty:?}");
            for child in self.collect_veclike_children(ptr) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }
        // Mapped conses.
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                for child in [unsafe { (*cell).load_car() }, unsafe { (*cell).load_cdr() }] {
                    if child.is_heap_object() && !self.is_value_marked(child) {
                        record("Cons", child);
                    }
                }
            }
        }
        // Mapped strings (text-property intervals).
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            for child in roots {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record("String", child);
                }
            }
        }
        // Tenured heap objects (old generation): their direct heap children
        // must also be marked, or a survival-promoted permanent mutated to
        // point at a young object would free it.
        let tenured: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let mut obj = self.tenured_objects;
            while !obj.is_null() {
                unsafe {
                    out.push(obj);
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in tenured {
            let kind = unsafe { (*header).kind };
            let owner = format!("tenured:{kind:?}");
            let children: Vec<TaggedValue> = self.heap_object_children(header);
            for child in children {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }
        // TENURED PAGE SLOTS (stage 3): page-tenured strings/vectors are on
        // NO intrusive list — without this walk they would be INVISIBLE to
        // this detector and a missed tenured→young barrier edge on them
        // would pass verification straight into a UAF. Allocated-bit-first;
        // clear-bit slot bytes are garbage.
        for header in self.collect_tenured_page_slot_headers() {
            let kind = unsafe { (*header).kind };
            let owner = format!("tenured-page:{kind:?}");
            for child in self.heap_object_children(header) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }

        if !violations.is_empty() {
            let total: usize = violations.values().sum();
            eprintln!("DUMP_PARTITION_VIOLATIONS total={total}");
            for (k, n) in &violations {
                eprintln!("  {n:>6}  {k}");
            }
            panic!(
                "dump-partition verification: {total} unmarked heap children of mapped objects \
                 (sample value={:#x}) — write barrier missed dumped->heap mutations (UAF risk). \
                 See DUMP_PARTITION_VIOLATIONS above.",
                sample.unwrap_or(0)
            );
        }
    }

    /// Verification gate for incremental marking (env `NEOVM_GC_VERIFY_PARTITION`,
    /// incremental builds). Complements `verify_dump_partition`, which covers
    /// mapped + tenured owners: this checks the remaining black objects —
    /// YOUNG non-cons (`all_objects`) and every marked heap CONS — for the
    /// strong tri-color invariant (no black object points to a white object).
    /// A violation means the incremental-update barrier missed a black->white
    /// edge created by the mutator during marking (a UAF about to happen).
    /// Panics on the first batch of violations. Expensive; verification only.
    fn verify_incremental_tricolor(&mut self) {
        let mut violations: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut sample: Option<usize> = None;

        // -- Young non-cons objects that are marked (black). `all_objects` is
        //    young-only (tenured objects live on `tenured_objects`), so the
        //    parity interpretation applies to every node. --
        let young: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let parity = self.mark_parity;
            let mut obj = self.all_objects;
            while !obj.is_null() {
                unsafe {
                    if (*obj).is_marked_at(parity) {
                        out.push(obj);
                    }
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in young {
            let kind = unsafe { (*header).kind };
            let children: Vec<TaggedValue> = self.heap_object_children(header);
            let owner = format!("young:{kind:?}");
            for child in children {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    *violations.entry(owner.clone()).or_insert(0) += 1;
                    sample.get_or_insert(child.0);
                }
            }
        }

        // -- YOUNG PAGE SLOTS that are marked (black), stage 3: page
        //    strings/vectors/floats are on NO intrusive list — without this
        //    walk a black page string/vector pointing at a white object would
        //    be INVISIBLE to this detector (a live hole in the black→white
        //    scan). Allocated-bit-first; tenured slots are covered by
        //    `verify_dump_partition`'s tenured-page walk. --
        for header in self.collect_young_marked_page_slot_headers() {
            let kind = unsafe { (*header).kind };
            let owner = format!("young-page:{kind:?}");
            for child in self.heap_object_children(header) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    *violations.entry(owner.clone()).or_insert(0) += 1;
                    sample.get_or_insert(child.0);
                }
            }
        }

        // -- Every marked heap cons cell: car/cdr must be marked. --
        let blocks: Vec<(*mut ConsCell, usize)> = self
            .cons_blocks
            .iter()
            .map(|b| (b.cells_ptr(), b.next_index as usize))
            .collect();
        for (cells, count) in blocks {
            for i in 0..count {
                let cell = unsafe { cells.add(i) };
                if !self.is_value_marked(unsafe { TaggedValue::from_cons_ptr(cell) }) {
                    continue;
                }
                for child in [unsafe { (*cell).load_car() }, unsafe { (*cell).load_cdr() }] {
                    if child.is_heap_object() && !self.is_value_marked(child) {
                        *violations.entry("young:Cons".to_string()).or_insert(0) += 1;
                        sample.get_or_insert(child.0);
                    }
                }
            }
        }

        if !violations.is_empty() {
            let total: usize = violations.values().sum();
            eprintln!("INCREMENTAL_TRICOLOR_VIOLATIONS total={total}");
            for (k, n) in &violations {
                eprintln!("  {n:>6}  {k}");
            }
            panic!(
                "incremental tri-color verification: {total} black->white edges \
                 (sample value={:#x}) — the incremental-update barrier missed a mutation \
                 (UAF risk). See INCREMENTAL_TRICOLOR_VIOLATIONS above.",
                sample.unwrap_or(0)
            );
        }
    }

    /// Direct heap children of any owned non-cons object by header, for the
    /// verifiers and the promotion-time permanents scan: veclike slots,
    /// string text-property interval roots, floats none.
    fn heap_object_children(&self, header: *mut GcHeader) -> Vec<TaggedValue> {
        match unsafe { (*header).kind } {
            HeapObjectKind::VecLike => self.collect_veclike_children(header as *mut VecLikeHeader),
            HeapObjectKind::String => {
                let mut roots = Vec::new();
                let intervals = unsafe { (*(header as *const StringObj)).data.intervals() };
                if !intervals.is_empty() {
                    intervals.for_each_root(|root| roots.push(root));
                }
                roots
            }
            HeapObjectKind::Float => Vec::new(),
        }
    }

    /// Every allocated page slot (all class arenas) whose header is
    /// TENURED, as `GcHeader` pointers. Allocated-bit-first walk.
    fn collect_tenured_page_slot_headers(&self) -> Vec<*mut GcHeader> {
        let mut out: Vec<*mut GcHeader> = Vec::new();
        let mut push = |header: *mut GcHeader| {
            if unsafe { (*header).tenured } {
                out.push(header);
            }
        };
        for slot in self.float_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.string_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.vector_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.bytecode_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.lambda_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.record_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        out
    }

    /// Every allocated page slot (all class arenas) that is YOUNG and
    /// MARKED at the current parity (black), as `GcHeader` pointers.
    fn collect_young_marked_page_slot_headers(&self) -> Vec<*mut GcHeader> {
        let parity = self.mark_parity;
        let mut out: Vec<*mut GcHeader> = Vec::new();
        let mut push = |header: *mut GcHeader| unsafe {
            if !(*header).tenured && (*header).is_marked_at(parity) {
                out.push(header);
            }
        };
        for slot in self.float_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.string_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.vector_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.bytecode_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.lambda_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.record_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        out
    }

    /// If `ptr` is a WEAK hash table, register it for this cycle's weak sweep
    /// (deduplicated) and return its NON-weak children — the custom test/hash
    /// closures from `define-hash-table-test`, which must be traced strongly so
    /// they outlive the table. Returns `None` for non-weak tables and every
    /// other veclike, signalling the caller to fall back to the normal strong
    /// child enumeration.
    ///
    /// This is the bridge that lets a dumped/tenured weak table — which the main
    /// mark never re-touches because it is permanent-black — still be swept every
    /// full collection, matching GNU (whose non-generational mark re-encounters
    /// every live weak table every GC and rebuilds `weak_hash_tables`).
    fn register_weak_hash_table_for_sweep(
        &mut self,
        ptr: *const VecLikeHeader,
    ) -> Option<Vec<TaggedValue>> {
        let ht_ptr = ptr as *mut HashTableObj;
        // SAFETY: caller verified `ptr` is a live veclike; the heap is owned
        // exclusively during marking. Reading the immutable weakness / closure
        // fields is race-free.
        let (is_weak, user_cmp, user_hash) = unsafe {
            if (*ptr).type_tag != VecLikeType::HashTable {
                return None;
            }
            let ht = &(*ht_ptr).table;
            (
                ht.weakness.is_some(),
                ht.user_cmp_function,
                ht.user_hash_function,
            )
        };
        if !is_weak {
            return None;
        }
        if self.weak_hash_tables_set.insert(ht_ptr) {
            self.weak_hash_tables.push(ht_ptr);
        }
        let mut nonweak = Vec::new();
        if let Some(f) = user_cmp {
            nonweak.push(f);
        }
        if let Some(f) = user_hash {
            nonweak.push(f);
        }
        Some(nonweak)
    }

    /// Direct children of a mapped vectorlike object (read-only) for the verifier.
    fn collect_veclike_children(&self, ptr: *mut VecLikeHeader) -> Vec<TaggedValue> {
        let mut out = Vec::new();
        unsafe {
            match (*ptr).type_tag {
                VecLikeType::Vector => {
                    out.extend((*(ptr as *const VectorObj)).data.iter().copied());
                }
                VecLikeType::Record | VecLikeType::WindowConfiguration => {
                    out.extend((*(ptr as *const RecordObj)).data.iter().copied());
                }
                VecLikeType::Font => {
                    let font = &(*(ptr as *const FontObj)).data;
                    out.extend(font.fields.iter().copied());
                    out.push(font.capability);
                }
                VecLikeType::CharTable => {
                    let o = &*(ptr as *const CharTableObj);
                    out.extend([o.defalt, o.parent, o.purpose, o.ascii]);
                    out.extend(o.contents.iter().copied());
                    out.extend(o.extras.iter().copied());
                }
                VecLikeType::SubCharTable => {
                    out.extend((*(ptr as *const SubCharTableObj)).contents.iter().copied());
                }
                VecLikeType::Obarray => {
                    out.extend((*(ptr as *const ObarrayObj)).buckets.iter().copied());
                }
                VecLikeType::Lambda | VecLikeType::Macro => {
                    out.extend((*(ptr as *const LambdaObj)).data.iter().copied());
                }
                VecLikeType::HashTable => {
                    let ht = &(*(ptr as *const HashTableObj)).table;
                    if let Some(pending) = ht.data.pending_entries() {
                        // Un-hydrated dump table (see `trace_veclike`).
                        for (_, value, snapshot) in pending {
                            out.push(*value);
                            if let Some(snapshot) = snapshot {
                                out.push(*snapshot);
                            }
                        }
                    }
                    out.extend(ht.data.values().copied());
                    out.extend(ht.key_snapshots().copied());
                    // Custom test/hash closures (`define-hash-table-test`) live
                    // ONLY in these fields and are traced by `trace_veclike`; keep
                    // the two enumerations in sync so the remembered/SATB strong-
                    // trace (which uses this) and the dump-partition verifier both
                    // cover them — otherwise a dumped/tenured custom-test table's
                    // closures are swept while the table still calls them (UAF).
                    if let Some(f) = ht.user_cmp_function {
                        out.push(f);
                    }
                    if let Some(f) = ht.user_hash_function {
                        out.push(f);
                    }
                }
                VecLikeType::ByteCode => {
                    let obj = ptr as *const ByteCodeObj;
                    let data = &(*obj).data;
                    // LAZY STUB LEG — keep in lockstep with the marking arm
                    // below: a stub's vectors are empty, its children live
                    // only in the PATCHED image regions. Walk those without
                    // materializing or allocating (GC context). On a stub,
                    // closure_slot_count carries the extras length.
                    if data.is_pdump_stub() {
                        crate::emacs_core::pdump::mapped_heap::for_each_stub_bytecode_child(
                            obj,
                            data.closure_slot_count,
                            |child| out.push(child),
                        );
                        return out;
                    }
                    out.push(data.arglist);
                    out.extend(data.constants.iter().copied());
                    if let Some(env) = data.env {
                        out.push(env);
                    }
                    if let Some(doc_form) = data.doc_form {
                        out.push(doc_form);
                    }
                    if let Some(interactive) = data.interactive {
                        out.push(interactive);
                    }
                    out.extend(data.extra_slots.iter().copied());
                }
                VecLikeType::Overlay => {
                    out.push((*(ptr as *const OverlayObj)).data.plist);
                }
                VecLikeType::SymbolWithPos => {
                    let o = &*(ptr as *const SymbolWithPosObj);
                    out.extend([o.sym, o.pos]);
                }
                VecLikeType::Finalizer => {
                    out.push((*(ptr as *const FinalizerObj)).function);
                }
                VecLikeType::ModuleFunction => {
                    let o = &*(ptr as *const ModuleFunctionObj);
                    out.extend([o.documentation, o.interactive_form]);
                }
                VecLikeType::Xwidget => {
                    let o = &*(ptr as *const XwidgetObj);
                    out.extend([o.plist, o.type_, o.buffer, o.title, o.script_callbacks]);
                }
                VecLikeType::XwidgetView => {
                    let o = &*(ptr as *const XwidgetViewObj);
                    out.extend([o.model, o.window]);
                }
                // Buffer/Window/Frame/Timer/Process/Terminal/Marker/Subr/
                // Bignum/Sqlite/UserPtr/SurfaceHandle/VideoHandle have no Value children
                // to trace (mirrors trace_veclike).
                VecLikeType::Buffer
                | VecLikeType::Window
                | VecLikeType::Frame
                | VecLikeType::Timer
                | VecLikeType::Process
                | VecLikeType::Terminal
                | VecLikeType::Marker
                | VecLikeType::Subr
                | VecLikeType::Bignum
                | VecLikeType::Sqlite
                | VecLikeType::UserPtr
                | VecLikeType::SurfaceHandle
                | VecLikeType::VideoHandle => {}
            }
        }
        out
    }

    pub(crate) fn seed_root(&mut self, root: TaggedValue) {
        self.seed_root_with_origin(root, "explicit-root");
    }

    pub(crate) fn seed_root_with_origin(&mut self, root: TaggedValue, origin: &str) {
        if let crate::tagged::value::ValueKind::Symbol(id) = root.kind() {
            self.mark_symbol(id);
            return;
        }
        if !root.is_heap_object() {
            return;
        }
        // Stage 0: in the blackened dump partition, a root that points into the
        // dump image is already permanent-black (never cleared or swept), so it
        // needs no marking; any young child it gained through mutation is covered
        // by the dump remembered set (`seed_mapped_remembered`). Skipping these
        // avoids pushing+draining the ~450k interned-symbol value/function/plist
        // cells that still point at dumped objects on every root handshake — the
        // dominant cost of the start + termination pauses.
        if self.dump_blackened && self.owner_is_mapped(root) {
            return;
        }
        self.push_gray(root, origin);
    }

    fn seed_internal_runtime_roots(&mut self) {
        let seed_t0 = std::time::Instant::now();
        // Static subr objects are leaked process/thread runtime objects, matching
        // GNU's static `Lisp_Subr` storage. They are not swept by this heap.
        let roots: Vec<(TaggedValue, &'static str)> = self
            .buffer_registry
            .values()
            .map(|value| (*value, "buffer-registry"))
            .chain(
                self.window_registry
                    .values()
                    .map(|value| (*value, "window-registry")),
            )
            .chain(
                self.frame_registry
                    .values()
                    .map(|value| (*value, "frame-registry")),
            )
            .chain(
                self.timer_registry
                    .values()
                    .map(|value| (*value, "timer-registry")),
            )
            .chain(
                self.process_registry
                    .values()
                    .map(|value| (*value, "process-registry")),
            )
            .chain(
                self.canonical_empty_strings
                    .values()
                    .map(|value| (value, "canonical-empty-string")),
            )
            // Doomed finalizer functions not yet run must survive any cycle
            // that starts before the evaluator drains them (e.g. one queued
            // during a finalizer run, or an explicit GC before the drain).
            .chain(
                self.doomed_finalizer_functions
                    .iter()
                    .map(|value| (*value, "doomed-finalizer-function")),
            )
            .collect();

        // Handshake instrumentation: enumeration volume + wall cost, routed to
        // the start/termination slot by the caller (`concurrent_begin` /
        // `reseed_runtime_and_remembered_roots`).
        self.last_runtime_seed_roots = roots.len();
        for (value, origin) in roots {
            self.mark_or_push_child(value, origin);
        }
        self.last_runtime_seed_us = seed_t0.elapsed().as_micros() as u64;
    }

    pub(crate) fn complete_collection(&mut self) {
        let bytes_before = self.live_bytes;
        let t0 = std::time::Instant::now();

        // -- Mark phase: drain the gray queue on the GC thread. This is the STW
        //    full/bootstrap path (first cycle, no-dump heaps, explicit
        //    garbage-collect); the mutator blocks until the GC thread finishes,
        //    so heap access is exclusive (no concurrency hazard here). --
        let mark_t0 = std::time::Instant::now();
        self.mark_all_on_gc_thread();
        // Queue doomed finalizers before the weak sweep (GNU
        // `queue_doomed_finalizers` runs before
        // `mark_and_sweep_weak_table_contents` in `garbage_collect`): their
        // functions are re-marked so both the weak sweep and the object sweep
        // see them as live.
        self.mark_and_queue_doomed_finalizers();
        // Resolve weak hash tables now that the main mark has drained. Both the
        // sync and concurrent paths converge here with the mutator stopped, so
        // this is single-threaded and path-agnostic.
        self.mark_and_sweep_weak_tables();
        let mark_us = mark_t0.elapsed().as_micros() as u64;

        // The mark has drained and the sweep has not started: the one moment
        // where "marked" and "owned" must agree. A marked object that no arena
        // or intrusive list owns is a root that pointed at freed memory.
        #[cfg(debug_assertions)]
        if verify_marked_objects_enabled() {
            let problems = self.verify_marked_objects_owned();
            assert_eq!(
                problems, 0,
                "post-mark ownership verification found {problems} problem(s):                  a root pointed at memory no arena owns (see the GC VERIFY                  lines above)"
            );
        }

        self.finalize_collection(mark_us, bytes_before, t0);
    }

    /// Queue the functions of finalizer objects this cycle found unreachable —
    /// GNU `queue_doomed_finalizers` + `mark_finalizers` (alloc.c). Must run
    /// at BOTH mark terminations (`complete_collection` and
    /// `incremental_finish`), after the main mark drains and before the weak
    /// sweep. A doomed finalizer leaves the registry and is swept normally;
    /// only its `function` is queued, re-marked transitively (same marking
    /// helpers as the weak-table fixpoint) so the imminent sweep keeps
    /// everything it needs. Still-marked finalizers stay registered.
    fn mark_and_queue_doomed_finalizers(&mut self) {
        if self.finalizer_registry.is_empty() {
            return;
        }
        let registry = std::mem::take(&mut self.finalizer_registry);
        let mut doomed = Vec::new();
        for ptr in registry {
            // SAFETY: registered at allocation; every sweep that could free an
            // unmarked finalizer is preceded by this scan, which removes it
            // from the registry first, so `ptr` is live. The world is stopped
            // and marking has drained, so the mark bit is final.
            //
            // The registry can hold TENURED finalizers (promoted at the first
            // partition cycle, never swept): their frozen bit must not be
            // interpreted against the current parity — a tenured finalizer is
            // permanently live, never doomed.
            if unsafe {
                (*ptr).header.gc.tenured || (*ptr).header.gc.is_marked_at(self.mark_parity)
            } {
                self.finalizer_registry.push(ptr);
            } else {
                doomed.push(unsafe { (*ptr).function });
            }
        }
        if doomed.is_empty() {
            return;
        }
        for function in doomed.iter().copied() {
            self.mark_or_push_child(function, "doomed-finalizer-function");
        }
        self.mark_all();
        self.doomed_finalizer_functions.extend(doomed);
    }

    /// Take every function queued by the doomed-finalizer scans so far. The
    /// evaluator's cycle-completed block calls each with zero args, errors
    /// ignored (GNU `run_finalizers`). Taking the whole batch means a
    /// finalizer created — and doomed — during a finalizer run lands in a
    /// later batch, run after a later cycle.
    pub fn take_doomed_finalizer_functions(&mut self) -> Vec<TaggedValue> {
        std::mem::take(&mut self.doomed_finalizer_functions)
    }

    /// Number of live finalizer objects still registered. `dump-emacs-portable`
    /// consults this after its pre-dump collection: the portable dump cannot
    /// represent finalizer objects (the writer arms refuse them), so a
    /// non-empty registry means the dump must be refused with an elisp error
    /// before writing starts. Registry emptiness is a sound precondition for
    /// the writer: every finalizer the dump walk could reach is live
    /// (registered at allocation, deregistered only when doomed — at which
    /// point it is unreachable and swept within the same completed cycle).
    pub(crate) fn live_finalizer_count(&self) -> usize {
        self.finalizer_registry.len()
    }

    /// True when doomed finalizer functions are queued but have not yet run.
    /// Empty whenever `gc_collect_exact` returns (its cycle-completed block
    /// drains and runs the whole batch); `dump-emacs-portable` asserts this
    /// before writing so a dumped image can never silently lose pending runs.
    pub(crate) fn has_pending_doomed_finalizers(&self) -> bool {
        !self.doomed_finalizer_functions.is_empty()
    }

    /// Resolve the weak hash tables discovered during this cycle's mark — GNU
    /// `mark_and_sweep_weak_table_contents` (alloc.c) + `sweep_weak_table`
    /// (fns.c). Runs at the stop-the-world `complete_collection` after the main
    /// mark drains. First a fixpoint marks the key/value of every entry that
    /// survives per its table's weakness — iterate to stability because a value
    /// in one weak table may be a key in another — then non-surviving entries
    /// are removed.
    fn mark_and_sweep_weak_tables(&mut self) {
        // Seed every PERMANENT (tenured/mapped) weak table into this cycle's
        // worklist. The main mark skips permanent-black objects, so these would
        // otherwise never be swept again and their entries would be pinned
        // forever. GNU re-encounters and re-sweeps every live weak table on every
        // GC; this restores that for permanents. Young/runtime weak tables are
        // already registered by `trace_veclike` / `register_weak_hash_table_for_
        // sweep` during this cycle's mark.
        for &tptr in &self.permanent_weak_hash_tables {
            if self.weak_hash_tables_set.insert(tptr) {
                self.weak_hash_tables.push(tptr);
            }
        }

        if self.weak_hash_tables.is_empty() {
            return;
        }

        // -- Mark phase: keep marking surviving entries until nothing changes. --
        loop {
            let mut marked = false;
            // The worklist holds raw pointers, stable across this stop-the-world
            // step; copy them so the body can call `&mut self` methods.
            let tables = self.weak_hash_tables.clone();
            for tptr in tables {
                // SAFETY: `tptr` was recorded this cycle from a live veclike; the
                // heap is exclusively owned here (mutator stopped). Snapshot the
                // entries so the `ht` borrow is released before `push_gray`.
                let (weakness, entries): (
                    Option<HashTableWeakness>,
                    Vec<(TaggedValue, TaggedValue)>,
                ) = unsafe {
                    let ht = &(*tptr).table;
                    let entries = ht
                        .data
                        .iter()
                        .map(|(hk, &value)| {
                            let key = ht.key_snapshot(hk).copied().unwrap_or(value);
                            (key, value)
                        })
                        .collect();
                    (ht.weakness, entries)
                };
                for (key, value) in entries {
                    let key_survives = self.is_value_marked(key);
                    let value_survives = self.is_value_marked(value);
                    if Self::keep_weak_entry(weakness, key_survives, value_survives) {
                        if !key_survives {
                            self.mark_or_push_child(key, "weak-hash-key");
                            marked = true;
                        }
                        if !value_survives {
                            self.mark_or_push_child(value, "weak-hash-value");
                            marked = true;
                        }
                    }
                }
            }
            // Drain whatever those surviving entries reached, then re-check.
            self.mark_all();
            if !marked {
                break;
            }
        }

        // -- Sweep phase: drop entries that did not survive. --
        let tables = std::mem::take(&mut self.weak_hash_tables);
        self.weak_hash_tables_set.clear();
        for tptr in tables {
            // SAFETY: as above; exclusive heap access.
            let (weakness, entries): (
                Option<HashTableWeakness>,
                Vec<(HashKey, TaggedValue, TaggedValue)>,
            ) = unsafe {
                let ht = &(*tptr).table;
                let entries = ht
                    .data
                    .iter()
                    .map(|(hk, &value)| {
                        let key = ht.key_snapshot(hk).copied().unwrap_or(value);
                        (hk.clone(), key, value)
                    })
                    .collect();
                (ht.weakness, entries)
            };
            let dead: Vec<HashKey> = entries
                .into_iter()
                .filter_map(|(hk, key, value)| {
                    let keep = Self::keep_weak_entry(
                        weakness,
                        self.is_value_marked(key),
                        self.is_value_marked(value),
                    );
                    (!keep).then_some(hk)
                })
                .collect();
            if dead.is_empty() {
                continue;
            }
            // SAFETY: exclusive heap access. Mirror `builtin_remhash`'s removal.
            let ht = unsafe { &mut (*tptr).table };
            for hk in dead {
                let _ = ht.data.remove(&hk);
            }
        }
    }

    /// GNU `keep_entry_p` (fns.c): does a weak-table entry survive, given whether
    /// its key and value are independently reachable?
    fn keep_weak_entry(
        weakness: Option<HashTableWeakness>,
        strong_key: bool,
        strong_value: bool,
    ) -> bool {
        match weakness {
            None => true,
            Some(HashTableWeakness::Key) => strong_key,
            Some(HashTableWeakness::Value) => strong_value,
            Some(HashTableWeakness::KeyOrValue) => strong_key || strong_value,
            Some(HashTableWeakness::KeyAndValue) => strong_key && strong_value,
        }
    }

    /// Post-mark portion of a collection: verify, sweep, promote, account, and
    /// clear the remembered/dirty bookkeeping. Shared by the stop-the-world
    /// `complete_collection` and the incremental mark-termination path. By the
    /// time this runs the gray queue is fully drained (marking is complete) and
    /// the marker chain heads are installed.
    fn finalize_collection(&mut self, mark_us: u64, bytes_before: usize, t0: std::time::Instant) {
        // Dump-partition safety gate: prove no live heap object reachable only
        // through a dumped object was left unmarked (i.e. the write barrier's
        // remembered set is complete). Off unless explicitly verifying.
        if self.partition_dump
            && self.dump_blackened
            && std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            self.verify_dump_partition();
            // Incremental marking adds young-black->young-white as a possible
            // failure mode (a missed write-barrier owner). Check it too.
            self.verify_incremental_tricolor();
        }

        let sweep_t0 = std::time::Instant::now();

        // Unchain dead markers BEFORE `sweep_objects` frees them; the
        // chain would otherwise hold dangling pointers after the sweep.
        // Mirrors GNU `sweep_buffer → unchain_dead_markers` (`alloc.c`).
        // Reading `header.gc.marked` is sound here because the
        // allocation is still live until `sweep_objects` runs below.
        self.unchain_dead_markers();

        // -- Sweep phase --
        let cons_live_bytes = self.sweep_cons();
        let object_live_bytes = self.sweep_objects();
        // Object arena pages: the intrusive-list sweep above never sees page
        // floats/strings/vectors; their page sweeps are the second half of
        // the eager sweep. Survivor bytes are VARIABLE-size
        // (`object_bytes_from_header`) and feed this recompute site exactly
        // like the list survivors' — `live_bytes` drives the adaptive pacer
        // (`effective_gc_threshold_bytes`), so an undercount here means
        // overtriggering.
        let (page_live_bytes, _page_freed) = self.sweep_arena_pages_ranges(
            (0, self.float_arena.pages.len()),
            (0, self.string_arena.pages.len()),
            (0, self.vector_arena.pages.len()),
            (0, self.bytecode_arena.pages.len()),
            (0, self.lambda_arena.pages.len()),
            (0, self.macro_arena.pages.len()),
            (0, self.record_arena.pages.len()),
            (0, self.symbol_with_pos_arena.pages.len()),
        );
        let _released_cons_blocks = self.release_empty_cons_blocks();
        let _released_object_pages = self.release_empty_object_pages();
        let mapped_object_live_bytes = self.mapped_non_cons_live_bytes();
        self.live_bytes = cons_live_bytes
            .saturating_add(object_live_bytes)
            .saturating_add(page_live_bytes)
            .saturating_add(mapped_object_live_bytes);
        self.bytes_since_gc = 0;
        // Pacer: a stop-the-world cycle has no concurrent mark window; drop
        // any stale stamp so the next concurrent cycle measures cleanly.
        self.pace_mark_start = None;
        self.forced_termination_pending = false;

        // End of the first partition cycle: every survivor is a permanent.
        // Promote them to the tenured old generation and blacken the dump so
        // all later cycles skip both regions.
        if self.partition_dump && !self.dump_blackened {
            self.promote_and_blacken();
            self.dump_blackened = true;
        }
        self.first_cycle_concurrent = false;
        self.staged_mapped_cons_scan = None;
        self.staged_mapped_veclikes = None;

        let sweep_us = sweep_t0.elapsed().as_micros() as u64;
        // Eager STW sweep cost feeds the same lifetime total as the deferred
        // slices, so the two sweep paths are comparable.
        self.sweep_lifetime_us += sweep_us;
        let elapsed = t0.elapsed();
        self.gc_collections += 1;
        self.gc_total_elapsed_us += elapsed.as_micros() as u64;

        // Phase split + dump-partition opportunity sizing. `mapped_marked` is
        // the immutable pdump (mapped) objects re-traced this cycle — the work
        // a "dump as permanent tenured region" partition would eliminate —
        // versus the mutable heap (`cons_live` + `heap_noncons`).
        let (mapped_total, mapped_marked) = self.mapped_object_stats();
        // Batch/headless runs don't install the tracing subscriber, so mirror
        // the phase split to stderr when `NEOVM_GC_TRACE=1` for profiling.
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            // Per-class dump composition: sizes the first-cycle-concurrent
            // work split (conses scan on the GC thread; veclikes/strings are
            // handshake-side until their concurrent-read safety is proven).
            let dump_cons: usize = self.mapped_cons_ranges.iter().map(|range| range.len).sum();
            let dump_float: usize = self.mapped_float_ranges.iter().map(|range| range.len).sum();
            eprintln!(
                "NEOVM_GC gc#{} {:.2}ms [clear={}us mark={}us sweep={}us] \
                 cons_live={} heap_noncons={} dump_marked={}/{} \
                 dump[cons={} vec={} str={} float={}] dirty_owners={} live={}B",
                self.gc_collections,
                elapsed.as_micros() as f64 / 1000.0,
                self.last_clear_us,
                mark_us,
                sweep_us,
                self.cons_live_count,
                self.non_cons_object_addrs.len(),
                mapped_marked,
                mapped_total,
                dump_cons,
                self.mapped_veclike_objects.len(),
                self.mapped_string_objects.len(),
                dump_float,
                self.dirty_owners.len(),
                self.live_bytes,
            );
        }
        tracing::debug!(
            "gc#{} {:.2}ms [clear={}us mark={}us sweep={}us] {} → {} bytes ({:+.1}%), \
             cons_live={}, heap_noncons={}, dump_marked={}/{}, dirty_owners={}, threshold={}",
            self.gc_collections,
            elapsed.as_micros() as f64 / 1000.0,
            self.last_clear_us,
            mark_us,
            sweep_us,
            bytes_before,
            self.live_bytes,
            if bytes_before > 0 {
                (self.live_bytes as f64 - bytes_before as f64) / bytes_before as f64 * 100.0
            } else {
                0.0
            },
            self.cons_live_count,
            self.non_cons_object_addrs.len(),
            mapped_marked,
            mapped_total,
            self.dirty_owners.len(),
            self.gc_threshold,
        );

        // Owner-tracking remembered-set precursor: NOT cleared here. Its
        // per-cycle lifecycle is clear-at-BEGIN (`begin_collection`), aligned with
        // the SATB dedup sets, so a freed owner's address cannot linger across a
        // sweep+arena-reuse into a stale dedup (the dirty_owners ABA). Clearing at
        // end would restore that hazard for any consumer that keeps the tables
        // live through the sweep.

        // A full STW cycle has completed: the heap now has consistent live
        // accounting and an empty gray queue, the baseline the concurrent
        // collector starts from. Dump-less heaps run concurrent marking from
        // the next safe-point collection on (`should_run_concurrent`).
        self.bootstrap_collected = true;
    }

    /// Drain the gray queue, marking and tracing all reachable objects.
    fn mark_all(&mut self) {
        while let Some(val) = self.gray_queue.pop() {
            self.mark_value(val);
        }
    }

    /// Drain the gray queue on the background GC thread (Phase 4). The mutator
    /// blocks on the done-channel until the GC thread finishes, so heap access
    /// is exclusive (no concurrency hazard yet). This proves the thread +
    /// heap-sharing + handshake; the pause is not yet reduced. Phase 5 removes
    /// the block so marking actually overlaps mutator execution.
    fn mark_all_on_gc_thread(&mut self) {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let ptr = self as *mut TaggedHeap;
        gc_thread()
            .send(GcRequest::MarkAll(HeapPtr(ptr), done_tx))
            .expect("neovm-gc thread is gone");
        // Block until the GC thread has finished marking on the shared heap.
        done_rx.recv().expect("neovm-gc thread did not respond");
    }

    // ---------------------------------------------------------------------
    // Concurrent marking (Phase 5) — background GC thread marks while the
    // mutator runs; only two short stop-the-world handshakes (start + finish).
    // ---------------------------------------------------------------------

    /// True if a concurrent mark should drive THIS collection.
    ///
    /// Dump heaps: a partitioned post-dump heap whose first partition cycle
    /// has promoted + blackened the image (the young/old split bounds what is
    /// traced); that first cycle falls to the STW full path.
    ///
    /// Dump-less heaps: after the first completed STW collection — the same
    /// one-STW-bootstrap-then-concurrent shape as the dump path. Nothing
    /// tenures without a dump, so every cycle re-clears and re-marks the whole
    /// young heap (correct, just unpartitioned), and the concurrent job's dump
    /// checks never match (`dump_addr_lo/hi` stay MAX/0) while the
    /// remembered-set seeding is skipped entirely (`partition_dump` is false).
    ///
    /// A heap that registers a dump AFTER dump-less cycles switches back to
    /// the dump rule: the first partition cycle must be the STW full trace
    /// that promotes + blackens the image, regardless of earlier bootstraps.
    pub fn should_run_concurrent(&self) -> bool {
        if self.partition_dump {
            self.dump_blackened
        } else {
            self.bootstrap_collected
        }
    }

    /// True when the NEXT collection would be the first partition cycle (a
    /// registered dump not yet promoted+blackened). The driver runs it
    /// concurrently via [`Self::arm_first_cycle_concurrent`] +
    /// `concurrent_begin`/`launch_concurrent_mark` instead of the STW
    /// bootstrap.
    pub fn is_partition_first_cycle(&self) -> bool {
        self.partition_dump && !self.dump_blackened
    }

    /// Arm the concurrent first partition cycle (see the field doc).
    pub fn arm_first_cycle_concurrent(&mut self) {
        self.first_cycle_concurrent = true;
    }

    /// Complete the first partition cycle once its (possibly deferred) sweep
    /// has drained: promote survivors, blacken the image, build the initial
    /// remembered set — exactly `complete_collection`'s end-of-first-cycle
    /// block, run at the concurrent cycle's completion point instead. Also
    /// restores the mapped contribution to `live_bytes`, which the
    /// termination's accounting undercounted (mapped objects are never marked
    /// during the concurrent first cycle; blackening makes the marked-based
    /// sums whole). No-op on every later cycle and on dump-less heaps.
    pub fn finish_first_partition_cycle(&mut self) {
        if !(self.partition_dump && !self.dump_blackened) {
            self.first_cycle_concurrent = false;
            return;
        }
        self.promote_and_blacken();
        self.dump_blackened = true;
        self.first_cycle_concurrent = false;
        let mapped_cons_bytes: usize = self
            .mapped_cons_ranges
            .iter()
            .map(|range| range.live_count().saturating_mul(size_of::<ConsCell>()))
            .sum();
        self.live_bytes = self
            .live_bytes
            .saturating_add(self.mapped_non_cons_live_bytes())
            .saturating_add(mapped_cons_bytes);
    }

    /// True while the background GC thread is marking (between the start and
    /// termination handshakes) — the mutator is running concurrently.
    pub fn concurrent_mark_running(&self) -> bool {
        self.concurrent_mark_running
    }

    /// The GC thread has tentatively drained gray + SATB (Acquire pairs with the
    /// thread's Release). The mutator polls this at safe points to terminate.
    pub fn concurrent_mark_done(&self) -> bool {
        self.gc_done.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Start-of-cycle setup for a concurrent mark: clear young marks + seed the
    /// collector-internal and remembered roots (`begin_collection`), arm
    /// `mark_in_progress`. The caller then seeds context roots and calls
    /// `launch_concurrent_mark`. No Steele owner-tracking: the concurrent SATB
    /// barrier (keyed on `concurrent_mark_running`) preserves the snapshot.
    pub(crate) fn concurrent_begin(&mut self) {
        // Zero the seeding scratch so a skipped `seed_mapped_remembered`
        // (non-partitioned heap) does not leave a stale previous value in the
        // start slots filled below.
        self.last_remembered_seed_us = 0;
        self.last_remembered_seed_roots = 0;
        self.begin_collection();
        // Route this handshake's `begin_collection` phase costs to the START
        // slots (this entry point is exclusively the concurrent start).
        self.handshake.start_count += 1;
        self.handshake.last_start_clear_us = self.last_clear_us;
        self.handshake.last_start_clear_cons_us = self.last_clear_cons_us;
        self.handshake.last_start_clear_noncons_us = self.last_clear_noncons_us;
        self.handshake.last_start_clear_mapped_us = self.last_clear_mapped_us;
        self.handshake.last_start_runtime_us = self.last_runtime_seed_us;
        self.handshake.last_start_runtime_roots = self.last_runtime_seed_roots;
        self.handshake.last_start_remembered_us = self.last_remembered_seed_us;
        self.handshake.last_start_remembered_roots = self.last_remembered_seed_roots;
        self.mark_in_progress = true;
        self.incremental_mark_us = 0;
    }

    /// Hand the seeded gray queue (the full root snapshot) to the GC thread and
    /// start non-blocking concurrent marking. Returns immediately; the mutator
    /// resumes while the GC thread marks. Allocate-black turns on so new objects
    /// survive this cycle's sweep, and the SATB barrier starts logging.
    /// Stage 1b: stash the start-captured obarray scan snapshot for the next
    /// `launch_concurrent_mark` to move into the job. Called from
    /// `start_concurrent_mark` at the world-stopped start handshake (once per
    /// concurrent mark).
    pub(crate) fn set_pending_obarray_scan(
        &mut self,
        snap: crate::emacs_core::symbol::ObarrayScanSnapshot,
    ) {
        // Retain the start slot count for the termination residual re-seed before
        // the snapshot is moved into the GC job at `launch_concurrent_mark`.
        self.concurrent_obarray_start_slots = Some(snap.n_slots());
        self.pending_obarray_scan = Some(snap);
    }

    /// Stage 1b: take the start-of-cycle obarray slot count (set at the start
    /// handshake) for the termination residual re-seed. `None` for a cycle with
    /// no concurrent mark (e.g. a stop-the-world full collection).
    pub(crate) fn take_concurrent_obarray_start_slots(&mut self) -> Option<usize> {
        self.concurrent_obarray_start_slots.take()
    }

    pub(crate) fn launch_concurrent_mark(&mut self) {
        // Immutable snapshot of owned cons-block bases — read-only on the GC
        // thread. New blocks allocated during marking are absent, which is fine:
        // their conses allocate-black and never enter the GC's gray queue.
        let conssnap_t0 = std::time::Instant::now();
        let mut owned =
            FxHashSet::with_capacity_and_hasher(self.cons_blocks.len(), Default::default());
        for block in &self.cons_blocks {
            owned.insert(block.base_addr());
        }
        // CONCURRENT STRING MARKING claim oracle (stage 3): capture the
        // string arena's page bases at this same world-stopped instant
        // (retired pages included — their tenured strings are claim-benign).
        // Built alongside the cons `owned_bases` so both snapshots share the
        // immutability argument: pages created after this point are absent
        // and their strings DEFER (fail-safe). Timed within the conssnap
        // handshake slot (same ownership-snapshot phase).
        let mut string_bases =
            FxHashSet::with_capacity_and_hasher(self.string_arena.pages.len(), Default::default());
        for page in &self.string_arena.pages {
            string_bases.insert(page.base_addr());
        }
        self.handshake.last_start_conssnap_us = conssnap_t0.elapsed().as_micros() as u64;
        self.handshake.probe_cons_blocks = self.cons_blocks.len();
        // CONCURRENT FLOAT CLAIMS (task 01) claim oracle: capture the float
        // arena's page bases at this same world-stopped instant (retired
        // pages included — their tenured floats recognize-and-drop at the
        // claim arm). Same immutability + Arc-publication argument as
        // `string_page_bases`; pages created after this point are absent and
        // their floats DEFER (fail-safe). O(pages); own handshake timer.
        let floatsnap_t0 = std::time::Instant::now();
        let mut float_bases =
            FxHashSet::with_capacity_and_hasher(self.float_arena.pages.len(), Default::default());
        for page in &self.float_arena.pages {
            float_bases.insert(page.base_addr());
        }
        self.handshake.last_start_floatsnap_us = floatsnap_t0.elapsed().as_micros() as u64;
        // CONCURRENT VECTOR-HEADER CLAIMS (task 01) claim oracle: capture
        // the vector arena's page bases at this same world-stopped instant
        // (retired pages included — tenured vectors recognize-and-drop at
        // the claim arm). Same discipline as the float/string snapshots.
        // O(pages); own handshake timer, distinct from the Tier-B backing
        // `vecsnap` below.
        let vecbasesnap_t0 = std::time::Instant::now();
        let mut vector_bases =
            FxHashSet::with_capacity_and_hasher(self.vector_arena.pages.len(), Default::default());
        for page in &self.vector_arena.pages {
            vector_bases.insert(page.base_addr());
        }
        self.handshake.last_start_vecbasesnap_us = vecbasesnap_t0.elapsed().as_micros() as u64;
        // CONCURRENT BYTECODE CLAIMS (task 01) claim oracle: capture the
        // bytecode arena's page bases at this same world-stopped instant
        // (retired pages included — tenured bytecode recognize-and-drops at
        // the claim arm). Same discipline as the float/vector snapshots.
        // O(pages); own handshake timer.
        let bcsnap_t0 = std::time::Instant::now();
        let mut bytecode_bases = FxHashSet::with_capacity_and_hasher(
            self.bytecode_arena.pages.len(),
            Default::default(),
        );
        for page in &self.bytecode_arena.pages {
            bytecode_bases.insert(page.base_addr());
        }
        self.handshake.last_start_bcsnap_us = bcsnap_t0.elapsed().as_micros() as u64;
        let vecsnap_t0 = std::time::Instant::now();
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: snapshot every
        // OWNED/Mapped vector backing AT THIS world-stopped point (same instant the
        // cons `owned_bases` snapshot is taken and the roots are seeded), so the GC
        // thread can trace vectors concurrently instead of deferring them to the STW
        // termination. Vectors are heap-side, so capture directly here (no eval.rs
        // seam, unlike the Context-side obarray). Task #7 stage 2a (Fix A): iterate
        // the INCREMENTAL VECTOR REGISTRY (`vector_object_addrs`, maintained at
        // `link_veclike` + the sweep free sites) instead of filtering the whole
        // `non_cons_object_addrs` set — the filter walk was 11-32% of this
        // world-stopped start handshake. Vectors allocated mid-cycle are absent from
        // this capture and are covered by allocate-black.
        if (cfg!(test) && cfg!(debug_assertions))
            || std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            // Fix A INVARIANT, stage-3 form: the registry equals the live
            // owned Vector population = ALLOCATED VECTOR-ARENA PAGE SLOTS ∪
            // the residual Box Vector subset of `non_cons_object_addrs`.
            // (The pre-stage-3 form — registry == Vector∩addr-set — would
            // fire on the first page vector; worse, if the registry were
            // silently EMPTY the old 0==0 check would pass and the Tier-B
            // vecsnap below would disable concurrent vector marking without
            // any test noticing.) Cross-check both directions: counts match
            // the union of the two disjoint sources, and every registry
            // address is page-owned xor addr-set-resident. Debug test builds
            // only (or explicit VERIFY_PARTITION): the release drain
            // profilers are themselves cfg(test) binaries, and this walk
            // would re-add cost inside the timed vecsnap region.
            let box_filter_count = self
                .non_cons_object_addrs
                .iter()
                .filter(|&&addr| unsafe {
                    (*(addr as *const GcHeader)).kind == HeapObjectKind::VecLike
                        && (*(addr as *const VecLikeHeader)).type_tag == VecLikeType::Vector
                })
                .count();
            let page_vector_count: usize =
                self.vector_arena.pages.iter().map(|p| p.allocated).sum();
            assert_eq!(
                self.vector_object_addrs.len(),
                box_filter_count + page_vector_count,
                "vector registry diverged from page slots ∪ residual Box vectors",
            );
            for &addr in &self.vector_object_addrs {
                let page_owned = self.vector_arena.owns(addr as *const u8);
                let box_owned = self.non_cons_object_addrs.contains(&addr);
                assert!(
                    page_owned ^ box_owned,
                    "vector registry address must be page-owned xor Box-owned",
                );
            }
            // Task 01 vector-claim inclusion, asserted from the CLAIM ARM's
            // perspective: every ALLOCATED vector-arena page slot must be
            // Tier-B-registered ({page vectors} ⊆ `vector_object_addrs`), so
            // a `vector_page_bases` HIT at `concurrent_try_mark_owned`
            // implies the claimed vector's backing is in the Tier-B snapshot
            // built below — its children trace concurrently, which is what
            // makes the header claim (and the removed termination re-trace)
            // sound. Retired pages included: their tenured slots drop at the
            // arm before any children question arises, but keeping them
            // registered is the standing registry invariant.
            for slot in self.vector_arena.collect_allocated_slots() {
                assert!(
                    self.vector_object_addrs.contains(&(slot as usize)),
                    "allocated vector page slot missing from the Tier-B \
                     registry — the claim arm would orphan its children",
                );
            }
        }
        let vectors = {
            let mut snap = crate::tagged::header::VectorScanSnapshot::with_capacity(
                self.vector_object_addrs.len(),
            );
            for &addr in &self.vector_object_addrs {
                // Safety: `addr` is a live owned Vector's `GcHeader` addr (the
                // registry invariant above); a VecLike header begins with its
                // `GcHeader`, so casting to `*const VectorObj` and reading its
                // backing is valid.
                let obj = unsafe { &*(addr as *const VectorObj) };
                snap.push(obj.data.scan_entry());
            }
            Some(snap)
        };
        self.handshake.last_start_vecsnap_us = vecsnap_t0.elapsed().as_micros() as u64;
        self.handshake.probe_vector_snapshot_len =
            vectors.as_ref().map(|snap| snap.len()).unwrap_or(0);
        let jobasm_t0 = std::time::Instant::now();
        let gray = std::mem::take(&mut self.gray_queue);
        let (exited_tx, exited_rx) = std::sync::mpsc::channel();
        self.gc_done
            .store(false, std::sync::atomic::Ordering::Release);
        // Fresh per-cycle concurrent claim/drop counters.
        self.concurrent_str_claimed.store(0, Ordering::Relaxed);
        self.concurrent_float_claimed.store(0, Ordering::Relaxed);
        self.concurrent_subr_dropped.store(0, Ordering::Relaxed);
        self.concurrent_vec_claimed.store(0, Ordering::Relaxed);
        self.concurrent_bc_claimed.store(0, Ordering::Relaxed);
        self.gc_stop
            .store(false, std::sync::atomic::Ordering::Release);
        self.gc_exited = Some(exited_rx);
        self.concurrent_mark_running = true;
        // Keep the write-barrier fast path reaching `record_heap_write` so the
        // SATB log fires even with owner-tracking Disabled / no partition.
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(true));
        let job = ConcurrentMarkJob {
            gray,
            owned_bases: std::sync::Arc::new(owned),
            claims: ConcurrentClaimJob {
                // Mandated carry: the GC thread claims at THIS cycle's parity.
                parity: self.mark_parity,
                string_page_bases: std::sync::Arc::new(string_bases),
                float_page_bases: std::sync::Arc::new(float_bases),
                vector_page_bases: std::sync::Arc::new(vector_bases),
                bytecode_page_bases: std::sync::Arc::new(bytecode_bases),
                dump_lo: self.dump_addr_lo,
                dump_hi: self.dump_addr_hi,
                drop_dump_children: self.first_cycle_concurrent,
                str_claimed: self.concurrent_str_claimed.clone(),
                float_claimed: self.concurrent_float_claimed.clone(),
                subr_dropped: self.concurrent_subr_dropped.clone(),
                vec_claimed: self.concurrent_vec_claimed.clone(),
                bc_claimed: self.concurrent_bc_claimed.clone(),
            },
            satb: self.satb_shared.clone(),
            deferred: self.deferred_veclikes.clone(),
            done: self.gc_done.clone(),
            stop: self.gc_stop.clone(),
            wake: self.gc_wake.clone(),
            exited: exited_tx,
            // Stage 1b: consume the obarray snapshot the start handshake staged.
            // Take it so it is not left dangling for a later cycle.
            obarray: self.pending_obarray_scan.take(),
            // Stage 2 Tier B: the vector-backing snapshot captured just above.
            vectors,
            // First partition cycle: the staged mapped cons ranges (else None).
            mapped_cons_ranges: self.staged_mapped_cons_scan.take(),
            mapped_veclikes: self.staged_mapped_veclikes.take(),
        };
        gc_thread()
            .send(GcRequest::ConcurrentMark(job))
            .expect("neovm-gc thread is gone");
        self.handshake.last_start_jobasm_us = jobasm_t0.elapsed().as_micros() as u64;
        // Pacer: open this cycle's mark window (closed by `incremental_finish`).
        self.pace_mark_start = Some(std::time::Instant::now());
        self.pace_mark_start_bytes = self.bytes_since_gc;
    }

    /// Stop the GC thread and fold its residual work back into the gray queue so
    /// the caller can finish marking stop-the-world. After this, the heap is
    /// owned exclusively by the mutator again (the GC thread has exited its loop).
    pub(crate) fn join_concurrent_mark(&mut self) {
        let join_t0 = std::time::Instant::now();
        self.gc_stop
            .store(true, std::sync::atomic::Ordering::Release);
        // Task #7 stage 2a (Fix B): wake the GC thread out of its idle nap
        // NOW. Store-then-lock+notify pairs with the GC thread's
        // check-under-lock before waiting, so the notify cannot fall between
        // its flag check and its wait (no lost wakeup, no full-nap latency).
        {
            let (lock, cvar) = &*self.gc_wake;
            let _guard = lock.lock().unwrap();
            cvar.notify_all();
        }
        if let Some(rx) = self.gc_exited.take() {
            let _ = rx.recv(); // block until the GC thread leaves its mark loop
        }
        self.concurrent_mark_running = false;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(false));
        // Residual SATB (children overwritten after the GC's last drain) +
        // deferred (every non-cons + non-owned cons the GC parked) become gray;
        // the caller reseeds roots, then drains to a fixpoint stop-the-world.
        // The fold is timed (`last_termination_fold_us`) so the termination's
        // cheap push half is attributable separately from the mark fixpoint.
        let fold_t0 = std::time::Instant::now();
        let satb = std::mem::take(&mut *self.satb_shared.lock().unwrap());
        self.last_termination_satb = satb.len();
        self.gray_queue.extend(satb);
        let deferred = std::mem::take(&mut *self.deferred_veclikes.lock().unwrap());
        self.last_termination_deferred = deferred.len();
        self.max_termination_deferred = self.max_termination_deferred.max(deferred.len());
        // Strings/floats the GC thread claimed concurrently and subrs it
        // dropped (they never reached `deferred`); the exit handshake above
        // (`rx.recv()`) established the happens-before, so a Relaxed read
        // sees the final counts.
        self.last_concurrent_str_claimed = self.concurrent_str_claimed.load(Ordering::Relaxed);
        self.last_concurrent_float_claimed = self.concurrent_float_claimed.load(Ordering::Relaxed);
        self.last_concurrent_subr_dropped = self.concurrent_subr_dropped.load(Ordering::Relaxed);
        self.last_concurrent_vec_claimed = self.concurrent_vec_claimed.load(Ordering::Relaxed);
        self.last_concurrent_bc_claimed = self.concurrent_bc_claimed.load(Ordering::Relaxed);
        // Task 01 INSERTION-COVERAGE RE-TRACE (the load-bearing companion of
        // the vector-header claims): re-gray the CURRENT children of every
        // multi-child owner mutated this cycle (`satb_snapshotted_owners` —
        // populated by the write barrier's first-mutation dedup, so it is
        // exactly the mutated-owner set). The SATB deletion barrier preserves
        // only SNAPSHOT-time children; a value INSERTED mid-cycle (stored
        // from a mutator register — root→heap motion) into an
        // already-CLAIMED owner is otherwise invisible: the claimed mark bit
        // makes the termination's `mark_value` early-return, so the old
        // "every deferred veclike is re-traced on its CURRENT backing"
        // backstop no longer covers it. Bounded by mutation volume (each
        // owner once), not by the live vector population — which is the
        // whole point of claiming. Also covers claimed STRINGS that gained
        // interval tables mid-cycle (their wrapper barriers land the owner
        // in the same set).
        let written = std::mem::take(&mut self.satb_snapshotted_owners);
        for bits in written {
            self.push_value_children_to_gray(TaggedValue(bits), "satb-written-retrace");
        }
        // Classify what the drain is about to trace, per kind — the measurement
        // that decides which kinds a concurrent-tracing extension should take
        // on. Pure counting (marking behavior is unchanged), but the header
        // reads cost real STW time on a large buffer (~20ns/entry), so outside
        // the crate's own tests it only runs when the trace that prints it is
        // on; the kind buckets stay zero otherwise.
        if cfg!(test) || std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let mut kinds = DrainKinds::default();
            for &val in &deferred {
                // Safety: parked entries are live heap values; nothing has been
                // swept since they were parked (see `DrainKinds::note`).
                unsafe { kinds.note(val) };
            }
            self.last_termination_kinds = kinds;
            self.max_termination_kinds.merge_max(&kinds);
        }
        self.termination_count += 1;
        self.gray_queue.extend(deferred);
        self.last_termination_fold_us = fold_t0.elapsed().as_micros() as u64;
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: the GC thread has provably exited its
        // mark loop (the `rx.recv()` above), so its snapshot pointers into the retired
        // vector backings are no longer in use — this is the ONLY safe free point.
        // Drain + drop the retired originals and clear the per-cycle clone-dedup set.
        // Both are empty unless a clone-on-write fired this cycle.
        let retired = std::mem::take(&mut self.retired_vector_buffers);
        drop(retired);
        self.concurrent_cloned_vectors.clear();
        // Whole join cost (stop signal + GC-thread exit wait + the fold above);
        // the fold alone stays separately visible as `last_termination_fold_us`.
        self.handshake.last_term_join_us = join_t0.elapsed().as_micros() as u64;
    }

    /// SATB barrier path for concurrent marking: append the owner's current
    /// (pre-overwrite) children to the shared buffer the GC thread drains. Reuses
    /// the gray-queue child enumeration with `self.gray_queue` as scratch (it is
    /// empty during concurrent marking — the snapshot was handed to the thread).
    ///
    /// Per-cycle dedup for multi-child owners (veclike/string): the barrier can't
    /// know which slot the bulk closure will touch, so it logs the owner's WHOLE
    /// pre-image; doing that on every write is O(n) per write => O(n²) to build an
    /// n-element container (hash table, char-table, or a vector filled by `aset`
    /// in a loop — the `(ucs-names)` OOM). SATB only needs each owner's
    /// start-of-cycle child set logged ONCE: at the owner's FIRST mutation this
    /// cycle every snapshot-time child is still present (a child can only be
    /// unlinked by a mutation of THIS owner, i.e. this very first barrier firing
    /// pre-store), so one snapshot is a superset of the snapshot-time children;
    /// later writes overwrite only already-logged values (or born-black new ones,
    /// which need no logging). So re-snapshotting is pure waste — skip it. The
    /// snapshot set is cleared at every mark start (`concurrent_begin`).
    ///
    /// Conses (exactly two children) bypass the dedup: their barrier is already
    /// O(1), and a per-write `HashSet` insert on the hot car/cdr path would cost
    /// more than it saves. Re-logging a cons's 2 children is still SATB-correct.
    /// Hand a batch of LIVE mutator roots to the concurrent marker via the
    /// SATB channel. Extra live values in the SATB log are always safe (the
    /// marker treats each entry as gray; already-marked entries are skipped
    /// by the atomic mark test) — this exists so young data reachable ONLY
    /// from the mutator's stack marks CONCURRENTLY instead of all at once in
    /// the stop-the-world termination fold. A value that dies before the
    /// cycle ends floats one cycle, the standard SATB trade.
    pub(crate) fn feed_satb_roots(&self, values: &[TaggedValue]) {
        let mut shared = self.satb_shared.lock().unwrap();
        shared.extend(values.iter().copied().filter(|v| v.is_heap_object()));
    }

    fn push_value_children_to_satb_shared(&mut self, owner: TaggedValue) {
        debug_assert!(self.gray_queue.is_empty());
        // Multi-child owners are deduped once per cycle; conses fall through to
        // the cheap direct enumeration below.
        if !owner.is_cons() && !self.satb_snapshotted_owners.insert(owner.bits()) {
            return; // this owner's full pre-image was already logged this cycle
        }
        self.push_value_children_to_gray(owner, "satb-concurrent");
        if !self.gray_queue.is_empty() {
            let mut shared = self.satb_shared.lock().unwrap();
            shared.extend(self.gray_queue.drain(..));
        }
    }

    /// SATB sink for a ROOT-slot overwrite (a symbol value/function/plist cell):
    /// log the pre-image VALUE itself so the concurrent mark grays and traces it
    /// (`join_concurrent_mark` folds `satb_shared` into the gray queue), keeping a
    /// symbol-only-reachable object live across the cycle. Unlike
    /// `push_value_children_to_satb_shared`, the retained thing is the overwritten
    /// value itself, not an owner's children — the symbol cell's "owner" is a
    /// non-heap root. No `concurrent_mark_running` assert: the caller already gated
    /// on the `TAGGED_HEAP_CONCURRENT_ACTIVE` thread-local (the source of truth),
    /// and an extra entry is at worst one cycle of floating garbage.
    fn note_root_overwrite_value(&mut self, pre_image: TaggedValue) {
        self.satb_shared.lock().unwrap().push(pre_image);
    }

    /// Stage 2 Tier B CONCURRENT VECTOR SCAN clone-on-write hook. Called from
    /// `with_vector_data_mut` BEFORE a vector's OWNED backing is bulk-mutated, while a
    /// concurrent mark is active. On the owner's FIRST such
    /// mutation this cycle, if the backing is currently OWNED, replace it with a clone
    /// and RETIRE the original (kept alive to join) so the GC thread's start-of-cycle
    /// snapshot pointer keeps addressing an immutable, live buffer; the closure then
    /// mutates the clone. Idempotent per owner per cycle (dedup set), and a no-op when
    /// the backing is MAPPED (the snapshot points at the immutable dump; `ensure_owned`
    /// will promote it to a fresh OWNED the snapshot never reads, so no clone needed).
    ///
    /// Reachability of the pre-image children is handled separately by the
    /// `note_heap_write(VectorBulk)` SATB barrier the caller fires first; this hook
    /// only preserves the snapshot pointer's buffer for the concurrent READ.
    ///
    /// Safety: `owner` must be a live `VecLikeType::Vector` value on this heap.
    pub(crate) fn concurrent_clone_on_write_vector(&mut self, owner: TaggedValue) {
        // First mutation of this owner this cycle? `insert` returns false if already
        // present, so later mutations of the same owner skip the clone (they touch the
        // already-cloned live backing the snapshot does not point at).
        if !self.concurrent_cloned_vectors.insert(owner.bits()) {
            return;
        }
        let Some(header) = owner.as_veclike_ptr() else {
            return;
        };
        let obj = unsafe { &mut *(header as *mut VectorObj) };
        // Only OWNED backings need cloning: a MAPPED backing reads the immutable dump
        // span the snapshot captured; `ensure_owned` (run by the caller next) promotes
        // it to a brand-new OWNED buffer the snapshot never addresses.
        if !obj.data.is_owned() {
            return;
        }
        // Replace the backing with a clone; retire the original so the GC's snapshot
        // pointer keeps addressing it (immutable + alive) until the join free point.
        let original = obj.data.clone_owned_backing();
        self.retired_vector_buffers.push(original);
    }

    // ---------------------------------------------------------------------
    // Incremental marking (step 7)
    // ---------------------------------------------------------------------

    /// True while a mark is underway (between the start handshake and sweep).
    pub fn mark_in_progress(&self) -> bool {
        self.mark_in_progress
    }

    /// Re-seed the collector-internal roots at mark termination: the runtime
    /// object registries and the dump remembered set (the non-clearing seeds
    /// that `begin_collection` runs at the start). Mark termination must
    /// re-snapshot the COMPLETE root set, not just the evaluator/context roots —
    /// otherwise an object that became reachable only through one of these roots
    /// during the marking window is left unmarked and swept while live.
    pub(crate) fn reseed_runtime_and_remembered_roots(&mut self) {
        // Zero the remembered scratch so the skip branch below does not leave
        // a stale previous value in the termination slots filled after.
        self.last_remembered_seed_us = 0;
        self.last_remembered_seed_roots = 0;
        self.seed_internal_runtime_roots();
        if self.partition_dump && self.dump_blackened {
            self.seed_mapped_remembered();
        }
        // Route this handshake's reseed costs to the TERMINATION slots (this
        // entry point is exclusively the concurrent termination).
        self.handshake.term_count += 1;
        self.handshake.last_term_runtime_us = self.last_runtime_seed_us;
        self.handshake.last_term_runtime_roots = self.last_runtime_seed_roots;
        self.handshake.last_term_remembered_us = self.last_remembered_seed_us;
        self.handshake.last_term_remembered_roots = self.last_remembered_seed_roots;
    }

    /// Drain ALL remaining marking work to a fixpoint (no budget). Used at mark
    /// termination, after the roots have been re-snapshotted, while the world is
    /// stopped. A single `mark_all` reaches the fixpoint: `mark_value` re-pushes
    /// each marked object's children, so the gray queue drains completely.
    pub(crate) fn incremental_drain_all(&mut self) {
        let t0 = std::time::Instant::now();
        self.mark_all();
        self.incremental_mark_us += t0.elapsed().as_micros() as u64;
    }

    /// Run mark termination's sweep + accounting, then leave the incremental
    /// state. Marking must already be drained to a fixpoint and the marker
    /// chain heads installed. `pause_t0` stamps the termination (sweep) pause.
    /// Mark termination: verify, unchain dead markers, then DEFER the sweep.
    /// The reclaim drains in bounded slices at later safe points
    /// (`incremental_sweep_slice`), so it is no longer part of the STW pause.
    /// Marking is complete here; the barrier is dropped.
    pub(crate) fn incremental_finish(
        &mut self,
        bytes_before: usize,
        _pause_t0: std::time::Instant,
    ) {
        // Queue doomed finalizers first (mirrors `complete_collection`; a miss
        // here would mean finalizers silently never run under the concurrent
        // collector). The main mark has drained — the termination handshake
        // already traced the deferred veclikes — so marks are final.
        let finalizer_t0 = std::time::Instant::now();
        self.mark_and_queue_doomed_finalizers();
        self.handshake.last_term_finalizer_us = finalizer_t0.elapsed().as_micros() as u64;
        // Resolve weak hash tables (GNU mark_and_sweep_weak_table_contents): mark
        // entries that survive per their table's weakness, then drop the rest. This
        // mirrors `complete_collection` and MUST run on the concurrent/incremental
        // termination too — otherwise a weak table's only-weakly-reachable entries
        // are neither marked nor removed, so they are swept while still referenced
        // by the table (UAF). The main mark has already drained at this point.
        let weak_t0 = std::time::Instant::now();
        self.mark_and_sweep_weak_tables();
        self.handshake.last_term_weak_us = weak_t0.elapsed().as_micros() as u64;

        // Dump-partition safety gate (marks still intact). Same as
        // `finalize_collection`'s, run before any object is freed.
        if self.partition_dump
            && self.dump_blackened
            && std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            self.verify_dump_partition();
            self.verify_incremental_tricolor();
        }
        // Unchain dead markers before the sweep frees them (mirrors GNU
        // sweep_buffer -> unchain_dead_markers). Reads marks, which are intact.
        let unchain_t0 = std::time::Instant::now();
        self.unchain_dead_markers();
        self.handshake.last_term_unchain_us = unchain_t0.elapsed().as_micros() as u64;

        // Begin the deferred sweep. Detach the young non-cons list (new non-cons
        // allocations link onto a fresh `all_objects` and are not swept this
        // cycle) and reset the cons free list (rebuilt as blocks are swept).
        self.sweep_noncons_pending = self.all_objects;
        self.all_objects = std::ptr::null_mut();
        self.cons_free_list = std::ptr::null_mut();
        self.sweep_cons_cursor = 0;
        // Object arena pages are swept in place behind these cursors (no
        // detached list exists for them; the bitmap is re-read per slice).
        self.sweep_float_page_cursor = 0;
        self.sweep_string_page_cursor = 0;
        self.sweep_vector_page_cursor = 0;
        self.sweep_bytecode_page_cursor = 0;
        self.sweep_lambda_page_cursor = 0;
        self.sweep_macro_page_cursor = 0;
        self.sweep_record_page_cursor = 0;
        self.sweep_symbol_with_pos_page_cursor = 0;
        self.sweep_noncons_live_bytes = 0;
        self.sweep_mark_us = self.incremental_mark_us;
        self.sweep_bytes_before = bytes_before;
        self.sweep_slice_us_total = 0;
        self.sweep_slice_count = 0;
        self.sweep_cons_blocks_swept = 0;
        self.sweep_noncons_freed = 0;
        self.sweep_in_progress = true;
        // Pacer: close the mark window. Sample the allocation rate + wall
        // duration of the just-terminated concurrent mark and project the
        // next window's allocation (`pace_lead_bytes`).
        let pace_wall_us = self
            .pace_mark_start
            .take()
            .map(|t0| t0.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let pace_alloc = self
            .bytes_since_gc
            .saturating_sub(self.pace_mark_start_bytes);
        let forced = self.forced_termination_pending;
        self.forced_termination_pending = false;
        self.pace_close_mark_window(pace_wall_us, pace_alloc, forced);
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC mark_window alloc={}B wall={}us start_bytes={} forced={} \
                 rate_ewma={}B/s dur_ewma={}us lead={}B",
                pace_alloc,
                pace_wall_us,
                self.pace_mark_start_bytes,
                forced,
                self.pace_alloc_rate_bps,
                self.pace_mark_dur_us,
                self.pace_lead_bytes,
            );
        }
        // The triggering allocation budget is spent; the next mark fires once a
        // fresh threshold's worth has been allocated.
        self.bytes_since_gc = 0;

        // Marking is done; drop the marking barrier. The dump remembered set is
        // still maintained unconditionally in `record_heap_write`.
        self.set_write_tracking_mode(WriteTrackingMode::Disabled);
        self.mark_in_progress = false;
    }

    /// True while the deferred sweep is draining.
    pub fn sweep_in_progress(&self) -> bool {
        self.sweep_in_progress
    }

    /// True if a panic ever unwound while one of the collector's own locks was
    /// held. Those critical sections live entirely inside GC machinery, so
    /// poison proves a panic escaped mid-protocol and the heap's invariants
    /// are unknown. Module-boundary panic containment probes this and refuses
    /// to contain (re-raises) when it fires; the locks keep plain `.unwrap()`
    /// at their use sites on purpose — clearing poison would assert a
    /// coherence nothing can verify and erase the only evidence.
    pub(crate) fn gc_locks_poisoned(&self) -> bool {
        self.satb_shared.is_poisoned()
            || self.deferred_veclikes.is_poisoned()
            || self.gc_wake.0.is_poisoned()
    }

    /// Test-only: poison one of the collector's own locks by panicking while
    /// holding it, so containment tests can exercise the refuse-to-contain
    /// probe without unwinding real GC machinery. Poison is permanent for the
    /// heap (that is the point) — callers run process-per-test under nextest.
    #[cfg(test)]
    pub(crate) fn poison_gc_locks_for_test(&self) {
        let lock = self.satb_shared.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = lock.lock().unwrap();
            panic!("poison a GC lock for the containment probe test");
        }));
        assert!(self.gc_locks_poisoned(), "test poison must be observable");
    }

    /// Advance the deferred sweep by one bounded slice: reclaim up to `budget`
    /// cons blocks and up to `budget` pending non-cons objects. Returns true
    /// (and finalizes accounting) once the whole sweep is done. New conses
    /// allocated meanwhile are born black (see `alloc_cons`), so an unswept
    /// block never reclaims a live new cell.
    pub(crate) fn incremental_sweep_slice(&mut self, budget: usize) -> bool {
        let t0 = std::time::Instant::now();
        // -- cons: reclaim up to `budget` blocks (each ~64KB of cells) --
        let mut swept_blocks = 0usize;
        while swept_blocks < budget && self.sweep_cons_cursor < self.cons_blocks.len() {
            let idx = self.sweep_cons_cursor;
            let free_list: *mut *mut ConsCell = &mut self.cons_free_list;
            self.cons_blocks[idx].sweep(unsafe { &mut *free_list });
            self.sweep_cons_cursor += 1;
            swept_blocks += 1;
        }
        // -- object arena pages: reclaim up to `budget` pages PER CLASS
        //    (64KB each, like cons blocks), page-at-a-time behind the
        //    per-class cursors. Each visit re-reads the live bitmap (the
        //    mutator can reallocate freed slots between slices — see
        //    `ObjectArena::sweep_range`). Pages created mid-sweep may or may
        //    not be visited by the moving cursor/len race; either is correct
        //    — every slot in them is born-at-parity (marked), so a visit
        //    counts survivors and a skip frees nothing it shouldn't. Page
        //    survivor bytes accumulate into `sweep_noncons_live_bytes`, the
        //    incremental half of the live-bytes recompute
        //    (`finish_incremental_sweep`). --
        let mut float_freed = 0usize;
        {
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_float_page_cursor < self.float_arena.pages.len()
            {
                let idx = self.sweep_float_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_float_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_string_page_cursor < self.string_arena.pages.len()
            {
                let idx = self.sweep_string_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_string_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_vector_page_cursor < self.vector_arena.pages.len()
            {
                let idx = self.sweep_vector_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_vector_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_bytecode_page_cursor < self.bytecode_arena.pages.len()
            {
                let idx = self.sweep_bytecode_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_bytecode_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_lambda_page_cursor < self.lambda_arena.pages.len()
            {
                let idx = self.sweep_lambda_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_lambda_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_macro_page_cursor < self.macro_arena.pages.len()
            {
                let idx = self.sweep_macro_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_macro_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_record_page_cursor < self.record_arena.pages.len()
            {
                let idx = self.sweep_record_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_record_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_symbol_with_pos_page_cursor < self.symbol_with_pos_arena.pages.len()
            {
                let idx = self.sweep_symbol_with_pos_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_symbol_with_pos_page_cursor += 1;
                swept_pages += 1;
            }
        }
        // -- non-cons: reclaim more objects per slice than cons blocks, since a
        //    cons block holds thousands of cells while a non-cons node is one
        //    object (with a heavier per-object free). --
        let noncons_budget = budget.saturating_mul(256);
        let mut processed = 0usize;
        let mut noncons_freed = 0usize;
        // The detached sweep list is young-only (`all_objects` never holds
        // tenured objects), and `begin_collection` hard-asserts no flip can
        // happen while this sweep drains, so the bits are interpreted at the
        // parity of the cycle that just marked them.
        let parity = self.mark_parity;
        while processed < noncons_budget && !self.sweep_noncons_pending.is_null() {
            let current = self.sweep_noncons_pending;
            unsafe {
                self.sweep_noncons_pending = (*current).next;
                debug_assert!(
                    !(*current).tenured,
                    "tenured object on the young sweep list"
                );
                if (*current).is_marked_at(parity) {
                    // Survivor: relink onto the (fresh) young list.
                    (*current).next = self.all_objects;
                    self.all_objects = current;
                    self.sweep_noncons_live_bytes = self
                        .sweep_noncons_live_bytes
                        .saturating_add(Self::object_bytes_from_header(current));
                } else {
                    self.non_cons_object_addrs.remove(&(current as usize));
                    self.unregister_vector_object(current);
                    self.free_gc_object(current);
                    self.allocated_count = self.allocated_count.saturating_sub(1);
                    noncons_freed += 1;
                }
            }
            processed += 1;
        }

        let done = self.sweep_cons_cursor >= self.cons_blocks.len()
            && self.sweep_noncons_pending.is_null()
            && self.sweep_float_page_cursor >= self.float_arena.pages.len()
            && self.sweep_string_page_cursor >= self.string_arena.pages.len()
            && self.sweep_vector_page_cursor >= self.vector_arena.pages.len()
            && self.sweep_bytecode_page_cursor >= self.bytecode_arena.pages.len()
            && self.sweep_lambda_page_cursor >= self.lambda_arena.pages.len()
            && self.sweep_macro_page_cursor >= self.macro_arena.pages.len()
            && self.sweep_record_page_cursor >= self.record_arena.pages.len()
            && self.sweep_symbol_with_pos_page_cursor >= self.symbol_with_pos_arena.pages.len();
        let slice_us = t0.elapsed().as_micros() as u64;
        self.sweep_slice_us_total += slice_us;
        self.sweep_slice_count += 1;
        self.sweep_cons_blocks_swept += swept_blocks;
        self.sweep_noncons_freed += noncons_freed + float_freed;
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC sweep_slice {slice_us}us cons={}/{} noncons_left={} done={done}",
                self.sweep_cons_cursor,
                self.cons_blocks.len(),
                if self.sweep_noncons_pending.is_null() {
                    0
                } else {
                    1
                },
            );
        }
        if done {
            self.finish_incremental_sweep();
        }
        done
    }

    /// Drive the deferred sweep to completion in one shot (forced GC, or before
    /// the next mark / a stop-the-world collection can begin).
    pub(crate) fn finish_incremental_sweep_now(&mut self) {
        while self.sweep_in_progress {
            self.incremental_sweep_slice(usize::MAX);
        }
    }

    /// Finalize the deferred sweep: recompute the cons live count from the mark
    /// bitmaps (cheap popcount; counts allocate-black new conses, excludes
    /// reclaimed ones), fix the allocation accounting, and emit the cycle trace.
    fn finish_incremental_sweep(&mut self) {
        let recount: usize = self.cons_blocks.iter().map(ConsBlock::count_marked).sum();
        // allocated_count carries the tracked cons live count; replace it with
        // the true recount (delta may be negative -> use checked sub).
        if recount >= self.cons_live_count {
            self.allocated_count = self
                .allocated_count
                .saturating_add(recount - self.cons_live_count);
        } else {
            self.allocated_count = self
                .allocated_count
                .saturating_sub(self.cons_live_count - recount);
        }
        self.cons_live_count = recount;

        let _released_cons_blocks = self.release_empty_cons_blocks();
        let _released_object_pages = self.release_empty_object_pages();

        let mapped_cons_live: usize = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum();
        let cons_live_bytes = recount
            .saturating_add(mapped_cons_live)
            .saturating_mul(size_of::<ConsCell>());
        let mapped_object_live_bytes = self.mapped_non_cons_live_bytes();
        self.live_bytes = cons_live_bytes
            .saturating_add(self.sweep_noncons_live_bytes)
            .saturating_add(mapped_object_live_bytes);

        self.gc_collections += 1;
        self.sweep_lifetime_us += self.sweep_slice_us_total;
        self.sweep_lifetime_slices += self.sweep_slice_count;
        self.sweep_lifetime_cons_blocks_swept += self.sweep_cons_blocks_swept;
        self.sweep_lifetime_noncons_freed += self.sweep_noncons_freed;
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let (mapped_total, mapped_marked) = self.mapped_object_stats();
            eprintln!(
                "NEOVM_GC gc#{} [incremental mark={}us sweep_total={}us slices={} blocks={} \
                 noncons_freed={}] cons_live={} heap_noncons={} dump_marked={}/{} live={}B",
                self.gc_collections,
                self.sweep_mark_us,
                self.sweep_slice_us_total,
                self.sweep_slice_count,
                self.sweep_cons_blocks_swept,
                self.sweep_noncons_freed,
                self.cons_live_count,
                self.non_cons_object_addrs.len(),
                mapped_marked,
                mapped_total,
                self.live_bytes,
            );
        }
        // Owner-tracking remembered-set precursor: NOT cleared at sweep
        // completion. Its lifecycle is clear-at-BEGIN (`begin_collection`), the
        // ABA-safe per-cycle discipline shared with the SATB sets; see the note
        // in `begin_collection` and `complete_collection`.
        self.sweep_in_progress = false;
    }

    fn push_gray(&mut self, val: TaggedValue, origin: &str) {
        debug_assert!(val.is_heap_object());
        self.debug_assert_heap_tag_matches_header(val, origin);
        self.gray_queue.push(val);
    }

    fn mark_symbol(&mut self, id: SymId) {
        // GNU `mark_object` sets the symbol's mark bit and moves on.  Whether
        // the symbol is interned is a sweep-time question (`is_value_marked`),
        // not a mark-time one: asking `is_canonical_id` here cost an
        // epoch-checked thread-local lookup per symbol reference visited.
        self.marked_symbols.insert(id);
    }

    fn mark_or_push_child(&mut self, val: TaggedValue, origin: &str) {
        match val.kind() {
            crate::tagged::value::ValueKind::Symbol(id) => self.mark_symbol(id),
            _ if val.is_heap_object() => self.push_gray(val, origin),
            _ => {}
        }
    }

    #[cfg(debug_assertions)]
    fn debug_assert_heap_tag_matches_header(&self, val: TaggedValue, origin: &str) {
        if val.is_cons() {
            return;
        }

        let (ptr, expected) = if val.is_string() {
            (
                val.as_string_ptr().unwrap() as *const u8,
                HeapObjectKind::String,
            )
        } else if val.is_float() {
            (
                val.as_float_ptr().unwrap() as *const u8,
                HeapObjectKind::Float,
            )
        } else if val.is_veclike() {
            (
                val.as_veclike_ptr().unwrap() as *const u8,
                HeapObjectKind::VecLike,
            )
        } else {
            return;
        };

        if !self.owns_non_cons_object(ptr) {
            return;
        }

        let header = unsafe { &*(ptr as *const GcHeader) };
        assert_eq!(
            header.kind,
            expected,
            "GC gray queue received malformed tagged heap value from {origin}: \
             value={:#x}, ptr={:?}, tag={}, header.kind={:?}, expected={:?}",
            val.0,
            ptr,
            val.tag(),
            header.kind,
            expected
        );
    }

    #[cfg(not(debug_assertions))]
    fn debug_assert_heap_tag_matches_header(&self, _val: TaggedValue, _origin: &str) {}

    /// Mark a single tagged value and push its children onto the gray queue.
    fn mark_value(&mut self, val: TaggedValue) {
        if let crate::tagged::value::ValueKind::Symbol(id) = val.kind() {
            self.mark_symbol(id);
        } else if val.is_cons() {
            // GNU `mark_object`'s cdr-chasing loop: a list is marked along
            // its spine inline, without a gray-queue push + pop + re-dispatch
            // per cell — for a megacons list that round trip is a second
            // multi-megabyte buffer streamed through the cache.
            let mut ptr = val.xcons_ptr();
            while self.mark_cons(ptr) {
                let car = unsafe { (*ptr).load_car() };
                let cdr = unsafe { (*ptr).load_cdr() };
                self.mark_or_push_child(car, "cons-car");
                if cdr.is_cons() {
                    ptr = cdr.xcons_ptr();
                    continue;
                }
                self.mark_or_push_child(cdr, "cons-cdr");
                break;
            }
        } else if val.is_string() {
            let ptr = val.as_string_ptr().unwrap() as *mut StringObj;
            // Dump-span test first: a mapped string used to walk the string
            // arena and miss the residual addr-set before classification.
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_string_object(ptr as *const u8)
            {
                if self.mark_mapped_string(ptr) {
                    unsafe {
                        let intervals = (*ptr).data.intervals();
                        if !intervals.is_empty() {
                            intervals.for_each_root(|root| {
                                self.mark_or_push_child(root, "mapped-string-interval");
                            });
                        }
                    }
                }
                return;
            }
            unsafe {
                // TENURED SHORT-CIRCUIT before the bit read: tenured objects
                // stay in `non_cons_object_addrs`, so this owned arm sees them
                // too. Their bit froze at promotion; interpreting it against
                // the current parity would read "unmarked" every other cycle
                // and re-trace the old generation (and trip the partition/
                // tricolor verifiers). Tenured ≡ permanently marked, never
                // re-traced — identical to the frozen-`true` behavior the
                // parity scheme replaced.
                if (*ptr).header.tenured {
                    return;
                }
                if (*ptr).header.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).header.set_marked(self.mark_parity);
                let intervals = (*ptr).data.intervals();
                if !intervals.is_empty() {
                    intervals.for_each_root(|root| {
                        self.mark_or_push_child(root, "string-interval");
                    });
                }
            };
        } else if val.is_float() {
            let ptr = val.as_float_ptr().unwrap() as *mut FloatObj;
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_float_object(ptr as *const u8)
            {
                let _ = self.mark_mapped_float(ptr);
                return;
            }
            unsafe {
                // Tenured short-circuit before the bit read (see string arm).
                if (*ptr).header.tenured || (*ptr).header.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).header.set_marked(self.mark_parity);
            };
        } else if val.is_veclike() {
            let ptr = val.as_veclike_ptr().unwrap() as *mut VecLikeHeader;
            // Dump-span test first: mapped veclikes paid all six arena range
            // checks plus an FxHashSet miss per mark (~10.7M Ir in the
            // first-cycle window of the type sim) before being classified.
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_veclike_object(ptr as *const u8)
            {
                if self.mark_mapped_veclike(ptr) {
                    unsafe {
                        self.trace_veclike(ptr);
                    }
                }
                return;
            }
            unsafe {
                // Tenured short-circuit before the bit read (see string arm):
                // permanent-black, never re-traced.
                if (*ptr).gc.tenured {
                    return;
                }
                if (*ptr).gc.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).gc.set_marked(self.mark_parity);
                self.trace_veclike(ptr);
            }
        }
    }

    /// Mark a cons cell. Returns true if newly marked (not previously marked).
    fn mark_cons(&mut self, ptr: *const ConsCell) -> bool {
        // Mapped-world fast classification: in a fresh session MOST marked
        // conses are dump objects, and the old order made each of them miss
        // the block cache and probe `cons_block_index_by_base` before being
        // classified. Two compares against the dump span settle it first.
        let addr = ptr as usize;
        if addr >= self.dump_addr_lo && addr < self.dump_addr_hi {
            return self.mark_mapped_cons(ptr);
        }
        if ptr.is_null() || !ConsBlock::ptr_is_cell_aligned(ptr) {
            return self.mark_mapped_cons(ptr);
        }
        let block_base = ConsBlock::block_base_for_ptr(ptr);
        let block_index = match self.mark_cons_block_cache {
            Some(cache) if cache.block_base == block_base => cache.block_index,
            _ => {
                let Some(&block_index) = self.cons_block_index_by_base.get(&block_base) else {
                    return self.mark_mapped_cons(ptr);
                };
                self.mark_cons_block_cache =
                    Some(ConsBlockCacheEntry::new(block_base, block_index));
                block_index
            }
        };
        let block = &mut self.cons_blocks[block_index];
        if block.is_marked_ptr(ptr) {
            return false;
        }
        block.mark_ptr(ptr);
        true
    }

    fn mark_mapped_cons(&mut self, ptr: *const ConsCell) -> bool {
        for range in &mut self.mapped_cons_ranges {
            if !range.contains_ptr(ptr) {
                continue;
            }
            if range.is_marked_ptr(ptr) {
                return false;
            }
            range.mark_ptr(ptr);
            return true;
        }
        false
    }

    fn mark_mapped_float(&mut self, ptr: *const FloatObj) -> bool {
        for range in &mut self.mapped_float_ranges {
            if !range.contains_ptr(ptr) {
                continue;
            }
            if range.is_marked_ptr(ptr) {
                return false;
            }
            range.mark_ptr(ptr);
            return true;
        }
        false
    }

    fn mark_mapped_veclike(&mut self, ptr: *const VecLikeHeader) -> bool {
        let Some(&index) = self.mapped_veclike_index_by_addr.get(&(ptr as usize)) else {
            return false;
        };
        let object = &mut self.mapped_veclike_objects[index];
        debug_assert!(std::ptr::eq(object.header as *const VecLikeHeader, ptr));
        if object.marked {
            return false;
        }
        object.marked = true;
        true
    }

    fn mark_mapped_string(&mut self, ptr: *const StringObj) -> bool {
        let Some(&index) = self.mapped_string_index_by_addr.get(&(ptr as usize)) else {
            return false;
        };
        let object = &mut self.mapped_string_objects[index];
        debug_assert!(std::ptr::eq(object.ptr as *const StringObj, ptr));
        if object.marked {
            return false;
        }
        object.marked = true;
        true
    }

    /// Trace children of a vectorlike object, pushing them onto the gray queue.
    unsafe fn trace_veclike(&mut self, ptr: *mut VecLikeHeader) {
        match unsafe { (*ptr).type_tag } {
            VecLikeType::Vector => {
                let obj = ptr as *const VectorObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "vector-slot");
                }
            }
            VecLikeType::CharTable => {
                let obj = unsafe { &*(ptr as *const CharTableObj) };
                for (value, origin) in [
                    (load_value_atomic(&obj.defalt), "char-table-default"),
                    (load_value_atomic(&obj.parent), "char-table-parent"),
                    (load_value_atomic(&obj.purpose), "char-table-purpose"),
                    (load_value_atomic(&obj.ascii), "char-table-ascii"),
                ] {
                    self.mark_or_push_child(value, origin);
                }
                for slot in &obj.contents {
                    let val = load_value_atomic(slot);
                    self.mark_or_push_child(val, "char-table-content");
                }
                for val in obj.extras.iter_atomic() {
                    self.mark_or_push_child(val, "char-table-extra");
                }
            }
            VecLikeType::SubCharTable => {
                let obj = unsafe { &*(ptr as *const SubCharTableObj) };
                for val in obj.contents.iter_atomic() {
                    self.mark_or_push_child(val, "sub-char-table-content");
                }
            }
            VecLikeType::Record | VecLikeType::WindowConfiguration => {
                let obj = ptr as *const RecordObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "record-slot");
                }
            }
            VecLikeType::Font => {
                let font = unsafe { &(*(ptr as *const FontObj)).data };
                for val in font.fields.iter_atomic() {
                    self.mark_or_push_child(val, "font-property");
                }
                self.mark_or_push_child(load_value_atomic(&font.capability), "font-capability");
            }
            VecLikeType::HashTable => {
                let obj = ptr as *const HashTableObj;
                let ht = unsafe { &(*obj).table };
                if ht.weakness.is_some() {
                    // Weak table: DON'T trace its entries here — that would keep
                    // every key/value alive and defeat weakness. Record it; the
                    // per-entry survival decision happens in
                    // `mark_and_sweep_weak_tables` at the stop-the-world
                    // `complete_collection`, after the main mark drains (GNU
                    // `mark_and_sweep_weak_table_contents`). The remembered-set /
                    // SATB / permanent-scan paths now also defer weak entries
                    // (`register_weak_hash_table_for_sweep` registers the table
                    // and pushes only its non-weak closures), and a tenured/mapped
                    // weak table is re-registered every cycle via
                    // `permanent_weak_hash_tables`, so weak semantics hold for
                    // young, tenured, and dumped tables alike. The weak sweep runs
                    // before `verify_dump_partition`, so dead entries are removed
                    // before the verifier enumerates — no UAF.
                    let tptr = obj as *mut HashTableObj;
                    if self.weak_hash_tables_set.insert(tptr) {
                        self.weak_hash_tables.push(tptr);
                    }
                } else if let Some(pending) = ht.data.pending_entries() {
                    // Un-hydrated dump table: its entries live in the parked
                    // vec; trace exactly the set the hydrated arms below
                    // would (values + key snapshots - HashKeys are not
                    // walked in either form).
                    for (_, value, snapshot) in pending {
                        self.mark_or_push_child(*value, "hash-table-pending-value");
                        if let Some(snapshot) = snapshot {
                            self.mark_or_push_child(*snapshot, "hash-table-pending-key");
                        }
                    }
                } else {
                    // Trace all values in the hash table
                    for slot in ht.data.values() {
                        let val = load_value_atomic(slot);
                        self.mark_or_push_child(val, "hash-table-value");
                    }
                    // Trace key snapshots (original key objects)
                    for slot in ht.key_snapshots() {
                        let val = load_value_atomic(slot);
                        self.mark_or_push_child(val, "hash-table-key-snapshot");
                    }
                }
                // Custom test/hash closures (from `define-hash-table-test`) live
                // ONLY in these fields. Without tracing them the closure is swept
                // while the table is still live, and the next custom-test
                // gethash/puthash calls a freed function (use-after-free). The
                // fields are immutable after table creation, so a plain read is
                // race-free during a concurrent mark.
                if let Some(f) = ht.user_cmp_function {
                    self.mark_or_push_child(f, "hash-table-user-cmp");
                }
                if let Some(f) = ht.user_hash_function {
                    self.mark_or_push_child(f, "hash-table-user-hash");
                }
            }
            VecLikeType::Obarray => {
                let obj = unsafe { &*(ptr as *const ObarrayObj) };
                for val in obj.buckets.iter_atomic() {
                    self.mark_or_push_child(val, "obarray-bucket");
                }
            }
            VecLikeType::Lambda | VecLikeType::Macro => {
                // Closures are plain Value vectors (GNU PVEC_CLOSURE compat).
                // Trace ALL slots uniformly — no type-specific logic needed.
                let obj = ptr as *const LambdaObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "closure-slot");
                }
            }
            VecLikeType::ByteCode => {
                let obj = ptr as *const ByteCodeObj;
                let data = unsafe { &(*obj).data };
                // LAZY STUB LEG — lockstep with the collect arm: children
                // are read from the patched image, never from the (empty)
                // struct vectors, and nothing allocates under GC.
                if data.is_pdump_stub() {
                    unsafe {
                        crate::emacs_core::pdump::mapped_heap::for_each_stub_bytecode_child(
                            obj,
                            data.closure_slot_count,
                            |child| self.mark_or_push_child(child, "bytecode-stub-image"),
                        );
                    }
                    return;
                }
                self.mark_or_push_child(data.arglist, "bytecode-arglist");
                // Trace constants vector
                for val in &data.constants {
                    self.mark_or_push_child(*val, "bytecode-constant");
                }
                // Trace captured lexical environment
                if let Some(env) = data.env {
                    self.mark_or_push_child(env, "bytecode-env");
                }
                // Trace doc_form (can be a Value)
                if let Some(doc_form) = data.doc_form {
                    self.mark_or_push_child(doc_form, "bytecode-doc-form");
                }
                // Trace interactive spec
                if let Some(interactive) = data.interactive {
                    self.mark_or_push_child(interactive, "bytecode-interactive");
                }
                for val in &data.extra_slots {
                    self.mark_or_push_child(*val, "bytecode-extra-slot");
                }
            }
            VecLikeType::Overlay => {
                let obj = ptr as *const OverlayObj;
                let data = unsafe { &(*obj).data };
                // Trace the property list
                let plist = load_value_atomic(&data.plist);
                self.mark_or_push_child(plist, "overlay-plist");
            }
            VecLikeType::SymbolWithPos => {
                // Trace both the symbol and the position fields.
                let obj = ptr as *const SymbolWithPosObj;
                let sym = unsafe { (*obj).sym };
                let pos = unsafe { (*obj).pos };
                self.mark_or_push_child(sym, "symbol-with-pos-symbol");
                self.mark_or_push_child(pos, "symbol-with-pos-position");
            }
            VecLikeType::Finalizer => {
                // A REACHABLE finalizer keeps its function alive (GNU
                // `mark_vectorlike` on PVEC_FINALIZER). Unreachable ones are
                // handled at mark termination by
                // `mark_and_queue_doomed_finalizers`.
                let function = unsafe { (*(ptr as *const FinalizerObj)).function };
                self.mark_or_push_child(function, "finalizer-function");
            }
            VecLikeType::ModuleFunction => {
                let obj = ptr as *const ModuleFunctionObj;
                let doc = unsafe { (*obj).documentation };
                let interactive = unsafe { (*obj).interactive_form };
                self.mark_or_push_child(doc, "module-function-documentation");
                self.mark_or_push_child(interactive, "module-function-interactive");
            }
            VecLikeType::Xwidget => {
                let obj = ptr as *const XwidgetObj;
                let fields = unsafe {
                    [
                        (load_value_atomic(&(*obj).plist), "xwidget-plist"),
                        (load_value_atomic(&(*obj).type_), "xwidget-type"),
                        (load_value_atomic(&(*obj).buffer), "xwidget-buffer"),
                        (load_value_atomic(&(*obj).title), "xwidget-title"),
                        (
                            load_value_atomic(&(*obj).script_callbacks),
                            "xwidget-script-callbacks",
                        ),
                    ]
                };
                for (value, label) in fields {
                    self.mark_or_push_child(value, label);
                }
            }
            VecLikeType::XwidgetView => {
                let obj = ptr as *const XwidgetViewObj;
                let fields = unsafe {
                    [
                        ((*obj).model, "xwidget-view-model"),
                        ((*obj).window, "xwidget-view-window"),
                    ]
                };
                for (value, label) in fields {
                    self.mark_or_push_child(value, label);
                }
            }
            VecLikeType::Buffer
            | VecLikeType::Window
            | VecLikeType::Frame
            | VecLikeType::Timer
            | VecLikeType::Process
            | VecLikeType::Terminal
            | VecLikeType::Marker
            | VecLikeType::Subr
            | VecLikeType::Bignum
            | VecLikeType::Sqlite
            | VecLikeType::UserPtr
            | VecLikeType::SurfaceHandle
            | VecLikeType::VideoHandle => {
                // These have no Value children to trace.
                //
                // Bignums own a `malachite::Integer`, which manages
                // its own limb buffer, but no Lisp_Object children —
                // `Drop` takes care of the memory in `free_gc_object`.
                //
                // UserPtr has only a raw C pointer and finalizer, no
                // Lisp children.
                //
                // SurfaceHandle and VideoHandle contain only typed ids.
            }
        }
    }

    /// Sweep unmarked cons cells back to free lists.
    fn sweep_cons(&mut self) -> usize {
        let old_live = self.cons_live_count;
        let mut new_live = 0;
        self.cons_free_list = std::ptr::null_mut();
        for block in &mut self.cons_blocks {
            new_live += block.sweep(&mut self.cons_free_list);
        }
        self.cons_live_count = new_live;
        self.allocated_count = self
            .allocated_count
            .saturating_sub(old_live)
            .saturating_add(new_live);
        let mapped_live = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum::<usize>();
        new_live
            .saturating_add(mapped_live)
            .saturating_mul(size_of::<ConsCell>())
    }

    /// Drop ordinary cons blocks with no survivors and rebuild the intrusive
    /// free list plus base-address registry. The free list contains pointers
    /// into dead cells, so it must be discarded before empty block storage is
    /// deallocated and reconstructed from the retained blocks afterward.
    /// Call only after a complete eager or deferred sweep, when mark bits are
    /// the authoritative live-cell set and no sweep cursor remains active.
    fn release_empty_cons_blocks(&mut self) -> usize {
        let old_len = self.cons_blocks.len();
        if !self
            .cons_blocks
            .iter()
            .any(|block| block.count_marked() == 0)
        {
            return 0;
        }

        self.cons_free_list = std::ptr::null_mut();
        self.mark_cons_block_cache = None;
        self.cons_blocks.retain(|block| block.count_marked() != 0);
        self.cons_blocks.shrink_to_fit();

        self.cons_block_index_by_base =
            FxHashMap::with_capacity_and_hasher(self.cons_blocks.len(), Default::default());
        let mut rebuilt_live = 0usize;
        for (block_index, block) in self.cons_blocks.iter_mut().enumerate() {
            let previous = self
                .cons_block_index_by_base
                .insert(block.base_addr(), block_index);
            debug_assert!(previous.is_none(), "cons block base registered twice");
            rebuilt_live += block.sweep(&mut self.cons_free_list);
        }
        debug_assert_eq!(rebuilt_live, self.cons_live_count);

        old_len - self.cons_blocks.len()
    }

    /// Release every completely empty young arena page after a full sweep.
    /// Per-class indices and partial chains are rebuilt inside each arena.
    fn release_empty_object_pages(&mut self) -> usize {
        self.float_arena.release_empty_pages()
            + self.string_arena.release_empty_pages()
            + self.vector_arena.release_empty_pages()
            + self.bytecode_arena.release_empty_pages()
            + self.lambda_arena.release_empty_pages()
            + self.macro_arena.release_empty_pages()
            + self.record_arena.release_empty_pages()
            + self.symbol_with_pos_arena.release_empty_pages()
    }

    /// Sweep non-cons objects: walk intrusive list, free unmarked, rebuild list.
    fn sweep_objects(&mut self) -> usize {
        // `unchain_dead_markers` (invoked in `complete_collection`
        // between mark and sweep) has already spliced unmarked markers
        // out of every live buffer's intrusive chain, so freeing them
        // here leaves no dangling chain pointers. Mirrors GNU
        // `sweep_buffer → unchain_dead_markers` (alloc.c).
        // `all_objects` is young-only; interpret bits at the parity of the
        // cycle that just marked them (this eager sweep runs inside the same
        // collection, before any next flip).
        let parity = self.mark_parity;
        let mut prev: *mut *mut GcHeader = &mut self.all_objects;
        let mut current = self.all_objects;
        let mut live_bytes = 0usize;
        while !current.is_null() {
            unsafe {
                let next = (*current).next;
                debug_assert!(!(*current).tenured, "tenured object on the young list");
                if (*current).is_marked_at(parity) {
                    // Keep it — advance prev
                    live_bytes = live_bytes.saturating_add(Self::object_bytes_from_header(current));
                    prev = &mut (*current).next;
                    current = next;
                } else {
                    // Free it — unlink from list
                    *prev = next;
                    self.non_cons_object_addrs.remove(&(current as usize));
                    self.unregister_vector_object(current);
                    self.free_gc_object(current);
                    self.allocated_count = self.allocated_count.saturating_sub(1);
                    current = next;
                }
            }
        }

        live_bytes
    }

    /// Sweep every class arena's pages `[start, end)` (per-class ranges) —
    /// the shared page reclaimer behind both sweep entry points. See
    /// `ObjectArena::sweep_range` for the visit contract (allocated-bit-first,
    /// tenured-skip, drop-in-place-before-bit-clear, retired-page skip).
    /// Vector slots are evicted from the incremental vector registry at the
    /// free hook — page vectors never pass `unregister_vector_object`.
    /// Bytecode / lambda / macro / record have no side registry: their free
    /// hook is a no-op (the `drop_in_place` inside `sweep_range` frees the
    /// REAL payload — bytecode's ops + constants vectors, params, GNU byte
    /// maps, docstring; lambda/macro/record's slot `Vec`). SymbolWithPos is
    /// POD (no payload — its `drop_in_place` compiles out) and likewise has no
    /// registry.
    ///
    /// Returns `(survivor bytes, slots freed)` summed over the classes.
    // One `(start, end)` per size class — a mechanical fan-out over the
    // per-class arenas, not distinct conceptual parameters. The eager path
    // passes every class's full range; the incremental path passes one real
    // range and `(0, 0)` for the rest.
    #[allow(clippy::too_many_arguments)]
    fn sweep_arena_pages_ranges(
        &mut self,
        float_range: (usize, usize),
        string_range: (usize, usize),
        vector_range: (usize, usize),
        bytecode_range: (usize, usize),
        lambda_range: (usize, usize),
        macro_range: (usize, usize),
        record_range: (usize, usize),
        symbol_with_pos_range: (usize, usize),
    ) -> (usize, usize) {
        let parity = self.mark_parity;
        let (fl, ff) = self
            .float_arena
            .sweep_range(float_range.0, float_range.1, parity, |_| {});
        let (sl, sf) =
            self.string_arena
                .sweep_range(string_range.0, string_range.1, parity, |_| {});
        let (bl, bf) =
            self.bytecode_arena
                .sweep_range(bytecode_range.0, bytecode_range.1, parity, |_| {});
        let (lal, laf) =
            self.lambda_arena
                .sweep_range(lambda_range.0, lambda_range.1, parity, |_| {});
        let (mal, maf) = self
            .macro_arena
            .sweep_range(macro_range.0, macro_range.1, parity, |_| {});
        let (rel, ref_) =
            self.record_arena
                .sweep_range(record_range.0, record_range.1, parity, |_| {});
        let (swl, swf) = self.symbol_with_pos_arena.sweep_range(
            symbol_with_pos_range.0,
            symbol_with_pos_range.1,
            parity,
            |_| {},
        );
        let TaggedHeap {
            vector_arena,
            vector_object_addrs,
            ..
        } = self;
        let (vl, vf) = vector_arena.sweep_range(vector_range.0, vector_range.1, parity, |addr| {
            let removed = vector_object_addrs.remove(&addr);
            debug_assert!(removed, "freed page vector was not in the registry");
        });
        let freed = ff + sf + vf + bf + laf + maf + ref_ + swf;
        self.allocated_count = self.allocated_count.saturating_sub(freed);
        (fl + sl + vl + bl + lal + mal + rel + swl, freed)
    }

    /// `(total mapped objects, mapped objects currently marked)`.
    ///
    /// The marked count is how many immutable pdump (mapped) objects the mark
    /// phase re-traced this cycle — pure overhead that a "dump as permanent
    /// tenured region" partition would eliminate, since mapped objects are
    /// never freed. Used only for GC phase instrumentation.
    fn mapped_object_stats(&self) -> (usize, usize) {
        let veclike_total = self.mapped_veclike_objects.len();
        let veclike_marked = self
            .mapped_veclike_objects
            .iter()
            .filter(|object| object.marked)
            .count();
        let string_total = self.mapped_string_objects.len();
        let string_marked = self
            .mapped_string_objects
            .iter()
            .filter(|object| object.marked)
            .count();
        let cons_total: usize = self.mapped_cons_ranges.iter().map(|range| range.len).sum();
        let cons_marked: usize = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum();
        let float_total: usize = self.mapped_float_ranges.iter().map(|range| range.len).sum();
        let float_marked: usize = self
            .mapped_float_ranges
            .iter()
            .map(MappedFloatRange::live_count)
            .sum();
        (
            veclike_total + string_total + cons_total + float_total,
            veclike_marked + string_marked + cons_marked + float_marked,
        )
    }

    fn mapped_non_cons_live_bytes(&self) -> usize {
        self.mapped_float_ranges
            .iter()
            .map(|range| range.live_count().saturating_mul(size_of::<FloatObj>()))
            .chain(
                self.mapped_veclike_objects
                    .iter()
                    .filter(|object| object.marked)
                    .map(|object| object.byte_len),
            )
            .chain(
                self.mapped_string_objects
                    .iter()
                    .filter(|object| object.marked)
                    .map(|object| object.byte_len),
            )
            .sum()
    }

    /// Free a GC object by its header pointer.
    /// Must determine the actual type to call the correct Drop and dealloc.
    unsafe fn free_gc_object(&mut self, header: *mut GcHeader) {
        let kind = unsafe { (*header).kind };
        match kind {
            HeapObjectKind::String => {
                unsafe { drop(Box::from_raw(header as *mut StringObj)) };
            }
            HeapObjectKind::Float => {
                unsafe { drop(Box::from_raw(header as *mut FloatObj)) };
            }
            HeapObjectKind::VecLike => {
                let ptr = header as *mut VecLikeHeader;
                let type_tag = unsafe { (*ptr).type_tag };
                match type_tag {
                    VecLikeType::Vector => unsafe { drop(Box::from_raw(ptr as *mut VectorObj)) },
                    VecLikeType::CharTable => unsafe {
                        drop(Box::from_raw(ptr as *mut CharTableObj))
                    },
                    VecLikeType::SubCharTable => unsafe {
                        drop(Box::from_raw(ptr as *mut SubCharTableObj))
                    },
                    VecLikeType::HashTable => unsafe {
                        drop(Box::from_raw(ptr as *mut HashTableObj))
                    },
                    VecLikeType::Obarray => unsafe { drop(Box::from_raw(ptr as *mut ObarrayObj)) },
                    VecLikeType::Lambda => unsafe { drop(Box::from_raw(ptr as *mut LambdaObj)) },
                    VecLikeType::Macro => unsafe { drop(Box::from_raw(ptr as *mut MacroObj)) },
                    VecLikeType::ByteCode => unsafe {
                        // Residual-Box seam only: page bytecode never enters
                        // the intrusive lists this fn sweeps (task 03/3a —
                        // `alloc_bytecode` is page-only, so this arm is
                        // unreachable today; kept so any future Box producer
                        // stays leak-free by construction).
                        drop(Box::from_raw(ptr as *mut ByteCodeObj))
                    },
                    VecLikeType::Record | VecLikeType::WindowConfiguration => unsafe {
                        drop(Box::from_raw(ptr as *mut RecordObj))
                    },
                    VecLikeType::Font => unsafe { drop(Box::from_raw(ptr as *mut FontObj)) },
                    VecLikeType::Overlay => unsafe { drop(Box::from_raw(ptr as *mut OverlayObj)) },
                    VecLikeType::Marker => unsafe { drop(Box::from_raw(ptr as *mut MarkerObj)) },
                    VecLikeType::Buffer => unsafe { drop(Box::from_raw(ptr as *mut BufferObj)) },
                    VecLikeType::Window => unsafe { drop(Box::from_raw(ptr as *mut WindowObj)) },
                    VecLikeType::Frame => unsafe { drop(Box::from_raw(ptr as *mut FrameObj)) },
                    VecLikeType::Timer => unsafe { drop(Box::from_raw(ptr as *mut TimerObj)) },
                    VecLikeType::Process => unsafe { drop(Box::from_raw(ptr as *mut ProcessObj)) },
                    VecLikeType::Terminal => unsafe {
                        drop(Box::from_raw(ptr as *mut TerminalObj))
                    },
                    VecLikeType::Xwidget => unsafe { drop(Box::from_raw(ptr as *mut XwidgetObj)) },
                    VecLikeType::XwidgetView => unsafe {
                        drop(Box::from_raw(ptr as *mut XwidgetViewObj))
                    },
                    VecLikeType::SurfaceHandle => {
                        // A dead handle means Lisp dropped its last reference
                        // to the GPU surface: queue the id so the evaluator's
                        // post-collection drain can destroy the host objects.
                        // The sweep has no display-host access, so record only.
                        let obj = ptr as *mut SurfaceObj;
                        let surface_id = unsafe { (*obj).surface_id };
                        self.pending_surface_destroys.push(surface_id);
                        unsafe { drop(Box::from_raw(obj)) };
                    }
                    VecLikeType::VideoHandle => {
                        let obj = ptr as *mut VideoObj;
                        let video_id = unsafe { (*obj).video_id };
                        self.pending_video_destroys.push(video_id);
                        unsafe { drop(Box::from_raw(obj)) };
                    }
                    VecLikeType::Subr => unsafe { drop(Box::from_raw(ptr as *mut SubrObj)) },
                    VecLikeType::Bignum => unsafe {
                        // Box::drop runs malachite::Integer::drop, which
                        // frees the underlying limb buffer.
                        drop(Box::from_raw(ptr as *mut BignumObj))
                    },
                    VecLikeType::SymbolWithPos => unsafe {
                        drop(Box::from_raw(ptr as *mut SymbolWithPosObj))
                    },
                    VecLikeType::Finalizer => unsafe {
                        // The registry entry was already removed by the
                        // mark-termination scan that doomed this object; the
                        // function it queued survives independently.
                        drop(Box::from_raw(ptr as *mut FinalizerObj))
                    },
                    VecLikeType::Sqlite => unsafe { drop(Box::from_raw(ptr as *mut SqliteObj)) },
                    VecLikeType::UserPtr => {
                        // Call the finalizer if present before dropping.
                        let up = ptr as *mut UserPtrObj;
                        if let Some(fin) = unsafe { (*up).finalizer } {
                            unsafe { fin((*up).ptr) };
                        }
                        unsafe { drop(Box::from_raw(up)) };
                    }
                    VecLikeType::ModuleFunction => {
                        // Call the finalizer if present before dropping.
                        let mf = ptr as *mut ModuleFunctionObj;
                        if let Some(fin) = unsafe { (*mf).finalizer } {
                            unsafe { fin((*mf).data) };
                        }
                        unsafe { drop(Box::from_raw(mf)) };
                    }
                }
            }
        }
    }

    /// Per-kind ownership oracles (tag-first dispatch): each consults ONLY
    /// its class's page-span registry plus the residual `Box` addr-set, so a
    /// page hit can never be a cross-class collision. Mapped (pdump) objects
    /// answer false everywhere here — the not-owned fallback keeps routing
    /// them to the mapped side-table arms, unchanged.
    #[inline]
    fn owns_float_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.float_arena.owns(ptr) || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    #[inline]
    fn owns_string_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.string_arena.owns(ptr) || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    #[inline]
    fn owns_veclike_object(&self, ptr: *const u8) -> bool {
        // `VecLikeType::Vector`, `ByteCode`, `Lambda`, `Macro`, `Record`
        // (incl. the `WindowConfiguration` tag — same `RecordObj`), and
        // `SymbolWithPos` are paged (each in its own class arena — distinct
        // registries, so a hit is never a cross-class collision); every other
        // veclike is a residual `Box` in the addr-set.
        !ptr.is_null()
            && (self.vector_arena.owns(ptr)
                || self.bytecode_arena.owns(ptr)
                || self.lambda_arena.owns(ptr)
                || self.macro_arena.owns(ptr)
                || self.record_arena.owns(ptr)
                || self.symbol_with_pos_arena.owns(ptr)
                || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    /// Tag-dispatched ownership for a heap value whose raw object address is
    /// `addr` (`value_heap_addr`). The per-class page registries are checked
    /// per the value's TAG (never merged — see `ObjectArena`), with the
    /// residual addr-set covering the unmigrated `Box` types.
    #[inline]
    fn owns_heap_value_object(&self, value: TaggedValue, addr: usize) -> bool {
        let ptr = addr as *const u8;
        if value.is_string() {
            self.owns_string_object(ptr)
        } else if value.is_float() {
            self.owns_float_object(ptr)
        } else if value.is_veclike() {
            self.owns_veclike_object(ptr)
        } else {
            false
        }
    }

    /// Tag-less union oracle used by debug checks and GC tests that have not
    /// decoded the value's heap tag yet.
    #[cfg(any(debug_assertions, test))]
    fn owns_non_cons_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.string_arena.owns(ptr)
                || self.vector_arena.owns(ptr)
                || self.bytecode_arena.owns(ptr)
                || self.lambda_arena.owns(ptr)
                || self.macro_arena.owns(ptr)
                || self.record_arena.owns(ptr)
                || self.symbol_with_pos_arena.owns(ptr)
                || self.float_arena.owns(ptr)
                || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    /// Post-mark verification: check that every marked non-cons object is
    /// actually in one of our intrusive lists (young `all_objects` or tenured
    /// `tenured_objects`). If a marked object is NOT in a list, it means a
    /// root pointed to freed memory that happened to look like a valid tagged
    /// pointer — precisely the failure DIVERGENCES.md 161 chased for a day.
    ///
    /// Returns the number of problems found. Wired into `complete_collection`
    /// behind [`verify_marked_objects_enabled`], which is off unless
    /// `NEOVM_GC_VERIFY_MARKED=1` or a test turns it on: the walk is O(live
    /// objects) per collection. It was dead code with an `#[allow(dead_code)]`
    /// "delete or wire up" note until ledger 162 wired it up.
    #[cfg(debug_assertions)]
    fn verify_marked_objects_owned(&self) -> usize {
        let mut problems = 0usize;
        // Build a set of all owned non-cons object addresses
        let mut owned_addrs: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for head in [self.all_objects, self.tenured_objects] {
            let mut obj = head;
            while !obj.is_null() {
                owned_addrs.insert(obj as usize);
                unsafe {
                    obj = (*obj).next;
                }
            }
        }

        // Now walk both lists again and check marked objects. Tenured objects
        // are permanently marked (frozen bit, exempt from parity); young ones
        // are interpreted at the current parity.
        let parity = self.mark_parity;
        let mut total_marked = 0usize;
        for head in [self.all_objects, self.tenured_objects] {
            let mut current = head;
            while !current.is_null() {
                unsafe {
                    if (*current).tenured || (*current).is_marked_at(parity) {
                        total_marked += 1;
                        // Verify the object's internal data is sane
                        if (*current).kind == HeapObjectKind::String {
                            let ptr = current as *const StringObj;
                            let s = &(*ptr).data;
                            // Check string data pointer is reasonable
                            let str_ptr = s.as_bytes().as_ptr() as usize;
                            if str_ptr != 0 && str_ptr < 0x1000 {
                                problems += 1;
                                tracing::error!(
                                    "GC VERIFY: marked StringObj at {:p} has \
                                     corrupt data pointer {:#x}",
                                    current,
                                    str_ptr
                                );
                            }
                        }
                    }
                    current = (*current).next;
                }
            }
        }
        // OBJECT ARENA PAGES: page objects live outside the intrusive lists —
        // their ownership authority is the page-span oracle (per-class
        // registry + stride + allocation bitmap), NOT `non_cons_object_addrs`.
        // Walk them ALLOCATED-BIT-FIRST (reading a clear-bit slot's header
        // would itself be the class of bug this verifier hunts) and check each
        // live slot's header coherence and that it is NOT in the residual
        // addr-set (a page slot in the addr-set would be a double-ownership
        // corruption: two reclaimers for one object). Tenured slots are legal
        // (the promotion page walk) and count as marked, frozen-bit-first.
        fn verify_arena_slots<T: PagedObject>(
            arena: &ObjectArena<T>,
            non_cons_object_addrs: &FxHashSet<usize>,
            parity: bool,
            total_marked: &mut usize,
            problems: &mut usize,
        ) {
            for slot in arena.collect_allocated_slots() {
                let header = slot as *const GcHeader;
                unsafe {
                    if (*header).kind != T::KIND {
                        *problems += 1;
                        tracing::error!(
                            "GC VERIFY: {} arena slot at {:p} has a wrong-kind header",
                            T::CLASS,
                            slot
                        );
                    }
                    if (*header).tenured || (*header).is_marked_at(parity) {
                        *total_marked += 1;
                    }
                }
                if non_cons_object_addrs.contains(&(slot as usize)) {
                    *problems += 1;
                    tracing::error!(
                        "GC VERIFY: {} arena slot {:p} must NOT be in \
                         non_cons_object_addrs (page-span oracle owns it)",
                        T::CLASS,
                        slot
                    );
                }
            }
        }
        verify_arena_slots(
            &self.float_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.string_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.vector_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.bytecode_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.lambda_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.macro_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.record_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.symbol_with_pos_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        tracing::trace!(
            "GC verify: {} marked non-cons objects, {} problem(s)",
            total_marked,
            problems
        );
        problems
    }

    /// TEST-ONLY object-arena coherence check over ALL class arenas:
    /// the allocation bitmaps, occupancy counts, page-local free lists,
    /// partial-page chains, page-base registries, retirement invariants, the
    /// page-span ownership oracle, and the residual addr-set must all agree.
    /// Free lists are walked via the trailing link words only — a freed
    /// slot's header bytes are never read (allocated-bit-first applies to
    /// verifiers too). Page slots must NOT be in `non_cons_object_addrs`
    /// (the page-span oracle is their sole ownership authority — the
    /// INVERSE of the float-v1 assertion). For the vector arena, every
    /// allocated slot must be in the incremental vector registry; bytecode
    /// slots must NOT be (the registry is the Tier-B vector snapshot source
    /// — bytecode stays deferred-at-termination and has no registry).
    /// TEST-ONLY page-span ownership probe for the bytecode arena, for tests
    /// that live outside this module (the pdump restore-path round-trip).
    #[cfg(test)]
    pub(crate) fn bytecode_arena_owns_for_test(&self, ptr: *const u8) -> bool {
        self.bytecode_arena.owns(ptr)
    }

    /// TEST-ONLY mapped-image ownership probe: true when the value's storage
    /// lives inside the loaded dump image span (image-resident objects).
    #[cfg(test)]
    pub(crate) fn mapped_image_owns_for_test(&self, value: TaggedValue) -> bool {
        self.owner_is_mapped(value)
    }

    #[cfg(test)]
    pub(crate) fn assert_object_arenas_coherent(&self) {
        self.assert_one_arena_coherent(&self.float_arena);
        self.assert_one_arena_coherent(&self.string_arena);
        self.assert_one_arena_coherent(&self.vector_arena);
        self.assert_one_arena_coherent(&self.bytecode_arena);
        self.assert_one_arena_coherent(&self.lambda_arena);
        self.assert_one_arena_coherent(&self.macro_arena);
        self.assert_one_arena_coherent(&self.record_arena);
        self.assert_one_arena_coherent(&self.symbol_with_pos_arena);
        // Vector registry ⊇ page vector slots (page alloc inserts; page sweep
        // removes). The registry may also hold residual Box vectors.
        for slot in self.vector_arena.collect_allocated_slots() {
            assert!(
                self.vector_object_addrs.contains(&(slot as usize)),
                "allocated page vector slot {slot:p} missing from the vector registry",
            );
        }
        // Bytecode slots carry the right type tag (the generic per-arena
        // check can only see the shared VecLike GcHeader kind) and never
        // leak into the vector registry.
        for slot in self.bytecode_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::ByteCode,
                "bytecode arena slot {slot:p} carries a non-ByteCode type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "bytecode arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        // Lambda/macro slots carry their own type tag and never leak into the
        // vector registry (they have no side registry — like bytecode).
        for slot in self.lambda_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::Lambda,
                "lambda arena slot {slot:p} carries a non-Lambda type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "lambda arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::Macro,
                "macro arena slot {slot:p} carries a non-Macro type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "macro arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        // Record slots carry the Record or WindowConfiguration tag (same
        // `RecordObj`, distinct pseudovector type) and never leak into the
        // vector registry. Native FontObj values use the residual boxed path.
        for slot in self.record_arena.collect_allocated_slots() {
            let tag = unsafe { (*(slot as *const VecLikeHeader)).type_tag };
            assert!(
                matches!(tag, VecLikeType::Record | VecLikeType::WindowConfiguration),
                "record arena slot {slot:p} carries an unrelated tag ({tag:?})",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "record arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::SymbolWithPos,
                "symbol-with-pos arena slot {slot:p} carries a non-SymbolWithPos type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "symbol-with-pos arena slot {slot:p} must NOT be in the vector registry",
            );
        }
    }

    #[cfg(test)]
    fn assert_one_arena_coherent<T: PagedObject>(&self, arena: &ObjectArena<T>) {
        use std::collections::HashSet;
        // Partial chain: acyclic, flags consistent, members have free slots,
        // no retired page on the chain.
        let mut on_chain: HashSet<usize> = HashSet::new();
        let mut cursor = arena.partial_head;
        while cursor != PAGE_NONE {
            assert!(
                on_chain.insert(cursor),
                "partial chain cycle at page {cursor}"
            );
            let page = &arena.pages[cursor];
            assert!(
                page.on_partial,
                "chained page {cursor} not flagged on_partial"
            );
            assert!(!page.retired, "retired page {cursor} on the partial chain");
            assert_ne!(
                page.free_head, PAGE_NONE,
                "chained page {cursor} has an empty free list",
            );
            cursor = page.next_partial;
        }
        for (page_index, page) in arena.pages.iter().enumerate() {
            assert_eq!(
                arena.page_index_by_base.get(&page.base_addr()),
                Some(&page_index),
                "page-base registry mismatch for {} page {page_index} (retired \
                 pages must STAY registered)",
                T::CLASS,
            );
            assert_eq!(
                page.on_partial,
                on_chain.contains(&page_index),
                "page {page_index} on_partial flag disagrees with the chain",
            );
            if page.retired {
                // Retirement invariants: full, no free slots, off the chain.
                assert_eq!(
                    page.allocated,
                    ObjectPage::<T>::SLOTS,
                    "retired {} page {page_index} is not full",
                    T::CLASS,
                );
                assert_eq!(page.free_head, PAGE_NONE, "retired page with free slots");
                assert!(!page.on_partial, "retired page on the partial chain");
            }
            // Occupancy == bitmap popcount; every allocated slot is
            // bump-reached, answers OWNED via the page-span oracle, and is
            // NOT in the residual addr-set.
            let mut popcount = 0usize;
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                popcount += bits.count_ones() as usize;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    assert!(
                        index < page.next_index,
                        "allocated bit beyond the bump cursor",
                    );
                    let addr = page.slot_ptr(index) as usize;
                    assert!(
                        arena.owns(addr as *const u8),
                        "allocated {} slot {addr:#x} not owned by the page-span oracle",
                        T::CLASS,
                    );
                    assert!(
                        !self.non_cons_object_addrs.contains(&addr),
                        "{} arena slot {addr:#x} must NOT be in non_cons_object_addrs",
                        T::CLASS,
                    );
                }
            }
            assert_eq!(
                page.allocated, popcount,
                "page {page_index} occupancy != bitmap popcount",
            );
            // Free list: entries bump-reached, bit-clear, duplicate-free,
            // NOT owned per the oracle, and together with the allocated
            // slots exactly cover the bumped span.
            let mut free_seen: HashSet<usize> = HashSet::new();
            let mut fcursor = page.free_head;
            while fcursor != PAGE_NONE {
                assert!(
                    fcursor < page.next_index,
                    "free slot beyond the bump cursor"
                );
                assert!(
                    !page.is_allocated(fcursor),
                    "free-listed slot {fcursor} has its alloc bit set",
                );
                assert!(
                    !arena.owns(page.slot_ptr(fcursor) as *const u8),
                    "freed slot must answer NOT-owned (alloc-bit oracle)",
                );
                assert!(
                    free_seen.insert(fcursor),
                    "free-list cycle/duplicate at slot {fcursor}",
                );
                fcursor = unsafe { page.free_link_ptr(fcursor).read() };
            }
            assert_eq!(
                page.allocated + free_seen.len(),
                page.next_index,
                "page {page_index}: occupancy + free-list length != bump cursor",
            );
            if page.free_head != PAGE_NONE {
                assert!(
                    page.on_partial,
                    "page {page_index} has free slots but is off the partial chain",
                );
            }
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
pub(crate) mod alloc_probe {
    use super::{GcHeader, HeapObjectKind, TaggedHeap, VecLikeHeader, VecLikeType};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    const N_BUCKETS: usize = 11;

    // One declaration owns dense indices, report names, and fixed layouts.
    // Adding a VecLike kind cannot silently shift only one of three parallel
    // tables (the bug this replaces when PVEC_FONT was introduced).
    macro_rules! allocation_kinds {
        ($( $variant:ident => ($name:literal, $ty:ty) ),+ $(,)?) => {
            #[derive(Clone, Copy)]
            #[repr(usize)]
            enum AllocKind { $( $variant, )+ Count }

            const N_KINDS: usize = AllocKind::Count as usize;
            pub(crate) const KIND_NAMES: [&str; N_KINDS] = [$( $name, )+];
            const FIXED_SIZES: [usize; N_KINDS] = [$( std::mem::size_of::<$ty>(), )+];
        };
    }

    allocation_kinds! {
        String => ("String", super::StringObj),
        Float => ("Float", super::FloatObj),
        Vector => ("Vector", super::VectorObj),
        Bignum => ("Bignum", super::BignumObj),
        Marker => ("Marker", super::MarkerObj),
        Overlay => ("Overlay", super::OverlayObj),
        Finalizer => ("Finalizer", super::FinalizerObj),
        SymbolWithPos => ("SymbolWithPos", super::SymbolWithPosObj),
        UserPtr => ("UserPtr", super::UserPtrObj),
        Process => ("Process", super::ProcessObj),
        Frame => ("Frame", super::FrameObj),
        Window => ("Window", super::WindowObj),
        Buffer => ("Buffer", super::BufferObj),
        HashTable => ("HashTable", super::HashTableObj),
        Obarray => ("Obarray", super::ObarrayObj),
        Terminal => ("Terminal", super::TerminalObj),
        WindowConfig => ("WindowConfig", super::RecordObj),
        Subr => ("Subr", super::SubrObj),
        Xwidget => ("Xwidget", super::XwidgetObj),
        XwidgetView => ("XwidgetView", super::XwidgetViewObj),
        ModuleFunction => ("ModuleFunction", super::ModuleFunctionObj),
        Sqlite => ("Sqlite", super::SqliteObj),
        Lambda => ("Lambda", super::LambdaObj),
        CharTable => ("CharTable", super::CharTableObj),
        SubCharTable => ("SubCharTable", super::SubCharTableObj),
        Record => ("Record", super::RecordObj),
        Font => ("Font", super::FontObj),
        Macro => ("Macro", super::MacroObj),
        ByteCode => ("ByteCode", super::ByteCodeObj),
        Timer => ("Timer", super::TimerObj),
        SurfaceHandle => ("SurfaceHandle", super::SurfaceObj),
        VideoHandle => ("VideoHandle", super::VideoObj),
    }
    /// Histogram bucket upper bounds (bytes).
    pub(crate) const BUCKET_LABELS: [&str; N_BUCKETS] = [
        "<=16", "<=32", "<=64", "<=128", "<=256", "<=512", "<=1K", "<=4K", "<=16K", "<=64K", ">64K",
    ];

    #[allow(clippy::declare_interior_mutable_const)]
    const ZERO: AtomicU64 = AtomicU64::new(0);
    #[allow(clippy::declare_interior_mutable_const)]
    const ROW: [AtomicU64; N_BUCKETS] = [ZERO; N_BUCKETS];
    static COUNTS: [[AtomicU64; N_BUCKETS]; N_KINDS] = [ROW; N_KINDS];
    static TOTAL_BYTES: [AtomicU64; N_KINDS] = [ZERO; N_KINDS];
    static PEAK_ADDR_SET: AtomicUsize = AtomicUsize::new(0);

    fn kind_index(header: *const GcHeader) -> usize {
        let kind = match unsafe { (*header).kind } {
            HeapObjectKind::String => AllocKind::String,
            HeapObjectKind::Float => AllocKind::Float,
            HeapObjectKind::VecLike => {
                match unsafe { (*(header as *const VecLikeHeader)).type_tag } {
                    VecLikeType::Vector => AllocKind::Vector,
                    VecLikeType::Bignum => AllocKind::Bignum,
                    VecLikeType::Marker => AllocKind::Marker,
                    VecLikeType::Overlay => AllocKind::Overlay,
                    VecLikeType::Finalizer => AllocKind::Finalizer,
                    VecLikeType::SymbolWithPos => AllocKind::SymbolWithPos,
                    VecLikeType::UserPtr => AllocKind::UserPtr,
                    VecLikeType::Process => AllocKind::Process,
                    VecLikeType::Frame => AllocKind::Frame,
                    VecLikeType::Window => AllocKind::Window,
                    VecLikeType::Buffer => AllocKind::Buffer,
                    VecLikeType::HashTable => AllocKind::HashTable,
                    VecLikeType::Obarray => AllocKind::Obarray,
                    VecLikeType::Terminal => AllocKind::Terminal,
                    VecLikeType::WindowConfiguration => AllocKind::WindowConfig,
                    VecLikeType::Subr => AllocKind::Subr,
                    VecLikeType::Xwidget => AllocKind::Xwidget,
                    VecLikeType::XwidgetView => AllocKind::XwidgetView,
                    VecLikeType::ModuleFunction => AllocKind::ModuleFunction,
                    VecLikeType::Sqlite => AllocKind::Sqlite,
                    VecLikeType::Lambda => AllocKind::Lambda,
                    VecLikeType::CharTable => AllocKind::CharTable,
                    VecLikeType::SubCharTable => AllocKind::SubCharTable,
                    VecLikeType::Record => AllocKind::Record,
                    VecLikeType::Font => AllocKind::Font,
                    VecLikeType::Macro => AllocKind::Macro,
                    VecLikeType::ByteCode => AllocKind::ByteCode,
                    VecLikeType::Timer => AllocKind::Timer,
                    VecLikeType::SurfaceHandle => AllocKind::SurfaceHandle,
                    VecLikeType::VideoHandle => AllocKind::VideoHandle,
                }
            }
        };
        kind as usize
    }

    fn bucket(bytes: usize) -> usize {
        match bytes {
            0..=16 => 0,
            17..=32 => 1,
            33..=64 => 2,
            65..=128 => 3,
            129..=256 => 4,
            257..=512 => 5,
            513..=1024 => 6,
            1025..=4096 => 7,
            4097..=16384 => 8,
            16385..=65536 => 9,
            _ => 10,
        }
    }

    const BYTECODE_KIND: usize = AllocKind::ByteCode as usize;

    /// Backtrace hook (call-chain evidence for probes): while armed, capture
    /// a Rust backtrace for each ByteCode-kind allocation, up to the armed
    /// budget. Zero cost unless a probe arms it.
    static BC_TRACE_REMAINING: AtomicUsize = AtomicUsize::new(0);
    static BC_TRACES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Arm the ByteCode allocation backtrace hook for the next `n`
    /// ByteCode-kind allocations (clears previously captured traces).
    pub(crate) fn arm_bytecode_backtraces(n: usize) {
        BC_TRACES.lock().unwrap().clear();
        BC_TRACE_REMAINING.store(n, Ordering::SeqCst);
    }

    /// The backtraces captured since the last `arm_bytecode_backtraces`.
    pub(crate) fn bytecode_backtraces() -> Vec<String> {
        BC_TRACES.lock().unwrap().clone()
    }

    /// Record one non-cons allocation at link time (`link_object` /
    /// `link_veclike`). The object is fully constructed before it is linked,
    /// so reading its payload sizes here is sound.
    pub(crate) fn record(header: *const GcHeader, addr_set_len: usize) {
        let bytes = TaggedHeap::object_bytes_from_header(header);
        let k = kind_index(header);
        COUNTS[k][bucket(bytes)].fetch_add(1, Ordering::Relaxed);
        TOTAL_BYTES[k].fetch_add(bytes as u64, Ordering::Relaxed);
        PEAK_ADDR_SET.fetch_max(addr_set_len, Ordering::Relaxed);
        if k == BYTECODE_KIND
            && BC_TRACE_REMAINING
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_sub(1))
                .is_ok()
        {
            BC_TRACES
                .lock()
                .unwrap()
                .push(std::backtrace::Backtrace::force_capture().to_string());
        }
    }

    /// Zero every counter (start of a probe's measured phase).
    pub(crate) fn reset() {
        for row in &COUNTS {
            for cell in row {
                cell.store(0, Ordering::Relaxed);
            }
        }
        for cell in &TOTAL_BYTES {
            cell.store(0, Ordering::Relaxed);
        }
        PEAK_ADDR_SET.store(0, Ordering::Relaxed);
    }

    /// Peak `non_cons_object_addrs` population observed since reset.
    pub(crate) fn peak_addr_set() -> usize {
        PEAK_ADDR_SET.load(Ordering::Relaxed)
    }

    /// The fixed (arena-resident) struct size per kind index — what a
    /// size-class arena page would actually hold. Payload storage (`Vec`
    /// backings, string text, hash-table internals) stays on the system
    /// allocator either way.
    pub(crate) fn fixed_size(kind: usize) -> usize {
        FIXED_SIZES.get(kind).copied().unwrap_or(0)
    }

    /// Render the per-kind allocation table: count, total bytes, fixed
    /// (arena-resident) struct size, and the total-bytes histogram row.
    pub(crate) fn report() -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "{:<14} {:>10} {:>13} {:>6}  {}\n",
            "kind",
            "allocs",
            "total_bytes",
            "fixed",
            BUCKET_LABELS.join(" ")
        ));
        let mut grand_allocs = 0u64;
        let mut grand_bytes = 0u64;
        for k in 0..N_KINDS {
            let count: u64 = COUNTS[k].iter().map(|c| c.load(Ordering::Relaxed)).sum();
            if count == 0 {
                continue;
            }
            let bytes = TOTAL_BYTES[k].load(Ordering::Relaxed);
            grand_allocs += count;
            grand_bytes += bytes;
            let histo: Vec<String> = COUNTS[k]
                .iter()
                .map(|c| c.load(Ordering::Relaxed).to_string())
                .collect();
            out.push_str(&format!(
                "{:<14} {:>10} {:>13} {:>6}  {}\n",
                KIND_NAMES[k],
                count,
                bytes,
                fixed_size(k),
                histo.join(" ")
            ));
        }
        out.push_str(&format!(
            "TOTAL allocs={grand_allocs} bytes={grand_bytes} peak_non_cons_object_addrs={}\n",
            peak_addr_set()
        ));
        out
    }
}

#[cfg(test)]
mod layout_stats_tests {
    use super::*;
    use crate::heap_types::LispString;

    #[test]
    fn layout_stats_report_exact_page_occupancy_and_payload_capacity() {
        let mut heap = TaggedHeap::new();
        let _cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let _string = heap.alloc_string(LispString::from_utf8("abc"));
        let _vector = heap.alloc_vector(vec![TaggedValue::NIL; 3]);

        let stats = heap.layout_stats();
        assert_eq!(stats.allocated_objects, 3);
        assert_eq!(stats.cons.pages, 1);
        assert_eq!(stats.cons.live_slots, 1);
        assert_eq!(stats.cons.reclaimed_slots, 0);
        assert_eq!(stats.cons.occupied_bytes, size_of::<ConsCell>());

        let string = stats
            .arenas
            .iter()
            .find(|arena| arena.class == "string")
            .unwrap();
        assert_eq!(string.pages, 1);
        assert_eq!(string.allocated_slots, 1);
        assert_eq!(string.young_slots, 1);
        assert_eq!(string.payload_logical_bytes, 4); // "abc" + trailing NUL
        assert!(string.payload_capacity_bytes >= 4);
        assert_eq!(string.owned_payloads, 1);

        let vector = stats
            .arenas
            .iter()
            .find(|arena| arena.class == "vector")
            .unwrap();
        assert_eq!(vector.pages, 1);
        assert_eq!(vector.allocated_slots, 1);
        assert_eq!(vector.payload_logical_bytes, 3 * size_of::<TaggedValue>());
        assert!(vector.payload_capacity_bytes >= vector.payload_logical_bytes);
        assert_eq!(
            stats.page_backing_bytes,
            3 * 64 * 1024,
            "one cons, string, and vector page should be resident",
        );
    }

    #[test]
    fn completed_sweep_releases_empty_cons_blocks_and_rebuilds_free_list() {
        let mut heap = TaggedHeap::new();
        let survivor = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
        for i in 0..CONS_BLOCK_SIZE {
            let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
        }
        assert_eq!(heap.cons_blocks.len(), 2);

        heap.collect_exact(std::iter::once(survivor));

        assert_eq!(heap.cons_blocks.len(), 1);
        assert_eq!(heap.cons_block_index_by_base.len(), 1);
        assert_eq!(
            unsafe { (*survivor.xcons_ptr()).load_car() }.as_fixnum(),
            Some(7)
        );

        for i in 0..(CONS_BLOCK_SIZE - 1) {
            let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
        }
        assert_eq!(
            heap.cons_blocks.len(),
            1,
            "the rebuilt free list must reuse every dead cell in the survivor block",
        );
        let _ = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        assert_eq!(heap.cons_blocks.len(), 2);
    }
}

#[cfg(test)]
mod pacer_tests {
    use super::*;

    /// Forced (cap-hit) terminations escalate the lead without polluting the
    /// EWMAs; the next clean window's full recompute drops it back. The lead
    /// is the cap-pressure field detector (`mark_window`/`pace[]` trace
    /// lines) — the paced start trigger it once fed was reverted after the
    /// release-regime probe stayed dormant (task-3/5 reports).
    #[test]
    fn pacer_lead_escalates_on_forced_termination_and_recovers_on_clean() {
        let mut heap = TaggedHeap::new();
        heap.gc_threshold = 1_000_000;

        // First forced window seeds the lead from the truncated window's
        // own allocation (the only lower bound available).
        heap.pace_close_mark_window(50_000, 3_000_000, true);
        assert_eq!(heap.pace_lead_bytes, 3_000_000);
        assert_eq!(
            heap.pace_alloc_rate_bps, 0,
            "forced sample must not feed EWMA"
        );
        assert_eq!(heap.pace_mark_dur_us, 0, "forced sample must not feed EWMA");

        // Repeated cap hits double the lead.
        heap.pace_close_mark_window(50_000, 2_000_000, true);
        assert_eq!(heap.pace_lead_bytes, 6_000_000);

        // A clean quiet window recomputes the lead from the EWMAs directly
        // (seeded from this first clean sample): 1KB over 10ms -> ~1KB lead.
        heap.pace_close_mark_window(10_000, 1_024, false);
        assert_eq!(heap.pace_alloc_rate_bps, 102_400);
        assert_eq!(heap.pace_mark_dur_us, 10_000);
        assert_eq!(heap.pace_lead_bytes, 1_024);

        // Zero-wall windows (no stamp) leave the state untouched.
        heap.pace_close_mark_window(0, 999_999, false);
        assert_eq!(heap.pace_lead_bytes, 1_024);

        // Steady storm converges the EWMAs toward the sample: 8MB over
        // 100ms windows -> lead approaches 8MB (alpha 1/2 per cycle).
        for _ in 0..8 {
            heap.pace_close_mark_window(100_000, 8_000_000, false);
        }
        assert!(
            heap.pace_lead_bytes > 7_000_000,
            "lead should converge toward the storm's per-window allocation, got {}",
            heap.pace_lead_bytes
        );
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    /// TASK 10 — the `dirty_owners` ABA regression.
    ///
    /// The owner-tracking side tables (`dirty_owners` / `dirty_owner_bits` /
    /// `dirty_writes`) are the remembered-set precursor. Their dedup is keyed on
    /// the owner's address bits. If an entry recorded in one window survives into
    /// a later cycle's sweep, that cycle can FREE the owner and the arena can hand
    /// its slot (same size class ⇒ same address AND tag ⇒ identical bits) to a new
    /// object — whose barriered write is then wrongly deduped ("suppressed") by
    /// the stale entry. This test drives exactly that sequence with REAL frees and
    /// REAL deterministic arena slot reuse, and asserts the tables track the new
    /// occupant, not the freed ghosts.
    ///
    /// It has teeth only under clear-at-BEGIN: revert `begin_collection`'s
    /// `clear_dirty_owners()/clear_dirty_writes()` (restore the end-of-collection
    /// clears) and BOTH the post-`begin_collection` `== 0` assertion and the final
    /// `== 1` assertion fail (the stale O/Q entries linger, and O''s write is
    /// deduped against O's ghost).
    #[test]
    fn dirty_owner_tracking_is_cleared_at_begin_so_freed_slot_reuse_is_not_deduped() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
        set_tagged_heap(&mut heap);

        // --- Previous window: two GARBAGE owners O and Q, both mutated (barriered
        //     write) so both land in the owner tables. Neither is rooted, so the
        //     next collection will sweep them. ---
        let o = heap.alloc_vector(vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
        let q = heap.alloc_vector(vec![TaggedValue::fixnum(3), TaggedValue::fixnum(4)]);
        let o_addr = o.as_veclike_ptr().unwrap() as usize;
        assert!(crate::tagged::mutate::set_vector_slot(
            o,
            0,
            TaggedValue::fixnum(10)
        ));
        assert!(crate::tagged::mutate::set_vector_slot(
            q,
            0,
            TaggedValue::fixnum(20)
        ));
        assert_eq!(
            heap.dirty_owner_count(),
            2,
            "O and Q are both recorded dirty owners in this window"
        );

        // --- Start the collection that will free O and Q. Clear-at-begin empties
        //     the owner tables HERE, before the sweep can free-and-reuse a slot.
        //     (mark_all drains the internal runtime roots seeded by
        //     begin_collection so only the unrooted O and Q are swept.) ---
        heap.begin_collection();
        heap.mark_all();
        assert_eq!(
            heap.dirty_owner_count(),
            0,
            "begin_collection must clear owner tracking (clear-at-begin): a stale \
             pre-cycle entry that outlives this cycle's sweep is the ABA hazard",
        );

        // --- Sweep the vector arena: O and Q are unmarked ⇒ freed, their slots
        //     returned to the class free list. ---
        let vpages = heap.vector_arena.pages.len();
        let (_live, freed) = heap.sweep_arena_pages_ranges(
            (0, 0),
            (0, 0),
            (0, vpages),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        );
        assert!(
            freed >= 2,
            "the sweep must reclaim the two unrooted garbage vectors (freed={freed})",
        );

        // --- Deterministic reuse: allocating the same class pops the just-freed
        //     slots off the free list, so O's exact address recurs. ---
        let mut o_prime = None;
        for _ in 0..64 {
            let v = heap.alloc_vector(vec![TaggedValue::fixnum(0), TaggedValue::fixnum(0)]);
            if v.as_veclike_ptr().unwrap() as usize == o_addr {
                o_prime = Some(v);
                break;
            }
        }
        let o_prime =
            o_prime.expect("arena must hand O's freed slot back to a new same-class vector");
        assert_eq!(
            o_prime.as_veclike_ptr().unwrap() as usize,
            o_addr,
            "O' must occupy O's reclaimed slot (identical owner bits)",
        );

        // --- The barriered write to O' must be recorded as a FRESH owner. Under
        //     the ABA-prone clear-at-end lifecycle, O's ghost bits (== O''s bits)
        //     would dedup this write away, and Q's freed-but-uncleared entry would
        //     still inflate the count — so the table would read 2 ghosts, never
        //     the one true owner O'. ---
        assert!(crate::tagged::mutate::set_vector_slot(
            o_prime,
            1,
            TaggedValue::fixnum(99)
        ));
        assert!(
            heap.is_dirty_owner(o_prime),
            "O''s write must be recorded in the owner tables"
        );
        assert_eq!(
            heap.dirty_owner_count(),
            1,
            "exactly O' is dirty; a lingering ghost O (deduped) plus ghost Q \
             (freed, never cleared) would make this 2 under the stale-dedup ABA",
        );
    }

    #[test]
    fn heap_identity_is_unique_across_heap_lifetimes() {
        crate::test_utils::init_test_tracing();

        let first_id = TaggedHeap::new().identity();
        let second_id = TaggedHeap::new().identity();

        assert_ne!(first_id, second_id);
    }

    /// Phase 5: drive a non-blocking concurrent mark with the GC thread marking
    /// a large cons spine while THIS thread mutates (firing the SATB barrier) and
    /// allocates (allocate-black). The graph is large on purpose so marking is
    /// still in flight during the mutation, creating genuine overlap — run under
    /// ThreadSanitizer (`-Zsanitizer=thread`) this is the race check. The liveness
    /// asserts confirm the snapshot + SATB + allocate-black retain the right set.
    #[test]
    fn concurrent_mark_overlaps_mutation_and_retains_live_set() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A long reachable list: head -> ... -> tail (cdr-terminated with a
        // fixnum so traversal stops). Root = head.
        const N: i64 = 300_000;
        let mut list = TaggedValue::fixnum(0); // non-heap terminator
        for i in 0..N {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let head = list;
        // A second cons whose cdr we will rewire mid-mark (exercises SATB).
        let pivot = heap.alloc_cons(TaggedValue::fixnum(-1), head);
        // Unreachable garbage allocated before the mark begins.
        let _garbage = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
        let allocated_before = heap.cons_live_count;

        // Start the concurrent mark with `pivot` as the sole root (pivot -> head
        // -> whole list). begin_collection clears marks + seeds internal roots.
        heap.concurrent_begin();
        heap.seed_root(pivot);
        heap.launch_concurrent_mark();

        // While the GC thread marks: rewire pivot.cdr to a fresh cons D (the old
        // child `head` is logged to SATB and must stay live), and churn-allocate
        // (each new cons is born black). The list is long enough that the GC is
        // still traversing it during this.
        let d = heap.alloc_cons(TaggedValue::fixnum(7), head);
        assert!(crate::tagged::mutate::set_cons_cdr(pivot, d));
        for _ in 0..5_000 {
            let _ = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::fixnum(0));
        }

        // Wait for the GC thread to drain, then terminate stop-the-world.
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(pivot);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // The whole list (N) + pivot + D survive; `head` is retained as floating
        // garbage via SATB (it left pivot's cdr but was logged); the churn conses
        // are allocate-black so they survive this cycle too; only `_garbage` is
        // reclaimed. So exactly one cons (the pre-mark garbage) was swept.
        assert_eq!(
            heap.cons_live_count,
            allocated_before + 1 /* D */ + 5_000 /* churn */ - 1, /* garbage */
            "concurrent mark must retain the live + SATB + allocate-black set",
        );
        // The reachable spine is intact: walk pivot -> D -> head -> ... and check
        // a few cars (reading a swept cons would be caught by the sanitizer).
        let after_pivot = unsafe { (*pivot.xcons_ptr()).load_cdr() };
        assert!(after_pivot.is_cons());
        let head_again = unsafe { (*after_pivot.xcons_ptr()).load_cdr() };
        assert!(head_again.is_cons());
        assert_eq!(
            unsafe { (*head_again.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(N - 1).0,
        );
    }

    /// Gap 3 instrumentation: a deferred sweep must aggregate per-slice cost
    /// (slice count, total µs, cons blocks, non-cons frees) into `sweep_stats`
    /// and fold the cycle into the lifetime totals at completion.
    #[test]
    fn deferred_sweep_aggregates_slice_stats() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A small rooted list plus lots of garbage: dead conses spanning many
        // blocks and dead non-cons objects, so the sliced sweep has real work.
        let mut rooted = TaggedValue::fixnum(0);
        for i in 0..1_000 {
            rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
        }
        for i in 0..400_000 {
            let _ = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
        }
        for i in 0..4_000 {
            let _ = heap.alloc_float(i as f64);
        }

        // Mark to a fixpoint, arm the deferred sweep (the incremental
        // termination path), then drain it in bounded slices.
        heap.begin_collection();
        heap.seed_root(rooted);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());
        let mut slices = 1usize;
        while !heap.incremental_sweep_slice(8) {
            slices += 1;
        }
        assert!(!heap.sweep_in_progress());

        let stats = heap.sweep_stats();
        assert_eq!(stats.slice_count, slices);
        assert!(stats.slice_count > 1, "budget 8 must take several slices");
        assert!(stats.sweep_us > 0, "aggregated sweep cost must be non-zero");
        assert!(stats.cons_blocks_swept > 0);
        assert!(
            stats.noncons_freed >= 4_000,
            "the dead floats must be reclaimed by the deferred sweep \
             (freed={})",
            stats.noncons_freed,
        );
        assert_eq!(stats.lifetime_slices, stats.slice_count);
        assert_eq!(stats.lifetime_sweep_us, stats.sweep_us);
        assert_eq!(stats.lifetime_cons_blocks_swept, stats.cons_blocks_swept);
        assert_eq!(stats.lifetime_noncons_freed, stats.noncons_freed);
    }

    /// Gap 3 instrumentation: `join_concurrent_mark` must record how many
    /// GC-thread-parked (deferred) values the STW termination drain was handed
    /// — the number that sizes a records/closures/strings concurrent tier.
    #[test]
    fn concurrent_termination_records_deferred_drain_size() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A rooted cons spine carrying non-cons cars the claim dispatcher
        // REFUSES (records — floats/strings are claimed concurrently since
        // task 01 and never park): the GC thread marks the owned conses but
        // parks every record in `deferred`, so the termination drain size is
        // deterministically >= the car count.
        let mut list = TaggedValue::fixnum(0);
        for i in 0..1_000 {
            let car = heap.alloc_record(vec![TaggedValue::fixnum(i)]);
            list = heap.alloc_cons(car, list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_deferred >= 1_000,
            "every non-cons car must be parked for the termination drain \
             (deferred={})",
            stats.last_termination_deferred,
        );
        assert!(stats.max_termination_deferred >= stats.last_termination_deferred);
        assert_eq!(stats.last_termination_satb, 0, "no mutation ran mid-mark");

        // Finish the cycle cleanly: termination drain + deferred sweep.
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
        assert_eq!(heap.sweep_stats().noncons_freed, 0, "all records are live");
    }

    /// Handshake instrumentation (root-scan floor probe): a concurrent cycle
    /// must populate the heap-side `HandshakeStats` phases — the start
    /// handshake counter + cons/vector snapshot probes recorded by
    /// `concurrent_begin`/`launch_concurrent_mark`, and the termination
    /// counter + join cost recorded by `reseed_runtime_and_remembered_roots`/
    /// `join_concurrent_mark`. Heap-level only: the per-group context-root
    /// breakdown is evaluator-side and covered by
    /// `eval_test::gc_concurrent_handshake_stats_populate_per_group`.
    #[test]
    fn concurrent_handshake_records_heap_side_phases() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A rooted spine with a vector so the Tier B vector snapshot has at
        // least one entry to count.
        let vec = heap.alloc_vector(vec![TaggedValue::fixnum(3); 4]);
        let mut list = heap.alloc_cons(vec, TaggedValue::fixnum(0));
        for i in 0..100 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        let hs = heap.handshake_stats();
        assert_eq!(hs.start_count, 1, "one concurrent start handshake ran");
        assert_eq!(hs.term_count, 1, "one termination reseed ran");
        assert!(
            hs.probe_cons_blocks >= 1,
            "the cons-base snapshot walked at least one owned block"
        );
        assert!(
            hs.probe_vector_snapshot_len >= 1,
            "the Tier B snapshot captured the allocated vector (len={})",
            hs.probe_vector_snapshot_len,
        );
        assert_eq!(
            hs.probe_mapped_remembered, 0,
            "no dump partition on a bare heap"
        );
        assert_eq!(
            hs.last_term_remembered_roots, 0,
            "termination reseed saw no remembered owners on a bare heap"
        );
        // µs fields can legitimately round to 0 on a tiny heap; the counters
        // above prove the recording points fired. The max tracks the last.
        assert!(hs.max_start_total_us >= hs.last_start_total_us);
        assert!(hs.max_term_roots_total_us >= hs.last_term_roots_total_us);
    }

    /// Termination-drain kind probe: a concurrent cycle over a rooted spine
    /// carrying known counts of strings/records/closures/floats/hash-tables/
    /// vectors must classify every parked entry into the right bucket. Each
    /// value is reachable ONLY through the rooted cons spine, so the GC
    /// thread's cons walk discovers it and parks it in `deferred` (vectors
    /// included — Tier B traces their BACKINGS concurrently, but the vector
    /// VALUE is still parked for its header mark). CONCURRENT STRING MARKING:
    /// interval-FREE strings are now claimed on the GC thread instead of
    /// parked, so the `str` bucket counts only the interval-BEARING ones and
    /// the claim counter covers the rest.
    #[test]
    fn concurrent_termination_classifies_deferred_kinds() {
        use crate::emacs_core::value::{HashTableTest, LispHashTable};

        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        const N_STR: usize = 300;
        const N_STR_PROPS: usize = 40;
        const N_REC: usize = 200;
        const N_LAMBDA: usize = 150;
        const N_MACRO: usize = 30;
        const N_FLT: usize = 120;
        const N_HT: usize = 8;
        const N_VEC: usize = 50;

        let mut list = TaggedValue::fixnum(0);
        for _ in 0..N_STR {
            let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("drain-kind"));
            list = heap.alloc_cons(s, list);
        }
        // Interval-BEARING strings: still parked for the termination drain
        // (their interval children must be traced by `mark_value`).
        for _ in 0..N_STR_PROPS {
            let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("drain-props"));
            let payload = heap.alloc_cons(TaggedValue::fixnum(9), TaggedValue::fixnum(0));
            let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
            // Pre-mark direct install on a just-allocated string (unpublished
            // to any concurrent cycle yet).
            unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(payload) };
            list = heap.alloc_cons(s, list);
        }
        for i in 0..N_REC {
            let r = heap.alloc_record(vec![TaggedValue::fixnum(i as i64)]);
            list = heap.alloc_cons(r, list);
        }
        for _ in 0..N_LAMBDA {
            let c = heap.alloc_lambda(vec![TaggedValue::fixnum(1)]);
            list = heap.alloc_cons(c, list);
        }
        for _ in 0..N_MACRO {
            let m = heap.alloc_macro(vec![TaggedValue::fixnum(2)]);
            list = heap.alloc_cons(m, list);
        }
        for i in 0..N_FLT {
            let f = heap.alloc_float(i as f64);
            list = heap.alloc_cons(f, list);
        }
        for _ in 0..N_HT {
            let h = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));
            list = heap.alloc_cons(h, list);
        }
        for i in 0..N_VEC {
            let v = heap.alloc_vector(vec![TaggedValue::fixnum(i as i64); 4]);
            list = heap.alloc_cons(v, list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        let kinds = stats.last_termination_kinds;
        assert!(
            stats.last_concurrent_str_claimed >= N_STR,
            "interval-free strings are claimed concurrently, not parked \
             (claimed={})",
            stats.last_concurrent_str_claimed,
        );
        assert!(
            kinds.string >= N_STR_PROPS,
            "interval-bearing strings stay parked (str={})",
            kinds.string,
        );
        assert!(
            kinds.string < N_STR,
            "the interval-free majority must have left the parked buffer \
             (str={})",
            kinds.string,
        );
        assert!(
            kinds.record >= N_REC,
            "records parked (rec={})",
            kinds.record
        );
        assert!(
            kinds.closure >= N_LAMBDA + N_MACRO,
            "lambdas + macros share the closure bucket (clo={})",
            kinds.closure,
        );
        // Task 01: owned young page floats are claimed on the GC thread
        // (zero children), so the float bucket collapses and the claim
        // counter carries the count instead.
        assert!(
            stats.last_concurrent_float_claimed >= N_FLT,
            "page floats are claimed concurrently, not parked (claimed={})",
            stats.last_concurrent_float_claimed,
        );
        assert_eq!(
            kinds.float, 0,
            "no float may remain parked on a bare page-only heap (f={})",
            kinds.float,
        );
        assert!(
            kinds.hash_table >= N_HT,
            "hash tables parked (ht={})",
            kinds.hash_table,
        );
        // Task 01: owned page vectors' headers are claimed on the GC thread
        // (their backings already traced concurrently via Tier B), so the
        // vector bucket collapses and the claim counter carries the count.
        assert!(
            stats.last_concurrent_vec_claimed >= N_VEC,
            "page vectors' headers are claimed concurrently, not parked \
             (claimed={})",
            stats.last_concurrent_vec_claimed,
        );
        assert_eq!(
            kinds.vector, 0,
            "no vector may remain parked on a bare page-only heap (vec={})",
            kinds.vector,
        );
        assert_eq!(
            kinds.total(),
            stats.last_termination_deferred,
            "every deferred entry lands in exactly one bucket",
        );
        assert_eq!(stats.termination_count, 1);
        assert!(stats.max_termination_kinds.string >= kinds.string);
        assert!(stats.max_termination_kinds.record >= kinds.record);
        assert!(stats.max_termination_kinds.closure >= kinds.closure);

        // Finish the cycle cleanly: termination drain + deferred sweep.
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
        assert_eq!(heap.sweep_stats().noncons_freed, 0, "everything is rooted");
    }

    /// Build an interval table whose sole plist value is `v` (chars [0, 1)).
    /// `for_each_root` yields the plist (a heap cons chain carrying `v`), so
    /// marking the table's roots transitively keeps `v` alive. Allocates the
    /// plist conses on the thread-local tagged heap.
    fn interval_table_carrying(v: TaggedValue) -> crate::buffer::text_props::TextPropertyTable {
        use crate::buffer::text_props::{PropertyInterval, TextPropertyTable};
        let key = TaggedValue::fixnum(1);
        let mut properties = std::collections::HashMap::new();
        properties.insert(key, v);
        TextPropertyTable::from_dump(vec![PropertyInterval {
            start: 0,
            end: 1,
            properties,
            key_order: vec![key],
        }])
    }

    /// Drive one full concurrent cycle to completion: wait for the GC thread,
    /// terminate stop-the-world with `root` re-seeded, and drain the deferred
    /// sweep. Mirrors the driver's state machine (and the other tests here).
    fn finish_concurrent_cycle(heap: &mut TaggedHeap, root: TaggedValue) {
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// CONCURRENT STRING MARKING, load-bearing-barrier proof (production
    /// path): a string S whose interval table is the ONLY reference to value V
    /// has that table dropped MID-MARK through the `mutate.rs` wrapper. V must
    /// survive the cycle purely via the SATB pre-image log — whichever side of
    /// the clear the GC thread observed S on (non-null ⇒ deferred, then the
    /// termination traces an already-empty table; null ⇒ claimed, never
    /// re-traced).
    #[test]
    fn concurrent_string_claim_and_interval_clear_keep_children_alive() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // V (and its plist chain): reachable ONLY via S's interval table.
        let v = heap.alloc_cons(TaggedValue::fixnum(41), TaggedValue::fixnum(42));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("props"));
        {
            let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
            // Pre-mark install on a fresh string: no barrier needed yet.
            unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(v) };
        }
        // S2: interval-free — exercises the claim fast path alongside.
        let s2 = heap.alloc_string(crate::heap_types::LispString::from_utf8("plain"));
        // Long spine so the GC thread is (almost certainly) still marking the
        // list when the mutator clears. Both correctness outcomes are asserted
        // identically, so the race direction cannot break the test.
        let mut list = heap.alloc_cons(s2, TaggedValue::fixnum(0));
        list = heap.alloc_cons(s, list);
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // Mid-mark, on the mutator thread: drop S's whole interval table via
        // the barrier wrapper (fires the StringData SATB pre-image push AND
        // the enforced in-mutator interval barrier).
        let cleared = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
        assert!(cleared.is_some());

        finish_concurrent_cycle(&mut heap, root);

        // V survived the cycle purely via SATB.
        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(41).0,
        );
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        assert!(heap.owns_non_cons_object(s2.as_string_ptr().unwrap() as *const u8));
    }

    /// MID-MARK-BORN STRING GAINS INTERVALS (the SATB argument at the claim
    /// site, exercised end to end): a string S is allocated DURING a
    /// concurrent mark (born-at-parity — the GC thread will never trace it
    /// this cycle) and its freshly installed interval table becomes the ONLY
    /// reference to young cons C, whose original home is overwritten
    /// mid-mark. C must survive this cycle — not through S, but because the
    /// overwrite of its snapshot-reachable home fired the SATB deletion
    /// barrier (pre-image logged) — and the NEXT cycle must keep C alive
    /// through S's interval trace (`mark_value` re-traces fresh marks).
    #[test]
    fn concurrent_mark_born_string_interval_child_survives() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // C: young cons, reachable at the snapshot ONLY via home H's car.
        let c = heap.alloc_cons(TaggedValue::fixnum(71), TaggedValue::fixnum(72));
        let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
        // Long spine so the GC thread is still marking during the mutation.
        let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // Mid-mark: allocate S (page string, absent from this cycle's claim
        // snapshot only if it opened a fresh page — either way born-at-parity
        // keeps it alive), install a table carrying C, then sever C's
        // original home. The home overwrite fires the SATB pre-image barrier
        // (`set_cons_car` -> record_heap_write), which is what keeps C alive
        // this cycle; S's table is never traced this cycle.
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("mid-mark"));
        let installed = crate::tagged::mutate::with_string_text_props_mut(s, |t| {
            *t = interval_table_carrying(c);
        });
        assert!(installed.is_some());
        assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

        // Terminate with S re-seeded alongside the spine (S is a live value
        // the mutator holds; the explicit-roots harness must name it).
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(s);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // C survived its birth-cycle severing (SATB), S survived (born-at-
        // parity), and C's payload is intact.
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(71).0,
        );

        // NEXT full cycle: C is now reachable ONLY through S's intervals —
        // the termination's `mark_value` must trace them (S is white again
        // at the new parity, so it cannot be skipped as already-marked, and
        // its non-null interval word defers it to the STW trace).
        heap.concurrent_begin();
        heap.seed_root(root);
        heap.seed_root(s);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(s);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(71).0,
        );
    }

    /// Same as above, but the mid-mark clear BYPASSES the `mutate.rs` wrappers
    /// entirely (raw `clear_intervals` on the payload) — proving the SATB
    /// barrier is enforced INSIDE the `LispString` mutators and cannot be
    /// skipped by any call site.
    #[test]
    fn concurrent_string_raw_interval_clear_keeps_children_alive() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let v = heap.alloc_cons(TaggedValue::fixnum(51), TaggedValue::fixnum(52));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("raw-clear"));
        let s_ptr = s.as_string_ptr().unwrap() as *mut StringObj;
        unsafe { *(*s_ptr).data.intervals_mut() = interval_table_carrying(v) };
        let mut list = heap.alloc_cons(s, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // Raw mutator call — no wrapper, no note_heap_write. The enforced
        // in-mutator barrier inside clear_intervals must log V's plist.
        unsafe { (*s_ptr).data.clear_intervals() };

        finish_concurrent_cycle(&mut heap, root);

        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(51).0,
        );
        assert!(heap.owns_non_cons_object(s_ptr as *const u8));
    }

    /// The claim + clear flow under the ARMED partition/tricolor verifiers
    /// (`NEOVM_GC_VERIFY_PARTITION=1`): `verify_incremental_tricolor` is the
    /// oracle that a concurrently-claimed (black) string presents no
    /// black->white edge at termination. The fake dump span only activates the
    /// partition; it maps no objects, so every string stays span-outside
    /// (owned, claim-eligible).
    #[test]
    fn concurrent_string_claim_passes_partition_verifier() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        // First partitioned cycle promotes + blackens; verifiers arm after it.
        heap.begin_collection();
        heap.complete_collection();
        assert!(heap.dump_blackened);

        let v = heap.alloc_cons(TaggedValue::fixnum(61), TaggedValue::fixnum(62));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("verified"));
        let s_ptr = s.as_string_ptr().unwrap() as *mut StringObj;
        unsafe { *(*s_ptr).data.intervals_mut() = interval_table_carrying(v) };
        let s2 = heap.alloc_string(crate::heap_types::LispString::from_utf8("verified-free"));
        let mut list = heap.alloc_cons(s2, TaggedValue::fixnum(0));
        list = heap.alloc_cons(s, list);
        for i in 0..200_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        let _ = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
        // `incremental_finish` (inside) runs verify_dump_partition +
        // verify_incremental_tricolor and panics on any violation.
        finish_concurrent_cycle(&mut heap, root);

        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(61).0,
        );
        assert!(heap.owns_non_cons_object(s_ptr as *const u8));
        assert!(heap.owns_non_cons_object(s2.as_string_ptr().unwrap() as *const u8));
    }

    /// MAPPED-STRING CLASSIFICATION (regression guard for the mis-claim UAF):
    /// with the partition span covering a registered mapped string, the GC
    /// thread must DEFER it (its `GcHeader` bit untouched — mapped strings
    /// mark via the `MappedStringObject` side bool) and the termination must
    /// mark it on the mapped path and trace its interval child.
    #[test]
    fn concurrent_mark_defers_mapped_strings_and_marks_their_interval_children() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Fake-mapped string: a leaked StringObj registered exactly like the
        // pdump loader registers image objects (extends the dump span over it).
        let mapped = Box::into_raw(Box::new(StringObj {
            header: GcHeader::new(HeapObjectKind::String),
            data: crate::heap_types::LispString::from_utf8("mapped"),
        }));
        unsafe { heap.register_mapped_string_object(mapped, std::mem::size_of::<StringObj>()) };
        // C: heap value reachable ONLY via the mapped string's interval table.
        let c = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::fixnum(8));
        unsafe { *(*mapped).data.intervals_mut() = interval_table_carrying(c) };
        let mapped_val = unsafe { TaggedValue::from_string_ptr(mapped) };
        let root = heap.alloc_cons(mapped_val, TaggedValue::fixnum(0));

        // First cycle with a partition is a full trace (dump not blackened):
        // mapped marks were cleared, so the termination must re-mark the
        // mapped string and trace its intervals.
        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.string >= 1,
            "the mapped string must be parked, not claimed (str={})",
            stats.last_termination_kinds.string,
        );
        assert_eq!(
            stats.last_concurrent_str_claimed, 0,
            "nothing here is claim-eligible",
        );
        // Parity-aware form + raw pinning: a wrongful claim would swap in the
        // CURRENT parity (true here — exactly one begin_collection flip has
        // run), so assert both that the bit reads unmarked at this cycle's
        // parity and that the raw bit is still the untouched `false` it was
        // born with.
        assert!(
            unsafe { !(*mapped).header.is_marked_at(heap.mark_parity) },
            "a mapped string's GcHeader bit must never be claimed by the GC \
             thread (mapped marks live in the side table)",
        );
        assert!(
            unsafe { !(*mapped).header.is_marked() },
            "the mapped string's raw GcHeader bit must be untouched",
        );

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // Termination marked it on the mapped path and traced the child.
        let idx = heap.mapped_string_index_by_addr[&(mapped as usize)];
        assert!(heap.mapped_string_objects[idx].marked);
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(7).0,
        );

        // Free the fake image object after the heap is gone.
        drop(heap);
        let _ = unsafe { Box::from_raw(mapped) };
    }

    /// Build a leaked-static `SubrObj` exactly like the production
    /// constructor (`allocate_static_subr_object` `Box::leak`s and never
    /// registers with any heap list), returning the veclike value + raw ptr.
    fn leaked_test_subr() -> (TaggedValue, *mut crate::tagged::header::SubrObj) {
        let obj = Box::new(crate::tagged::header::SubrObj {
            header: VecLikeHeader::new(VecLikeType::Subr),
            sym_id: crate::emacs_core::intern::SymId(1),
            name: crate::emacs_core::intern::NameId(1),
            min_args: 1,
            max_args: Some(2),
            dispatch_kind: crate::tagged::header::SubrDispatchKind::Builtin,
            interactivity: crate::tagged::header::SubrInteractivity::NonInteractive,
            function: None,
        });
        let ptr = Box::into_raw(obj);
        let val = unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) };
        (val, ptr)
    }

    /// Task 01 SUBR RECOGNIZE-AND-DROP: a leaked-static subr (the only
    /// non-mapped subr population — `allocate_static_subr_object`
    /// `Box::leak`s and never links) discovered by the GC thread is DROPPED
    /// from the defer path: the subr bucket collapses to zero, the drop
    /// counter records it, and its header — dead state nobody reads — is
    /// never written. The subr stays permanently live with its payload
    /// intact (`is_value_marked` unconditionally true for
    /// not-owned/not-mapped).
    #[test]
    fn concurrent_leaked_subr_dropped_from_defer_path() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let (subr_val, subr_ptr) = leaked_test_subr();
        let root = heap.alloc_cons(subr_val, TaggedValue::fixnum(0));

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert_eq!(
            stats.last_termination_kinds.subr, 0,
            "a leaked subr must no longer park in `deferred` (sub={})",
            stats.last_termination_kinds.subr,
        );
        assert!(
            stats.last_concurrent_subr_dropped >= 1,
            "the drop must be counted (got {})",
            stats.last_concurrent_subr_dropped,
        );
        // Dead-state header: the raw bit is still the constructor's `false`
        // (a drop is NOT a claim).
        assert!(unsafe { !(*subr_ptr).header.gc.is_marked() });
        assert!(
            heap.is_value_marked(subr_val),
            "not-owned/not-mapped values answer unconditionally live",
        );

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // Subr payload intact after the full cycle (permanently live; the
        // sweep never visits it).
        unsafe {
            assert_eq!((*subr_ptr).min_args, 1);
            assert_eq!((*subr_ptr).max_args, Some(2));
            assert_eq!((*subr_ptr).header.type_tag, VecLikeType::Subr);
        }
        // Leaked on purpose, like production subrs (freeing it would U-A-F
        // the canonical registry pattern this mirrors).
    }

    /// Task 01 MAPPED-SUBR CLASSIFICATION (regression guard for the mis-drop
    /// UAF): with the partition span covering a registered mapped subr, the
    /// GC thread must DEFER it — the dump-span range check runs BEFORE the
    /// leaked-static recognition, because a mapped subr's mark lives in the
    /// `mapped_veclike_objects` side table that only the mutator's
    /// termination may write. The termination must mark it there, and the
    /// armed partition/tricolor verifiers must pass.
    #[test]
    fn concurrent_mapped_subr_still_deferred_and_side_table_marked() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Fake-mapped subr: registered exactly like the pdump loader
        // registers image veclikes (extends the dump span over it).
        let (subr_val, mapped) = leaked_test_subr();
        unsafe {
            heap.register_mapped_veclike_object(
                mapped as *mut VecLikeHeader,
                std::mem::size_of::<crate::tagged::header::SubrObj>(),
            )
        };
        let root = heap.alloc_cons(subr_val, TaggedValue::fixnum(0));

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.subr >= 1,
            "the mapped subr must be parked, not dropped (sub={})",
            stats.last_termination_kinds.subr,
        );
        assert_eq!(
            stats.last_concurrent_subr_dropped, 0,
            "nothing here is a leaked static",
        );
        assert!(
            unsafe { !(*mapped).header.gc.is_marked() },
            "a mapped subr's GcHeader bit must never be written by the GC \
             thread (mapped marks live in the side table)",
        );

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // The termination marked it via the mapped side table.
        let idx = heap.mapped_veclike_index_by_addr[&(mapped as usize)];
        assert!(heap.mapped_veclike_objects[idx].marked);

        // Free the fake image object after the heap is gone.
        drop(heap);
        let _ = unsafe { Box::from_raw(mapped) };
    }

    /// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (a): a page vector reachable
    /// only via a rooted cons is claimed on the GC thread (header black at
    /// parity, vec bucket empty, claim counter hot), its children survive
    /// through the Tier-B backing scan, and a garbage vector plus its
    /// otherwise-unreachable child are collected by the cycle's sweep.
    #[test]
    fn concurrent_vector_header_claimed_children_survive_garbage_freed() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Live: root cons -> V[c, s]; c and s are reachable ONLY through V.
        let c = heap.alloc_cons(TaggedValue::fixnum(11), TaggedValue::fixnum(12));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-kid"));
        let v = heap.alloc_vector(vec![c, s]);
        let root = heap.alloc_cons(v, TaggedValue::fixnum(0));
        // Garbage: G[cg] with no inbound edge.
        let cg = heap.alloc_cons(TaggedValue::fixnum(13), TaggedValue::fixnum(14));
        let g = heap.alloc_vector(vec![cg]);
        let g_ptr = g.as_veclike_ptr().unwrap();

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_concurrent_vec_claimed >= 1,
            "the rooted page vector's header must be claimed on the GC \
             thread (claimed={})",
            stats.last_concurrent_vec_claimed,
        );
        assert_eq!(
            stats.last_termination_kinds.vector, 0,
            "no vector may park on a bare page-only heap (vec={})",
            stats.last_termination_kinds.vector,
        );
        // Claimed ≡ black at THIS cycle's parity.
        assert!(unsafe {
            (*v.as_veclike_ptr().unwrap())
                .gc
                .is_marked_at(heap.mark_parity)
        });

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // Children survived via the Tier-B backing scan (the claimed header
        // was never re-traced at termination).
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(11).0,
        );
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
        // The garbage vector (and with it its only reference to cg) is gone.
        assert!(
            !heap.owns_non_cons_object(g_ptr as *const u8),
            "the unrooted vector must be reclaimed",
        );
        heap.assert_object_arenas_coherent();
    }

    /// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (b), THE ADVERSARIAL ONE: a
    /// vector allocated MID-CYCLE into a REUSED SLOT of an
    /// already-snapshotted page (page-base HIT — it does NOT defer) holds
    /// the only surviving reference to child C after C's snapshot home is
    /// severed. C must survive: not through the vector (born-at-parity ⇒
    /// the claim arm treats it as already-marked ⇒ never traced this cycle;
    /// its backing is absent from the Tier-B snapshot) but through the SATB
    /// deletion barrier on the home overwrite. Runs with the partition +
    /// tricolor verifiers armed — `verify_incremental_tricolor` is the
    /// oracle for the removed termination re-trace backstop.
    #[test]
    fn concurrent_mid_cycle_vector_in_reused_slot_keeps_child_alive() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16); // activates the partition

        // Page setup: keeper pins the page; v_dead's slot becomes the free
        // slot the mid-cycle allocation will reuse.
        let v_keep = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
        let v_dead = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
        let dead_ptr = v_dead.as_veclike_ptr().unwrap() as usize;
        // C: young cons, reachable at the snapshot ONLY via home H's car.
        let c = heap.alloc_cons(TaggedValue::fixnum(81), TaggedValue::fixnum(82));
        let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
        // Long rooted spine (home at the bottom) so the GC thread is still
        // walking when the mutator severs; both race outcomes are asserted
        // identically (if the GC got to H first, C is simply already black).
        let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
        list = heap.alloc_cons(v_keep, list);
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;
        // Bootstrap STW cycle: blackens the fake dump (arming the
        // verifiers), promotes survivors, and frees v_dead's slot.
        heap.collect_exact(std::iter::once(root));
        let pre_launch_bases: std::collections::HashSet<usize> = heap
            .vector_arena
            .pages
            .iter()
            .map(|p| p.base_addr())
            .collect();

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-CYCLE: allocate V_NEW carrying C — the arena's class free list
        // hands back v_dead's slot (page-base in this cycle's snapshot) —
        // then sever C's original home (fires the SATB pre-image barrier).
        let v_new = heap.alloc_vector(vec![c]);
        let new_ptr = v_new.as_veclike_ptr().unwrap() as usize;
        assert_eq!(
            new_ptr, dead_ptr,
            "the mid-cycle vector must land in the freed slot of a \
             snapshotted page (allocator changed? fix the test setup)",
        );
        assert!(
            pre_launch_bases.contains(&(new_ptr & !(OBJECT_PAGE_ALIGN - 1))),
            "the reused slot's page must be in this cycle's snapshot",
        );
        assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

        // Terminate with v_new re-seeded alongside the spine (it is a live
        // value the mutator holds; the explicit-roots harness must name it).
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(v_new);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        // Runs verify_dump_partition + verify_incremental_tricolor (armed
        // above): a black v_new with a white C would panic here.
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // C survived its birth-cycle severing (SATB), with payload intact.
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(81).0,
        );
        assert!(heap.owns_non_cons_object(v_new.as_veclike_ptr().unwrap() as *const u8));

        // NEXT full cycle: C is now reachable ONLY through V_NEW's backing —
        // the fresh Tier-B snapshot must carry it.
        heap.concurrent_begin();
        heap.seed_root(root);
        heap.seed_root(v_new);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(v_new);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(81).0,
        );
    }

    /// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (c): a vector whose backing
    /// is BULK-MUTATED mid-mark (`with_vector_data_mut` clone-on-write)
    /// while its header was claimed. Old-backing children survive via the
    /// retire (the Tier-B snapshot keeps reading the retired original);
    /// the new contents survive via SATB/born-black. Both race directions
    /// (claim before/after the mutation) assert identically.
    #[test]
    fn concurrent_vector_bulk_cow_while_header_claimed() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // C_OLD is reachable ONLY through V's start-of-cycle backing.
        let c_old = heap.alloc_cons(TaggedValue::fixnum(91), TaggedValue::fixnum(92));
        let v = heap.alloc_vector(vec![c_old]);
        let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-MARK bulk mutation through the production wrapper: replaces
        // the whole backing (clone-on-write retires the original the GC's
        // snapshot points at) and grows it (realloc — the historical TOCTOU
        // shape). C_OLD's only reference is now the retired buffer.
        let c_new = heap.alloc_cons(TaggedValue::fixnum(93), TaggedValue::fixnum(94));
        let mutated = crate::tagged::mutate::with_vector_data_mut(v, |d| {
            d.clear();
            d.push(c_new);
            for i in 0..64 {
                d.push(TaggedValue::fixnum(i));
            }
        });
        assert!(mutated.is_some());

        finish_concurrent_cycle(&mut heap, root);

        let stats = heap.sweep_stats();
        assert!(
            stats.last_concurrent_vec_claimed >= 1,
            "V's header claim races the mutation but must land either way \
             (claimed={})",
            stats.last_concurrent_vec_claimed,
        );
        // Old-backing child survived via the retired buffer's Tier-B scan
        // (+ the VectorBulk SATB pre-image log).
        assert_eq!(
            unsafe { (*c_old.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(91).0,
        );
        // New content survived (allocate-black + live backing).
        assert_eq!(
            unsafe { (*c_new.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(93).0,
        );
        assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
    }

    /// Task 01 INSERTION-COVERAGE (the regression the vm_mapatoms SIGSEGV
    /// exposed): a pre-existing value held only "in a register" (a Rust
    /// local the explicit-roots harness does not seed — root→heap motion)
    /// is stored mid-cycle into an already-CLAIMED vector's slot. The SATB
    /// deletion barrier only logs pre-images and the claimed header
    /// suppresses the termination re-trace, so ONLY the dirty-owner
    /// insertion re-trace at `join_concurrent_mark` keeps the value alive.
    #[test]
    fn concurrent_vector_slot_insertion_of_inflight_value_survives() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // X: allocated BEFORE the cycle, never seeded as a root this cycle —
        // an in-flight register value.
        let x = heap.alloc_float(6.5);
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
        let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-CYCLE: root→heap motion into the (likely already claimed)
        // vector through the production barrier path.
        assert!(crate::tagged::mutate::set_vector_slot(v, 0, x));

        finish_concurrent_cycle(&mut heap, root);

        assert!(
            heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
            "the inserted in-flight value must survive via the dirty-owner \
             insertion re-trace",
        );
        assert!((x.xfloat() - 6.5).abs() < f64::EPSILON);
        // V's slot still reads X (no dangling slot).
        let slot0 = unsafe {
            (*(v.as_veclike_ptr().unwrap() as *const VectorObj))
                .data
                .load_atomic(0)
        };
        assert_eq!(slot0.0, x.0);
    }

    /// Same insertion-coverage regression through the BULK path: the value
    /// is pushed into the claimed vector via `with_vector_data_mut`
    /// (clone-on-write) — the post-mutation backing is only reachable
    /// through the dirty-owner re-trace.
    #[test]
    fn concurrent_vector_bulk_insertion_of_inflight_value_survives() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let x = heap.alloc_float(7.5);
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
        let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        let mutated = crate::tagged::mutate::with_vector_data_mut(v, |d| {
            d.push(x);
        });
        assert!(mutated.is_some());

        finish_concurrent_cycle(&mut heap, root);

        assert!(
            heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
            "the bulk-inserted in-flight value must survive via the \
             dirty-owner insertion re-trace",
        );
        assert!((x.xfloat() - 7.5).abs() < f64::EPSILON);
    }

    /// #17 — CONS INTERIOR under concurrent marking. A value `x` reachable at
    /// the snapshot ONLY through a pre-existing cons `p` is re-homed MID-MARK
    /// into a FRESH (born-black) cons `c` and unlinked from `p`, both via the
    /// production `mutate::set_cons_*` deletion barriers. `x` must survive: it
    /// was snapshot-reachable (grayed when `p` is traced, or logged by the cons
    /// deletion barrier on the unlink race), and the born-black `c` is merely
    /// another reference to the already-protected value.
    ///
    /// Deliberate asymmetry vs the vector insertion tests above: conses are
    /// EXCLUDED from the dirty-owner re-gray (`satb_snapshotted_owners`, see
    /// `record_heap_write`), so a fresh cons has NO fix-(2) insertion net. Cons
    /// interiors are sound purely by SATB provenance — the value MUST be
    /// snapshot-reachable (precise rooting). An UNSEEDED value laundered through
    /// a fresh cons is CORRECTLY swept (a root-discipline violation the STW
    /// collector mishandles at the same safe point too), so this test keeps `x`
    /// snapshot-reachable. See CONCURRENT_GC.md, "Insertion coverage".
    #[test]
    fn concurrent_fresh_cons_interior_of_snapshot_value_survives() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // x reachable at the snapshot ONLY via p.car; p is buried in the seeded
        // list so it is traced late (the deletion barrier is the race net).
        let x = heap.alloc_float(6.5);
        let p = heap.alloc_cons(x, TaggedValue::fixnum(0));
        let mut list = heap.alloc_cons(p, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-CYCLE: re-home x into a FRESH born-black cons reachable from the
        // seeded root (p.cdr), then UNLINK x from p.car — both barriered.
        let c = heap.alloc_cons(x, TaggedValue::fixnum(0));
        assert!(crate::tagged::mutate::set_cons_cdr(p, c));
        assert!(crate::tagged::mutate::set_cons_car(
            p,
            TaggedValue::fixnum(99)
        ));

        finish_concurrent_cycle(&mut heap, root);

        assert!(
            heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
            "a snapshot-reachable value re-homed into a fresh born-black cons \
             must survive (SATB provenance; conses have no dirty-owner net)",
        );
        assert!((x.xfloat() - 6.5).abs() < f64::EPSILON);
    }

    /// #18 — MODULE-FUNCTION `interactive_form` barrier. A value V reachable
    /// ONLY through a live `ModuleFunctionObj.interactive_form` slot is
    /// overwritten MID-MARK (as `module_make_interactive` does), preceded by the
    /// `note_heap_write(ModuleFunction)` SATB barrier the write site now fires.
    /// V must survive purely via the barrier's pre-image log: the object is
    /// Box-allocated ⇒ deferred ⇒ traced at STW on its CURRENT (overwritten)
    /// form, so the barrier is V's ONLY net. Guards the barrier + `ModuleFunction`
    /// coverage in `collect_veclike_children` (drop either ⇒ V is swept).
    #[test]
    fn concurrent_module_function_interactive_form_overwrite_keeps_child_alive() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // V reachable ONLY via mf.interactive_form.
        let v = heap.alloc_float(9.5);
        let mf = heap.alloc_module_function(
            0,
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            TaggedValue::fixnum(0),
            v,
        );
        // Bury mf in a seeded list so the mark takes real time.
        let mut list = heap.alloc_cons(mf, TaggedValue::fixnum(0));
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-CYCLE: overwrite interactive_form (unlinking V) exactly as
        // module_make_interactive does — SATB barrier BEFORE the raw store.
        note_heap_write(mf, HeapWriteKind::ModuleFunction);
        unsafe {
            let mf_ptr = mf.as_veclike_ptr().unwrap() as *mut ModuleFunctionObj;
            (*mf_ptr).interactive_form = TaggedValue::fixnum(99);
        }

        finish_concurrent_cycle(&mut heap, root);

        assert!(
            heap.owns_non_cons_object(v.as_float_ptr().unwrap() as *const u8),
            "the overwritten interactive_form value must survive via the SATB \
             pre-image barrier (module-function objects are Box-deferred and \
             traced at STW on their CURRENT form only)",
        );
        assert!((v.xfloat() - 9.5).abs() < f64::EPSILON);
    }

    /// Task 01 MAPPED-VECTOR CLASSIFICATION (d): a registered mapped vector
    /// page-MISSES the claim arm and keeps the STW defer path — the
    /// termination marks it via the mapped side table AND re-traces its
    /// CURRENT backing (`trace_veclike`), keeping its child alive; its
    /// `GcHeader` bit is never written by the GC thread. (Box-residual
    /// vectors are NOT constructible today — `alloc_vector` is the single
    /// Vector chokepoint and the pdump restore writes into mapped storage —
    /// so the Box population has no test; a miss would merely defer.)
    #[test]
    fn concurrent_mapped_vector_still_deferred_and_traced() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // C: heap value reachable ONLY via the mapped vector's slot.
        let c = heap.alloc_cons(TaggedValue::fixnum(21), TaggedValue::fixnum(22));
        let mapped = Box::into_raw(Box::new(VectorObj {
            header: VecLikeHeader::new(VecLikeType::Vector),
            data: vec![c].into(),
        }));
        unsafe {
            heap.register_mapped_veclike_object(
                mapped as *mut VecLikeHeader,
                std::mem::size_of::<VectorObj>(),
            )
        };
        let mapped_val = unsafe { TaggedValue::from_veclike_ptr(mapped as *const VecLikeHeader) };
        let root = heap.alloc_cons(mapped_val, TaggedValue::fixnum(0));

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.vector >= 1,
            "the mapped vector must be parked, not claimed (vec={})",
            stats.last_termination_kinds.vector,
        );
        assert_eq!(
            stats.last_concurrent_vec_claimed, 0,
            "nothing here is a page vector",
        );
        assert!(
            unsafe { !(*mapped).header.gc.is_marked() },
            "a mapped vector's GcHeader bit must never be written by the GC \
             thread (mapped marks live in the side table)",
        );

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // Termination marked it on the mapped path and traced its child.
        let idx = heap.mapped_veclike_index_by_addr[&(mapped as usize)];
        assert!(heap.mapped_veclike_objects[idx].marked);
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(21).0,
        );

        // Free the fake image object after the heap is gone.
        drop(heap);
        let _ = unsafe { Box::from_raw(mapped) };
    }

    /// RACE TEST: the mutator flips strings' interval tables None<->Some in a
    /// loop (through the production wrappers) while the GC thread marks a
    /// large spine. Liveness: every flipped-in value and every string must
    /// survive; run under a data-race detector this is the strings race check
    /// (the seqlock test is the precedent).
    #[test]
    fn concurrent_mark_races_interval_flips_and_retains_live_set() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        const N_STR: usize = 512;
        let mut strings = Vec::with_capacity(N_STR);
        let mut values = Vec::with_capacity(N_STR);
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N_STR {
            let v = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::fixnum(-1));
            let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("flip"));
            let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
            unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(v) };
            strings.push(s);
            values.push(v);
            list = heap.alloc_cons(s, list);
        }
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // Mutator: clear + reinstall every string's table, twice, while the
        // GC thread walks the spine and claims/defers the strings.
        for round in 0..2 {
            for (i, s) in strings.iter().enumerate() {
                let _ = crate::tagged::mutate::with_lisp_string_mut(*s, |ls| ls.clear_intervals());
                if round == 0 || i % 2 == 0 {
                    let table = interval_table_carrying(values[i]);
                    let _ = crate::tagged::mutate::with_string_text_props_mut(*s, |t| *t = table);
                }
            }
        }

        finish_concurrent_cycle(&mut heap, root);

        for (i, v) in values.iter().enumerate() {
            assert_eq!(
                unsafe { (*v.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(i as i64).0,
                "flipped-in interval value #{i} must survive",
            );
        }
        for s in &strings {
            assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        }
    }

    /// TSan ADVERSARIAL (task 11): the widest write/claim overlap in one test.
    /// The mutator (this thread) hammers, across many strings and vectors:
    ///   * remove-text-properties — `clear_intervals` swaps the `intervals`
    ///     `AtomicPtr` to null and frees the old table;
    ///   * put-text-property — `with_string_text_props_mut` -> `ensure_intervals`
    ///     Release-stores a freshly-allocated table into the same `AtomicPtr`;
    ///   * vector `aset` — `set_vector_slot` does an atomic slot store + notes
    ///     the remembered set,
    /// while the GC thread concurrently marks a large cons spine and CLAIMS
    /// floats/strings/vectors through the 2026-07 concurrent claim dispatcher
    /// (parity mark bits + `mark_claim_at`). This is the exact overlap the new
    /// machinery must survive with zero data races: the `intervals` AtomicPtr
    /// store/swap vs. the GC's `intervals_ptr` word read, the SATB pre-image
    /// Mutex log vs. the GC drain, and the atomic vector-slot store vs. the GC
    /// Tier B backing scan. Under `-Zsanitizer=thread` this is a race check; the
    /// liveness asserts confirm the last-installed children survive uncorrupted.
    #[test]
    fn concurrent_mark_races_textprop_churn_and_aset_with_claiming() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        const N: usize = 256;
        const ROUNDS: i64 = 4;
        let mut strings = Vec::with_capacity(N);
        let mut vectors = Vec::with_capacity(N);
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N {
            // A value reachable ONLY through the string's initial interval table.
            let born = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::fixnum(-1));
            let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("adv"));
            unsafe {
                *(*(s.as_string_ptr().unwrap() as *mut StringObj))
                    .data
                    .intervals_mut() = interval_table_carrying(born);
            }
            // A vector with a placeholder slot we will `aset` in-flight values into.
            let vec = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
            // Root both via the spine so they MUST survive the cycle.
            list = heap.alloc_cons(s, list);
            list = heap.alloc_cons(vec, list);
            strings.push(s);
            vectors.push(vec);
        }
        // Large filler spine so the GC thread is still marking during the churn.
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // Mutator: hammer put/remove text-property + ensure_intervals churn +
        // vector aset while the GC thread claims/defers. Track the LAST value
        // installed into each sink so the liveness asserts are exact.
        let mut last_prop = vec![TaggedValue::fixnum(0); N];
        let mut last_slot = vec![TaggedValue::fixnum(0); N];
        for round in 0..ROUNDS {
            for i in 0..N {
                let s = strings[i];
                let vec = vectors[i];
                // remove-text-properties: drop the whole table (AtomicPtr swap).
                let _ = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
                // put-text-property: reinstall a fresh table (ensure_intervals
                // AtomicPtr store) carrying a fresh in-flight child value.
                let prop_v = heap.alloc_cons(
                    TaggedValue::fixnum(round * N as i64 + i as i64),
                    TaggedValue::fixnum(-2),
                );
                let table = interval_table_carrying(prop_v);
                let _ = crate::tagged::mutate::with_string_text_props_mut(s, |t| *t = table);
                last_prop[i] = prop_v;
                // vector aset of a fresh in-flight value (atomic slot store).
                let slot_v = heap.alloc_cons(
                    TaggedValue::fixnum(1_000_000 + round * N as i64 + i as i64),
                    TaggedValue::fixnum(-3),
                );
                crate::tagged::mutate::set_vector_slot(vec, 0, slot_v);
                last_slot[i] = slot_v;
            }
        }

        finish_concurrent_cycle(&mut heap, root);

        // Every rooted string + vector survived the concurrent cycle.
        for s in &strings {
            assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
        }
        for v in &vectors {
            assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
        }
        // The last-installed interval child + last-`aset` slot child of each sink
        // are reachable from the re-seeded root at termination, so they survive
        // uncorrupted (a swept or torn child reads back the wrong car here).
        for (i, v) in last_prop.iter().enumerate() {
            assert_eq!(
                unsafe { (*v.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum((ROUNDS - 1) * N as i64 + i as i64).0,
                "final interval child of string #{i} must survive uncorrupted",
            );
        }
        for (i, v) in last_slot.iter().enumerate() {
            assert_eq!(
                unsafe { (*v.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(1_000_000 + (ROUNDS - 1) * N as i64 + i as i64).0,
                "final aset slot child of vector #{i} must survive uncorrupted",
            );
        }
    }

    /// CLAIM-AT-ALL-SINKS (vector sink): strings reachable ONLY through a
    /// vector's slots are discovered by the Tier B backing scan on the GC
    /// thread; the interval-free one must be claimed there (claim counter),
    /// the interval-bearing one parked (str bucket) and its child traced.
    #[test]
    fn concurrent_claim_reaches_vector_slot_strings() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let s_free = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-free"));
        let c = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(4));
        let s_props = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-props"));
        unsafe {
            *(*(s_props.as_string_ptr().unwrap() as *mut StringObj))
                .data
                .intervals_mut() = interval_table_carrying(c)
        };
        let vec = heap.alloc_vector(vec![s_free, s_props]);
        let root = heap.alloc_cons(vec, TaggedValue::fixnum(0));

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();

        let stats = heap.sweep_stats();
        assert!(
            stats.last_concurrent_str_claimed >= 1,
            "the interval-free vector-slot string must be claimed on the GC \
             thread (claimed={})",
            stats.last_concurrent_str_claimed,
        );
        assert!(
            stats.last_termination_kinds.string >= 1,
            "the interval-bearing vector-slot string must be parked (str={})",
            stats.last_termination_kinds.string,
        );

        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        assert!(heap.owns_non_cons_object(s_free.as_string_ptr().unwrap() as *const u8));
        assert!(heap.owns_non_cons_object(s_props.as_string_ptr().unwrap() as *const u8));
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(3).0,
        );
    }

    /// CLAIM-AT-ALL-SINKS (obarray sink): a string reachable ONLY through an
    /// obarray symbol's value cell is discovered by the Stage 1b symbol-cell
    /// scan on the GC thread and must be claimed there.
    #[test]
    fn concurrent_claim_reaches_obarray_symbol_value_strings() {
        crate::test_utils::init_test_tracing();
        let mut ev = crate::emacs_core::eval::Context::new();
        set_tagged_heap(&mut ev.tagged_heap);

        // Interval-free string reachable ONLY via the symbol value cell.
        let s = ev
            .tagged_heap
            .alloc_string(crate::heap_types::LispString::from_utf8("obarray-only"));
        ev.obarray.set_symbol_value("neovm--str-claim-probe", s);

        // Stage the obarray snapshot exactly like the start handshake does.
        let snap = ev.obarray.scan_snapshot();
        ev.tagged_heap.set_pending_obarray_scan(snap);
        ev.tagged_heap.concurrent_begin();
        ev.tagged_heap.launch_concurrent_mark();
        while !ev.tagged_heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        ev.tagged_heap.join_concurrent_mark();

        let stats = ev.tagged_heap.sweep_stats();
        assert!(
            stats.last_concurrent_str_claimed >= 1,
            "the obarray-value string must be claimed via the symbol-cell scan \
             (claimed={})",
            stats.last_concurrent_str_claimed,
        );
        // The claimed string is black — at THIS cycle's parity (the raw bit
        // value alone is meaningless under parity marks).
        assert!(unsafe {
            (*(s.as_string_ptr().unwrap()))
                .header
                .is_marked_at(ev.tagged_heap.mark_parity)
        });
        // No sweep here: this bare-heap driver does not re-seed the Context
        // roots at termination, so sweeping would free live Context objects.
        // Claim + mark are the assertions under test (survival-under-sweep is
        // covered by the vector-sink test); the heap frees everything at drop.
    }

    /// Gap 3: a dump-less heap enables the concurrent collector after its
    /// first completed STW collection (the bootstrap), and a full concurrent
    /// cycle on such a heap retains the rooted live set and reclaims garbage
    /// (mirrors `collect_exact_retains_rooted_and_frees_unrooted`). The dump
    /// span is empty (`dump_addr_lo/hi` = MAX/0), so the GC thread's dump
    /// check must never match and the remembered-set seeding must no-op.
    #[test]
    fn dumpless_heap_enables_concurrent_after_bootstrap_and_collects() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Fresh dump-less heap: the first collection must be the STW bootstrap.
        assert!(!heap.should_run_concurrent());

        const N: i64 = 10_000;
        // Rooted list: rooted_head -> cons(N-1) -> ... -> cons(0) -> fixnum(0).
        let mut rooted = TaggedValue::fixnum(0);
        for i in 0..N {
            rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
        }
        let rooted_head = rooted;
        heap.collect_exact(std::iter::once(rooted_head));
        assert!(
            heap.should_run_concurrent(),
            "the completed STW bootstrap must enable concurrent marking"
        );

        // Allocation churn after the bootstrap: garbage for the concurrent
        // cycle to reclaim.
        let mut unrooted = TaggedValue::fixnum(0);
        for i in 0..N {
            unrooted = heap.alloc_cons(TaggedValue::fixnum(1_000_000 + i), unrooted);
        }
        let _unrooted_head = unrooted;
        let before = heap.cons_live_count;

        // One full concurrent cycle, mirroring the driver's state machine:
        // start handshake -> GC thread marks -> STW termination -> deferred
        // sweep drained.
        heap.concurrent_begin();
        heap.seed_root(rooted_head);
        heap.launch_concurrent_mark();
        assert!(heap.concurrent_mark_running());
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(rooted_head);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // The unrooted churn was reclaimed...
        let after = heap.cons_live_count;
        assert!(
            after < before,
            "the concurrent cycle must reclaim garbage (before={before}, after={after})",
        );
        // ...and the rooted spine survives, fully readable.
        let mut node = rooted_head;
        let mut count = 0i64;
        while node.is_cons() {
            let car = unsafe { (*node.xcons_ptr()).load_car() };
            assert_eq!(
                car.0,
                TaggedValue::fixnum(N - 1 - count).0,
                "rooted car intact at index {count}",
            );
            node = unsafe { (*node.xcons_ptr()).load_cdr() };
            count += 1;
        }
        assert_eq!(
            count, N,
            "the whole rooted list survived the concurrent cycle"
        );
    }

    /// Drive one full concurrent cycle re-seeding SEVERAL roots at the
    /// termination (the single-root `finish_concurrent_cycle` generalized).
    /// Parity tests use this because single-cycle tests are structurally
    /// blind: cycle 1 behaves like the pre-parity collector by construction,
    /// so every parity property is asserted across at least TWO cycles.
    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// PARITY MARK BITS (a): two-cycle survival, allocate-black variant. A
    /// non-cons object allocated DURING a concurrent mark is born at the
    /// cycle parity (allocate-black) and must survive THAT cycle's sweep
    /// unrooted; re-seeded the next cycle (opposite parity) it must be traced
    /// as unmarked and survive again. The cycle-2 (parity=false) allocation
    /// is the regression for the literal `set_marked(true)` allocate-black,
    /// which would read as WHITE on a false-parity cycle and be swept while
    /// live (UAF).
    #[test]
    fn parity_allocate_black_object_survives_two_cycles() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // STW bootstrap (flip #1: parity false -> true) enables concurrent.
        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());
        assert!(heap.mark_parity, "bootstrap flip must yield parity=true");

        // Cycle 2 (flip #2: parity true -> false): allocate non-cons objects
        // MID-MARK. They are reachable only from Rust locals (not seeded), so
        // surviving this cycle's sweep proves allocate-black at parity=false.
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        assert!(!heap.mark_parity, "second flip must yield parity=false");
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(77)]);
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("mid-mark"));
        let v_ptr = v.as_veclike_ptr().unwrap() as *const u8;
        let s_ptr = s.as_string_ptr().unwrap() as *const u8;
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine); // v/s deliberately NOT seeded this cycle
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(v_ptr),
            "allocate-black vector must survive the cycle it was born in",
        );
        assert!(
            heap.owns_non_cons_object(s_ptr),
            "allocate-black string must survive the cycle it was born in",
        );

        // Cycle 3 (flip #3: parity false -> true): the survivors' bits hold
        // the OLD parity, so they must read unmarked, be traced via their
        // seeds, and survive this cycle's sweep too.
        run_concurrent_cycle(&mut heap, &[spine, v, s]);
        assert!(heap.owns_non_cons_object(v_ptr));
        assert!(heap.owns_non_cons_object(s_ptr));
        let slot = unsafe { (*(v_ptr as *const VectorObj)).data.load_atomic(0) };
        assert_eq!(slot.0, TaggedValue::fixnum(77).0, "vector payload intact");
        assert_eq!(
            unsafe { (*(s_ptr as *const StringObj)).data.as_bytes() },
            b"mid-mark",
            "string payload intact",
        );
    }

    /// PARITY MARK BITS (b): two-cycle reclaim. Garbage born between cycles
    /// is freed by the very next cycle; garbage born DURING a mark
    /// (allocate-black) floats through that cycle and is freed by the one
    /// after — "freed by cycle 2 at the latest", with the deferred sweep
    /// completing between cycles (a parity flip mid-sweep is forbidden).
    #[test]
    fn parity_reclaims_garbage_within_two_cycles() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // G1: born BETWEEN cycles (idle) — never seeded, no allocate-black.
        let g1 = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
        let g1_ptr = g1.as_veclike_ptr().unwrap() as *const u8;

        // Cycle 2: G2 born MID-MARK (allocate-black at this cycle's parity).
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let g2 = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
        let g2_ptr = g2.as_veclike_ptr().unwrap() as *const u8;
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage must be reclaimed by the first cycle after its birth",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage floats through its birth cycle (allocate-black)",
        );

        // Cycle 3: G2's bit now holds the old parity — unmarked, unseeded,
        // reclaimed. (No allocations happened since the cycle-2 sweep, so the
        // ownership-set probes cannot be confused by address reuse.)
        run_concurrent_cycle(&mut heap, &[spine]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage must be reclaimed by the NEXT cycle",
        );
    }

    /// PARITY MARK BITS (c): tenured stability. After `promote_and_blacken`,
    /// a tenured object's frozen mark bit must never be re-interpreted or
    /// re-written: across two subsequent concurrent cycles (one at each
    /// parity) the raw bit stays exactly as frozen (a re-trace would have
    /// stored the flipped cycle's parity into it), the object stays owned,
    /// its young child stays live via the remembered set, and the armed
    /// partition + tricolor verifiers stay green (without the tenured
    /// short-circuit, `is_value_marked` would read the frozen bit as WHITE on
    /// the flipped cycle and panic the tricolor verifier on the black root ->
    /// tenured edge).
    #[test]
    fn parity_tenured_objects_stay_frozen_across_cycles_under_verifier() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16); // fake span: activates the partition

        // T: a Box RECORD that will be tenured by the first partition
        // cycle's list promotion (page vectors tenure via the stage-3
        // promotion page walk and carry their own coverage). Y: a young cons
        // reachable ONLY through T (conses never tenure), so its survival on
        // later cycles proves the promotion-time remembered set, not
        // accidental re-tracing of T.
        let y = heap.alloc_cons(TaggedValue::fixnum(424_242), TaggedValue::fixnum(0));
        let t = heap.alloc_record(vec![y]);
        let root = heap.alloc_cons(t, TaggedValue::fixnum(0));

        // First partition cycle: STW full trace + sweep, then promotion.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        let t_header = t.as_veclike_ptr().unwrap();
        assert!(
            unsafe { (*t_header).gc.tenured },
            "the surviving record must have been promoted to the old generation",
        );
        let frozen_bit = unsafe { (*t_header).gc.is_marked() };

        // Two concurrent cycles — parities false then true — with the
        // verifiers armed at each termination.
        for cycle in 0..2 {
            run_concurrent_cycle(&mut heap, &[root]);
            assert!(
                heap.owns_non_cons_object(t_header as *const u8),
                "tenured record swept on post-promotion cycle {cycle}",
            );
            assert_eq!(
                unsafe { (*t_header).gc.is_marked() },
                frozen_bit,
                "tenured mark bit re-written on post-promotion cycle {cycle} \
                 (a parity-blind re-trace stored into the frozen bit)",
            );
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(424_242).0,
                "young child of the tenured record lost on cycle {cycle}",
            );
        }
    }

    /// PARITY MARK BITS (d): the concurrent string claim works at BOTH
    /// parities. The same rooted interval-free string is claimed by the GC
    /// thread on two consecutive cycles: on the second one its bit holds the
    /// previous cycle's parity, which a parity-blind `swap(true)` claim would
    /// misread as "already marked" — the string would never be marked that
    /// cycle and the sweep would free it while rooted.
    #[test]
    fn parity_string_claim_works_across_two_cycles() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("still-here"));
        let s_ptr = s.as_string_ptr().unwrap() as *const u8;
        let mut spine = heap.alloc_cons(s, TaggedValue::fixnum(0));
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine)); // bootstrap

        for cycle in 0..2 {
            run_concurrent_cycle(&mut heap, &[spine]);
            assert!(
                heap.sweep_stats().last_concurrent_str_claimed >= 1,
                "cycle {cycle}: the interval-free string must be claimed on \
                 the GC thread at this cycle's parity",
            );
            assert!(
                heap.owns_non_cons_object(s_ptr),
                "cycle {cycle}: claimed string swept while rooted",
            );
            assert!(
                unsafe {
                    (*(s_ptr as *const StringObj))
                        .header
                        .is_marked_at(heap.mark_parity)
                },
                "cycle {cycle}: claimed string must be black at the cycle parity",
            );
        }
        assert_eq!(
            unsafe { (*(s_ptr as *const StringObj)).data.as_bytes() },
            b"still-here",
        );
    }

    /// PARITY MARK BITS (born-at-parity, idle window): an object allocated
    /// BETWEEN cycles is born with bit == current parity, so the next flip
    /// reads it as white and traces it. Born at `!parity` instead (the naive
    /// "born white NOW" store), the next flip would read it as BLACK: never
    /// traced, its sole-reference child swept while referenced — this test's
    /// X->Y chain is exactly that UAF, asserted across two full cycles.
    #[test]
    fn parity_idle_born_object_is_traced_on_the_next_cycle() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..10_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine)); // bootstrap (parity -> true)

        // Idle window: no mark, no sweep. Y is reachable ONLY through X.
        let y = heap.alloc_cons(TaggedValue::fixnum(31_337), TaggedValue::fixnum(0));
        let x = heap.alloc_vector(vec![y]);
        let x_ptr = x.as_veclike_ptr().unwrap() as *const u8;

        for cycle in 0..2 {
            run_concurrent_cycle(&mut heap, &[spine, x]);
            assert!(
                heap.owns_non_cons_object(x_ptr),
                "cycle {cycle}: idle-born rooted vector swept",
            );
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(31_337).0,
                "cycle {cycle}: child reachable only through the idle-born \
                 vector was swept (the vector was falsely black and never traced)",
            );
        }
    }

    /// Gap 3 drop safety: dropping a heap while the GC thread is still
    /// concurrently marking it must stop + join the GC thread before any
    /// storage it can read is freed (dump-less heaps now reach this state at
    /// every safe-point collection after bootstrap, e.g. a test Context
    /// dropped mid-mark).
    #[test]
    fn dropping_heap_mid_concurrent_mark_joins_gc_thread() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A long spine so the GC thread is genuinely still marking at drop.
        const N: i64 = 300_000;
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        heap.concurrent_begin();
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        assert!(heap.concurrent_mark_running());
        // Drop with the mark in flight; under TSAN/ASAN a missing join is a
        // use-after-free the sanitizer catches, and the join panics if the GC
        // thread is gone.
        drop(heap);
    }

    /// GNU `sweep_conses` (src/alloc.c:6856-6858) threads the free list through
    /// the dead cells and then writes `dead_object ()` into the car, so a cell
    /// on the free list is "recognizable in O(1)" (`deadp`, src/alloc.c:425-429).
    ///
    /// That poison is what makes a use-after-free diagnosable HERE, because the
    /// free-list link lives in the cdr union and a raw `*mut ConsCell` has
    /// `TAG_SYMBOL` (0b000) in its low three bits — so an unpoisoned reclaimed
    /// cons reads back as the perfectly ordinary `(nil . SOME-SYMBOL)` and the
    /// garbage only faults much later, in the symbol resolver
    /// (DIVERGENCES.md 161).
    #[test]
    fn a_reclaimed_cons_is_recognizable_as_dead() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let rooted = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::fixnum(2));
        let doomed = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(4));
        assert!(!unsafe { (*doomed.xcons_ptr()).load_car() }.is_dead());

        heap.collect_exact(std::iter::once(rooted));

        assert!(
            !unsafe { (*rooted.xcons_ptr()).load_car() }.is_dead(),
            "a rooted cons must not be poisoned",
        );
        assert!(
            unsafe { (*doomed.xcons_ptr()).load_car() }.is_dead(),
            "a reclaimed cons must carry GNU's dead_object in its car, not nil: \
             without it a use-after-free is indistinguishable from live data",
        );
        // And the free-list link the cdr now holds is exactly the shape that
        // decodes as a bogus symbol: an aligned raw pointer under TAG_SYMBOL.
        let link = unsafe { (*doomed.xcons_ptr()).load_cdr() };
        assert!(
            link.is_symbol(),
            "the free-list link decodes through TAG_SYMBOL (bits 0x{:x})",
            link.bits(),
        );
    }

    /// The string-side twin of `a_reclaimed_cons_is_recognizable_as_dead`.
    ///
    /// GNU `sweep_strings` (src/alloc.c:1878-1882) ends a dead string with
    ///
    /// ```c
    ///   /* Reset the strings's `data' member so that we
    ///      know it's free.  */
    ///   s->u.s.data = NULL;
    /// ```
    ///
    /// and reads the marker back at :1851 and :1892.  `LispString::drop` only
    /// nulled `data` for a string that OWNED its bytes
    /// (`release_owned_storage` returns early when `storage_capacity == 0`),
    /// so a swept BORROWED payload — every pdump-mapped and static-rodata
    /// string — stayed byte-identical to a live one and a stale borrow of it
    /// read on silently (DIVERGENCES.md 163).
    #[test]
    fn a_reclaimed_string_is_recognizable_as_dead() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // The rooted string keeps the arena page populated, so the doomed
        // slot's storage is still mapped and readable after the sweep — the
        // same arrangement the cons pin above relies on.
        let rooted = heap.alloc_string(crate::heap_types::LispString::from_utf8("rooted"));
        let doomed = heap.alloc_string(crate::heap_types::LispString::from_utf8("doomed"));
        let doomed_ptr = doomed.as_string_ptr().unwrap();
        assert!(
            !unsafe { (*doomed_ptr).data.is_reclaimed() },
            "a live string must not look reclaimed",
        );

        heap.collect_exact(std::iter::once(rooted));

        assert!(
            !unsafe { (*rooted.as_string_ptr().unwrap()).data.is_reclaimed() },
            "a rooted string must not be marked free",
        );
        assert!(
            unsafe { (*doomed_ptr).data.is_reclaimed() },
            "a reclaimed string must carry GNU's free marker (data == NULL, \
             src/alloc.c:1878-1882): without it a `&LispString` that outlived \
             its object is indistinguishable from a live borrow",
        );
    }

    /// The borrowed-payload half of the parity, which is the half that was
    /// actually missing: `release_owned_storage` returns early for a string
    /// whose bytes it does not own, so before DIVERGENCES.md 163 a swept
    /// mapped/rodata string kept a perfectly valid-looking `data` pointer.
    #[test]
    fn a_reclaimed_string_with_borrowed_bytes_is_also_marked_free() {
        crate::test_utils::init_test_tracing();
        // Static, NUL-terminated: exactly the shape a pdump-mapped or
        // static-rodata payload has (`storage_capacity == 0`).
        static BYTES: &[u8] = b"borrowed\0";
        let borrowed =
            unsafe { crate::heap_types::LispString::from_mapped_bytes(BYTES.as_ptr(), 8, 8, -1) };
        assert!(!borrowed.is_reclaimed());
        let mut owner = std::mem::ManuallyDrop::new(borrowed);
        unsafe { std::ptr::drop_in_place(&mut *owner as *mut crate::heap_types::LispString) };
        assert!(
            owner.is_reclaimed(),
            "GNU nulls `data` for EVERY dead string, not only for one whose \
             bytes it owned (src/alloc.c:1878-1882)",
        );
    }

    /// `verify_marked_objects_owned` was written for the missing-root class
    /// and had zero callers — 161 listed it as "dead code written for exactly
    /// this failure". It is wired into `complete_collection` now, behind a
    /// gate. This is the pin that it RUNS and that a healthy heap reports zero
    /// problems: without the wiring the gate function does not exist and this
    /// does not compile (DIVERGENCES.md 162). The helpers are
    /// debug-only, so the pin compiles in debug builds only — a release
    /// test build must not see this function at all.
    #[test]
    #[cfg(debug_assertions)]
    fn post_mark_ownership_verification_runs_and_finds_nothing() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        super::set_verify_marked_objects_for_test(true);
        assert!(super::verify_marked_objects_enabled());

        let kept = heap.alloc_string(crate::heap_types::LispString::new("kept".into(), false));
        let vector = heap.alloc_vector(vec![TaggedValue::fixnum(7)]);
        let rooted = heap.alloc_cons(kept, vector);
        let _doomed =
            heap.alloc_string(crate::heap_types::LispString::new("dropped".into(), false));

        // Asserts internally (problems == 0) at the one moment where "marked"
        // and "owned" must agree.
        heap.collect_exact(std::iter::once(rooted));
        assert_eq!(heap.verify_marked_objects_owned(), 0);

        super::set_verify_marked_objects_for_test(false);
    }

    /// Workstream A path-collapse safety net (characterization): a forced
    /// `collect_exact` retains a rooted live cons graph and reclaims an unrooted
    /// one, INDEPENDENT of which internal path (concurrent / incremental /
    /// STW-full) runs it. This must keep passing as the incremental slicer + the
    /// `NEOVM_GC_CONCURRENT`/`NEOVM_GC_SATB` env flags are deleted in the collapse.
    #[test]
    fn collect_exact_retains_rooted_and_frees_unrooted() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        const N: i64 = 1_000;
        // Rooted list: rooted_head -> cons(N-1) -> ... -> cons(0) -> fixnum(0).
        let mut rooted = TaggedValue::fixnum(0);
        for i in 0..N {
            rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
        }
        let rooted_head = rooted;
        // Unrooted list (never named in the explicit root set): must be reclaimed.
        // A precise collector roots only the iterator passed to collect_exact, not
        // the Rust stack, so holding this local does NOT keep it alive.
        let mut unrooted = TaggedValue::fixnum(0);
        for i in 0..N {
            unrooted = heap.alloc_cons(TaggedValue::fixnum(1_000_000 + i), unrooted);
        }
        let _unrooted_head = unrooted;
        let before = heap.cons_live_count;

        // Force a full collection with only the rooted list reachable.
        heap.collect_exact(std::iter::once(rooted_head));
        let after = heap.cons_live_count;

        // The unrooted list was reclaimed...
        assert!(
            after < before,
            "unrooted conses must be reclaimed (before={before}, after={after})",
        );
        // ...and the entire rooted spine survives + is readable (a swept cons here
        // would be a use-after-free the asserts / sanitizer catch).
        let mut node = rooted_head;
        let mut count = 0i64;
        while node.is_cons() {
            let car = unsafe { (*node.xcons_ptr()).load_car() };
            assert_eq!(
                car.0,
                TaggedValue::fixnum(N - 1 - count).0,
                "rooted car intact at index {count}",
            );
            node = unsafe { (*node.xcons_ptr()).load_cdr() };
            count += 1;
        }
        assert_eq!(count, N, "the whole rooted list survived collection");
    }

    #[test]
    fn ordinary_non_cons_ownership_index_tracks_sweep() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();

        let live = heap.alloc_float(1.0);
        let dead = heap.alloc_float(2.0);
        let live_ptr = live.as_float_ptr().unwrap() as *const u8;
        let dead_ptr = dead.as_float_ptr().unwrap() as *const u8;

        // Stage-3 fold-in: page floats are owned via the PAGE-SPAN oracle
        // and never touch the residual `non_cons_object_addrs` set.
        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(heap.owns_non_cons_object(dead_ptr));
        assert!(heap.float_arena.owns(live_ptr));
        assert!(heap.float_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);

        heap.collect_exact(std::iter::once(live));

        // The sweep's alloc-bit clear IS the ownership eviction: the freed
        // slot answers NOT-owned with no addr-set bookkeeping involved.
        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(!heap.owns_non_cons_object(dead_ptr));
        assert!(heap.float_arena.owns(live_ptr));
        assert!(!heap.float_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!((live.xfloat() - 1.0).abs() < f64::EPSILON);
    }

    /// Task #7 stage 2a (Fix A): the incremental vector registry must yield
    /// exactly the Tier-B snapshot the old full-set filter produced, across
    /// alloc/free cycles and both sweep paths. Computes BOTH methods — the
    /// registry walk and the old `non_cons_object_addrs` filter — and compares
    /// snapshot contents (backing base/len/kind), not just counts.
    #[test]
    fn vector_registry_matches_full_filter_across_cycles() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        fn entry_key(addr: usize) -> (usize, usize, bool) {
            // Safety: `addr` is a live owned Vector's `GcHeader` address (both
            // callers iterate live-object sets under a stopped world).
            let obj = unsafe { &*(addr as *const VectorObj) };
            let entry = obj.data.scan_entry();
            (entry.base as usize, entry.len, entry.is_mapped)
        }
        // Ground-truth snapshot contents, stage-3 form: allocated VECTOR
        // ARENA PAGE SLOTS (walked allocated-bit-first) ∪ any residual
        // Box Vector in the non-cons set (none are allocated anymore, but
        // the union keeps the test honest about the invariant's shape).
        fn full_filter_entries(heap: &TaggedHeap) -> Vec<(usize, usize, bool)> {
            let mut entries: Vec<(usize, usize, bool)> = heap
                .non_cons_object_addrs
                .iter()
                .filter(|&&addr| unsafe {
                    (*(addr as *const GcHeader)).kind == HeapObjectKind::VecLike
                        && (*(addr as *const VecLikeHeader)).type_tag == VecLikeType::Vector
                })
                .map(|&addr| entry_key(addr))
                .collect();
            entries.extend(
                heap.vector_arena
                    .collect_allocated_slots()
                    .into_iter()
                    .map(|slot| entry_key(slot as usize)),
            );
            entries.sort_unstable();
            entries
        }
        // New-method snapshot contents: iterate the incremental registry.
        fn registry_entries(heap: &TaggedHeap) -> Vec<(usize, usize, bool)> {
            let mut entries: Vec<(usize, usize, bool)> = heap
                .vector_object_addrs
                .iter()
                .map(|&addr| entry_key(addr))
                .collect();
            entries.sort_unstable();
            entries
        }
        fn assert_snapshots_match(heap: &TaggedHeap) {
            assert_eq!(
                registry_entries(heap),
                full_filter_entries(heap),
                "registry snapshot != full-filter snapshot",
            );
        }

        // Mixed population: vectors + non-vector decoys, both non-veclike
        // (float) and veclike-non-Vector (record) — the registry must exclude
        // every decoy kind.
        let keep_vec = heap.alloc_vector(vec![TaggedValue::fixnum(1); 8]);
        let dead_vec = heap.alloc_vector(vec![TaggedValue::fixnum(2); 4]);
        let keep_float = heap.alloc_float(1.5);
        let _dead_record = heap.alloc_record(vec![TaggedValue::fixnum(3); 5]);
        assert_snapshots_match(&heap);
        assert_eq!(registry_entries(&heap).len(), 2);

        // Cycle 1 (synchronous sweep_objects path): the unrooted vector and
        // record are reclaimed; the registry follows.
        let _ = dead_vec;
        heap.collect_exact([keep_vec, keep_float].into_iter());
        assert_snapshots_match(&heap);
        assert_eq!(registry_entries(&heap).len(), 1);

        // Cycle 2: fresh vectors on the reused address space, then free one.
        let dead_vec2 = heap.alloc_vector(vec![keep_float; 3]);
        let keep_vec2 = heap.alloc_vector(vec![TaggedValue::fixnum(4); 2]);
        let _ = dead_vec2;
        assert_snapshots_match(&heap);
        assert_eq!(registry_entries(&heap).len(), 3);
        heap.collect_exact([keep_vec, keep_vec2].into_iter());
        assert_snapshots_match(&heap);
        assert_eq!(registry_entries(&heap).len(), 2);

        // A full CONCURRENT cycle exercises the launch-time invariant
        // cross-check (`cfg(test)`) plus the deferred-sweep removal path
        // (`incremental_sweep_slice`) end to end. Only `keep_vec` is rooted,
        // so `keep_vec2` is reclaimed by the deferred sweep.
        heap.concurrent_begin();
        heap.seed_root(keep_vec);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(keep_vec);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_snapshots_match(&heap);
        assert_eq!(registry_entries(&heap).len(), 1);
    }

    /// Task #7 stage 2a (Fix B): a stop request that lands MID-DRAIN (joining
    /// without waiting for `concurrent_mark_done`) makes the GC thread break
    /// at its stop-check quantum and hand ALL residual gray work to the
    /// termination fold via `deferred` — the STW drain then finishes it, so
    /// the live set is retained bit-for-bit and only real garbage is swept.
    /// Exercises every interleaving outcome (job not yet started / mid-drain
    /// quantum break / already drained) with the same outcome-based asserts.
    #[test]
    fn immediate_join_mid_drain_hands_residual_work_to_termination() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A long rooted list so the GC thread is (almost surely) still
        // draining when the join lands, plus one unrooted garbage cons.
        const N: i64 = 300_000;
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;
        let _garbage = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
        let live_before = heap.cons_live_count;

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        // JOIN IMMEDIATELY — no `concurrent_mark_done` wait.
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        assert_eq!(
            heap.cons_live_count,
            live_before - 1,
            "exactly the one garbage cons is swept; the whole rooted list survives",
        );
        // The rooted spine is intact and readable (a swept live cons here
        // would be a use-after-free the asserts / sanitizer catch).
        let mut node = root;
        let mut count = 0i64;
        while node.is_cons() {
            let car = unsafe { (*node.xcons_ptr()).load_car() };
            assert_eq!(
                car.0,
                TaggedValue::fixnum(N - 1 - count).0,
                "rooted car intact at index {count}",
            );
            node = unsafe { (*node.xcons_ptr()).load_cdr() };
            count += 1;
        }
        assert_eq!(count, N, "the whole rooted list survived the early join");
    }

    /// Characterization safety net for the path-collapse refactor: a forced full
    /// collection must retain a rooted cons graph and reclaim an unrooted one,
    /// regardless of which internal mark path runs. Pins the observable contract
    /// (`collect_exact` keeps the live set, frees garbage, leaves the spine
    /// readable) so collapsing the three GC paths into one cannot silently change
    /// it.
    #[test]
    fn collect_exact_retains_rooted_graph_and_frees_garbage() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Rooted spine: a -> b -> c (cdr-terminated by a fixnum).
        let c = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(0));
        let b = heap.alloc_cons(TaggedValue::fixnum(2), c);
        let a = heap.alloc_cons(TaggedValue::fixnum(1), b);
        // Unrooted garbage: reachable from neither the root nor the spine.
        let _g1 = heap.alloc_cons(TaggedValue::fixnum(-1), TaggedValue::fixnum(0));
        let _g2 = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
        let live_before = heap.cons_live_count;
        assert!(live_before >= 5);

        // Force a full collection rooted only at `a`.
        heap.collect_exact(std::iter::once(a));

        // The 3-cons rooted spine survives; the 2 garbage conses are reclaimed.
        assert_eq!(
            heap.cons_live_count,
            live_before - 2,
            "rooted graph retained, unrooted garbage reclaimed",
        );
        // The spine is intact and readable (reading a swept cons would corrupt).
        let a_cdr = unsafe { (*a.xcons_ptr()).load_cdr() };
        assert!(a_cdr.is_cons());
        let b_cdr = unsafe { (*a_cdr.xcons_ptr()).load_cdr() };
        assert!(b_cdr.is_cons());
        assert_eq!(
            unsafe { (*b_cdr.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(3).0,
        );
    }

    #[test]
    fn native_font_object_traces_properties_and_capability() {
        use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};

        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let property = heap.alloc_string(crate::heap_types::LispString::from_utf8("font-name"));
        let capability =
            heap.alloc_string(crate::heap_types::LispString::from_utf8("font-capability"));
        let property_ptr = property.as_string_ptr().unwrap() as *const u8;
        let capability_ptr = capability.as_string_ptr().unwrap() as *const u8;
        let font = heap.alloc_font(FontObjectData {
            fields: vec![property].into(),
            metrics: FontObjectMetrics {
                pixel_size: 16,
                height: 19,
                max_width: 9,
                ascent: 14,
                descent: 5,
                space_width: 8,
                average_width: 8,
            },
            capability,
            identity: ResolvedFontIdentity::from_memory(
                FontBackendKind::Fontconfig,
                "test:native-font".to_string(),
                0,
                None,
            ),
        });

        heap.collect_exact(std::iter::once(font));
        assert!(heap.owns_non_cons_object(property_ptr));
        assert!(heap.owns_non_cons_object(capability_ptr));

        heap.collect_exact(std::iter::empty());
        assert!(!heap.owns_non_cons_object(property_ptr));
        assert!(!heap.owns_non_cons_object(capability_ptr));
    }

    /// Regression test for the O(n²) SATB blow-up: building a large container
    /// (here a hash table) in a loop WHILE a concurrent mark is running must log
    /// each container's pre-image to the SATB buffer at most ONCE per cycle, not
    /// re-enumerate ALL of the container's children on every single mutation.
    ///
    /// Before the per-cycle dedup fix, every `puthash` ran
    /// `push_value_children_to_satb_shared` -> `collect_veclike_children`, which
    /// enumerates `ht.data.values()` + `ht.key_snapshots.values()` — the WHOLE
    /// table. N inserts each snapshot ~k*N values => Θ(N²) entries pushed into
    /// `satb_shared` (and the equivalent memory), which OOMs on a 200K-entry
    /// build like `(ucs-names)`. The fix snapshots the table's full pre-image
    /// once, so the cumulative SATB volume is O(N).
    ///
    /// We drive the SATB barrier directly (set `concurrent_mark_running` without
    /// launching the background GC thread) so nothing drains `satb_shared`
    /// concurrently and the cumulative push count is deterministic.
    #[test]
    fn satb_barrier_on_growing_hash_table_is_linear_not_quadratic() {
        use crate::emacs_core::value::{HashTableTest, LispHashTable};

        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // An `equal` hash table whose VALUES are heap objects (conses), so the
        // SATB enumeration actually pushes them to the shared buffer.
        let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));

        // Arm the SATB barrier exactly as `launch_concurrent_mark` does, but
        // WITHOUT the GC thread, so `satb_shared` is never drained and its length
        // measures the cumulative SATB push volume deterministically.
        heap.concurrent_mark_running = true;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(true));

        const N: i64 = 50_000;
        for i in 0..N {
            // Each value is a fresh heap cons (a brand-new key => an INSERT, no
            // prior value at that key for SATB to log).
            let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
            let key = crate::emacs_core::value::HashKey::Int(i);
            let key_snapshot = TaggedValue::fixnum(i);
            crate::tagged::mutate::with_hash_table_mut(table, |ht| {
                ht.insert(key, key_snapshot, value);
            });
        }

        let satb_len = heap.satb_shared.lock().unwrap().len();

        // Disarm before dropping the heap so no later mutation hits the barrier.
        heap.concurrent_mark_running = false;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(false));

        // O(n) bound. The full pre-image is snapshotted at most a small constant
        // number of times across the whole cycle (ideally once), so the
        // cumulative pushes are within a small multiple of N. The buggy
        // (re-enumerate-on-every-write) barrier produces ~N²/2 ≈ 1.25e9 pushes
        // for N=50_000, blowing far past this bound.
        let bound = (N as usize) * 4;
        assert!(
            satb_len <= bound,
            "SATB barrier is super-linear: pushed {satb_len} values for {N} inserts \
             (O(n) bound is {bound}); the per-write full-container enumeration was \
             not deduplicated per cycle",
        );
    }

    /// End-to-end correctness for the per-cycle SATB dedup under a REAL concurrent
    /// mark + sweep: a hash table is mutated MANY times during marking (so the
    /// dedup suppresses all but the first per-owner snapshot), values are
    /// OVERWRITTEN (update) and the table is GROWN (insert+resize/rehash), and
    /// churn garbage is allocated and dropped. After termination + sweep:
    ///   * every value reachable through the live table survives and is readable;
    ///   * a value that was OVERWRITTEN before the snapshot-time first mutation is
    ///     retained by the SATB pre-image (Yuasa: it was live at snapshot time);
    ///   * unrooted pre-mark garbage is reclaimed.
    /// If the dedup ever dropped a still-reachable value's pre-image, the sweep
    /// would free a live cons and the readback would observe corruption (and TSan
    /// /ASan would fault). Mirrors `concurrent_mark_overlaps_mutation_and_retains_live_set`
    /// but exercises the deduped multi-child (hash-table) owner path specifically.
    #[test]
    fn concurrent_mark_dedup_retains_hash_table_live_set() {
        use crate::emacs_core::value::{HashKey, HashTableTest, LispHashTable};

        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Build the table BEFORE the mark so its initial values are part of the
        // start-of-cycle snapshot. Each value is a heap cons we can read back.
        let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));
        const PRE: i64 = 2_000;
        for i in 0..PRE {
            let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
            let key = HashKey::Int(i);
            crate::tagged::mutate::with_hash_table_mut(table, |ht| {
                ht.insert(key, TaggedValue::fixnum(i), value);
            });
        }
        // Pre-mark garbage: reachable from nothing.
        let _garbage = heap.alloc_cons(TaggedValue::fixnum(-99), TaggedValue::fixnum(0));

        // Start a real concurrent mark with the table as the sole root.
        heap.concurrent_begin();
        heap.seed_root(table);
        heap.launch_concurrent_mark();

        // While the GC thread marks: (a) OVERWRITE an existing key's value — the
        // OLD cons leaves the table and must be retained via the SATB pre-image;
        // (b) GROW the table with many new keys (insert + resize/rehash), whose
        // values are born-black; (c) churn-allocate dropped garbage.
        let key0 = HashKey::Int(0);
        let old_value0 =
            crate::tagged::mutate::with_hash_table_mut(table, |ht| ht.data[&key0]).unwrap();
        let new_value0 = heap.alloc_cons(TaggedValue::fixnum(123_456), TaggedValue::fixnum(0));
        crate::tagged::mutate::with_hash_table_mut(table, |ht| {
            *ht.data.get_mut(&key0).unwrap() = new_value0;
        });
        for i in PRE..(PRE + 3_000) {
            let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
            let key = HashKey::Int(i);
            crate::tagged::mutate::with_hash_table_mut(table, |ht| {
                maybe_resize_for_test(ht);
                ht.insert(key, TaggedValue::fixnum(i), value);
            });
        }
        for _ in 0..5_000 {
            let _ = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::fixnum(0));
        }

        // Terminate stop-the-world + sweep.
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(table);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // (1) The overwritten OLD value (live at snapshot time, then unlinked) must
        //     still be a readable, non-swept cons (SATB pre-image retained it).
        assert!(old_value0.is_cons());
        assert_eq!(
            unsafe { (*old_value0.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(0).0,
            "overwritten pre-snapshot value was swept — dedup dropped a live pre-image",
        );
        // (2) Every value currently in the table is readable (none swept).
        let snapshot = table.with_hash_table_mut(|ht| {
            ht.data
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>()
        });
        let entries = snapshot.expect("hash table");
        assert_eq!(entries.len() as i64, PRE + 3_000);
        for (key, value) in entries {
            assert!(
                value.is_cons(),
                "table value {key:?} is not a cons (swept?)"
            );
            let car = unsafe { (*value.xcons_ptr()).load_car() }.0;
            let expected = match key {
                HashKey::Int(0) => TaggedValue::fixnum(123_456).0, // the updated value
                HashKey::Int(n) => TaggedValue::fixnum(n).0,
                other => panic!("unexpected key {other:?}"),
            };
            assert_eq!(car, expected, "table value {key:?} corrupted/swept");
        }
    }

    /// GNU-parity finalizers, STW path: a finalizer a full collection finds
    /// unreachable leaves the registry, its function is queued + re-marked
    /// (transitively) so the sweep keeps it, and the finalizer object itself
    /// is swept. A queued-but-not-taken function survives later cycles via
    /// the runtime-root seeding.
    #[test]
    fn finalizer_doomed_on_stw_collection_queues_and_keeps_function() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let payload = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::fixnum(8));
        let function = heap.alloc_cons(TaggedValue::fixnum(42), payload);
        let finalizer = heap.alloc_finalizer(function);
        let fin_ptr = finalizer.as_veclike_ptr().unwrap();
        // The verifier enumeration must cover the function slot
        // (`collect_veclike_children` stays a superset of `trace_veclike`).
        let children = heap.collect_veclike_children(fin_ptr as *mut VecLikeHeader);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, function.0);

        // Cycle 1: the finalizer is rooted — still registered, nothing queued,
        // and the traced function survives.
        heap.begin_collection();
        heap.seed_root(finalizer);
        heap.complete_collection();
        assert!(heap.doomed_finalizer_functions.is_empty());
        assert_eq!(heap.finalizer_registry.len(), 1);
        assert_eq!(
            unsafe { (*function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(42).0,
        );

        // Cycle 2: nothing roots the finalizer — doomed. The function (and
        // what it reaches) survives the sweep; the finalizer object does not.
        heap.begin_collection();
        heap.complete_collection();
        assert!(heap.finalizer_registry.is_empty());
        assert!(
            !heap.owns_non_cons_object(fin_ptr as *const u8),
            "doomed finalizer object must be swept",
        );
        assert_eq!(
            unsafe { (*function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(42).0,
        );
        assert_eq!(
            unsafe { (*payload.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(7).0,
            "everything the queued function reaches must survive",
        );

        // Cycle 3, queue still undrained: the queued function is a runtime
        // root and must survive again.
        heap.begin_collection();
        heap.complete_collection();
        assert_eq!(
            unsafe { (*function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(42).0,
        );

        let doomed = heap.take_doomed_finalizer_functions();
        assert_eq!(doomed.len(), 1);
        assert_eq!(doomed[0].0, function.0);
        assert!(heap.take_doomed_finalizer_functions().is_empty());
    }

    /// GNU-parity finalizers, concurrent path: the doomed-finalizer scan must
    /// run at `incremental_finish` too — a miss there means finalizers never
    /// run under the concurrent collector. Also checks allocate-black: a
    /// finalizer born during the mark survives that cycle and is doomable on
    /// the next one.
    #[test]
    fn finalizer_doomed_on_concurrent_termination_queues_and_keeps_function() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // A long spine keeps the GC thread marking while the mutator runs.
        const N: i64 = 100_000;
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let function = heap.alloc_cons(TaggedValue::fixnum(43), TaggedValue::fixnum(0));
        let doomed_fin = heap.alloc_finalizer(function);
        let doomed_ptr = doomed_fin.as_veclike_ptr().unwrap();
        let live_fin = heap.alloc_finalizer(function);

        heap.concurrent_begin();
        heap.seed_root(list);
        heap.seed_root(live_fin); // doomed_fin is unreachable this cycle
        heap.launch_concurrent_mark();

        // Born during the mark: allocate-black, so it survives this cycle
        // even though nothing references it.
        let churn_function = heap.alloc_cons(TaggedValue::fixnum(44), TaggedValue::fixnum(0));
        let churn_fin = heap.alloc_finalizer(churn_function);
        let churn_ptr = churn_fin.as_veclike_ptr().unwrap();

        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(list);
        heap.seed_root(live_fin);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        assert!(
            !heap.owns_non_cons_object(doomed_ptr as *const u8),
            "doomed finalizer object must be swept",
        );
        assert!(heap.owns_non_cons_object(live_fin.as_veclike_ptr().unwrap() as *const u8));
        assert!(
            heap.owns_non_cons_object(churn_ptr as *const u8),
            "a finalizer born during the mark must survive that cycle",
        );
        assert_eq!(heap.finalizer_registry.len(), 2);
        let doomed = heap.take_doomed_finalizer_functions();
        assert_eq!(doomed.len(), 1);
        assert_eq!(doomed[0].0, function.0);
        assert_eq!(
            unsafe { (*function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(43).0,
        );

        // Next cycle: the born-black churn finalizer (still unreferenced) is
        // doomed now; the rooted one stays registered.
        heap.begin_collection();
        heap.seed_root(live_fin);
        heap.complete_collection();
        assert!(!heap.owns_non_cons_object(churn_ptr as *const u8));
        assert_eq!(heap.finalizer_registry.len(), 1);
        let doomed = heap.take_doomed_finalizer_functions();
        assert_eq!(doomed.len(), 1);
        assert_eq!(doomed[0].0, churn_function.0);
        assert_eq!(
            unsafe { (*churn_function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(44).0,
        );
    }

    /// The dump-partition + tricolor verifiers must accept the finalizer
    /// arms: a LIVE finalizer is enumerated through
    /// `collect_veclike_children`, and a doomed one's re-marked function must
    /// not present a black->white edge. The fake dump span only activates the
    /// partition; it maps no objects.
    #[test]
    fn finalizer_cycle_passes_partition_verifier() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        // First partitioned cycle promotes survivors + blackens the (empty)
        // dump; verification gates arm on the cycles after it.
        heap.begin_collection();
        heap.complete_collection();
        assert!(heap.dump_blackened);

        let payload = heap.alloc_cons(TaggedValue::fixnum(5), TaggedValue::fixnum(6));
        let doomed_function = heap.alloc_cons(TaggedValue::fixnum(45), payload);
        let _doomed_fin = heap.alloc_finalizer(doomed_function);
        let live_function = heap.alloc_cons(TaggedValue::fixnum(46), TaggedValue::fixnum(0));
        let live_fin = heap.alloc_finalizer(live_function);

        // Verified cycle: `complete_collection` panics if the finalizer arms
        // break the partition/tricolor invariants.
        heap.begin_collection();
        heap.seed_root(live_fin);
        heap.complete_collection();

        let doomed = heap.take_doomed_finalizer_functions();
        assert_eq!(doomed.len(), 1);
        assert_eq!(doomed[0].0, doomed_function.0);
        assert_eq!(
            unsafe { (*payload.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(5).0,
        );
        assert_eq!(heap.finalizer_registry.len(), 1);
        assert_eq!(
            unsafe { (*live_function.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(46).0,
        );
    }
}

/// FLOAT ARENA PAGES test suite. Every scenario runs twice: plain and with
/// `NEOVM_GC_VERIFY_PARTITION=1` (which also arms the partition via a fake
/// dump span + a bootstrap cycle where the flow allows, so the dump-partition
/// and tricolor verifiers actually engage at each termination). The suite
/// relies on nextest's process-per-test model for the env var and the global
/// `LIVE_FLOAT_PAGES` counter.
#[cfg(test)]
mod float_arena_tests {
    use super::*;

    fn arm_verify(heap: &mut TaggedHeap) {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        // Fake dump span: activates the dump partition so the first full
        // cycle promotes + blackens and later terminations run the verifiers.
        heap.extend_dump_span(4096, 16);
    }

    /// Drive one full concurrent cycle (start handshake → GC-thread drain →
    /// termination → deferred sweep drained). Copy of the ownership_tests
    /// helper, local so this module stands alone.
    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// (a) Slot reuse WITHIN one cooperative sweep window: a page is swept in
    /// an early slice, the mutator reallocates its freed slots between
    /// slices (class free-list pop), and the rest of the sweep must neither
    /// double-free nor prematurely free the reused slots. The arena stays
    /// bitmap-coherent at every step.
    fn reuse_within_one_cooperative_sweep_window_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_verify(&mut heap);
            // Bootstrap cycle: blackens the (fake) dump so later
            // terminations run the armed verifiers.
            heap.collect_exact(std::iter::empty());
        }

        // Exactly three full pages of floats.
        let n = 3 * FLOAT_PAGE_SLOTS;
        let mut floats = Vec::with_capacity(n);
        for i in 0..n {
            floats.push(heap.alloc_float(i as f64));
        }
        assert_eq!(
            heap.float_arena.pages.len(),
            3,
            "3 * PAGE_SLOTS floats = 3 pages"
        );
        heap.assert_object_arenas_coherent();

        // Keep the even-indexed half; the odd half is garbage.
        let keep: Vec<TaggedValue> = floats.iter().copied().step_by(2).collect();
        let dead_addrs: std::collections::HashSet<usize> = floats
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_float_ptr().unwrap() as usize)
            .collect();
        let page0_base = heap.float_arena.pages[0].base_addr();

        // Mark to a fixpoint and ARM the deferred sweep (the incremental
        // termination path), then drain it slice by slice.
        heap.begin_collection();
        for &k in &keep {
            heap.seed_root(k);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());

        // Slice 1 (budget 1): sweeps float page 0 only — the window is open.
        assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
        assert!(heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        // BETWEEN cooperative slices the mutator reallocates: the class free
        // list must hand back the slots the slice just freed, in page 0.
        let mut reused = Vec::new();
        for i in 0..64 {
            reused.push(heap.alloc_float(1_000.0 + i as f64));
        }
        for r in &reused {
            let ptr = r.as_float_ptr().unwrap();
            assert_eq!(
                ObjectPage::<FloatObj>::page_base_for_ptr(ptr),
                page0_base,
                "mid-sweep reuse must come from the just-swept page",
            );
            assert!(
                dead_addrs.contains(&(ptr as usize)),
                "reused slot must be one the sweep just freed",
            );
        }
        heap.assert_object_arenas_coherent();

        // Drain the rest. The reallocated slots were re-read from the LIVE
        // bitmap and born at the cycle parity, so the remaining slices must
        // not free them (no premature free) nor re-free their slots.
        while !heap.incremental_sweep_slice(1) {}
        assert!(!heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        for (i, r) in reused.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(r.as_float_ptr().unwrap() as *const u8),
                "mid-sweep reallocation was prematurely freed",
            );
            assert!((r.xfloat() - (1_000.0 + i as f64)).abs() < f64::EPSILON);
        }
        for (i, k) in keep.iter().enumerate() {
            assert!((k.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
        }
        // Every dead slot is now either evicted (freed) or reallocated —
        // owned iff reused. A violation in either direction is the
        // double-free / premature-free the window test exists to catch.
        let reused_addrs: std::collections::HashSet<usize> = reused
            .iter()
            .map(|r| r.as_float_ptr().unwrap() as usize)
            .collect();
        for &addr in &dead_addrs {
            assert_eq!(
                heap.owns_non_cons_object(addr as *const u8),
                reused_addrs.contains(&addr),
                "freed slot must be owned iff reallocated",
            );
        }
    }

    #[test]
    fn reuse_within_one_cooperative_sweep_window() {
        reuse_within_one_cooperative_sweep_window_body(false);
    }

    #[test]
    fn reuse_within_one_cooperative_sweep_window_verified() {
        reuse_within_one_cooperative_sweep_window_body(true);
    }

    /// (b) The parity two-cycle properties hold for page floats: an
    /// allocate-black float survives the cycle it was born in (unrooted) and
    /// the next one (rooted); idle-born garbage is reclaimed by the first
    /// cycle after its birth; mark-born garbage floats through its birth
    /// cycle and is reclaimed by the next.
    fn parity_two_cycle_float_survival_and_reclaim_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_verify(&mut heap);
        }

        // STW bootstrap (flip #1) enables the concurrent collector (and
        // blackens the fake dump under verify).
        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // Cycle 2: float born MID-MARK (allocate-black at this cycle's
        // parity), deliberately NOT seeded at the termination.
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let f = heap.alloc_float(2.5);
        let f_ptr = f.as_float_ptr().unwrap() as *const u8;
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine); // f deliberately NOT seeded
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(f_ptr),
            "allocate-black float must survive the cycle it was born in",
        );
        heap.assert_object_arenas_coherent();

        // Cycle 3 (opposite parity): rooted now — must be traced as unmarked
        // via the seed and survive with its payload intact.
        run_concurrent_cycle(&mut heap, &[spine, f]);
        assert!(heap.owns_non_cons_object(f_ptr));
        assert!((f.xfloat() - 2.5).abs() < f64::EPSILON);
        heap.assert_object_arenas_coherent();

        // Reclaim: g1 idle-born (no allocate-black), g2 mark-born.
        let g1 = heap.alloc_float(9.0);
        let g1_ptr = g1.as_float_ptr().unwrap() as *const u8;
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(f);
        heap.launch_concurrent_mark();
        let g2 = heap.alloc_float(8.0);
        let g2_ptr = g2.as_float_ptr().unwrap() as *const u8;
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(f);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        // No allocations since the sweep: the ownership probes below cannot
        // be confused by slot reuse.
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage float must be reclaimed by the next cycle",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage float floats through its birth cycle",
        );
        heap.assert_object_arenas_coherent();

        run_concurrent_cycle(&mut heap, &[spine, f]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage float must be reclaimed by the SECOND cycle",
        );
        assert!((f.xfloat() - 2.5).abs() < f64::EPSILON);
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn parity_two_cycle_float_survival_and_reclaim() {
        parity_two_cycle_float_survival_and_reclaim_body(false);
    }

    #[test]
    fn parity_two_cycle_float_survival_and_reclaim_verified() {
        parity_two_cycle_float_survival_and_reclaim_body(true);
    }

    /// (c) Task 01 CONCURRENT FLOAT CLAIMS: every rooted young page float
    /// discovered during a concurrent mark is CLAIMED on the GC thread
    /// (page-snapshot hit + `mark_claim_at`; zero children so the claim is
    /// the whole trace), never parked — the float bucket collapses to zero
    /// and the claim counter carries the count. Claimed floats survive the
    /// sweep with their payloads intact; a garbage float is still collected
    /// (claims only mark what the marker discovers — the garbage float has
    /// no inbound edge, stays white, and the deferred sweep frees it within
    /// this same cycle).
    fn deferred_floats_resolve_at_termination_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_verify(&mut heap);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // A rooted cons list carrying float cars: the GC thread marks the
        // conses concurrently but parks every float in `deferred`.
        let mut list = TaggedValue::fixnum(0);
        let mut float_vals = Vec::new();
        for i in 0..500 {
            let f = heap.alloc_float(i as f64);
            float_vals.push(f);
            list = heap.alloc_cons(f, list);
        }
        let garbage = heap.alloc_float(-1.0);
        let garbage_ptr = garbage.as_float_ptr().unwrap() as *const u8;

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        let stats = heap.sweep_stats();
        assert!(
            stats.last_concurrent_float_claimed >= 500,
            "every rooted page float must be claimed on the GC thread \
             (claimed={})",
            stats.last_concurrent_float_claimed,
        );
        assert_eq!(
            stats.last_termination_kinds.float, 0,
            "no float may be parked once the claim arm is live (f={})",
            stats.last_termination_kinds.float,
        );
        // Claimed ≡ black at THIS cycle's parity (spot-check one header).
        assert!(unsafe {
            (*(float_vals[0].as_float_ptr().unwrap()))
                .header
                .is_marked_at(heap.mark_parity)
        });
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(list);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        for (i, f) in float_vals.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(f.as_float_ptr().unwrap() as *const u8),
                "deferred-then-resolved float {i} was swept while rooted",
            );
            assert!((f.xfloat() - i as f64).abs() < f64::EPSILON);
        }
        assert!(
            !heap.owns_non_cons_object(garbage_ptr),
            "unrooted float must not be retained by the deferred machinery",
        );
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn concurrent_floats_claimed_and_garbage_freed() {
        deferred_floats_resolve_at_termination_body(false);
    }

    #[test]
    fn concurrent_floats_claimed_and_garbage_freed_verified() {
        deferred_floats_resolve_at_termination_body(true);
    }

    /// Task 01 H2 (snapshot-miss direction, deterministic unit test of the
    /// dispatcher arm): a float living in a page created AFTER the
    /// start-handshake snapshot must DEFER (miss ⇒ defer, never "miss ⇒
    /// mapped" — the mid-cycle-float population), and a deferred float must
    /// not bump the claim counter; a snapshot-page float claims at the job
    /// parity. Drives `concurrent_try_mark_owned` directly with a hand-built
    /// `ConcurrentClaimJob` so the page-boundary race is not left to timing.
    #[test]
    fn concurrent_claim_arm_defers_mid_cycle_float_pages() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // F_OLD lives in a page that exists at the "snapshot" instant.
        let f_old = heap.alloc_float(1.0);
        let snap: rustc_hash::FxHashSet<usize> = heap
            .float_arena
            .pages
            .iter()
            .map(|p| p.base_addr())
            .collect();
        // Allocate until the arena opens a NEW page; the last allocation is
        // the one that triggered it, so it lives in the post-snapshot page.
        let pages_before = heap.float_arena.pages.len();
        let mut f_new = f_old;
        while heap.float_arena.pages.len() == pages_before {
            f_new = heap.alloc_float(2.0);
        }
        let new_base = (f_new.as_float_ptr().unwrap() as usize) & !(OBJECT_PAGE_ALIGN - 1);
        assert!(
            !snap.contains(&new_base),
            "the defer probe must live in a post-snapshot page",
        );

        // Hand-built claim job. Both floats were born at the CURRENT heap
        // parity; a real cycle flips parity at `begin_collection` before
        // launching, so claim at the flipped value exactly like the job
        // a launch would carry.
        let job = ConcurrentClaimJob {
            parity: !heap.mark_parity,
            string_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            float_page_bases: std::sync::Arc::new(snap),
            vector_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            bytecode_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            dump_lo: usize::MAX,
            dump_hi: 0,
            drop_dump_children: false,
            str_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            float_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            subr_dropped: std::sync::Arc::new(AtomicUsize::new(0)),
            vec_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            bc_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
        };
        let mut gray = Vec::new();
        assert!(
            concurrent_try_mark_owned(f_old, &job, &mut gray),
            "snapshot-page float must be handled (claimed)",
        );
        assert_eq!(job.float_claimed.load(Ordering::Relaxed), 1);
        assert!(gray.is_empty(), "floats have no children to gray-push");
        assert!(unsafe {
            (*f_old.as_float_ptr().unwrap())
                .header
                .is_marked_at(!heap.mark_parity)
        });
        assert!(
            !concurrent_try_mark_owned(f_new, &job, &mut gray),
            "post-snapshot-page float must DEFER",
        );
        assert_eq!(
            job.float_claimed.load(Ordering::Relaxed),
            1,
            "a deferred float must not bump the claim counter",
        );
        // The deferred float's header was never touched: still born-at-the-
        // OLD-parity (i.e. unmarked at the job parity).
        assert!(unsafe {
            !(*f_new.as_float_ptr().unwrap())
                .header
                .is_marked_at(!heap.mark_parity)
        });
    }

    /// Task 01 H5 (tenured short-circuit): a TENURED page float discovered
    /// by the GC thread is recognize-and-DROPPED — handled without a parity
    /// claim (counter stays zero), never parked (float bucket zero), and its
    /// FROZEN mark bit is not scribbled. Runs with the partition + verifiers
    /// armed; the first STW cycle performs the one-shot promotion.
    #[test]
    fn concurrent_tenured_float_dropped_not_claimed() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_verify(&mut heap);

        // F is alive across the FIRST partitioned cycle, so the promotion
        // page walk tenures it (page slots of survivors freeze).
        let f = heap.alloc_float(3.25);
        let root = heap.alloc_cons(f, TaggedValue::fixnum(0));
        heap.collect_exact(std::iter::once(root));
        let f_ptr = f.as_float_ptr().unwrap();
        assert!(
            unsafe { (*f_ptr).header.tenured },
            "the first partitioned cycle must promote the surviving float",
        );
        let frozen_bit = unsafe { (*f_ptr).header.is_marked() };

        // One full concurrent cycle with F reachable via the rooted cons:
        // the GC thread discovers F, page-hits (retired/tenured pages stay
        // in the snapshot), sees `tenured`, and drops it.
        run_concurrent_cycle(&mut heap, &[root]);
        let stats = heap.sweep_stats();
        assert_eq!(
            stats.last_concurrent_float_claimed, 0,
            "tenured floats are dropped, not claimed",
        );
        assert_eq!(
            stats.last_termination_kinds.float, 0,
            "tenured floats are dropped, not parked",
        );
        assert_eq!(
            unsafe { (*f_ptr).header.is_marked() },
            frozen_bit,
            "the frozen tenured mark bit must not be scribbled",
        );
        assert!(unsafe { (*f_ptr).header.tenured });
        assert!((f.xfloat() - 3.25).abs() < f64::EPSILON);
        heap.assert_object_arenas_coherent();
    }

    /// (d) Teardown: dropping the heap frees every float page exactly once
    /// (page floats are on none of the intrusive lists, so this is the
    /// explicit `Vec<ObjectPage<FloatObj>>` drop path). Counter deltas are deterministic
    /// under nextest's process-per-test execution.
    fn pages_freed_at_heap_drop_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            heap.extend_dump_span(4096, 16);
            heap.collect_exact(std::iter::empty());
        }
        for i in 0..(2 * FLOAT_PAGE_SLOTS + 5) {
            let _ = heap.alloc_float(i as f64);
        }
        assert_eq!(
            LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
            before + 3,
            "2 full pages + 5 slots must occupy exactly 3 pages",
        );
        // A GC in between releases pages that have no surviving slots.
        heap.collect_exact(std::iter::empty());
        assert_eq!(
            LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
            before,
            "a completed sweep must release completely empty arena pages",
        );
        assert!(heap.float_arena.pages.is_empty());
        heap.assert_object_arenas_coherent();
        drop(heap);
        assert_eq!(
            LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
            before,
            "heap teardown must free every float page exactly once",
        );
    }

    #[test]
    fn pages_freed_at_heap_drop() {
        pages_freed_at_heap_drop_body(false);
    }

    #[test]
    fn pages_freed_at_heap_drop_verified() {
        pages_freed_at_heap_drop_body(true);
    }

    /// (d, mid-mark variant) Dropping the heap while the GC thread is still
    /// concurrently marking must join the thread FIRST and then free the
    /// pages — the join runs in `TaggedHeap::drop`'s body, before the
    /// `Vec<ObjectPage<FloatObj>>` field drop. Under TSAN/ASAN a page freed early would
    /// be a use-after-free on the GC thread; the counter catches leaks and
    /// double-frees.
    fn pages_freed_at_heap_drop_mid_concurrent_mark_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        // A long spine (the GC thread is genuinely marking at drop) whose
        // cars are floats spanning multiple pages.
        const N: usize = 3 * FLOAT_PAGE_SLOTS;
        let mut list = TaggedValue::fixnum(0);
        for i in 0..N {
            let f = heap.alloc_float(i as f64);
            list = heap.alloc_cons(f, list);
        }
        assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 3);
        heap.concurrent_begin();
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        assert!(heap.concurrent_mark_running());
        drop(heap); // must join, then free 3 pages exactly once
        assert_eq!(
            LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
            before,
            "mid-mark teardown must join the GC thread and free every page",
        );
    }

    #[test]
    fn pages_freed_at_heap_drop_mid_concurrent_mark() {
        pages_freed_at_heap_drop_mid_concurrent_mark_body(false);
    }

    #[test]
    fn pages_freed_at_heap_drop_mid_concurrent_mark_verified() {
        pages_freed_at_heap_drop_mid_concurrent_mark_body(true);
    }

    /// (e) Mapped-float coexistence: `register_mapped_float_range` floats are
    /// a third storage class — side-table marks, never in the addr set,
    /// never freed — and must route correctly alongside heap page floats
    /// within one object graph, across the first (promote/blacken) cycle and
    /// a partitioned cycle.
    fn mapped_and_page_floats_coexist_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Stand-in for a pdump image: a leaked, heap-external FloatObj array
        // (must stay mapped for the heap's lifetime — leaking satisfies it).
        let mapped: &'static mut [FloatObj] = Box::leak(
            (0..4)
                .map(|i| FloatObj {
                    header: GcHeader::new(HeapObjectKind::Float),
                    value: 10.0 + i as f64,
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let mapped_ptr = mapped.as_mut_ptr();
        // Registers the range AND activates the dump partition.
        unsafe { heap.register_mapped_float_range(mapped_ptr, 4) };

        let h = heap.alloc_float(1.25);
        let h_ptr = h.as_float_ptr().unwrap() as *const u8;
        let g = heap.alloc_float(2.5);
        let g_ptr = g.as_float_ptr().unwrap() as *const u8;
        let m_ptr = unsafe { mapped_ptr.add(2) };
        let m = unsafe { TaggedValue::from_float_ptr(m_ptr) };
        // One root reaching both storage classes.
        let root = heap.alloc_cons(h, m);

        // First partition cycle: full STW trace + sweep, then promote/blacken.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);

        // Routing: the page float is owned (addr set); the mapped float is
        // NOT in the set (side tables are its mark state) yet was marked and
        // is fully readable; the garbage page float was swept.
        assert!(heap.owns_non_cons_object(h_ptr));
        assert!(!heap.owns_non_cons_object(m_ptr as *const u8));
        assert!(!heap.owns_non_cons_object(g_ptr));
        assert!((h.xfloat() - 1.25).abs() < f64::EPSILON);
        assert!((m.xfloat() - 12.0).abs() < f64::EPSILON);
        // The mapped float's masked page base can never be a live page's
        // base (a page owns its whole 64KB span; allocations are disjoint) —
        // so the page registry cannot misroute mapped floats.
        assert!(
            !heap
                .float_arena
                .page_index_by_base
                .contains_key(&ObjectPage::<FloatObj>::page_base_for_ptr(m_ptr)),
        );
        heap.assert_object_arenas_coherent();

        // Partitioned cycle: the mapped float is permanent-black (skipped),
        // the page float re-marks via the root, fresh garbage is swept.
        let g2 = heap.alloc_float(3.5);
        let g2_ptr = g2.as_float_ptr().unwrap() as *const u8;
        heap.collect_exact(std::iter::once(root));
        assert!(heap.owns_non_cons_object(h_ptr));
        assert!(!heap.owns_non_cons_object(g2_ptr));
        assert!((h.xfloat() - 1.25).abs() < f64::EPSILON);
        assert!((m.xfloat() - 12.0).abs() < f64::EPSILON);
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn mapped_and_page_floats_coexist() {
        mapped_and_page_floats_coexist_body(false);
    }

    #[test]
    fn mapped_and_page_floats_coexist_verified() {
        mapped_and_page_floats_coexist_body(true);
    }

    /// (f) ALLOCATED-BIT-FIRST under adversarial staleness: garbage written
    /// into freed slots' OBJECT bytes (header + value — an invalid kind, a
    /// tenured-looking flag, a junk next pointer) must never be read by the
    /// sweep, the verifiers, or teardown; reallocation must FULL-HEADER-WRITE
    /// every stale byte away. The trailing free-list link word (bytes 24..32)
    /// is arena metadata, not object bytes, and is untouched by the
    /// adversary.
    fn freed_slot_garbage_headers_are_never_read_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_verify(&mut heap);
        }

        let mut floats = Vec::new();
        for i in 0..100 {
            floats.push(heap.alloc_float(i as f64));
        }
        let keep: Vec<TaggedValue> = floats.iter().copied().step_by(2).collect();
        let dead_ptrs: Vec<*mut FloatObj> = floats
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_float_ptr().unwrap() as *mut FloatObj)
            .collect();

        // Free the odd half (this is the first/promote cycle under verify).
        heap.collect_exact(keep.iter().copied());
        for &p in &dead_ptrs {
            assert!(!heap.owns_non_cons_object(p as *const u8));
        }

        // ADVERSARY: scribble every freed slot's first 24 bytes with 0xFF.
        for &p in &dead_ptrs {
            unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<FloatObj>()) };
        }
        // The free list (trailing link words) survived the scribble.
        heap.assert_object_arenas_coherent();

        // A full cycle re-sweeps the page: the scribbled slots' bits are
        // clear, so no header is Drop-dispatched, size-read, or parity-read
        // (reading one would trip the kind/tenured debug asserts — or UB).
        heap.collect_exact(keep.iter().copied());
        for (i, k) in keep.iter().enumerate() {
            assert!((k.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
        }
        heap.assert_object_arenas_coherent();

        // Reallocate exactly the freed population: the class free list hands
        // the scribbled slots back; the FULL-HEADER WRITE must rebuild every
        // header byte (kind, mark bit, tenured, next) from scratch.
        let mut reused = Vec::new();
        for i in 0..dead_ptrs.len() {
            reused.push(heap.alloc_float(500.0 + i as f64));
        }
        let dead_addrs: std::collections::HashSet<usize> =
            dead_ptrs.iter().map(|&p| p as usize).collect();
        for (i, r) in reused.iter().enumerate() {
            let ptr = r.as_float_ptr().unwrap();
            assert!(
                dead_addrs.contains(&(ptr as usize)),
                "reallocation must reuse the freed (scribbled) slots",
            );
            unsafe {
                assert_eq!((*ptr).header.kind, HeapObjectKind::Float);
                assert!(
                    !(*ptr).header.tenured,
                    "stale tenured byte must be rewritten"
                );
                assert!(
                    (*ptr).header.next.is_null(),
                    "stale next ptr must be rewritten"
                );
            }
            assert!((r.xfloat() - (500.0 + i as f64)).abs() < f64::EPSILON);
        }
        heap.assert_object_arenas_coherent();

        // The rebuilt headers survive a rooted cycle (the sweep now reads
        // them — the debug asserts prove they are coherent again), and a
        // final unrooted cycle reclaims them cleanly.
        let mut roots: Vec<TaggedValue> = keep.clone();
        roots.extend(reused.iter().copied());
        heap.collect_exact(roots.iter().copied());
        for (i, r) in reused.iter().enumerate() {
            assert!((r.xfloat() - (500.0 + i as f64)).abs() < f64::EPSILON);
        }
        heap.collect_exact(keep.iter().copied());
        for r in &reused {
            assert!(!heap.owns_non_cons_object(r.as_float_ptr().unwrap() as *const u8));
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn freed_slot_garbage_headers_are_never_read() {
        freed_slot_garbage_headers_are_never_read_body(false);
    }

    #[test]
    fn freed_slot_garbage_headers_are_never_read_verified() {
        freed_slot_garbage_headers_are_never_read_body(true);
    }

    /// Remembered-set safety across the young/tenured boundary: when a page
    /// float's only owner is TENURED at promotion (a Box RECORD here — the
    /// list-promotion path; page vectors get their own tenure coverage with
    /// the stage-3 promotion page walk), the promotion-time permanents scan
    /// must record the owner so the float is re-seeded and survives every
    /// later partitioned cycle.
    fn tenured_owner_keeps_young_page_float_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16); // activates the partition

        // F reachable ONLY through record T (T is a Box veclike, so the
        // list promotion tenures it).
        let f = heap.alloc_float(42.5);
        let f_ptr = f.as_float_ptr().unwrap() as *const u8;
        let t = heap.alloc_record(vec![f]);
        let root = heap.alloc_cons(t, TaggedValue::fixnum(0));

        // First partition cycle: T promotes to tenured.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        let t_header = t.as_veclike_ptr().unwrap();
        assert!(
            unsafe { (*t_header).gc.tenured },
            "record must have tenured"
        );
        assert!(heap.owns_non_cons_object(f_ptr));

        // Two partitioned cycles (one per parity): T is permanent-black and
        // never re-traced; F survives ONLY via the promotion-time remembered
        // set — the permanent owner's young-float edge.
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert!(
                heap.owns_non_cons_object(f_ptr),
                "young page float lost on partitioned cycle {cycle} \
                 (remembered set must retain the tenured owner's float edge)",
            );
            assert!((f.xfloat() - 42.5).abs() < f64::EPSILON);
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_owner_keeps_young_page_float_alive() {
        tenured_owner_keeps_young_page_float_alive_body(false);
    }

    #[test]
    fn tenured_owner_keeps_young_page_float_alive_verified() {
        tenured_owner_keeps_young_page_float_alive_body(true);
    }
}

/// ARENA PROMOTION + RETIREMENT test suite (stage 3, commit 4): the
/// promotion page walk, full-page retirement, mixed-page tenured survival
/// across parities, page-span-oracle exactness, payload-bearing teardown,
/// variable-size live-bytes accounting, and the tenured-page-owner
/// remembered-set scan. Scenarios run plain and (where the partition
/// verifiers add coverage) with `NEOVM_GC_VERIFY_PARTITION=1`.
#[cfg(test)]
mod arena_promotion_tests {
    use super::*;

    fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        // Fake dump span: activates the dump partition so the first full
        // cycle promotes + blackens.
        heap.extend_dump_span(4096, 16);
    }

    /// Build an interval table whose sole plist value is `v` (chars [0, 1)) —
    /// local copy of the ownership_tests helper so this module stands alone.
    fn interval_table_carrying(v: TaggedValue) -> crate::buffer::text_props::TextPropertyTable {
        use crate::buffer::text_props::{PropertyInterval, TextPropertyTable};
        let key = TaggedValue::fixnum(1);
        let mut properties = std::collections::HashMap::new();
        properties.insert(key, v);
        TextPropertyTable::from_dump(vec![PropertyInterval {
            start: 0,
            end: 1,
            properties,
            key_order: vec![key],
        }])
    }

    /// Past the first partition cycle: every paged survivor (float, string,
    /// vector) carries `header.tenured`; a FULL page retires (never swept,
    /// never allocated into, STILL OWNED via the page oracle); partial pages
    /// stay unretired; and the whole tenured population survives TWO further
    /// cycles — one per parity — with payloads intact (the alternating
    /// parity is what frees tenured slots if the sweep parity-reads them).
    fn paged_survivors_tenure_and_full_pages_retire_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        // Exactly one FULL float page, all rooted through a cons spine.
        let mut root = TaggedValue::fixnum(0);
        let mut floats = Vec::with_capacity(FLOAT_PAGE_SLOTS);
        for i in 0..FLOAT_PAGE_SLOTS {
            let f = heap.alloc_float(i as f64);
            floats.push(f);
            root = heap.alloc_cons(f, root);
        }
        assert_eq!(heap.float_arena.pages.len(), 1);
        // A few strings and vectors: their pages stay PARTIAL (mixed).
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("tenure-me"));
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(9); 4]);
        root = heap.alloc_cons(s, root);
        root = heap.alloc_cons(v, root);

        // First partition cycle: full trace + sweep, then promotion.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);

        // Every paged survivor is tenured (the promotion page walk).
        for f in &floats {
            let ptr = f.as_float_ptr().unwrap();
            assert!(unsafe { (*ptr).header.tenured }, "page float not tenured");
        }
        let s_ptr = s.as_string_ptr().unwrap();
        assert!(
            unsafe { (*s_ptr).header.tenured },
            "page string not tenured",
        );
        let v_ptr = v.as_veclike_ptr().unwrap();
        assert!(unsafe { (*v_ptr).gc.tenured }, "page vector not tenured",);

        // The full float page RETIRED; the partial string/vector pages did
        // not. Retired ⇒ still registered + owned (C1), full, no free list.
        assert!(heap.float_arena.pages[0].retired, "full page must retire");
        assert!(!heap.string_arena.pages[0].retired, "partial page retired");
        assert!(!heap.vector_arena.pages[0].retired, "partial page retired");
        assert_eq!(
            heap.float_arena.pages[0].allocated, FLOAT_PAGE_SLOTS,
            "retired page must stay full",
        );
        heap.assert_object_arenas_coherent();

        // Two further cycles — parities false/true — the tenured slots are
        // never freed (retired page skipped whole; mixed pages tenured-skip)
        // and stay owned + intact.
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, f) in floats.iter().enumerate() {
                let ptr = f.as_float_ptr().unwrap() as *const u8;
                assert!(
                    heap.owns_non_cons_object(ptr),
                    "tenured page float #{i} lost on cycle {cycle}",
                );
                assert!((f.xfloat() - i as f64).abs() < f64::EPSILON);
            }
            assert!(heap.owns_non_cons_object(s_ptr as *const u8));
            assert_eq!(
                unsafe { (*s_ptr).data.as_bytes() },
                b"tenure-me",
                "tenured string payload corrupted on cycle {cycle}",
            );
            assert!(heap.owns_non_cons_object(v_ptr as *const u8));
            assert_eq!(
                unsafe { &*(v_ptr as *const VectorObj) }.data.len(),
                4,
                "tenured vector payload lost on cycle {cycle}",
            );
            assert_eq!(heap.float_arena.pages[0].allocated, FLOAT_PAGE_SLOTS);
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn paged_survivors_tenure_and_full_pages_retire() {
        paged_survivors_tenure_and_full_pages_retire_body(false);
    }

    #[test]
    fn paged_survivors_tenure_and_full_pages_retire_verified() {
        paged_survivors_tenure_and_full_pages_retire_body(true);
    }

    /// MIXED page: tenured slots and post-promotion YOUNG slots share a
    /// page. Across TWO alternating-parity cycles the tenured slots survive
    /// with intact payloads (the parity-blind sweep of the float-v1 template
    /// would free them on the flipped cycle) while young garbage in the SAME
    /// page is reclaimed, and freed slots are reused for young objects
    /// without disturbing their tenured neighbors.
    fn mixed_page_tenured_slots_survive_alternating_parities_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        // Interleave keepers and garbage in the same (single) page per class.
        let mut keep_floats = Vec::new();
        let mut keep_strings = Vec::new();
        let mut keep_vectors = Vec::new();
        let mut root = TaggedValue::fixnum(0);
        for i in 0..10 {
            let f = heap.alloc_float(i as f64);
            let s = heap.alloc_string(crate::heap_types::LispString::from_utf8(&format!(
                "mixed-{i}"
            )));
            let v = heap.alloc_vector(vec![TaggedValue::fixnum(i as i64); 3]);
            if i % 2 == 0 {
                keep_floats.push(f);
                keep_strings.push(s);
                keep_vectors.push(v);
                root = heap.alloc_cons(f, root);
                root = heap.alloc_cons(s, root);
                root = heap.alloc_cons(v, root);
            }
        }

        // Promotion cycle: odd-indexed garbage is swept FIRST (its slots are
        // free at promotion), then the survivors tenure ⇒ MIXED pages.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(!heap.float_arena.pages[0].retired);
        assert!(!heap.string_arena.pages[0].retired);
        assert!(!heap.vector_arena.pages[0].retired);

        // Refill the freed slots with YOUNG garbage (free-list reuse puts it
        // in the same mixed pages), then run one cycle per parity.
        for cycle in 0..2 {
            for i in 0..5 {
                let _ = heap.alloc_float(1000.0 + i as f64);
                let _ =
                    heap.alloc_string(crate::heap_types::LispString::from_utf8("young-garbage"));
                let _ = heap.alloc_vector(vec![TaggedValue::fixnum(-1); 2]);
            }
            heap.collect_exact(std::iter::once(root));
            for (i, f) in keep_floats.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(f.as_float_ptr().unwrap() as *const u8),
                    "tenured float #{i} freed on parity cycle {cycle}",
                );
                assert!((f.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
            }
            for (i, s) in keep_strings.iter().enumerate() {
                let ptr = s.as_string_ptr().unwrap();
                assert!(
                    heap.owns_non_cons_object(ptr as *const u8),
                    "tenured string #{i} freed on parity cycle {cycle}",
                );
                assert_eq!(
                    unsafe { (*ptr).data.as_bytes() },
                    format!("mixed-{}", 2 * i).as_bytes(),
                );
            }
            for (i, v) in keep_vectors.iter().enumerate() {
                let ptr = v.as_veclike_ptr().unwrap();
                assert!(
                    heap.owns_non_cons_object(ptr as *const u8),
                    "tenured vector #{i} freed on parity cycle {cycle}",
                );
                let obj = unsafe { &*(ptr as *const VectorObj) };
                assert_eq!(obj.data.as_slice()[0].as_fixnum(), Some(2 * i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn mixed_page_tenured_slots_survive_alternating_parities() {
        mixed_page_tenured_slots_survive_alternating_parities_body(false);
    }

    #[test]
    fn mixed_page_tenured_slots_survive_alternating_parities_verified() {
        mixed_page_tenured_slots_survive_alternating_parities_body(true);
    }

    /// PAGE-SPAN ORACLE EXACTNESS: `owns` answers true for a live slot's
    /// base address ONLY — false for a freed slot (alloc bit), for an
    /// interior address of a live object (stride misalignment), for a
    /// non-slot-aligned address, and for a never-allocated slot beyond the
    /// bump cursor. Per class.
    #[test]
    fn page_span_oracle_freed_slot_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // float: keep, dead, keep2 occupy consecutive slots.
        let keep_f = heap.alloc_float(1.0);
        let dead_f = heap.alloc_float(2.0);
        let keep_f2 = heap.alloc_float(3.0);
        let keep_s = heap.alloc_string(crate::heap_types::LispString::from_utf8("live"));
        let dead_s = heap.alloc_string(crate::heap_types::LispString::from_utf8("dead"));
        let keep_v = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
        let dead_v = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
        let dead_f_ptr = dead_f.as_float_ptr().unwrap() as usize;
        let dead_s_ptr = dead_s.as_string_ptr().unwrap() as usize;
        let dead_v_ptr = dead_v.as_veclike_ptr().unwrap() as usize;

        heap.collect_exact([keep_f, keep_f2, keep_s, keep_v].into_iter());

        let f_addr = keep_f.as_float_ptr().unwrap() as usize;
        let s_addr = keep_s.as_string_ptr().unwrap() as usize;
        let v_addr = keep_v.as_veclike_ptr().unwrap() as usize;
        // Live slot bases answer owned.
        assert!(heap.float_arena.owns(f_addr as *const u8));
        assert!(heap.string_arena.owns(s_addr as *const u8));
        assert!(heap.vector_arena.owns(v_addr as *const u8));
        // Freed-slot addresses answer NOT owned (alloc bit cleared).
        assert!(!heap.float_arena.owns(dead_f_ptr as *const u8));
        assert!(!heap.string_arena.owns(dead_s_ptr as *const u8));
        assert!(!heap.vector_arena.owns(dead_v_ptr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_f_ptr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_s_ptr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_v_ptr as *const u8));
        // Mid-object interior addresses (stride-misaligned) answer NOT owned.
        assert!(!heap.float_arena.owns((f_addr + 8) as *const u8));
        assert!(!heap.string_arena.owns((s_addr + 16) as *const u8));
        assert!(!heap.vector_arena.owns((v_addr + 24) as *const u8));
        // Arbitrary non-slot-aligned addresses answer NOT owned.
        assert!(!heap.float_arena.owns((f_addr + 1) as *const u8));
        assert!(!heap.string_arena.owns((s_addr + 63) as *const u8));
        // Never-allocated slots beyond the bump cursor answer NOT owned even
        // though they are inside a registered page.
        let f_page_base = ObjectPage::<FloatObj>::page_base_for_ptr(f_addr as *const FloatObj);
        let beyond_bump = f_page_base + 100 * <FloatObj as PagedObject>::SLOT_BYTES;
        assert!(!heap.float_arena.owns(beyond_bump as *const u8));
        // Wrong-class registry: a float slot address is not owned by the
        // string/vector arenas (tag-first dispatch to distinct registries).
        assert!(!heap.string_arena.owns(f_addr as *const u8));
        assert!(!heap.vector_arena.owns(f_addr as *const u8));
        heap.assert_object_arenas_coherent();
    }

    /// Teardown with PAYLOAD-BEARING strings + vectors: every string page
    /// and vector page is freed exactly once at heap drop — including
    /// RETIRED pages — with the per-slot `drop_in_place` releasing byte
    /// storage, interval tables, and element Vecs (a leak or double-free
    /// here is what ASAN/MIRI lanes would catch; the counters prove the
    /// page-level accounting either way).
    fn payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
        crate::test_utils::init_test_tracing();
        let strings_before = LIVE_STRING_PAGES.load(Ordering::Relaxed);
        let vectors_before = LIVE_VECTOR_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            heap.extend_dump_span(4096, 16);

            let mut root = TaggedValue::fixnum(0);
            for i in 0..200 {
                let s = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
                    b'p';
                    1024
                ]));
                // Half the strings carry interval tables (dropped at Drop).
                if i % 2 == 0 {
                    let carried = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL);
                    let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
                    unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(carried) };
                }
                let v = heap.alloc_vector(vec![s; 8]);
                root = heap.alloc_cons(v, root);
            }
            assert!(LIVE_STRING_PAGES.load(Ordering::Relaxed) > strings_before);
            assert!(LIVE_VECTOR_PAGES.load(Ordering::Relaxed) > vectors_before);

            // Promotion + retirement happen before the drop (retired pages
            // must be freed by teardown too).
            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            heap.assert_object_arenas_coherent();

            if mid_mark {
                // Drop while the GC thread is concurrently marking: the heap
                // Drop must join FIRST, then free pages (under TSAN/ASAN an
                // early page free is a UAF on the GC thread).
                heap.concurrent_begin();
                heap.seed_root(root);
                heap.launch_concurrent_mark();
                assert!(heap.concurrent_mark_running());
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_STRING_PAGES.load(Ordering::Relaxed),
            strings_before,
            "string pages leaked or double-freed at teardown",
        );
        assert_eq!(
            LIVE_VECTOR_PAGES.load(Ordering::Relaxed),
            vectors_before,
            "vector pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn payload_pages_freed_at_heap_drop() {
        payload_pages_freed_at_heap_drop_body(false);
    }

    #[test]
    fn payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
        payload_pages_freed_at_heap_drop_body(true);
    }

    /// VARIABLE-size live-bytes accounting on BOTH recompute sites: after a
    /// sweep, `live_bytes` equals the independently summed per-survivor
    /// sizes (fixed struct + payload storage) — big string payloads and
    /// vector backings included. An undercount here (e.g. summing fixed
    /// sizes only) skews the adaptive pacer into overtriggering.
    #[test]
    fn sweep_live_bytes_track_variable_payload_sizes() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let s_big = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
            b'q';
            10_000
        ]));
        let s_small = heap.alloc_string(crate::heap_types::LispString::from_utf8("s"));
        let v_big = heap.alloc_vector(vec![TaggedValue::fixnum(5); 1000]);
        let f = heap.alloc_float(2.5);
        // Garbage that must NOT be counted after the sweep.
        let _dead = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
            b'd';
            50_000
        ]));
        let mut root = TaggedValue::fixnum(0);
        let mut cons_count = 0usize;
        for val in [s_big, s_small, v_big, f] {
            root = heap.alloc_cons(val, root);
            cons_count += 1;
        }

        let expected_objects: usize = [s_big, s_small]
            .iter()
            .map(|s| {
                TaggedHeap::object_bytes_from_header(s.as_string_ptr().unwrap() as *const GcHeader)
            })
            .sum::<usize>()
            + TaggedHeap::object_bytes_from_header(
                v_big.as_veclike_ptr().unwrap() as *const GcHeader
            )
            + TaggedHeap::object_bytes_from_header(f.as_float_ptr().unwrap() as *const GcHeader);
        let expected = expected_objects + cons_count * size_of::<ConsCell>();

        // Eager (finalize_collection) recompute site.
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            heap.live_bytes(),
            expected,
            "eager sweep live_bytes != summed survivor bytes",
        );

        // Incremental (sweep slices -> finish_incremental_sweep) site.
        heap.begin_collection();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(
            heap.live_bytes(),
            expected,
            "incremental sweep live_bytes != summed survivor bytes",
        );
    }

    /// THE PROMOTION-SCAN UAF REGRESSION (the reason
    /// `scan_permanents_for_young_children` walks page-tenured slots): a
    /// page vector/string tenured at promotion holds a young CONS child
    /// (conses never tenure) and is never mutated again. Without the
    /// page-tenured remembered-set scan, the next cycle sweeps the cons
    /// while its permanently-black owner still points at it. Two cycles
    /// (both parities) must keep the children readable; under
    /// `NEOVM_GC_VERIFY_PARTITION=1` the extended dump-partition verifier
    /// independently checks every tenured-page child is marked.
    fn tenured_page_owner_keeps_young_cons_child_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        // Young cons children reachable ONLY through paged owners.
        let y_vec = heap.alloc_cons(TaggedValue::fixnum(777), TaggedValue::fixnum(0));
        let v = heap.alloc_vector(vec![y_vec]);
        let y_str = heap.alloc_cons(TaggedValue::fixnum(888), TaggedValue::fixnum(0));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("carrier"));
        unsafe {
            *(*(s.as_string_ptr().unwrap() as *mut StringObj))
                .data
                .intervals_mut() = interval_table_carrying(y_str)
        };
        let tail = heap.alloc_cons(s, TaggedValue::fixnum(0));
        let root = heap.alloc_cons(v, tail);

        // Promotion: v and s tenure via the page walk; y_* stay young.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(unsafe { (*(v.as_veclike_ptr().unwrap())).gc.tenured });
        assert!(unsafe { (*(s.as_string_ptr().unwrap())).header.tenured });

        // Two partitioned cycles (one per parity): the owners are black and
        // never re-traced; the children survive ONLY via the promotion-time
        // page-tenured remembered-set scan.
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*y_vec.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(777).0,
                "tenured page vector's young cons child lost on cycle {cycle}",
            );
            assert_eq!(
                unsafe { (*y_str.xcons_ptr()).load_car() }.0,
                TaggedValue::fixnum(888).0,
                "tenured page string's young interval child lost on cycle {cycle}",
            );
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_page_owner_keeps_young_cons_child_alive() {
        tenured_page_owner_keeps_young_cons_child_alive_body(false);
    }

    #[test]
    fn tenured_page_owner_keeps_young_cons_child_alive_verified() {
        tenured_page_owner_keeps_young_cons_child_alive_body(true);
    }
}

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
mod bytecode_arena_tests {
    use super::*;
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
    use crate::emacs_core::value::LambdaParams;

    fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        // Fake dump span: activates the dump partition so the first full
        // cycle promotes + blackens.
        heap.extend_dump_span(4096, 16);
    }

    /// Drive one full concurrent cycle (start handshake → GC-thread drain →
    /// termination → deferred sweep drained). Copy of the float_arena_tests
    /// helper, local so this module stands alone.
    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// A `ByteCodeFunction` carrying `constants`, `n_ops` no-op instructions,
    /// and `payload` raw GNU bytecode bytes — the REAL-`Drop` payloads the
    /// page sweep must `drop_in_place`. Empty params keep the arglist NIL so
    /// the object's only heap children are its constants (GC-exact tests).
    fn bytecode_fn(constants: Vec<TaggedValue>, n_ops: usize, payload: usize) -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        f.constants = constants.into();
        f.ops = vec![Op::Nil; n_ops];
        if payload > 0 {
            f.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(vec![
                0xAA;
                payload
            ]));
        }
        f
    }

    fn bc_ptr(v: TaggedValue) -> *const u8 {
        v.as_veclike_ptr().unwrap() as *const u8
    }

    /// Read constant `i` of a live bytecode value (payload-intact probe).
    fn bc_constant(v: TaggedValue, i: usize) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const ByteCodeObj) };
        obj.data.constants[i]
    }

    /// (a) PAGE-SPAN ORACLE EXACTNESS for the 384B stride: owned for a live
    /// slot base ONLY — false for freed slots, interior/unaligned addresses,
    /// never-bumped slots, and (unique to the non-power-of-two stride) the
    /// stride-aligned first byte of the 256B page TAIL. Cross-class
    /// registries never collide.
    #[test]
    fn bytecode_page_span_oracle_freed_slot_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let keep = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 4, 0));
        let dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 4, 0));
        let keep2 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(3)], 4, 0));
        let f = heap.alloc_float(1.5);
        let dead_addr = bc_ptr(dead) as usize;

        // Page bytecode never touches the residual addr-set (TRAP A/B: the
        // page oracle owns it from birth).
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!(heap.bytecode_arena.owns(bc_ptr(dead)));

        heap.collect_exact([keep, keep2, f].into_iter());

        let b_addr = bc_ptr(keep) as usize;
        // Live slot bases answer owned (arena + union + veclike routing).
        assert!(heap.bytecode_arena.owns(b_addr as *const u8));
        assert!(heap.owns_non_cons_object(b_addr as *const u8));
        assert!(heap.owns_veclike_object(b_addr as *const u8));
        // Freed slot answers NOT owned the instant its bit clears.
        assert!(!heap.bytecode_arena.owns(dead_addr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
        // Interior (stride-misaligned) + arbitrary unaligned addresses.
        assert!(!heap.bytecode_arena.owns((b_addr + 8) as *const u8));
        assert!(!heap.bytecode_arena.owns((b_addr + 192) as *const u8));
        assert!(!heap.bytecode_arena.owns((b_addr + 1) as *const u8));
        // Never-allocated slot beyond the bump cursor, inside the page.
        let page_base = ObjectPage::<ByteCodeObj>::page_base_for_ptr(b_addr as *const ByteCodeObj);
        let beyond_bump = page_base + 100 * <ByteCodeObj as PagedObject>::SLOT_BYTES;
        assert!(!heap.bytecode_arena.owns(beyond_bump as *const u8));
        // THE PAGE TAIL: slot index SLOTS (byte 65280) is stride-aligned but
        // past the last real slot — the explicit `< SLOTS` bound in `owns`
        // must answer NOT-owned (a power-of-two-stride oracle never sees
        // this case; the 384B class does).
        assert_eq!(ObjectPage::<ByteCodeObj>::SLOTS, BYTECODE_PAGE_SLOTS);
        let tail = page_base + BYTECODE_PAGE_SLOTS * <ByteCodeObj as PagedObject>::SLOT_BYTES;
        assert!(
            tail - page_base < OBJECT_PAGE_BYTES,
            "tail is inside the page"
        );
        assert!(!heap.bytecode_arena.owns(tail as *const u8));
        // Wrong-class registries: never merged, never colliding.
        let f_addr = f.as_float_ptr().unwrap() as usize;
        assert!(!heap.bytecode_arena.owns(f_addr as *const u8));
        assert!(!heap.float_arena.owns(b_addr as *const u8));
        assert!(!heap.vector_arena.owns(b_addr as *const u8));
        assert!(!heap.string_arena.owns(b_addr as *const u8));
        heap.assert_object_arenas_coherent();
    }

    /// (g) `ordinary_non_cons_ownership_index_tracks_sweep`, bytecode form:
    /// the sweep's alloc-bit clear IS the ownership eviction; the residual
    /// addr-set stays empty throughout and payloads stay intact.
    #[test]
    fn bytecode_ownership_tracks_sweep() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let live = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(10)], 8, 64));
        let dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(20)], 8, 64));
        let live_ptr = bc_ptr(live);
        let dead_ptr = bc_ptr(dead);

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(heap.owns_non_cons_object(dead_ptr));
        assert!(heap.bytecode_arena.owns(live_ptr));
        assert!(heap.bytecode_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);

        heap.collect_exact(std::iter::once(live));

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(!heap.owns_non_cons_object(dead_ptr));
        assert!(heap.bytecode_arena.owns(live_ptr));
        assert!(!heap.bytecode_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert_eq!(bc_constant(live, 0).as_fixnum(), Some(10));
        heap.assert_object_arenas_coherent();
    }

    /// (b) Parity two-cycle properties for page bytecode: mark-born
    /// (allocate-black) survives its birth cycle unrooted and the next one
    /// rooted; idle-born garbage is reclaimed by the first cycle after its
    /// birth; mark-born garbage floats through its birth cycle and is
    /// reclaimed by the next.
    fn parity_two_cycle_bytecode_survival_and_reclaim_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        // STW bootstrap (flip #1) enables the concurrent collector (and
        // blackens the fake dump under verify).
        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // Cycle 2: bytecode born MID-MARK (allocate-black at this cycle's
        // parity), deliberately NOT seeded at the termination.
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(25)], 4, 32));
        let b_ptr = bc_ptr(b);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine); // b deliberately NOT seeded
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(b_ptr),
            "allocate-black bytecode must survive the cycle it was born in",
        );
        heap.assert_object_arenas_coherent();

        // Cycle 3 (opposite parity): rooted now — traced as unmarked via the
        // seed and survives with its payload intact.
        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(heap.owns_non_cons_object(b_ptr));
        assert_eq!(bc_constant(b, 0).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();

        // Reclaim: g1 idle-born (no allocate-black), g2 mark-born.
        let g1 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-9)], 4, 32));
        let g1_ptr = bc_ptr(g1);
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(b);
        heap.launch_concurrent_mark();
        let g2 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-8)], 4, 32));
        let g2_ptr = bc_ptr(g2);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(b);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        // No allocations since the sweep: the ownership probes below cannot
        // be confused by slot reuse.
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage bytecode must be reclaimed by the next cycle",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage bytecode floats through its birth cycle",
        );
        heap.assert_object_arenas_coherent();

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage bytecode must be reclaimed by the SECOND cycle",
        );
        assert_eq!(bc_constant(b, 0).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn parity_two_cycle_bytecode_survival_and_reclaim() {
        parity_two_cycle_bytecode_survival_and_reclaim_body(false);
    }

    #[test]
    fn parity_two_cycle_bytecode_survival_and_reclaim_verified() {
        parity_two_cycle_bytecode_survival_and_reclaim_body(true);
    }

    /// (TRAP A, updated for the task 01 bytecode arm) Rooted page bytecode
    /// discovered during a concurrent mark is CLAIMED on the GC thread
    /// (page-snapshot hit + `mark_claim_at` + children gray-push) — the
    /// deferred bytecode bucket collapses to zero and the claim counter
    /// carries the count. Every field `trace_veclike`'s ByteCode arm traces
    /// (arglist, constants, env, doc_form, interactive, extra_slots) holds a
    /// child reachable ONLY through the bytecode; all must survive via the
    /// GC-thread gray-push (the claimed header suppresses the termination
    /// re-trace, so nothing else covers them). Garbage bytecode + its
    /// otherwise-unreachable children must be collected within two cycles.
    fn deferred_bytecode_resolves_at_termination_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // A rooted cons list carrying bytecode cars whose constants carry a
        // cons child reachable ONLY through the bytecode (children coverage).
        let mut list = TaggedValue::fixnum(0);
        let mut bytecodes = Vec::new();
        let mut children = Vec::new();
        for i in 0..300 {
            let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
            children.push(child);
            let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i), child], 4, 16));
            bytecodes.push(b);
            list = heap.alloc_cons(b, list);
        }
        // One bytecode exercising EVERY traced field: each child cons is
        // reachable only through that field.
        let c_arg = heap.alloc_cons(TaggedValue::fixnum(1_001), TaggedValue::fixnum(0));
        let c_env = heap.alloc_cons(TaggedValue::fixnum(1_002), TaggedValue::fixnum(0));
        let c_doc = heap.alloc_cons(TaggedValue::fixnum(1_003), TaggedValue::fixnum(0));
        let c_int = heap.alloc_cons(TaggedValue::fixnum(1_004), TaggedValue::fixnum(0));
        let c_extra = heap.alloc_cons(TaggedValue::fixnum(1_005), TaggedValue::fixnum(0));
        let full = {
            let mut f = bytecode_fn(vec![TaggedValue::fixnum(0)], 4, 16);
            f.arglist = c_arg;
            f.env = Some(c_env);
            f.doc_form = Some(c_doc);
            f.interactive = Some(c_int);
            f.extra_slots = vec![c_extra];
            heap.alloc_bytecode(f)
        };
        list = heap.alloc_cons(full, list);
        // Garbage bytecode whose constants hold an otherwise-unreachable
        // string child (ownership-probe-able, unlike a cons): both must go.
        let g_child = heap.alloc_string(crate::heap_types::LispString::from_utf8("bc-garbage-kid"));
        let g_child_ptr = g_child.as_string_ptr().unwrap() as *const u8;
        let garbage = heap.alloc_bytecode(bytecode_fn(vec![g_child], 4, 16));
        let garbage_ptr = bc_ptr(garbage);

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        let stats = heap.sweep_stats();
        assert!(
            stats.last_concurrent_bc_claimed >= 301,
            "every rooted page bytecode must be claimed on the GC thread \
             (claimed={})",
            stats.last_concurrent_bc_claimed,
        );
        assert_eq!(
            stats.last_termination_kinds.bytecode, 0,
            "no bytecode may park on a bare page-only heap (bc={})",
            stats.last_termination_kinds.bytecode,
        );
        // Claimed ≡ black at THIS cycle's parity (spot-check one header).
        assert!(unsafe {
            (*bytecodes[0].as_veclike_ptr().unwrap())
                .gc
                .is_marked_at(heap.mark_parity)
        });
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(list);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        for (i, b) in bytecodes.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(bc_ptr(*b)),
                "claimed bytecode {i} was swept while rooted",
            );
            assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(i as i64));
            // The constants child (reachable only through the bytecode) was
            // traced via the claim arm's gray-push.
            assert_eq!(
                unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
                Some(10_000 + i as i64),
                "bytecode {i}'s constants child was swept while live",
            );
        }
        for (child, expect, field) in [
            (c_arg, 1_001, "arglist"),
            (c_env, 1_002, "env"),
            (c_doc, 1_003, "doc_form"),
            (c_int, 1_004, "interactive"),
            (c_extra, 1_005, "extra_slots"),
        ] {
            assert_eq!(
                unsafe { (*child.xcons_ptr()).load_car() }.as_fixnum(),
                Some(expect),
                "claimed bytecode's {field} child was swept while live \
                 (the claim arm must gray-push every trace_veclike field)",
            );
        }
        assert!(
            !heap.owns_non_cons_object(garbage_ptr),
            "unrooted bytecode must not be retained by the claim machinery",
        );
        // Second cycle: the garbage child must be gone too (the garbage
        // bytecode was never discovered, so nothing pushed its children).
        run_concurrent_cycle(&mut heap, &[spine, list]);
        assert!(
            !heap.owns_non_cons_object(g_child_ptr),
            "the garbage bytecode's only child must be collected by cycle 2",
        );
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn deferred_bytecode_resolves_at_termination() {
        deferred_bytecode_resolves_at_termination_body(false);
    }

    #[test]
    fn deferred_bytecode_resolves_at_termination_verified() {
        deferred_bytecode_resolves_at_termination_body(true);
    }

    /// Task 01 H2 (snapshot-miss direction, deterministic unit test of the
    /// bytecode arm): bytecode living in a page created AFTER the
    /// start-handshake snapshot must DEFER (miss ⇒ defer, never "miss ⇒
    /// mapped"), without a counter bump or a header write; a snapshot-page
    /// bytecode claims at the job parity AND gray-pushes exactly its heap
    /// children; a re-discovered (already-marked) one is handled WITHOUT a
    /// second push. Drives `concurrent_try_mark_owned` directly with a
    /// hand-built `ConcurrentClaimJob` so the page-boundary race is not
    /// left to timing.
    #[test]
    fn concurrent_claim_arm_defers_mid_cycle_bytecode_pages() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // B_OLD lives in a page that exists at the "snapshot" instant; its
        // constants carry one heap child (plus a fixnum that must NOT be
        // pushed).
        let child = heap.alloc_cons(TaggedValue::fixnum(51), TaggedValue::fixnum(52));
        let b_old = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(7), child], 2, 0));
        let snap: rustc_hash::FxHashSet<usize> = heap
            .bytecode_arena
            .pages
            .iter()
            .map(|p| p.base_addr())
            .collect();
        // Allocate until the arena opens a NEW page; the last allocation
        // lives in the post-snapshot page.
        let pages_before = heap.bytecode_arena.pages.len();
        let mut b_new = b_old;
        while heap.bytecode_arena.pages.len() == pages_before {
            b_new = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 2, 0));
        }
        let new_base = (bc_ptr(b_new) as usize) & !(OBJECT_PAGE_ALIGN - 1);
        assert!(
            !snap.contains(&new_base),
            "the defer probe must live in a post-snapshot page",
        );

        // Hand-built claim job (both bytecodes were born at the CURRENT
        // heap parity; a real cycle flips parity at `begin_collection`
        // before launching, so claim at the flipped value).
        let job = ConcurrentClaimJob {
            parity: !heap.mark_parity,
            string_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            float_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            vector_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
            bytecode_page_bases: std::sync::Arc::new(snap),
            dump_lo: usize::MAX,
            dump_hi: 0,
            drop_dump_children: false,
            str_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            float_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            subr_dropped: std::sync::Arc::new(AtomicUsize::new(0)),
            vec_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
            bc_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
        };
        let mut gray = Vec::new();
        assert!(
            concurrent_try_mark_owned(b_old, &job, &mut gray),
            "snapshot-page bytecode must be handled (claimed)",
        );
        assert_eq!(job.bc_claimed.load(Ordering::Relaxed), 1);
        assert!(unsafe {
            (*b_old.as_veclike_ptr().unwrap())
                .gc
                .is_marked_at(!heap.mark_parity)
        });
        // The fresh claim gray-pushed exactly the HEAP children: the one
        // constants cons (the fixnum constant and the NIL arglist are not
        // heap objects).
        assert_eq!(
            gray.iter().map(|v| v.0).collect::<Vec<_>>(),
            vec![child.0],
            "a fresh bytecode claim must gray-push exactly its heap children",
        );
        // Re-discovery through another edge: already marked ⇒ handled, no
        // counter bump, no duplicate children push.
        gray.clear();
        assert!(
            concurrent_try_mark_owned(b_old, &job, &mut gray),
            "an already-claimed bytecode is handled (nothing further owed)",
        );
        assert_eq!(job.bc_claimed.load(Ordering::Relaxed), 1);
        assert!(
            gray.is_empty(),
            "an already-marked bytecode must not re-push its children",
        );
        // Post-snapshot-page bytecode DEFERS: no claim, no counter, no push,
        // header untouched (still unmarked at the job parity).
        assert!(
            !concurrent_try_mark_owned(b_new, &job, &mut gray),
            "post-snapshot-page bytecode must DEFER",
        );
        assert_eq!(
            job.bc_claimed.load(Ordering::Relaxed),
            1,
            "a deferred bytecode must not bump the claim counter",
        );
        assert!(
            gray.is_empty(),
            "a deferred bytecode must not push children"
        );
        assert!(unsafe {
            !(*b_new.as_veclike_ptr().unwrap())
                .gc
                .is_marked_at(!heap.mark_parity)
        });
    }

    /// Task 01 H5 (tenured short-circuit): a TENURED page bytecode
    /// discovered by the GC thread is recognize-and-DROPPED — handled
    /// without a parity claim (counter stays zero), never parked (bytecode
    /// bucket zero), its FROZEN mark bit is not scribbled, and its young
    /// constants child is not orphaned (the promotion-time page-tenured
    /// remembered-set scan keeps covering it, exactly as on the old defer
    /// path). Partition + tricolor verifiers armed.
    #[test]
    fn concurrent_tenured_bytecode_dropped_not_claimed() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, true);

        // B survives the FIRST partitioned cycle, so the promotion page
        // walk tenures it; its young cons child stays young (conses never
        // tenure) and is reachable ONLY through B's constants.
        let young = heap.alloc_cons(TaggedValue::fixnum(4_321), TaggedValue::fixnum(0));
        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(6), young], 4, 16));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));
        heap.collect_exact(std::iter::once(root));
        let b_hdr = b.as_veclike_ptr().unwrap();
        assert!(
            unsafe { (*b_hdr).gc.tenured },
            "the first partitioned cycle must promote the surviving bytecode",
        );
        let frozen_bit = unsafe { (*b_hdr).gc.is_marked() };

        // One full concurrent cycle with B reachable via the rooted cons:
        // the GC thread discovers B, page-hits, sees `tenured`, and drops.
        run_concurrent_cycle(&mut heap, &[root]);
        let stats = heap.sweep_stats();
        assert_eq!(
            stats.last_concurrent_bc_claimed, 0,
            "tenured bytecode is dropped, not claimed",
        );
        assert_eq!(
            stats.last_termination_kinds.bytecode, 0,
            "tenured bytecode is dropped, not parked",
        );
        assert_eq!(
            unsafe { (*b_hdr).gc.is_marked() },
            frozen_bit,
            "the frozen tenured mark bit must not be scribbled",
        );
        assert!(unsafe { (*b_hdr).gc.tenured });
        assert_eq!(bc_constant(b, 0).as_fixnum(), Some(6));
        assert_eq!(
            unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
            Some(4_321),
            "the tenured bytecode's young constants child must survive the \
             drop (page-tenured remembered-set coverage)",
        );
        heap.assert_object_arenas_coherent();
    }

    /// Task 01 bytecode-claim coverage leg (c), THE ADVERSARIAL ONE:
    /// bytecode constructed MID-CYCLE into a REUSED SLOT of an
    /// already-snapshotted page (page-base HIT — it does NOT defer) holds,
    /// in its constants, the only surviving reference to child C after C's
    /// snapshot home is severed. C must survive its birth cycle: not
    /// through the bytecode (born-at-parity ⇒ the claim arm treats it as
    /// already-marked ⇒ handled WITHOUT a children push) but through the
    /// SATB deletion barrier on the home overwrite. The NEXT cycle then
    /// re-traces: C is reachable ONLY through the bytecode's constants, so
    /// the fresh claim's GC-thread gray-push is the ONLY thing carrying it
    /// (bytecode has no Tier-B backing snapshot — this is where the arm's
    /// children push is load-bearing). Runs with the partition + tricolor
    /// verifiers armed (`verify_incremental_tricolor` is the oracle for
    /// the removed termination re-trace backstop).
    #[test]
    fn concurrent_mid_cycle_bytecode_in_reused_slot_keeps_child_alive() {
        crate::test_utils::init_test_tracing();
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16); // activates the partition

        // Page setup: keeper pins the page; b_dead's slot becomes the free
        // slot the mid-cycle allocation will reuse.
        let b_keep = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 2, 0));
        let b_dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 2, 0));
        let dead_ptr = bc_ptr(b_dead) as usize;
        // C: young cons, reachable at the snapshot ONLY via home H's car.
        let c = heap.alloc_cons(TaggedValue::fixnum(81), TaggedValue::fixnum(82));
        let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
        // Long rooted spine (home at the bottom) so the GC thread is still
        // walking when the mutator severs; both race outcomes are asserted
        // identically (if the GC got to H first, C is simply already black).
        let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
        list = heap.alloc_cons(b_keep, list);
        for i in 0..300_000 {
            list = heap.alloc_cons(TaggedValue::fixnum(i), list);
        }
        let root = list;
        // Bootstrap STW cycle: blackens the fake dump (arming the
        // verifiers), promotes survivors, and frees b_dead's slot.
        heap.collect_exact(std::iter::once(root));
        let pre_launch_bases: std::collections::HashSet<usize> = heap
            .bytecode_arena
            .pages
            .iter()
            .map(|p| p.base_addr())
            .collect();

        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();

        // MID-CYCLE: construct B_NEW carrying C in its constants — the
        // arena's class free list hands back b_dead's slot (page-base in
        // this cycle's snapshot) — then sever C's original home (fires the
        // SATB pre-image barrier).
        let b_new = heap.alloc_bytecode(bytecode_fn(vec![c], 2, 0));
        let new_ptr = bc_ptr(b_new) as usize;
        assert_eq!(
            new_ptr, dead_ptr,
            "the mid-cycle bytecode must land in the freed slot of a \
             snapshotted page (allocator changed? fix the test setup)",
        );
        assert!(
            pre_launch_bases.contains(&(new_ptr & !(OBJECT_PAGE_ALIGN - 1))),
            "the reused slot's page must be in this cycle's snapshot",
        );
        assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

        // Terminate with b_new re-seeded alongside the spine (it is a live
        // value the mutator holds; the explicit-roots harness must name it).
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.seed_root(b_new);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        // Runs verify_dump_partition + verify_incremental_tricolor (armed
        // above): a black b_new with a white C would panic here.
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        // C survived its birth-cycle severing (SATB), with payload intact.
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(81).0,
        );
        assert!(heap.owns_non_cons_object(bc_ptr(b_new)));
        assert_eq!(bc_constant(b_new, 0).0, c.0);

        // NEXT full cycle: C is now reachable ONLY through B_NEW's
        // constants — the fresh claim's children gray-push must carry it.
        run_concurrent_cycle(&mut heap, &[root, b_new]);
        assert!(
            heap.sweep_stats().last_concurrent_bc_claimed >= 1,
            "the second cycle must claim the (now pre-existing) bytecode",
        );
        assert_eq!(
            unsafe { (*c.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(81).0,
        );
        assert_eq!(bc_constant(b_new, 0).0, c.0);
        heap.assert_object_arenas_coherent();
    }

    /// ALLOCATED-BIT-FIRST under adversarial staleness, payload-class form:
    /// garbage scribbled into freed slots' object bytes (a junk kind would
    /// Drop-dispatch garbage `Vec` pointers if any reader trusted it) is
    /// never read by the sweep, verifiers, or teardown; reallocation
    /// FULL-HEADER-WRITEs every stale byte away. The trailing link word
    /// (bytes 376..384) is arena metadata the adversary leaves alone.
    fn bytecode_freed_slot_garbage_never_read_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
            heap.collect_exact(std::iter::empty());
        }

        let mut bytecodes = Vec::new();
        for i in 0..100 {
            bytecodes.push(heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i)], 4, 16)));
        }
        let keep: Vec<TaggedValue> = bytecodes.iter().copied().step_by(2).collect();
        let dead_ptrs: Vec<*mut ByteCodeObj> = bytecodes
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut ByteCodeObj)
            .collect();

        heap.collect_exact(keep.iter().copied());
        for &p in &dead_ptrs {
            assert!(!heap.owns_non_cons_object(p as *const u8));
        }

        // ADVERSARY: scribble every freed slot's object bytes with 0xFF
        // (kind, type tag, vec pointers — everything but the link word).
        for &p in &dead_ptrs {
            unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<ByteCodeObj>()) };
        }
        // The free list (trailing link words) survived the scribble.
        heap.assert_object_arenas_coherent();

        // A full cycle re-sweeps the page: the scribbled slots' bits are
        // clear, so no header is Drop-dispatched, size-read, or parity-read.
        heap.collect_exact(keep.iter().copied());
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(bc_constant(*k, 0).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();

        // Reallocate exactly the freed population: the class free list hands
        // the scribbled slots back; the FULL-HEADER WRITE must rebuild every
        // byte — a stale 0xFF kind/type would misroute the next sweep's
        // `drop_in_place` (type-confused Drop of garbage pointers).
        let mut reused = Vec::new();
        for i in 0..dead_ptrs.len() {
            reused.push(heap.alloc_bytecode(bytecode_fn(
                vec![TaggedValue::fixnum(500 + i as i64)],
                8,
                32,
            )));
        }
        let dead_addrs: std::collections::HashSet<usize> =
            dead_ptrs.iter().map(|&p| p as usize).collect();
        for (i, r) in reused.iter().enumerate() {
            let ptr = r.as_veclike_ptr().unwrap() as *const ByteCodeObj;
            assert!(
                dead_addrs.contains(&(ptr as usize)),
                "reallocation must reuse the freed (scribbled) slots",
            );
            unsafe {
                assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
                assert_eq!((*ptr).header.type_tag, VecLikeType::ByteCode);
                assert!(
                    !(*ptr).header.gc.tenured,
                    "stale tenured byte must be rewritten"
                );
                assert!(
                    (*ptr).header.gc.next.is_null(),
                    "stale next ptr must be rewritten"
                );
            }
            assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(500 + i as i64));
        }
        heap.assert_object_arenas_coherent();

        // The rebuilt headers + payloads survive a rooted cycle, and a final
        // unrooted cycle reclaims them cleanly (their REAL Drop runs on the
        // rewritten — valid — vec pointers, not the scribble).
        let mut roots: Vec<TaggedValue> = keep.clone();
        roots.extend(reused.iter().copied());
        heap.collect_exact(roots.iter().copied());
        for (i, r) in reused.iter().enumerate() {
            assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(500 + i as i64));
        }
        heap.collect_exact(keep.iter().copied());
        for r in &reused {
            assert!(!heap.owns_non_cons_object(bc_ptr(*r)));
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn bytecode_freed_slot_garbage_never_read() {
        bytecode_freed_slot_garbage_never_read_body(false);
    }

    #[test]
    fn bytecode_freed_slot_garbage_never_read_verified() {
        bytecode_freed_slot_garbage_never_read_body(true);
    }

    /// Mid-sweep slot reuse within one cooperative sweep window (the class
    /// free list hands freed slots to a mutator running BETWEEN slices) for
    /// the payload class: no double-free, no premature free, `drop_in_place`
    /// only on dead slots.
    #[test]
    fn bytecode_reuse_within_one_cooperative_sweep_window() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Exactly three full pages of bytecode.
        let n = 3 * BYTECODE_PAGE_SLOTS;
        let mut bytecodes = Vec::with_capacity(n);
        for i in 0..n {
            bytecodes.push(heap.alloc_bytecode(bytecode_fn(
                vec![TaggedValue::fixnum(i as i64)],
                2,
                8,
            )));
        }
        assert_eq!(heap.bytecode_arena.pages.len(), 3);
        heap.assert_object_arenas_coherent();

        let keep: Vec<TaggedValue> = bytecodes.iter().copied().step_by(2).collect();
        let dead_addrs: std::collections::HashSet<usize> = bytecodes
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| bc_ptr(*v) as usize)
            .collect();
        let page0_base = heap.bytecode_arena.pages[0].base_addr();

        heap.begin_collection();
        for &k in &keep {
            heap.seed_root(k);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());

        // Slice 1 (budget 1): sweeps bytecode page 0 only.
        assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
        assert!(heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        // BETWEEN slices the mutator reallocates from the just-swept page.
        let mut reused = Vec::new();
        for i in 0..32 {
            reused.push(heap.alloc_bytecode(bytecode_fn(
                vec![TaggedValue::fixnum(1_000 + i)],
                2,
                8,
            )));
        }
        for r in &reused {
            let ptr = r.as_veclike_ptr().unwrap() as *const ByteCodeObj;
            assert_eq!(
                ObjectPage::<ByteCodeObj>::page_base_for_ptr(ptr),
                page0_base,
                "mid-sweep reuse must come from the just-swept page",
            );
            assert!(dead_addrs.contains(&(ptr as usize)));
        }
        heap.assert_object_arenas_coherent();

        // Drain the rest; reallocated slots are born-at-parity survivors.
        while !heap.incremental_sweep_slice(1) {}
        assert!(!heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        for (i, r) in reused.iter().enumerate() {
            assert!(heap.owns_non_cons_object(bc_ptr(*r)));
            assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(1_000 + i as i64));
        }
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(bc_constant(*k, 0).as_fixnum(), Some(2 * i as i64));
        }
        let reused_addrs: std::collections::HashSet<usize> =
            reused.iter().map(|r| bc_ptr(*r) as usize).collect();
        for &addr in &dead_addrs {
            assert_eq!(
                heap.owns_non_cons_object(addr as *const u8),
                reused_addrs.contains(&addr),
                "freed slot must be owned iff reallocated",
            );
        }
    }

    /// (c) VARIABLE-size live-bytes accounting on BOTH recompute sites for
    /// bytecode: big ops/constants/raw-bytes payloads counted for survivors
    /// (fixed struct + every separately-allocated payload), garbage not.
    #[test]
    fn bytecode_sweep_live_bytes_track_variable_payload_sizes() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let b_big = heap.alloc_bytecode(bytecode_fn(
            vec![TaggedValue::fixnum(7); 500],
            1_000,
            10_000,
        ));
        let b_small = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 1, 0));
        // Garbage that must NOT be counted after the sweep.
        let _dead = heap.alloc_bytecode(bytecode_fn(
            vec![TaggedValue::fixnum(0); 2_000],
            4_000,
            50_000,
        ));
        let mut root = TaggedValue::fixnum(0);
        let mut cons_count = 0usize;
        for val in [b_big, b_small] {
            root = heap.alloc_cons(val, root);
            cons_count += 1;
        }

        let expected_objects: usize = [b_big, b_small]
            .iter()
            .map(|b| {
                TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
            })
            .sum::<usize>();
        let expected = expected_objects + cons_count * size_of::<ConsCell>();
        // The payload really is variable-size (ops + constants + raw bytes
        // dominate the 384B slot).
        assert!(expected_objects > 2 * size_of::<ByteCodeObj>() + 1_000 * size_of::<Op>() + 10_000);

        // Eager (finalize_collection) recompute site.
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            heap.live_bytes(),
            expected,
            "eager sweep live_bytes != summed survivor bytes",
        );

        // Incremental (sweep slices -> finish_incremental_sweep) site.
        heap.begin_collection();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(
            heap.live_bytes(),
            expected,
            "incremental sweep live_bytes != summed survivor bytes",
        );
    }

    /// (d) LOADUP-SHAPED tenure + retirement: bytecode is the first class
    /// where FULL-page retirement meaningfully fires. A full page of rooted
    /// bytecode retires at the one-time promotion (still owned — C1), a
    /// partial page does not; the tenured population survives one cycle per
    /// parity with payloads intact; post-retirement allocation never lands
    /// in the retired page; and the C1 write-barrier edge holds: a RETIRED-
    /// page bytecode given a young cons child (through the test-only seam)
    /// keeps that child alive across both parities.
    fn bytecode_survivors_tenure_and_full_pages_retire_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        // Exactly one FULL bytecode page, all rooted through a cons spine,
        // plus two overflow objects on a second (partial) page.
        let mut root = TaggedValue::fixnum(0);
        let mut bytecodes = Vec::with_capacity(BYTECODE_PAGE_SLOTS + 2);
        for i in 0..(BYTECODE_PAGE_SLOTS + 2) {
            let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i as i64)], 4, 16));
            bytecodes.push(b);
            root = heap.alloc_cons(b, root);
        }
        assert_eq!(heap.bytecode_arena.pages.len(), 2);
        assert_eq!(heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS);

        // First partition cycle: full trace + sweep, then promotion.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);

        // Every paged survivor is tenured (the promotion page walk covers
        // the bytecode arena).
        for b in &bytecodes {
            let ptr = b.as_veclike_ptr().unwrap();
            assert!(unsafe { (*ptr).gc.tenured }, "page bytecode not tenured");
        }
        // The FULL page retired; the partial overflow page did not.
        assert!(
            heap.bytecode_arena.pages[0].retired,
            "full page must retire"
        );
        assert!(
            !heap.bytecode_arena.pages[1].retired,
            "partial page retired"
        );
        assert_eq!(
            heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS,
            "retired page must stay full",
        );
        // C1: retired-page slots STAY owned via the page oracle.
        assert!(heap.owns_non_cons_object(bc_ptr(bytecodes[0])));
        assert!(heap.bytecode_arena.owns(bc_ptr(bytecodes[0])));
        heap.assert_object_arenas_coherent();

        // Post-retirement allocation must never land in the retired page.
        let retired_base = heap.bytecode_arena.pages[0].base_addr();
        let fresh = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-5)], 2, 0));
        assert_ne!(
            ObjectPage::<ByteCodeObj>::page_base_for_ptr(
                fresh.as_veclike_ptr().unwrap() as *const ByteCodeObj
            ),
            retired_base,
            "allocation reused a retired page",
        );

        // C1 write-barrier edge on a RETIRED page: hand a retired-page
        // tenured bytecode a YOUNG cons child through the (test-only,
        // barrier-firing) seam. `value_is_tenured` must answer through the
        // page oracle (retired pages included) so `record_heap_write`
        // remembers the owner and the child survives both parities.
        let young = heap.alloc_cons(TaggedValue::fixnum(777_777), TaggedValue::fixnum(0));
        let carrier = bytecodes[3];
        assert!(
            crate::tagged::mutate::with_bytecode_data_mut_for_test(carrier, |data| {
                data.constants[0] = young;
            })
            .is_some()
        );

        // Two further cycles — parities false/true — retired page skipped
        // whole, partial page tenured-skipped, payloads intact, and the
        // young child of the retired-page owner survives.
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, b) in bytecodes.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(bc_ptr(*b)),
                    "tenured page bytecode #{i} lost on cycle {cycle}",
                );
                if i != 3 {
                    assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(i as i64));
                }
            }
            assert_eq!(
                unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
                Some(777_777),
                "retired-page owner's young cons child lost on cycle {cycle} (C1)",
            );
            assert_eq!(heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS);
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn bytecode_survivors_tenure_and_full_pages_retire() {
        bytecode_survivors_tenure_and_full_pages_retire_body(false);
    }

    #[test]
    fn bytecode_survivors_tenure_and_full_pages_retire_verified() {
        bytecode_survivors_tenure_and_full_pages_retire_body(true);
    }

    /// (d, mixed) Tenured and post-promotion YOUNG slots share a bytecode
    /// page across TWO alternating-parity cycles: tenured slots survive with
    /// intact payloads (a parity-blind sweep would free them on the flipped
    /// cycle), young garbage in the SAME page is reclaimed, and freed slots
    /// are reused without disturbing tenured neighbors.
    fn bytecode_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut keep = Vec::new();
        let mut root = TaggedValue::fixnum(0);
        for i in 0..10 {
            let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i as i64)], 4, 16));
            if i % 2 == 0 {
                keep.push(b);
                root = heap.alloc_cons(b, root);
            }
        }

        // Promotion cycle: odd-indexed garbage swept first, survivors tenure
        // ⇒ a MIXED page.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(!heap.bytecode_arena.pages[0].retired);

        // Refill freed slots with YOUNG garbage, one cycle per parity.
        for cycle in 0..2 {
            for i in 0..5 {
                let _ =
                    heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-(i as i64))], 4, 16));
            }
            heap.collect_exact(std::iter::once(root));
            for (i, b) in keep.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(bc_ptr(*b)),
                    "tenured bytecode #{i} freed on parity cycle {cycle}",
                );
                assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(2 * i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn bytecode_mixed_page_tenured_survive_alternating_parities() {
        bytecode_mixed_page_tenured_survive_alternating_parities_body(false);
    }

    #[test]
    fn bytecode_mixed_page_tenured_survive_alternating_parities_verified() {
        bytecode_mixed_page_tenured_survive_alternating_parities_body(true);
    }

    /// (e) Teardown with payload-bearing bytecode: every bytecode page is
    /// freed exactly once at heap drop — retired pages included — with the
    /// per-slot `drop_in_place` releasing ops/constants vectors, raw GNU
    /// bytes, and docstrings (ASAN/MIRI lanes catch a leak or double-free;
    /// the counters prove page-level accounting either way). The sweep-time
    /// `drop_in_place` path is exercised too (half the population dies
    /// before the drop).
    fn bytecode_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
        crate::test_utils::init_test_tracing();
        let before = LIVE_BYTECODE_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            heap.extend_dump_span(4096, 16);

            let mut root = TaggedValue::fixnum(0);
            for i in 0..300 {
                let mut f = bytecode_fn(vec![TaggedValue::fixnum(i); 16], 128, 1024);
                f.docstring = Some(crate::heap_types::LispString::from_utf8(
                    "payload-bearing bytecode docstring",
                ));
                let b = heap.alloc_bytecode(f);
                // Root every other one; the rest dies at the collection
                // below (sweep-time drop_in_place on this page class).
                if i % 2 == 0 {
                    root = heap.alloc_cons(b, root);
                }
            }
            assert!(LIVE_BYTECODE_PAGES.load(Ordering::Relaxed) > before);

            // Promotion + (partial-page) tenure happen before the drop;
            // retired/mixed pages must be freed by teardown too.
            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            heap.assert_object_arenas_coherent();

            if mid_mark {
                // Drop while the GC thread is concurrently marking: the heap
                // Drop must join FIRST, then free pages.
                heap.concurrent_begin();
                heap.seed_root(root);
                heap.launch_concurrent_mark();
                assert!(heap.concurrent_mark_running());
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_BYTECODE_PAGES.load(Ordering::Relaxed),
            before,
            "bytecode pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn bytecode_payload_pages_freed_at_heap_drop() {
        bytecode_payload_pages_freed_at_heap_drop_body(false);
    }

    #[test]
    fn bytecode_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
        bytecode_payload_pages_freed_at_heap_drop_body(true);
    }

    /// (f) The constants-immutability seam: production bytecode is immutable
    /// post-publish (the mutation helper is `#[cfg(test)]` — enforced at
    /// compile time; this is the invariant task 01's concurrent claim
    /// consumes). The blessed TEST seam still fires the write barrier, so a
    /// tenured owner mutated mid-test keeps its new young child alive —
    /// verified under the partition verifier, which would flag a missed
    /// barrier as a tenured→young violation.
    fn bytecode_constants_test_seam_fires_write_barrier_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(0)], 2, 0));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));
        heap.collect_exact(std::iter::once(root));
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

        // The seam refuses non-bytecode values.
        assert!(crate::tagged::mutate::with_bytecode_data_mut_for_test(root, |_| ()).is_none());

        // Mutate the tenured owner's constants to a YOUNG cons through the
        // seam; the pre-write barrier must remember the owner.
        let young = heap.alloc_cons(TaggedValue::fixnum(4_242), TaggedValue::fixnum(0));
        assert!(
            crate::tagged::mutate::with_bytecode_data_mut_for_test(b, |data| {
                data.constants[0] = young;
            })
            .is_some()
        );

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
                Some(4_242),
                "seam-written young child lost on cycle {cycle} — the \
                 test-only mutation seam must fire the write barrier",
            );
            assert_eq!(bc_constant(b, 0).0, young.0);
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn bytecode_constants_test_seam_fires_write_barrier() {
        bytecode_constants_test_seam_fires_write_barrier_body(false);
    }

    #[test]
    fn bytecode_constants_test_seam_fires_write_barrier_verified() {
        bytecode_constants_test_seam_fires_write_barrier_body(true);
    }

    /// Promotion-scan coverage for the bytecode arena: a page bytecode
    /// tenured at promotion whose constants hold a young CONS child (conses
    /// never tenure) and is never mutated again — the promotion-time
    /// page-tenured remembered-set scan must walk bytecode pages or the
    /// child is swept while its permanently-black owner still points at it.
    fn tenured_page_bytecode_keeps_young_cons_child_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        // A young cons child reachable ONLY through the bytecode's constants.
        let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
        let b = heap.alloc_bytecode(bytecode_fn(vec![y], 4, 0));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

        // Promotion: b tenures via the page walk; y stays young.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

        // Two partitioned cycles (one per parity): the owner is black and
        // never re-traced; the child survives ONLY via the promotion-time
        // page-tenured remembered-set scan (which now walks bytecode pages).
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
                Some(999),
                "tenured page bytecode's young cons child lost on cycle {cycle}",
            );
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_page_bytecode_keeps_young_cons_child_alive() {
        tenured_page_bytecode_keeps_young_cons_child_alive_body(false);
    }

    #[test]
    fn tenured_page_bytecode_keeps_young_cons_child_alive_verified() {
        tenured_page_bytecode_keeps_young_cons_child_alive_body(true);
    }
}

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
mod lambda_macro_arena_tests {
    use super::*;

    fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        heap.extend_dump_span(4096, 16);
    }

    /// Drive one full concurrent cycle (copy of the bytecode_arena_tests
    /// helper, local so this module stands alone).
    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// A closure slot vector whose slot 0 is a fixnum IDENTITY and slot 1 is
    /// `child` (an arbitrary value — a cons in the children-coverage tests),
    /// padded to `n_slots` NILs. The slot `Vec` is the REAL `drop_in_place`
    /// payload the page sweep must free.
    fn lambda_slots(id: i64, child: TaggedValue, n_slots: usize) -> Vec<TaggedValue> {
        let mut v = vec![TaggedValue::NIL; n_slots.max(2)];
        v[0] = TaggedValue::fixnum(id);
        v[1] = child;
        v
    }

    fn lam_ptr(v: TaggedValue) -> *const u8 {
        v.as_veclike_ptr().unwrap() as *const u8
    }
    fn lam_slot(v: TaggedValue, i: usize) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const LambdaObj) };
        obj.data.as_slice()[i]
    }
    fn mac_slot(v: TaggedValue, i: usize) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const MacroObj) };
        obj.data.as_slice()[i]
    }

    /// (a) PAGE-SPAN ORACLE EXACTNESS for the 128B stride: owned for a live
    /// slot base ONLY — false for freed slots, interior/unaligned addresses,
    /// and never-bumped slots. Cross-class registries never collide.
    #[test]
    fn lambda_page_span_oracle_freed_slot_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let keep = heap.alloc_lambda(lambda_slots(1, TaggedValue::NIL, 6));
        let dead = heap.alloc_lambda(lambda_slots(2, TaggedValue::NIL, 6));
        let keep2 = heap.alloc_lambda(lambda_slots(3, TaggedValue::NIL, 6));
        let f = heap.alloc_float(1.5);
        let m = heap.alloc_macro(lambda_slots(9, TaggedValue::NIL, 6));
        let dead_addr = lam_ptr(dead) as usize;

        // Page lambdas never touch the residual addr-set (TRAP A/B).
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!(heap.lambda_arena.owns(lam_ptr(dead)));

        heap.collect_exact([keep, keep2, f, m].into_iter());

        let b_addr = lam_ptr(keep) as usize;
        assert!(heap.lambda_arena.owns(b_addr as *const u8));
        assert!(heap.owns_non_cons_object(b_addr as *const u8));
        assert!(heap.owns_veclike_object(b_addr as *const u8));
        // Freed slot answers NOT owned the instant its bit clears.
        assert!(!heap.lambda_arena.owns(dead_addr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
        // Interior (stride-misaligned) + arbitrary unaligned addresses.
        assert!(!heap.lambda_arena.owns((b_addr + 8) as *const u8));
        assert!(!heap.lambda_arena.owns((b_addr + 64) as *const u8));
        assert!(!heap.lambda_arena.owns((b_addr + 1) as *const u8));
        // Never-allocated slot beyond the bump cursor, inside the page.
        let page_base = ObjectPage::<LambdaObj>::page_base_for_ptr(b_addr as *const LambdaObj);
        let beyond_bump = page_base + 400 * <LambdaObj as PagedObject>::SLOT_BYTES;
        assert!(!heap.lambda_arena.owns(beyond_bump as *const u8));
        // Cross-class registries: never merged, never colliding — including
        // the SIBLING 128B macro arena (same stride, distinct registry).
        let f_addr = f.as_float_ptr().unwrap() as usize;
        assert!(!heap.lambda_arena.owns(f_addr as *const u8));
        assert!(!heap.float_arena.owns(b_addr as *const u8));
        assert!(!heap.vector_arena.owns(b_addr as *const u8));
        assert!(!heap.macro_arena.owns(b_addr as *const u8));
        assert!(!heap.lambda_arena.owns(lam_ptr(m)));
        assert!(heap.macro_arena.owns(lam_ptr(m)));
        heap.assert_object_arenas_coherent();
    }

    /// (g) ownership-index-tracks-sweep: the sweep's alloc-bit clear IS the
    /// ownership eviction; the residual addr-set stays empty throughout and
    /// payloads stay intact.
    #[test]
    fn lambda_ownership_tracks_sweep() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let live = heap.alloc_lambda(lambda_slots(10, TaggedValue::NIL, 8));
        let dead = heap.alloc_lambda(lambda_slots(20, TaggedValue::NIL, 8));
        let live_ptr = lam_ptr(live);
        let dead_ptr = lam_ptr(dead);

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(heap.owns_non_cons_object(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);

        heap.collect_exact(std::iter::once(live));

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(!heap.owns_non_cons_object(dead_ptr));
        assert!(heap.lambda_arena.owns(live_ptr));
        assert!(!heap.lambda_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert_eq!(lam_slot(live, 0).as_fixnum(), Some(10));
        heap.assert_object_arenas_coherent();
    }

    /// (b) Parity two-cycle properties: mark-born survives its birth cycle
    /// unrooted then the next rooted; idle-born garbage reclaimed by the
    /// first cycle after birth; mark-born garbage floats through its birth
    /// cycle and is reclaimed by the next.
    fn parity_two_cycle_lambda_survival_and_reclaim_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        // Cycle 2: lambda born MID-MARK (allocate-black), NOT seeded.
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let b = heap.alloc_lambda(lambda_slots(25, TaggedValue::NIL, 6));
        let b_ptr = lam_ptr(b);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(b_ptr),
            "allocate-black lambda must survive the cycle it was born in",
        );
        heap.assert_object_arenas_coherent();

        // Cycle 3 (opposite parity): rooted — survives with payload intact.
        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(heap.owns_non_cons_object(b_ptr));
        assert_eq!(lam_slot(b, 0).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();

        // Reclaim: g1 idle-born, g2 mark-born.
        let g1 = heap.alloc_lambda(lambda_slots(-9, TaggedValue::NIL, 6));
        let g1_ptr = lam_ptr(g1);
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(b);
        heap.launch_concurrent_mark();
        let g2 = heap.alloc_lambda(lambda_slots(-8, TaggedValue::NIL, 6));
        let g2_ptr = lam_ptr(g2);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(b);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage lambda must be reclaimed by the next cycle",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage lambda floats through its birth cycle",
        );
        heap.assert_object_arenas_coherent();

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage lambda must be reclaimed by the SECOND cycle",
        );
        assert_eq!(lam_slot(b, 0).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn parity_two_cycle_lambda_survival_and_reclaim() {
        parity_two_cycle_lambda_survival_and_reclaim_body(false);
    }
    #[test]
    fn parity_two_cycle_lambda_survival_and_reclaim_verified() {
        parity_two_cycle_lambda_survival_and_reclaim_body(true);
    }

    /// (TRAP A) A lambda parked in `deferred` by the GC thread resolves at
    /// the STW termination through `mark_value`'s OWNED veclike arm — routed
    /// (since this commit) through the page-span oracle. A dropped route
    /// reads as "mapped" and silently drops the mark (UAF). Slot children
    /// (reachable only through the lambda) must be traced. Closures stay
    /// DEFERRED for marking (`closure` drain bucket), unchanged by paging.
    fn deferred_lambda_resolves_at_termination_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        let mut list = TaggedValue::fixnum(0);
        let mut lambdas = Vec::new();
        let mut children = Vec::new();
        for i in 0..300 {
            let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
            children.push(child);
            let b = heap.alloc_lambda(lambda_slots(i, child, 6));
            lambdas.push(b);
            list = heap.alloc_cons(b, list);
        }
        let garbage = heap.alloc_lambda(lambda_slots(-1, TaggedValue::NIL, 6));
        let garbage_ptr = lam_ptr(garbage);

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.closure >= 300,
            "every rooted lambda must reach the termination via `deferred` \
             (got {}) — closures stay deferred in this commit",
            stats.last_termination_kinds.closure,
        );
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(list);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        for (i, b) in lambdas.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(lam_ptr(*b)),
                "deferred-then-resolved lambda {i} was swept while rooted",
            );
            assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(i as i64));
            assert_eq!(
                unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
                Some(10_000 + i as i64),
                "lambda {i}'s slot child was swept while live",
            );
        }
        assert!(
            !heap.owns_non_cons_object(garbage_ptr),
            "unrooted lambda must not be retained by the deferred machinery",
        );
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn deferred_lambda_resolves_at_termination() {
        deferred_lambda_resolves_at_termination_body(false);
    }
    #[test]
    fn deferred_lambda_resolves_at_termination_verified() {
        deferred_lambda_resolves_at_termination_body(true);
    }

    /// ALLOCATED-BIT-FIRST under adversarial staleness, payload-class form:
    /// garbage scribbled into freed slots' object bytes (a junk kind would
    /// Drop-dispatch garbage `Vec`/`OnceLock` pointers if trusted) is never
    /// read by the sweep, verifiers, or teardown; reallocation
    /// FULL-HEADER-WRITEs every stale byte away.
    fn lambda_freed_slot_garbage_never_read_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
            heap.collect_exact(std::iter::empty());
        }

        let mut lambdas = Vec::new();
        for i in 0..100 {
            lambdas.push(heap.alloc_lambda(lambda_slots(i, TaggedValue::NIL, 6)));
        }
        let keep: Vec<TaggedValue> = lambdas.iter().copied().step_by(2).collect();
        let dead_ptrs: Vec<*mut LambdaObj> = lambdas
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut LambdaObj)
            .collect();

        heap.collect_exact(keep.iter().copied());
        for &p in &dead_ptrs {
            assert!(!heap.owns_non_cons_object(p as *const u8));
        }

        // ADVERSARY: scribble every freed slot's object bytes with 0xFF
        // (everything but the trailing link word at 120..128).
        for &p in &dead_ptrs {
            unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<LambdaObj>()) };
        }
        heap.assert_object_arenas_coherent();

        // A full cycle re-sweeps: scribbled slots' bits are clear, so no
        // header is Drop-dispatched, size-read, or parity-read.
        heap.collect_exact(keep.iter().copied());
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(lam_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();

        // Reallocate exactly the freed population: the FULL-HEADER WRITE must
        // rebuild every byte — a stale 0xFF kind/type would misroute the next
        // sweep's `drop_in_place` (type-confused Drop of garbage pointers).
        let mut reused = Vec::new();
        for i in 0..dead_ptrs.len() {
            reused.push(heap.alloc_lambda(lambda_slots(500 + i as i64, TaggedValue::NIL, 8)));
        }
        let dead_addrs: std::collections::HashSet<usize> =
            dead_ptrs.iter().map(|&p| p as usize).collect();
        for (i, r) in reused.iter().enumerate() {
            let ptr = r.as_veclike_ptr().unwrap() as *const LambdaObj;
            assert!(
                dead_addrs.contains(&(ptr as usize)),
                "reallocation must reuse the freed (scribbled) slots",
            );
            unsafe {
                assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
                assert_eq!((*ptr).header.type_tag, VecLikeType::Lambda);
                assert!(
                    !(*ptr).header.gc.tenured,
                    "stale tenured byte must be rewritten"
                );
                assert!(
                    (*ptr).header.gc.next.is_null(),
                    "stale next ptr must be rewritten"
                );
            }
            assert_eq!(lam_slot(*r, 0).as_fixnum(), Some(500 + i as i64));
        }
        heap.assert_object_arenas_coherent();

        // Rebuilt headers + payloads survive a rooted cycle, and a final
        // unrooted cycle reclaims them cleanly (REAL Drop on rewritten — valid
        // — pointers, not the scribble).
        let mut roots: Vec<TaggedValue> = keep.clone();
        roots.extend(reused.iter().copied());
        heap.collect_exact(roots.iter().copied());
        heap.collect_exact(keep.iter().copied());
        for r in &reused {
            assert!(!heap.owns_non_cons_object(lam_ptr(*r)));
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn lambda_freed_slot_garbage_never_read() {
        lambda_freed_slot_garbage_never_read_body(false);
    }
    #[test]
    fn lambda_freed_slot_garbage_never_read_verified() {
        lambda_freed_slot_garbage_never_read_body(true);
    }

    /// Mid-sweep slot reuse within one cooperative sweep window (the class
    /// free list hands freed slots to a mutator running BETWEEN slices) for
    /// the payload class: no double-free, no premature free, `drop_in_place`
    /// only on dead slots.
    #[test]
    fn lambda_reuse_within_one_cooperative_sweep_window() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        // Exactly three full pages of lambdas.
        let n = 3 * LAMBDA_PAGE_SLOTS;
        let mut lambdas = Vec::with_capacity(n);
        for i in 0..n {
            lambdas.push(heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 2)));
        }
        assert_eq!(heap.lambda_arena.pages.len(), 3);
        heap.assert_object_arenas_coherent();

        let keep: Vec<TaggedValue> = lambdas.iter().copied().step_by(2).collect();
        let dead_addrs: std::collections::HashSet<usize> = lambdas
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| lam_ptr(*v) as usize)
            .collect();
        let page0_base = heap.lambda_arena.pages[0].base_addr();

        heap.begin_collection();
        for &k in &keep {
            heap.seed_root(k);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());

        // Slice 1 (budget 1): sweeps lambda page 0 only.
        assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
        assert!(heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        // BETWEEN slices the mutator reallocates from the just-swept page.
        let mut reused = Vec::new();
        for i in 0..32 {
            reused.push(heap.alloc_lambda(lambda_slots(1_000 + i, TaggedValue::NIL, 2)));
        }
        for r in &reused {
            let ptr = r.as_veclike_ptr().unwrap() as *const LambdaObj;
            assert_eq!(
                ObjectPage::<LambdaObj>::page_base_for_ptr(ptr),
                page0_base,
                "mid-sweep reuse must come from the just-swept page",
            );
            assert!(dead_addrs.contains(&(ptr as usize)));
        }
        heap.assert_object_arenas_coherent();

        while !heap.incremental_sweep_slice(1) {}
        assert!(!heap.sweep_in_progress());
        heap.assert_object_arenas_coherent();

        for (i, r) in reused.iter().enumerate() {
            assert!(heap.owns_non_cons_object(lam_ptr(*r)));
            assert_eq!(lam_slot(*r, 0).as_fixnum(), Some(1_000 + i as i64));
        }
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(lam_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
        }
    }

    /// (c) VARIABLE-size live-bytes accounting on BOTH recompute sites: big
    /// slot vectors counted for survivors (fixed struct + owned slot storage),
    /// garbage not.
    #[test]
    fn lambda_sweep_live_bytes_track_variable_payload_sizes() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let l_big = heap.alloc_lambda(lambda_slots(7, TaggedValue::NIL, 2_000));
        let l_small = heap.alloc_lambda(lambda_slots(1, TaggedValue::NIL, 2));
        let _dead = heap.alloc_lambda(lambda_slots(0, TaggedValue::NIL, 4_000));
        let mut root = TaggedValue::fixnum(0);
        let mut cons_count = 0usize;
        for val in [l_big, l_small] {
            root = heap.alloc_cons(val, root);
            cons_count += 1;
        }

        let expected_objects: usize = [l_big, l_small]
            .iter()
            .map(|b| {
                TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
            })
            .sum::<usize>();
        let expected = expected_objects + cons_count * size_of::<ConsCell>();
        assert!(expected_objects > 2 * size_of::<LambdaObj>() + 2_000 * size_of::<TaggedValue>());

        // Eager (finalize_collection) recompute site.
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            heap.live_bytes(),
            expected,
            "eager sweep live_bytes != summed survivor bytes",
        );

        // Incremental (sweep slices -> finish_incremental_sweep) site.
        heap.begin_collection();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(
            heap.live_bytes(),
            expected,
            "incremental sweep live_bytes != summed survivor bytes",
        );
    }

    /// (d) LOADUP-SHAPED tenure + retirement: a full page of rooted lambdas
    /// retires at the one-time promotion (still owned — C1), a partial page
    /// does not; the tenured population survives one cycle per parity with
    /// payloads intact; post-retirement allocation never lands in the retired
    /// page.
    fn lambda_survivors_tenure_and_full_pages_retire_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut root = TaggedValue::fixnum(0);
        let mut lambdas = Vec::with_capacity(LAMBDA_PAGE_SLOTS + 2);
        for i in 0..(LAMBDA_PAGE_SLOTS + 2) {
            let b = heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 4));
            lambdas.push(b);
            root = heap.alloc_cons(b, root);
        }
        assert_eq!(heap.lambda_arena.pages.len(), 2);
        assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);

        for b in &lambdas {
            let ptr = b.as_veclike_ptr().unwrap();
            assert!(unsafe { (*ptr).gc.tenured }, "page lambda not tenured");
        }
        assert!(heap.lambda_arena.pages[0].retired, "full page must retire");
        assert!(!heap.lambda_arena.pages[1].retired, "partial page retired");
        assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);
        // C1: retired-page slots STAY owned.
        assert!(heap.owns_non_cons_object(lam_ptr(lambdas[0])));
        assert!(heap.lambda_arena.owns(lam_ptr(lambdas[0])));
        heap.assert_object_arenas_coherent();

        // Post-retirement allocation must never land in the retired page.
        let retired_base = heap.lambda_arena.pages[0].base_addr();
        let fresh = heap.alloc_lambda(lambda_slots(-5, TaggedValue::NIL, 2));
        assert_ne!(
            ObjectPage::<LambdaObj>::page_base_for_ptr(
                fresh.as_veclike_ptr().unwrap() as *const LambdaObj
            ),
            retired_base,
            "allocation reused a retired page",
        );

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, b) in lambdas.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(lam_ptr(*b)),
                    "tenured page lambda #{i} lost on cycle {cycle}",
                );
                assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(i as i64));
            }
            assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn lambda_survivors_tenure_and_full_pages_retire() {
        lambda_survivors_tenure_and_full_pages_retire_body(false);
    }
    #[test]
    fn lambda_survivors_tenure_and_full_pages_retire_verified() {
        lambda_survivors_tenure_and_full_pages_retire_body(true);
    }

    /// (d, mixed) Tenured + post-promotion YOUNG slots share a lambda page
    /// across TWO alternating-parity cycles: tenured survive intact (a
    /// parity-blind sweep would free them on the flipped cycle), young
    /// garbage in the SAME page is reclaimed.
    fn lambda_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut keep = Vec::new();
        let mut root = TaggedValue::fixnum(0);
        for i in 0..10 {
            let b = heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 4));
            if i % 2 == 0 {
                keep.push(b);
                root = heap.alloc_cons(b, root);
            }
        }
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(!heap.lambda_arena.pages[0].retired);

        for cycle in 0..2 {
            for i in 0..5 {
                let _ = heap.alloc_lambda(lambda_slots(-(i as i64), TaggedValue::NIL, 4));
            }
            heap.collect_exact(std::iter::once(root));
            for (i, b) in keep.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(lam_ptr(*b)),
                    "tenured lambda #{i} freed on parity cycle {cycle}",
                );
                assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(2 * i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn lambda_mixed_page_tenured_survive_alternating_parities() {
        lambda_mixed_page_tenured_survive_alternating_parities_body(false);
    }
    #[test]
    fn lambda_mixed_page_tenured_survive_alternating_parities_verified() {
        lambda_mixed_page_tenured_survive_alternating_parities_body(true);
    }

    /// (e) Teardown with payload-bearing lambdas: every lambda page is freed
    /// exactly once at heap drop — retired pages included — with the per-slot
    /// `drop_in_place` releasing the closure slot `Vec` (ASAN/MIRI catch a
    /// leak/double-free; the counters prove page accounting either way). The
    /// sweep-time `drop_in_place` path is exercised too (half die first).
    fn lambda_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
        crate::test_utils::init_test_tracing();
        let before = LIVE_LAMBDA_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            heap.extend_dump_span(4096, 16);

            let mut root = TaggedValue::fixnum(0);
            for i in 0..1_500 {
                let b = heap.alloc_lambda(lambda_slots(i, TaggedValue::NIL, 32));
                if i % 2 == 0 {
                    root = heap.alloc_cons(b, root);
                }
            }
            assert!(LIVE_LAMBDA_PAGES.load(Ordering::Relaxed) > before);

            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            heap.assert_object_arenas_coherent();

            if mid_mark {
                heap.concurrent_begin();
                heap.seed_root(root);
                heap.launch_concurrent_mark();
                assert!(heap.concurrent_mark_running());
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_LAMBDA_PAGES.load(Ordering::Relaxed),
            before,
            "lambda pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn lambda_payload_pages_freed_at_heap_drop() {
        lambda_payload_pages_freed_at_heap_drop_body(false);
    }
    #[test]
    fn lambda_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
        lambda_payload_pages_freed_at_heap_drop_body(true);
    }

    /// Promotion-scan coverage: a page lambda tenured at promotion whose slot
    /// holds a young CONS child (conses never tenure) and is never mutated —
    /// the promotion-time page-tenured remembered-set scan must walk lambda
    /// pages or the child is swept while its permanently-black owner points
    /// at it.
    fn tenured_page_lambda_keeps_young_cons_child_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
        let b = heap.alloc_lambda(lambda_slots(1, y, 4));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
                Some(999),
                "tenured page lambda's young cons child lost on cycle {cycle}",
            );
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_page_lambda_keeps_young_cons_child_alive() {
        tenured_page_lambda_keeps_young_cons_child_alive_body(false);
    }
    #[test]
    fn tenured_page_lambda_keeps_young_cons_child_alive_verified() {
        tenured_page_lambda_keeps_young_cons_child_alive_body(true);
    }

    // ---- MACRO arena: an independent battery proving its own 128B arena ----

    /// Macro page-span oracle exactness + ownership-tracks-sweep + payload
    /// intact + no cross-class collision with the sibling lambda arena.
    #[test]
    fn macro_oracle_and_sweep_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let live = heap.alloc_macro(lambda_slots(10, TaggedValue::NIL, 8));
        let dead = heap.alloc_macro(lambda_slots(20, TaggedValue::NIL, 8));
        let sibling = heap.alloc_lambda(lambda_slots(30, TaggedValue::NIL, 8));
        let live_ptr = lam_ptr(live);
        let dead_ptr = lam_ptr(dead);

        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!(heap.macro_arena.owns(live_ptr));
        assert!(heap.macro_arena.owns(dead_ptr));
        assert!(!heap.macro_arena.owns(lam_ptr(sibling)));
        assert!(!heap.lambda_arena.owns(live_ptr));

        heap.collect_exact(std::iter::once(live));

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(!heap.owns_non_cons_object(dead_ptr));
        assert!(heap.macro_arena.owns(live_ptr));
        assert!(!heap.macro_arena.owns(dead_ptr));
        // Interior + unaligned answer NOT-owned.
        assert!(!heap.macro_arena.owns((live_ptr as usize + 8) as *const u8));
        assert!(!heap.macro_arena.owns((live_ptr as usize + 1) as *const u8));
        assert_eq!(mac_slot(live, 0).as_fixnum(), Some(10));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        heap.assert_object_arenas_coherent();
    }

    /// Macro loadup-shaped tenure + FULL-page retirement (C1) + teardown
    /// counters with payload `drop_in_place`.
    fn macro_tenure_retire_and_teardown_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let before = LIVE_MACRO_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            arm_partition(&mut heap, verify);

            let mut root = TaggedValue::fixnum(0);
            let mut macros = Vec::with_capacity(LAMBDA_PAGE_SLOTS + 2);
            for i in 0..(LAMBDA_PAGE_SLOTS + 2) {
                let m = heap.alloc_macro(lambda_slots(i as i64, TaggedValue::NIL, 8));
                macros.push(m);
                root = heap.alloc_cons(m, root);
            }
            assert_eq!(heap.macro_arena.pages.len(), 2);
            assert!(LIVE_MACRO_PAGES.load(Ordering::Relaxed) > before);

            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            assert!(
                heap.macro_arena.pages[0].retired,
                "full macro page must retire"
            );
            assert!(!heap.macro_arena.pages[1].retired);
            // C1: retired-page macro slots stay owned across both parities.
            for cycle in 0..2 {
                heap.collect_exact(std::iter::once(root));
                for (i, m) in macros.iter().enumerate() {
                    assert!(
                        heap.owns_non_cons_object(lam_ptr(*m)),
                        "tenured macro #{i} lost on cycle {cycle}",
                    );
                    assert_eq!(mac_slot(*m, 0).as_fixnum(), Some(i as i64));
                }
                heap.assert_object_arenas_coherent();
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_MACRO_PAGES.load(Ordering::Relaxed),
            before,
            "macro pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn macro_tenure_retire_and_teardown() {
        macro_tenure_retire_and_teardown_body(false);
    }
    #[test]
    fn macro_tenure_retire_and_teardown_verified() {
        macro_tenure_retire_and_teardown_body(true);
    }
}

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
mod record_arena_tests {
    use super::*;

    fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        heap.extend_dump_span(4096, 16);
    }

    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// slot 0 = fixnum IDENTITY, slot 1 = `child`, padded to `n` NILs.
    fn record_items(id: i64, child: TaggedValue, n: usize) -> Vec<TaggedValue> {
        let mut v = vec![TaggedValue::NIL; n.max(2)];
        v[0] = TaggedValue::fixnum(id);
        v[1] = child;
        v
    }
    fn rec_ptr(v: TaggedValue) -> *const u8 {
        v.as_veclike_ptr().unwrap() as *const u8
    }
    fn rec_slot(v: TaggedValue, i: usize) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const RecordObj) };
        obj.data.as_slice()[i]
    }

    /// (a) PAGE-SPAN ORACLE EXACTNESS for the 64B record class + cross-class
    /// no-collision (incl. the same-stride string/vector arenas).
    #[test]
    fn record_page_span_oracle_freed_slot_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let keep = heap.alloc_record(record_items(1, TaggedValue::NIL, 4));
        let dead = heap.alloc_record(record_items(2, TaggedValue::NIL, 4));
        let keep2 = heap.alloc_record(record_items(3, TaggedValue::NIL, 4));
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(1); 4]);
        let dead_addr = rec_ptr(dead) as usize;

        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!(heap.record_arena.owns(rec_ptr(dead)));

        heap.collect_exact([keep, keep2, v].into_iter());

        let b_addr = rec_ptr(keep) as usize;
        assert!(heap.record_arena.owns(b_addr as *const u8));
        assert!(heap.owns_non_cons_object(b_addr as *const u8));
        assert!(heap.owns_veclike_object(b_addr as *const u8));
        assert!(!heap.record_arena.owns(dead_addr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
        assert!(!heap.record_arena.owns((b_addr + 8) as *const u8));
        assert!(!heap.record_arena.owns((b_addr + 32) as *const u8));
        assert!(!heap.record_arena.owns((b_addr + 1) as *const u8));
        let page_base = ObjectPage::<RecordObj>::page_base_for_ptr(b_addr as *const RecordObj);
        let beyond_bump = page_base + 800 * <RecordObj as PagedObject>::SLOT_BYTES;
        assert!(!heap.record_arena.owns(beyond_bump as *const u8));
        // Same-stride sibling arenas (vector/string 64B) never collide.
        let v_addr = v.as_veclike_ptr().unwrap() as usize;
        assert!(!heap.record_arena.owns(v_addr as *const u8));
        assert!(!heap.vector_arena.owns(b_addr as *const u8));
        assert!(!heap.string_arena.owns(b_addr as *const u8));
        assert!(!heap.float_arena.owns(b_addr as *const u8));
        heap.assert_object_arenas_coherent();
    }

    /// (g) ownership-index-tracks-sweep; addr-set stays empty; payload intact.
    #[test]
    fn record_ownership_tracks_sweep() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let live = heap.alloc_record(record_items(10, TaggedValue::NIL, 6));
        let dead = heap.alloc_record(record_items(20, TaggedValue::NIL, 6));
        let live_ptr = rec_ptr(live);
        let dead_ptr = rec_ptr(dead);

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(heap.owns_non_cons_object(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);

        heap.collect_exact(std::iter::once(live));

        assert!(heap.record_arena.owns(live_ptr));
        assert!(!heap.record_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert_eq!(rec_slot(live, 0).as_fixnum(), Some(10));
        heap.assert_object_arenas_coherent();
    }

    /// (b) Parity two-cycle survival/reclaim.
    fn parity_two_cycle_record_survival_and_reclaim_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let b = heap.alloc_record(record_items(25, TaggedValue::NIL, 4));
        let b_ptr = rec_ptr(b);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(b_ptr),
            "allocate-black record must survive the cycle it was born in",
        );
        heap.assert_object_arenas_coherent();

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(heap.owns_non_cons_object(b_ptr));
        assert_eq!(rec_slot(b, 0).as_fixnum(), Some(25));

        let g1 = heap.alloc_record(record_items(-9, TaggedValue::NIL, 4));
        let g1_ptr = rec_ptr(g1);
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(b);
        heap.launch_concurrent_mark();
        let g2 = heap.alloc_record(record_items(-8, TaggedValue::NIL, 4));
        let g2_ptr = rec_ptr(g2);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(b);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage record must be reclaimed by the next cycle",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage record floats through its birth cycle",
        );

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage record must be reclaimed by the SECOND cycle",
        );
        assert_eq!(rec_slot(b, 0).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn parity_two_cycle_record_survival_and_reclaim() {
        parity_two_cycle_record_survival_and_reclaim_body(false);
    }
    #[test]
    fn parity_two_cycle_record_survival_and_reclaim_verified() {
        parity_two_cycle_record_survival_and_reclaim_body(true);
    }

    /// (TRAP A) Records parked in `deferred` resolve at termination through
    /// the page-oracle-routed veclike arm; slot children traced. Records stay
    /// DEFERRED for marking (`record` drain bucket).
    fn deferred_record_resolves_at_termination_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        let mut list = TaggedValue::fixnum(0);
        let mut records = Vec::new();
        let mut children = Vec::new();
        for i in 0..300 {
            let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
            children.push(child);
            let b = heap.alloc_record(record_items(i, child, 4));
            records.push(b);
            list = heap.alloc_cons(b, list);
        }
        let garbage = heap.alloc_record(record_items(-1, TaggedValue::NIL, 4));
        let garbage_ptr = rec_ptr(garbage);

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.record >= 300,
            "every rooted record must reach the termination via `deferred` \
             (got {})",
            stats.last_termination_kinds.record,
        );
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(list);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        for (i, b) in records.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(rec_ptr(*b)),
                "deferred-then-resolved record {i} was swept while rooted",
            );
            assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(i as i64));
            assert_eq!(
                unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
                Some(10_000 + i as i64),
                "record {i}'s slot child was swept while live",
            );
        }
        assert!(!heap.owns_non_cons_object(garbage_ptr));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn deferred_record_resolves_at_termination() {
        deferred_record_resolves_at_termination_body(false);
    }
    #[test]
    fn deferred_record_resolves_at_termination_verified() {
        deferred_record_resolves_at_termination_body(true);
    }

    /// ALLOCATED-BIT-FIRST under adversarial staleness (payload class).
    fn record_freed_slot_garbage_never_read_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
            heap.collect_exact(std::iter::empty());
        }

        let mut records = Vec::new();
        for i in 0..100 {
            records.push(heap.alloc_record(record_items(i, TaggedValue::NIL, 4)));
        }
        let keep: Vec<TaggedValue> = records.iter().copied().step_by(2).collect();
        let dead_ptrs: Vec<*mut RecordObj> = records
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut RecordObj)
            .collect();

        heap.collect_exact(keep.iter().copied());
        for &p in &dead_ptrs {
            assert!(!heap.owns_non_cons_object(p as *const u8));
        }
        for &p in &dead_ptrs {
            unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<RecordObj>()) };
        }
        heap.assert_object_arenas_coherent();

        heap.collect_exact(keep.iter().copied());
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(rec_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
        }

        let mut reused = Vec::new();
        for i in 0..dead_ptrs.len() {
            reused.push(heap.alloc_record(record_items(500 + i as i64, TaggedValue::NIL, 6)));
        }
        let dead_addrs: std::collections::HashSet<usize> =
            dead_ptrs.iter().map(|&p| p as usize).collect();
        for (i, r) in reused.iter().enumerate() {
            let ptr = r.as_veclike_ptr().unwrap() as *const RecordObj;
            assert!(dead_addrs.contains(&(ptr as usize)));
            unsafe {
                assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
                assert_eq!((*ptr).header.type_tag, VecLikeType::Record);
                assert!(
                    !(*ptr).header.gc.tenured,
                    "stale tenured byte must be rewritten"
                );
                assert!(
                    (*ptr).header.gc.next.is_null(),
                    "stale next ptr must be rewritten"
                );
            }
            assert_eq!(rec_slot(*r, 0).as_fixnum(), Some(500 + i as i64));
        }
        heap.assert_object_arenas_coherent();

        let mut roots: Vec<TaggedValue> = keep.clone();
        roots.extend(reused.iter().copied());
        heap.collect_exact(roots.iter().copied());
        heap.collect_exact(keep.iter().copied());
        for r in &reused {
            assert!(!heap.owns_non_cons_object(rec_ptr(*r)));
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn record_freed_slot_garbage_never_read() {
        record_freed_slot_garbage_never_read_body(false);
    }
    #[test]
    fn record_freed_slot_garbage_never_read_verified() {
        record_freed_slot_garbage_never_read_body(true);
    }

    /// Mid-sweep cooperative-window slot reuse (payload class).
    #[test]
    fn record_reuse_within_one_cooperative_sweep_window() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let n = 3 * RECORD_PAGE_SLOTS;
        let mut records = Vec::with_capacity(n);
        for i in 0..n {
            records.push(heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 2)));
        }
        assert_eq!(heap.record_arena.pages.len(), 3);

        let keep: Vec<TaggedValue> = records.iter().copied().step_by(2).collect();
        let dead_addrs: std::collections::HashSet<usize> = records
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| rec_ptr(*v) as usize)
            .collect();
        let page0_base = heap.record_arena.pages[0].base_addr();

        heap.begin_collection();
        for &k in &keep {
            heap.seed_root(k);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());
        assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");

        let mut reused = Vec::new();
        for i in 0..32 {
            reused.push(heap.alloc_record(record_items(1_000 + i, TaggedValue::NIL, 2)));
        }
        for r in &reused {
            let ptr = r.as_veclike_ptr().unwrap() as *const RecordObj;
            assert_eq!(ObjectPage::<RecordObj>::page_base_for_ptr(ptr), page0_base);
            assert!(dead_addrs.contains(&(ptr as usize)));
        }
        heap.assert_object_arenas_coherent();

        while !heap.incremental_sweep_slice(1) {}
        assert!(!heap.sweep_in_progress());
        for (i, r) in reused.iter().enumerate() {
            assert!(heap.owns_non_cons_object(rec_ptr(*r)));
            assert_eq!(rec_slot(*r, 0).as_fixnum(), Some(1_000 + i as i64));
        }
    }

    /// (c) VARIABLE-size live-bytes on BOTH recompute sites.
    #[test]
    fn record_sweep_live_bytes_track_variable_payload_sizes() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let r_big = heap.alloc_record(record_items(7, TaggedValue::NIL, 2_000));
        let r_small = heap.alloc_record(record_items(1, TaggedValue::NIL, 2));
        let _dead = heap.alloc_record(record_items(0, TaggedValue::NIL, 4_000));
        let mut root = TaggedValue::fixnum(0);
        let mut cons_count = 0usize;
        for val in [r_big, r_small] {
            root = heap.alloc_cons(val, root);
            cons_count += 1;
        }

        let expected_objects: usize = [r_big, r_small]
            .iter()
            .map(|b| {
                TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
            })
            .sum::<usize>();
        let expected = expected_objects + cons_count * size_of::<ConsCell>();
        assert!(expected_objects > 2 * size_of::<RecordObj>() + 2_000 * size_of::<TaggedValue>());

        heap.collect_exact(std::iter::once(root));
        assert_eq!(heap.live_bytes(), expected, "eager site");

        heap.begin_collection();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(heap.live_bytes(), expected, "incremental site");
    }

    /// (d) LOADUP-SHAPED tenure + FULL-page retirement (C1).
    fn record_survivors_tenure_and_full_pages_retire_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut root = TaggedValue::fixnum(0);
        let mut records = Vec::with_capacity(RECORD_PAGE_SLOTS + 2);
        for i in 0..(RECORD_PAGE_SLOTS + 2) {
            let b = heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 4));
            records.push(b);
            root = heap.alloc_cons(b, root);
        }
        assert_eq!(heap.record_arena.pages.len(), 2);
        assert_eq!(heap.record_arena.pages[0].allocated, RECORD_PAGE_SLOTS);

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        for b in &records {
            assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });
        }
        assert!(heap.record_arena.pages[0].retired, "full page must retire");
        assert!(!heap.record_arena.pages[1].retired);
        assert!(heap.owns_non_cons_object(rec_ptr(records[0])));
        heap.assert_object_arenas_coherent();

        let retired_base = heap.record_arena.pages[0].base_addr();
        let fresh = heap.alloc_record(record_items(-5, TaggedValue::NIL, 2));
        assert_ne!(
            ObjectPage::<RecordObj>::page_base_for_ptr(
                fresh.as_veclike_ptr().unwrap() as *const RecordObj
            ),
            retired_base,
        );

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, b) in records.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(rec_ptr(*b)),
                    "tenured page record #{i} lost on cycle {cycle}",
                );
                assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(i as i64));
            }
            assert_eq!(heap.record_arena.pages[0].allocated, RECORD_PAGE_SLOTS);
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn record_survivors_tenure_and_full_pages_retire() {
        record_survivors_tenure_and_full_pages_retire_body(false);
    }
    #[test]
    fn record_survivors_tenure_and_full_pages_retire_verified() {
        record_survivors_tenure_and_full_pages_retire_body(true);
    }

    /// (d, mixed) Tenured + young slots share a record page across parities.
    fn record_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut keep = Vec::new();
        let mut root = TaggedValue::fixnum(0);
        for i in 0..10 {
            let b = heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 4));
            if i % 2 == 0 {
                keep.push(b);
                root = heap.alloc_cons(b, root);
            }
        }
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(!heap.record_arena.pages[0].retired);

        for cycle in 0..2 {
            for i in 0..5 {
                let _ = heap.alloc_record(record_items(-(i as i64), TaggedValue::NIL, 4));
            }
            heap.collect_exact(std::iter::once(root));
            for (i, b) in keep.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(rec_ptr(*b)),
                    "tenured record #{i} freed on parity cycle {cycle}",
                );
                assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(2 * i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn record_mixed_page_tenured_survive_alternating_parities() {
        record_mixed_page_tenured_survive_alternating_parities_body(false);
    }
    #[test]
    fn record_mixed_page_tenured_survive_alternating_parities_verified() {
        record_mixed_page_tenured_survive_alternating_parities_body(true);
    }

    /// (e) Payload-bearing teardown counters + sweep-time drop_in_place.
    fn record_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
        crate::test_utils::init_test_tracing();
        let before = LIVE_RECORD_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            heap.extend_dump_span(4096, 16);

            let mut root = TaggedValue::fixnum(0);
            for i in 0..3_000 {
                let b = heap.alloc_record(record_items(i, TaggedValue::NIL, 16));
                if i % 2 == 0 {
                    root = heap.alloc_cons(b, root);
                }
            }
            assert!(LIVE_RECORD_PAGES.load(Ordering::Relaxed) > before);

            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            heap.assert_object_arenas_coherent();

            if mid_mark {
                heap.concurrent_begin();
                heap.seed_root(root);
                heap.launch_concurrent_mark();
                assert!(heap.concurrent_mark_running());
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_RECORD_PAGES.load(Ordering::Relaxed),
            before,
            "record pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn record_payload_pages_freed_at_heap_drop() {
        record_payload_pages_freed_at_heap_drop_body(false);
    }
    #[test]
    fn record_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
        record_payload_pages_freed_at_heap_drop_body(true);
    }

    /// Promotion-scan coverage: a tenured page record whose slot holds a
    /// young cons child keeps it alive across both parities.
    fn tenured_page_record_keeps_young_cons_child_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
        let b = heap.alloc_record(record_items(1, y, 4));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
                Some(999),
                "tenured page record's young cons child lost on cycle {cycle}",
            );
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_page_record_keeps_young_cons_child_alive() {
        tenured_page_record_keeps_young_cons_child_alive_body(false);
    }
    #[test]
    fn tenured_page_record_keeps_young_cons_child_alive_verified() {
        tenured_page_record_keeps_young_cons_child_alive_body(true);
    }

    /// WindowConfiguration shares the record arena (same `RecordObj`, distinct
    /// tag): page-owned, coherent (the type-tag check accepts the tag), and
    /// survives a GC alongside plain records.
    #[test]
    fn window_configuration_shares_record_arena() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let wc = heap.alloc_window_configuration(record_items(42, TaggedValue::NIL, 4));
        let rec = heap.alloc_record(record_items(7, TaggedValue::NIL, 4));
        assert_eq!(
            wc.veclike_type(),
            Some(VecLikeType::WindowConfiguration),
            "tag must be WindowConfiguration",
        );
        assert!(
            heap.record_arena.owns(rec_ptr(wc)),
            "window-configuration must live on the record arena pages",
        );
        assert!(heap.owns_veclike_object(rec_ptr(wc)));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        heap.assert_object_arenas_coherent();

        let tail = heap.alloc_cons(rec, TaggedValue::fixnum(0));
        let root = heap.alloc_cons(wc, tail);
        heap.collect_exact(std::iter::once(root));
        assert!(heap.owns_non_cons_object(rec_ptr(wc)));
        assert!(heap.owns_non_cons_object(rec_ptr(rec)));
        assert_eq!(rec_slot(wc, 0).as_fixnum(), Some(42));
        assert_eq!(
            wc.veclike_type(),
            Some(VecLikeType::WindowConfiguration),
            "tag survives the arena round-trip",
        );

        // A dead window-configuration is reclaimed via the record page sweep.
        let dead_wc = heap.alloc_window_configuration(record_items(-1, TaggedValue::NIL, 4));
        let dead_ptr = rec_ptr(dead_wc);
        heap.collect_exact(std::iter::once(root));
        assert!(!heap.owns_non_cons_object(dead_ptr));
        heap.assert_object_arenas_coherent();
    }
}

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
#[cfg(test)]
mod symbol_with_pos_arena_tests {
    use super::*;

    fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
        if verify {
            unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
        }
        heap.extend_dump_span(4096, 16);
    }

    fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        for &root in roots {
            heap.seed_root(root);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
    }

    /// A symbol-with-pos whose `pos` fixnum is the IDENTITY and `sym` is
    /// `sym_val` (`T` in the basic tests, a young cons in the child tests).
    fn swp(heap: &mut TaggedHeap, id: i64, sym_val: TaggedValue) -> TaggedValue {
        heap.alloc_symbol_with_pos(sym_val, TaggedValue::fixnum(id))
    }
    fn swp_ptr(v: TaggedValue) -> *const u8 {
        v.as_veclike_ptr().unwrap() as *const u8
    }
    fn swp_pos(v: TaggedValue) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const SymbolWithPosObj) };
        obj.pos
    }
    fn swp_sym(v: TaggedValue) -> TaggedValue {
        let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const SymbolWithPosObj) };
        obj.sym
    }

    /// SymbolWithPos is POD (no Drop) — the class behaves like FloatObj, so
    /// the generic sweep/teardown `drop_in_place` walk compiles out.
    #[test]
    fn symbol_with_pos_is_pod() {
        assert!(
            !std::mem::needs_drop::<SymbolWithPosObj>(),
            "SymbolWithPosObj must stay POD (no Drop) — if this fails a \
             Drop-worthy field was added and the sweep must drop_in_place it",
        );
    }

    /// (a) PAGE-SPAN ORACLE EXACTNESS for the 64B class + cross-class
    /// no-collision (incl. the same-stride record/vector/string arenas).
    #[test]
    fn symbol_with_pos_page_span_oracle_freed_slot_exactness() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let keep = swp(&mut heap, 1, TaggedValue::T);
        let dead = swp(&mut heap, 2, TaggedValue::T);
        let keep2 = swp(&mut heap, 3, TaggedValue::T);
        let r = heap.alloc_record(vec![TaggedValue::fixnum(1); 4]);
        let dead_addr = swp_ptr(dead) as usize;

        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert!(heap.symbol_with_pos_arena.owns(swp_ptr(dead)));

        heap.collect_exact([keep, keep2, r].into_iter());

        let b_addr = swp_ptr(keep) as usize;
        assert!(heap.symbol_with_pos_arena.owns(b_addr as *const u8));
        assert!(heap.owns_non_cons_object(b_addr as *const u8));
        assert!(heap.owns_veclike_object(b_addr as *const u8));
        assert!(!heap.symbol_with_pos_arena.owns(dead_addr as *const u8));
        assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
        assert!(!heap.symbol_with_pos_arena.owns((b_addr + 8) as *const u8));
        assert!(!heap.symbol_with_pos_arena.owns((b_addr + 32) as *const u8));
        assert!(!heap.symbol_with_pos_arena.owns((b_addr + 1) as *const u8));
        let page_base =
            ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(b_addr as *const SymbolWithPosObj);
        let beyond_bump = page_base + 900 * <SymbolWithPosObj as PagedObject>::SLOT_BYTES;
        assert!(!heap.symbol_with_pos_arena.owns(beyond_bump as *const u8));
        // Same-stride sibling arenas (record/vector/string 64B) never collide.
        let r_addr = r.as_veclike_ptr().unwrap() as usize;
        assert!(!heap.symbol_with_pos_arena.owns(r_addr as *const u8));
        assert!(!heap.record_arena.owns(b_addr as *const u8));
        assert!(!heap.vector_arena.owns(b_addr as *const u8));
        assert!(!heap.string_arena.owns(b_addr as *const u8));
        heap.assert_object_arenas_coherent();
    }

    /// (g) ownership-index-tracks-sweep; addr-set empty; sym/pos intact.
    #[test]
    fn symbol_with_pos_ownership_tracks_sweep() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let live = swp(&mut heap, 10, TaggedValue::T);
        let dead = swp(&mut heap, 20, TaggedValue::T);
        let live_ptr = swp_ptr(live);
        let dead_ptr = swp_ptr(dead);

        assert!(heap.owns_non_cons_object(live_ptr));
        assert!(heap.owns_non_cons_object(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);

        heap.collect_exact(std::iter::once(live));

        assert!(heap.symbol_with_pos_arena.owns(live_ptr));
        assert!(!heap.symbol_with_pos_arena.owns(dead_ptr));
        assert_eq!(heap.non_cons_object_addrs.len(), 0);
        assert_eq!(swp_pos(live).as_fixnum(), Some(10));
        assert_eq!(swp_sym(live).0, TaggedValue::T.0);
        heap.assert_object_arenas_coherent();
    }

    /// (b) Parity two-cycle survival/reclaim.
    fn parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.launch_concurrent_mark();
        let b = swp(&mut heap, 25, TaggedValue::T);
        let b_ptr = swp_ptr(b);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            heap.owns_non_cons_object(b_ptr),
            "allocate-black symbol-with-pos must survive its birth cycle",
        );

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(heap.owns_non_cons_object(b_ptr));
        assert_eq!(swp_pos(b).as_fixnum(), Some(25));

        let g1 = swp(&mut heap, -9, TaggedValue::T);
        let g1_ptr = swp_ptr(g1);
        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(b);
        heap.launch_concurrent_mark();
        let g2 = swp(&mut heap, -8, TaggedValue::T);
        let g2_ptr = swp_ptr(g2);
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(b);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(
            !heap.owns_non_cons_object(g1_ptr),
            "idle-born garbage must be reclaimed by the next cycle",
        );
        assert!(
            heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage floats through its birth cycle",
        );

        run_concurrent_cycle(&mut heap, &[spine, b]);
        assert!(
            !heap.owns_non_cons_object(g2_ptr),
            "mark-born garbage must be reclaimed by the SECOND cycle",
        );
        assert_eq!(swp_pos(b).as_fixnum(), Some(25));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn parity_two_cycle_symbol_with_pos_survival_and_reclaim() {
        parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(false);
    }
    #[test]
    fn parity_two_cycle_symbol_with_pos_survival_and_reclaim_verified() {
        parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(true);
    }

    /// (TRAP A) SymbolWithPos parked in `deferred` resolves at termination
    /// through the page-oracle-routed veclike arm; its `sym` child (a young
    /// cons reachable only through it) is traced. SymbolWithPos parks in the
    /// `other` drain bucket (marking unchanged by paging).
    fn deferred_symbol_with_pos_resolves_at_termination_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
        }

        let mut spine = TaggedValue::fixnum(0);
        for i in 0..100_000 {
            spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
        }
        heap.collect_exact(std::iter::once(spine));
        assert!(heap.should_run_concurrent());

        let mut list = TaggedValue::fixnum(0);
        let mut swps = Vec::new();
        let mut children = Vec::new();
        for i in 0..300 {
            let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
            children.push(child);
            // `sym` = the young cons child (traced by collect_veclike_children).
            let b = heap.alloc_symbol_with_pos(child, TaggedValue::fixnum(i));
            swps.push(b);
            list = heap.alloc_cons(b, list);
        }
        let garbage = swp(&mut heap, -1, TaggedValue::T);
        let garbage_ptr = swp_ptr(garbage);

        heap.concurrent_begin();
        heap.seed_root(spine);
        heap.seed_root(list);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        let stats = heap.sweep_stats();
        assert!(
            stats.last_termination_kinds.other >= 300,
            "every rooted symbol-with-pos must reach the termination via \
             `deferred` (other bucket, got {})",
            stats.last_termination_kinds.other,
        );
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(spine);
        heap.seed_root(list);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();

        for (i, b) in swps.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(swp_ptr(*b)),
                "deferred-then-resolved symbol-with-pos {i} was swept while rooted",
            );
            assert_eq!(swp_pos(*b).as_fixnum(), Some(i as i64));
            assert_eq!(
                unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
                Some(10_000 + i as i64),
                "symbol-with-pos {i}'s sym child was swept while live",
            );
        }
        assert!(!heap.owns_non_cons_object(garbage_ptr));
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn deferred_symbol_with_pos_resolves_at_termination() {
        deferred_symbol_with_pos_resolves_at_termination_body(false);
    }
    #[test]
    fn deferred_symbol_with_pos_resolves_at_termination_verified() {
        deferred_symbol_with_pos_resolves_at_termination_body(true);
    }

    /// ALLOCATED-BIT-FIRST under adversarial staleness. POD: no Drop to
    /// type-confuse, but a stale header still misreads parity/tenured/size;
    /// the full-header rewrite + allocated-bit-first keep the sweep exact.
    fn symbol_with_pos_freed_slot_garbage_never_read_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        if verify {
            arm_partition(&mut heap, true);
            heap.collect_exact(std::iter::empty());
        }

        let mut objs = Vec::new();
        for i in 0..100 {
            objs.push(swp(&mut heap, i, TaggedValue::T));
        }
        let keep: Vec<TaggedValue> = objs.iter().copied().step_by(2).collect();
        let dead_ptrs: Vec<*mut SymbolWithPosObj> = objs
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut SymbolWithPosObj)
            .collect();

        heap.collect_exact(keep.iter().copied());
        for &p in &dead_ptrs {
            assert!(!heap.owns_non_cons_object(p as *const u8));
        }
        for &p in &dead_ptrs {
            unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<SymbolWithPosObj>()) };
        }
        heap.assert_object_arenas_coherent();

        heap.collect_exact(keep.iter().copied());
        for (i, k) in keep.iter().enumerate() {
            assert_eq!(swp_pos(*k).as_fixnum(), Some(2 * i as i64));
        }

        let mut reused = Vec::new();
        for i in 0..dead_ptrs.len() {
            reused.push(swp(&mut heap, 500 + i as i64, TaggedValue::T));
        }
        let dead_addrs: std::collections::HashSet<usize> =
            dead_ptrs.iter().map(|&p| p as usize).collect();
        for (i, r) in reused.iter().enumerate() {
            let ptr = r.as_veclike_ptr().unwrap() as *const SymbolWithPosObj;
            assert!(dead_addrs.contains(&(ptr as usize)));
            unsafe {
                assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
                assert_eq!((*ptr).header.type_tag, VecLikeType::SymbolWithPos);
                assert!(
                    !(*ptr).header.gc.tenured,
                    "stale tenured byte must be rewritten"
                );
                assert!(
                    (*ptr).header.gc.next.is_null(),
                    "stale next ptr must be rewritten"
                );
            }
            assert_eq!(swp_pos(*r).as_fixnum(), Some(500 + i as i64));
        }
        heap.assert_object_arenas_coherent();

        let mut roots: Vec<TaggedValue> = keep.clone();
        roots.extend(reused.iter().copied());
        heap.collect_exact(roots.iter().copied());
        heap.collect_exact(keep.iter().copied());
        for r in &reused {
            assert!(!heap.owns_non_cons_object(swp_ptr(*r)));
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn symbol_with_pos_freed_slot_garbage_never_read() {
        symbol_with_pos_freed_slot_garbage_never_read_body(false);
    }
    #[test]
    fn symbol_with_pos_freed_slot_garbage_never_read_verified() {
        symbol_with_pos_freed_slot_garbage_never_read_body(true);
    }

    /// Mid-sweep cooperative-window slot reuse.
    #[test]
    fn symbol_with_pos_reuse_within_one_cooperative_sweep_window() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let n = 3 * SYMBOL_WITH_POS_PAGE_SLOTS;
        let mut objs = Vec::with_capacity(n);
        for i in 0..n {
            objs.push(swp(&mut heap, i as i64, TaggedValue::T));
        }
        assert_eq!(heap.symbol_with_pos_arena.pages.len(), 3);

        let keep: Vec<TaggedValue> = objs.iter().copied().step_by(2).collect();
        let dead_addrs: std::collections::HashSet<usize> = objs
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| swp_ptr(*v) as usize)
            .collect();
        let page0_base = heap.symbol_with_pos_arena.pages[0].base_addr();

        heap.begin_collection();
        for &k in &keep {
            heap.seed_root(k);
        }
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        assert!(heap.sweep_in_progress());
        assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");

        let mut reused = Vec::new();
        for i in 0..32 {
            reused.push(swp(&mut heap, 1_000 + i, TaggedValue::T));
        }
        for r in &reused {
            let ptr = r.as_veclike_ptr().unwrap() as *const SymbolWithPosObj;
            assert_eq!(
                ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(ptr),
                page0_base
            );
            assert!(dead_addrs.contains(&(ptr as usize)));
        }
        heap.assert_object_arenas_coherent();

        while !heap.incremental_sweep_slice(1) {}
        assert!(!heap.sweep_in_progress());
        for (i, r) in reused.iter().enumerate() {
            assert!(heap.owns_non_cons_object(swp_ptr(*r)));
            assert_eq!(swp_pos(*r).as_fixnum(), Some(1_000 + i as i64));
        }
    }

    /// (c) FIXED-size live-bytes on BOTH recompute sites (SymbolWithPos has no
    /// variable payload, so survivors count exactly size_of::<SymbolWithPosObj>()
    /// each — but the accounting must still be exact on both paths).
    #[test]
    fn symbol_with_pos_sweep_live_bytes_fixed_size_both_sites() {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let a = swp(&mut heap, 1, TaggedValue::T);
        let b = swp(&mut heap, 2, TaggedValue::T);
        let _dead = swp(&mut heap, 3, TaggedValue::T);
        let mut root = TaggedValue::fixnum(0);
        let mut cons_count = 0usize;
        for val in [a, b] {
            root = heap.alloc_cons(val, root);
            cons_count += 1;
        }
        let expected = 2 * size_of::<SymbolWithPosObj>() + cons_count * size_of::<ConsCell>();

        heap.collect_exact(std::iter::once(root));
        assert_eq!(heap.live_bytes(), expected, "eager site");

        heap.begin_collection();
        heap.seed_root(root);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert_eq!(heap.live_bytes(), expected, "incremental site");
    }

    /// (d) LOADUP-SHAPED tenure + FULL-page retirement (C1).
    fn symbol_with_pos_survivors_tenure_and_full_pages_retire_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut root = TaggedValue::fixnum(0);
        let mut objs = Vec::with_capacity(SYMBOL_WITH_POS_PAGE_SLOTS + 2);
        for i in 0..(SYMBOL_WITH_POS_PAGE_SLOTS + 2) {
            let b = swp(&mut heap, i as i64, TaggedValue::T);
            objs.push(b);
            root = heap.alloc_cons(b, root);
        }
        assert_eq!(heap.symbol_with_pos_arena.pages.len(), 2);
        assert_eq!(
            heap.symbol_with_pos_arena.pages[0].allocated,
            SYMBOL_WITH_POS_PAGE_SLOTS
        );

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        for b in &objs {
            assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });
        }
        assert!(
            heap.symbol_with_pos_arena.pages[0].retired,
            "full page must retire"
        );
        assert!(!heap.symbol_with_pos_arena.pages[1].retired);
        assert!(heap.owns_non_cons_object(swp_ptr(objs[0])));
        heap.assert_object_arenas_coherent();

        let retired_base = heap.symbol_with_pos_arena.pages[0].base_addr();
        let fresh = swp(&mut heap, -5, TaggedValue::T);
        assert_ne!(
            ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(
                fresh.as_veclike_ptr().unwrap() as *const SymbolWithPosObj
            ),
            retired_base,
        );

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, b) in objs.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(swp_ptr(*b)),
                    "tenured page symbol-with-pos #{i} lost on cycle {cycle}",
                );
                assert_eq!(swp_pos(*b).as_fixnum(), Some(i as i64));
            }
            assert_eq!(
                heap.symbol_with_pos_arena.pages[0].allocated,
                SYMBOL_WITH_POS_PAGE_SLOTS
            );
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn symbol_with_pos_survivors_tenure_and_full_pages_retire() {
        symbol_with_pos_survivors_tenure_and_full_pages_retire_body(false);
    }
    #[test]
    fn symbol_with_pos_survivors_tenure_and_full_pages_retire_verified() {
        symbol_with_pos_survivors_tenure_and_full_pages_retire_body(true);
    }

    /// (d, mixed) Tenured + young slots share a page across parities.
    fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut keep = Vec::new();
        let mut root = TaggedValue::fixnum(0);
        for i in 0..10 {
            let b = swp(&mut heap, i as i64, TaggedValue::T);
            if i % 2 == 0 {
                keep.push(b);
                root = heap.alloc_cons(b, root);
            }
        }
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(!heap.symbol_with_pos_arena.pages[0].retired);

        for cycle in 0..2 {
            for i in 0..5 {
                let _ = swp(&mut heap, -(i as i64), TaggedValue::T);
            }
            heap.collect_exact(std::iter::once(root));
            for (i, b) in keep.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(swp_ptr(*b)),
                    "tenured symbol-with-pos #{i} freed on parity cycle {cycle}",
                );
                assert_eq!(swp_pos(*b).as_fixnum(), Some(2 * i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
    }

    #[test]
    fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities() {
        symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(false);
    }
    #[test]
    fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities_verified() {
        symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(true);
    }

    /// (e) Teardown page counters (POD — the drop_in_place walk compiles out,
    /// but every page — retired included — is still dealloc'd exactly once).
    fn symbol_with_pos_pages_freed_at_heap_drop_body(mid_mark: bool) {
        crate::test_utils::init_test_tracing();
        let before = LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed);
        {
            let mut heap = TaggedHeap::new();
            set_tagged_heap(&mut heap);
            heap.extend_dump_span(4096, 16);

            let mut root = TaggedValue::fixnum(0);
            for i in 0..3_000 {
                let b = swp(&mut heap, i, TaggedValue::T);
                if i % 2 == 0 {
                    root = heap.alloc_cons(b, root);
                }
            }
            assert!(LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed) > before);

            heap.collect_exact(std::iter::once(root));
            assert!(heap.dump_blackened);
            heap.assert_object_arenas_coherent();

            if mid_mark {
                heap.concurrent_begin();
                heap.seed_root(root);
                heap.launch_concurrent_mark();
                assert!(heap.concurrent_mark_running());
            }
            drop(heap);
        }
        assert_eq!(
            LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed),
            before,
            "symbol-with-pos pages leaked or double-freed at teardown",
        );
    }

    #[test]
    fn symbol_with_pos_pages_freed_at_heap_drop() {
        symbol_with_pos_pages_freed_at_heap_drop_body(false);
    }
    #[test]
    fn symbol_with_pos_pages_freed_at_heap_drop_mid_concurrent_mark() {
        symbol_with_pos_pages_freed_at_heap_drop_body(true);
    }

    /// Promotion-scan coverage: a tenured page symbol-with-pos whose `sym`
    /// holds a young cons keeps it alive across both parities (the promotion
    /// page walk covers the symbol-with-pos arena; `sym` is a traced child).
    fn tenured_page_symbol_with_pos_keeps_young_child_alive_body(verify: bool) {
        crate::test_utils::init_test_tracing();
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
        let b = heap.alloc_symbol_with_pos(y, TaggedValue::fixnum(1));
        let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert_eq!(
                unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
                Some(999),
                "tenured page symbol-with-pos's young sym child lost on cycle {cycle}",
            );
        }
        heap.assert_object_arenas_coherent();
    }

    #[test]
    fn tenured_page_symbol_with_pos_keeps_young_child_alive() {
        tenured_page_symbol_with_pos_keeps_young_child_alive_body(false);
    }
    #[test]
    fn tenured_page_symbol_with_pos_keeps_young_child_alive_verified() {
        tenured_page_symbol_with_pos_keeps_young_child_alive_body(true);
    }
}

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
