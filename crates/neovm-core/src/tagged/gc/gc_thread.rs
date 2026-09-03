//! The background GC thread: the marker's shared state, its message protocol with the mutator, the SATB buffer, and the concurrent mark loop it runs (CONCURRENT_GC.md).
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// A non-blocking concurrent-mark job (Phase 5). Carries everything the GC
/// thread needs WITHOUT a `&mut TaggedHeap` — two threads holding `&mut` to the
/// same heap is UB in Rust's model even with atomic fields. The GC thread marks
/// only conses (fixed 16B; car/cdr + mark bits are atomic) and DEFERS every
/// non-cons (and any non-owned cons) to `deferred`, traced at the stop-the-world
/// termination. So it touches no growable/reallocatable heap structure.
pub(super) struct ConcurrentMarkJob {
    /// Root snapshot, moved out of the heap's gray queue at the start handshake.
    pub(super) gray: Vec<TaggedValue>,
    /// Base addresses of every owned cons block at the snapshot (immutable,
    /// read-only on the GC thread). A cons whose block base is here is markable
    /// via block arithmetic; others (mapped/dump, or new blocks) are deferred.
    pub(super) owned_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT CLAIM DISPATCHER state (per-kind page-base snapshots,
    /// cycle parity, dump span, claim counters) for
    /// `concurrent_try_mark_owned`. Grouped in a sub-struct so the scan
    /// closures below — which mutably borrow `gray` — can borrow it
    /// disjointly. The cons arm reads the dump span from here too.
    pub(super) claims: ConcurrentClaimJob,
    /// Overwritten children appended by the mutator's SATB barrier; drained here.
    pub(super) satb: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// Non-cons / non-owned-cons values to trace at the STW termination.
    pub(super) deferred: std::sync::Arc<std::sync::Mutex<Vec<TaggedValue>>>,
    /// Set when gray + SATB are drained (tentatively done); polled by the mutator.
    pub(super) done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by the mutator to ask this loop to exit.
    pub(super) stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Task #7 stage 2a (Fix B): idle-nap wakeup latch. The mutator's
    /// `join_concurrent_mark` notifies it after setting `stop`, so the idle
    /// wait below wakes immediately instead of finishing a fixed sleep.
    pub(super) wake: std::sync::Arc<(std::sync::Mutex<()>, std::sync::Condvar)>,
    /// Signalled when the loop exits, so the mutator can take over the gray queue.
    pub(super) exited: std::sync::mpsc::Sender<()>,
    /// Stage 1b CONCURRENT OBARRAY SCAN: a start-captured snapshot of the obarray's
    /// chunked symbol store. When `Some`, the GC thread scans these symbol cells
    /// ONCE per cycle, feeding each symbol's heap children into `gray` (conses) /
    /// `deferred` (non-cons) like the gray-drain cons branch. Always `Some` for a
    /// concurrent mark — the start handshake captures it.
    pub(super) obarray: Option<crate::emacs_core::symbol::ObarrayScanSnapshot>,
    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: a start-captured snapshot of every
    /// OWNED/Mapped vector backing (base ptr + len). When `Some`, the GC thread
    /// traces these backings ONCE per cycle, feeding each slot's heap children into
    /// `gray` (conses) / `deferred` (non-cons) like the gray-drain cons branch, so
    /// vectors are marked concurrently instead of deferred to the STW termination.
    /// Always `Some` for a concurrent mark — the start handshake captures it.
    pub(super) vectors: Option<crate::tagged::header::VectorScanSnapshot>,
    /// FIRST PARTITION CYCLE: the mapped (pdump) cons ranges, staged by
    /// `begin_collection` and moved here by `launch_concurrent_mark`, as
    /// `(start_addr, len)` pairs. Scanned on the GC thread BEFORE the drain
    /// (same load-bearing order as the obarray/vector snapshots): the ranges
    /// address immutable process-lifetime mappings, the cons slots are the
    /// Phase-1 atomic slots (`load_car`/`load_cdr`), and racing mutator
    /// writes are covered by the SATB barrier exactly as for young conses.
    /// `None` for every later cycle (the image is black; young children come
    /// from the remembered set).
    pub(super) mapped_cons_ranges: Option<Vec<(usize, usize)>>,
    /// FIRST PARTITION CYCLE: mapped veclike header addresses, staged like
    /// the cons ranges. Scanned on the GC thread by
    /// [`concurrent_trace_mapped_veclike`]: every arm reads slots through
    /// the Phase-1 atomic loads (`iter_atomic`/`load_value_atomic` — the
    /// same accessors `trace_veclike` uses), mapped `LispValueVec` backings
    /// are retire-on-write (immutable in place), and any kind the
    /// GC-thread tracer does not port (hash tables: mutator-side weak
    /// registry + non-atomic map iteration) defers the OBJECT to the
    /// termination's full `mark_value`.
    pub(super) mapped_veclikes: Option<Vec<usize>>,
}

/// CONCURRENT CLAIM DISPATCHER (task 01) per-cycle state: everything
/// `concurrent_try_mark_owned` needs to classify + claim a discovered value
/// on the GC thread. All snapshots are captured at the world-stopped start
/// handshake (immutable, read-only on the GC thread — the live registries /
/// bitmaps belong to the mutator) and published through the same
/// `Arc`/channel happens-before as the cons `owned_bases`.
pub(super) struct ConcurrentClaimJob {
    /// THIS cycle's young non-cons mark parity, captured at launch. The GC
    /// thread's claims must mark to the CURRENT parity ("marked" ≡ bit
    /// == parity); the heap cannot flip mid-cycle (`begin_collection` is the
    /// only flip point and the next one cannot run before this mark joins),
    /// so the captured value is valid for the job's whole lifetime.
    pub(super) parity: bool,
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
    pub(super) string_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// CONCURRENT FLOAT CLAIMS (task 01): the base address of every FLOAT
    /// ARENA PAGE at the world-stopped start handshake (retired pages
    /// included — their tenured floats short-circuit to drop). Same
    /// discipline as `string_page_bases`: HIT ⇒ owned page float ⇒
    /// claim-eligible; MISS ⇒ DEFER (fail-safe for mid-cycle pages, mapped
    /// (pdump) floats — which mark via the heap's `mapped_float_ranges` side
    /// bitmaps only the mutator may touch — and any residual `Box` float).
    pub(super) float_page_bases: std::sync::Arc<FxHashSet<usize>>,
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
    pub(super) vector_page_bases: std::sync::Arc<FxHashSet<usize>>,
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
    pub(super) bytecode_page_bases: std::sync::Arc<FxHashSet<usize>>,
    /// Dump (pdump mmap) address span. The cons arm skips conses inside
    /// (permanent-black; young children come from the remembered set); the
    /// subr arm defers span-inside veclikes (every MAPPED veclike
    /// registration extends this span, so span-inside covers the whole
    /// mapped-veclike population whose marks live in mutator-only side
    /// tables).
    pub(super) dump_lo: usize,
    pub(super) dump_hi: usize,
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
    pub(super) drop_dump_children: bool,
    /// CONCURRENT STRING MARKING: count of owned interval-free strings this
    /// cycle's GC thread claimed via `concurrent_try_mark_string` (one per
    /// successful `mark_claim_at`, Relaxed — single writer). Read by
    /// `join_concurrent_mark` (after the exit handshake's happens-before) into
    /// the cycle stats; sizes how much string work left the STW drain.
    pub(super) str_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT FLOAT CLAIMS: same pattern as `str_claimed` — owned young
    /// page floats claimed this cycle (one per successful `mark_claim_at`).
    pub(super) float_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT VECTOR-HEADER CLAIMS: same pattern — owned young page
    /// vectors whose header this cycle's GC thread claimed.
    pub(super) vec_claimed: std::sync::Arc<AtomicUsize>,
    /// CONCURRENT BYTECODE CLAIMS: same pattern — owned young page bytecode
    /// this cycle's GC thread claimed (and gray-pushed the children of).
    pub(super) bc_claimed: std::sync::Arc<AtomicUsize>,
    /// SUBR RECOGNIZE-AND-DROP: how many times the GC thread dropped a
    /// leaked-static subr from the defer path this cycle. Counts drop
    /// EVENTS, not unique subrs (dropping is stateless, so a subr
    /// re-discovered through many edges counts once per edge) — a
    /// diagnostic for how much parked-buffer traffic the drop removes.
    pub(super) subr_dropped: std::sync::Arc<AtomicUsize>,
}

std::cfg_select! {
    target_family = "wasm" => {
        /// Browser Wasm has no ambient native-thread facility. Its stop-the-world
        /// collector drains on the editor Worker which already owns the heap.
        pub(super) fn run_stop_the_world_mark(heap: &mut TaggedHeap) {
            heap.mark_all();
        }

        pub(super) fn launch_background_mark(_job: ConcurrentMarkJob) {
            unreachable!("background marking is not compiled for browser Wasm")
        }
    }
    _ => {
        /// A raw `*mut TaggedHeap` that can cross to the GC thread. The heap is
        /// `!Send` (raw pointers), but during a stop-the-world handshake the
        /// mutator is blocked, so the GC thread owns the heap exclusively.
        struct HeapPtr(*mut TaggedHeap);
        unsafe impl Send for HeapPtr {}

        /// A unit of work handed to the GC thread, plus a oneshot done-channel
        /// the GC thread signals when finished so the mutator can resume.
        ///
        /// The variant sizes differ (the mark job carries the per-cycle claim
        /// snapshots), but exactly one request is in flight per GC cycle, so
        /// boxing the large variant would buy nothing.
        #[allow(clippy::large_enum_variant)]
        enum GcRequest {
            /// Drain the gray queue (mark to a fixpoint) on the GC thread.
            MarkAll(HeapPtr, std::sync::mpsc::Sender<()>),
            /// Non-blocking concurrent mark (Phase 5): mark conses while the
            /// mutator runs; defer everything else to the termination handshake.
            ConcurrentMark(ConcurrentMarkJob),
        }

        static GC_THREAD: std::sync::OnceLock<
            std::sync::Mutex<std::sync::mpsc::Sender<GcRequest>>,
        > = std::sync::OnceLock::new();

        /// Lazily spawn the process-global GC thread and return its request
        /// channel. The thread lives for the process; it loops draining requests.
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

        pub(super) fn run_stop_the_world_mark(heap: &mut TaggedHeap) {
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let ptr = heap as *mut TaggedHeap;
            gc_thread()
                .send(GcRequest::MarkAll(HeapPtr(ptr), done_tx))
                .expect("neovm-gc thread is gone");
            // Block until the GC thread has finished marking on the shared heap.
            done_rx.recv().expect("neovm-gc thread did not respond");
        }

        pub(super) fn launch_background_mark(job: ConcurrentMarkJob) {
            gc_thread()
                .send(GcRequest::ConcurrentMark(job))
                .expect("neovm-gc thread is gone");
        }
    }
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
pub(super) unsafe fn atomic_mark_owned_cons_ptr(ptr: *const ConsCell) -> bool {
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
pub(super) fn concurrent_try_mark_string(
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
pub(super) fn ewma_half(prev: u64, sample: u64) -> u64 {
    if prev == 0 {
        sample
    } else {
        (prev / 2).saturating_add(sample / 2)
    }
}

#[inline]
pub(super) fn concurrent_try_mark_owned(
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
pub(super) fn concurrent_trace_mapped_veclike(
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

pub(super) fn run_concurrent_mark(mut job: ConcurrentMarkJob) {
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
    pub(super) const STOP_CHECK_QUANTUM: usize = 512;
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
pub(super) fn note_heap_write_record(record: HeapWriteRecord) {
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
