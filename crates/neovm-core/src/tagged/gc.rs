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
