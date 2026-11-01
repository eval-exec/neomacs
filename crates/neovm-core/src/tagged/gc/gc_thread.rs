//! The background GC thread: the marker's shared state, its message protocol with the mutator, the SATB buffer, and the concurrent mark loop it runs (CONCURRENT_GC.md).
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::mapped_veclike_scan::MappedVeclikeScanItem;
use super::*;

// Rendezvous installed only on dedicated test worker threads. Production jobs
// carry no hook state.
#[cfg(test)]
thread_local! {
    static CONCURRENT_HASH_BEFORE_CLAIM_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    static CONCURRENT_HASH_SCAN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn concurrent_hash_before_claim_hook_for_test() {
    let hook = CONCURRENT_HASH_BEFORE_CLAIM_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(test)]
fn concurrent_hash_scan_hook_for_test() {
    let hook = CONCURRENT_HASH_SCAN_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

/// A raw `*mut TaggedHeap` that can cross to the GC thread. The heap is `!Send`
/// (raw pointers), but during a handshake the mutator is BLOCKED waiting for the
/// GC thread, so the two threads never touch the heap at the same time — the GC
/// thread has exclusive access for the duration. (Phase 5 makes access genuinely
/// concurrent via the atomic slots + SATB built in Phases 1-3.)
pub(super) struct HeapPtr(pub(super) *mut TaggedHeap);
unsafe impl Send for HeapPtr {}

/// A non-blocking concurrent-mark job (Phase 5). Carries everything the GC
/// thread needs WITHOUT a `&mut TaggedHeap` — two threads holding `&mut` to the
/// same heap is UB in Rust's model even with atomic fields. The GC thread reads
/// cons slots and claimable non-cons payloads through immutable ownership and
/// backing snapshots. Unsupported kinds go to `deferred` for termination.
/// It never reads a growable live registry or a mutable hash backing.
pub(super) struct ConcurrentMarkJob {
    /// Root snapshot, moved out of the heap's gray queue at the start handshake.
    pub(super) gray: MarkStack,
    /// CONCURRENT CLAIM DISPATCHER state (the start handshake's page
    /// snapshot, cycle parity, dump span, claim counters) for
    /// `concurrent_try_mark_owned`. Grouped in a sub-struct so the scan
    /// closures below — which mutably borrow `gray` — can borrow it
    /// disjointly. The cons arm reads the dump span and the cons blocks of
    /// the snapshot from here too: a cons in a snapshot block is markable via
    /// block arithmetic; others (mapped/dump, or new blocks) are deferred.
    pub(super) claims: ConcurrentClaimJob,
    /// Overwritten children appended by the mutator's SATB barrier; drained here.
    pub(super) satb: std::sync::Arc<SatbHandoff>,
    /// Set when gray + SATB are drained (tentatively done); polled by the mutator.
    pub(super) done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by the mutator to ask this loop to exit.
    pub(super) stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Task #7 stage 2a (Fix B): idle-nap wakeup latch. The mutator's
    /// `join_concurrent_mark` notifies it after setting `stop`, so the idle
    /// wait below wakes immediately instead of finishing a fixed sleep.
    pub(super) wake: std::sync::Arc<(std::sync::Mutex<()>, std::sync::Condvar)>,
    /// Signalled when the loop exits, so the mutator can take over the gray queue.
    pub(super) exited: std::sync::mpsc::Sender<ConcurrentMarkResult>,
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
    /// FIRST PARTITION CYCLE: stopped-start mapped veclike descriptors.
    /// [`concurrent_trace_mapped_veclike`] reads captured spans and fixed
    /// char-table slots with Acquire; it never rereads a live `LispValueVec`
    /// enum while the mutator promotes its mapped backing. Mappings remain
    /// immutable, owned vectors retire before replacement, and char-table
    /// backings keep fixed lengths. Owned records (which have no retirement
    /// hook) and unported kinds defer to termination's full `mark_value`.
    pub(super) mapped_veclikes: Option<MappedVeclikeScanSnapshot>,
}

/// CONCURRENT CLAIM DISPATCHER (task 01) per-cycle state: everything
/// `concurrent_try_mark_owned` needs to classify + claim a discovered value
/// on the GC thread. All snapshots are captured at the world-stopped start
/// handshake (immutable, read-only on the GC thread — the live registries /
/// bitmaps belong to the mutator) and published through the job's
/// channel happens-before.
pub(super) struct ConcurrentClaimJob {
    /// THIS cycle's young non-cons mark parity, captured at launch. The GC
    /// thread's claims must mark to the CURRENT parity ("marked" ≡ bit
    /// == parity); the heap cannot flip mid-cycle (`begin_collection` is the
    /// only flip point and the next one cannot run before this mark joins),
    /// so the captured value is valid for the job's whole lifetime.
    pub(super) parity: MarkParity,
    /// Enabled major, captured once at the stop-all start handshake.
    /// First-partition jobs also carry true; their non-cons promo addresses
    /// are discarded by the coordinator before legacy permanent promotion.
    pub(super) major: bool,
    /// OWNERSHIP SNAPSHOT (`chunk_map::PageSnapshot`): the cons blocks and
    /// claimable STRING, FLOAT, VECTOR and BYTECODE arena pages that existed
    /// at the world-stopped start handshake (retired pages included — their tenured
    /// objects short-circuit to drop at the arms). A HIT both proves
    /// ownership and classifies the value (a block or page owns its whole
    /// 64 KiB granule and is homogeneous), without reading any header. MISS ⇒
    /// DEFER, which is fail-safe for everything else: blocks and pages
    /// created mid-cycle (their objects are born at the cycle parity, or
    /// allocate-black), mapped (pdump) objects (marked via mutator-only side
    /// tables — claiming their `GcHeader` bit would skip the mapped mark and
    /// the termination's trace, a UAF of their children), and residual `Box`
    /// objects. Either the per-class base sets captured at the handshake or
    /// the chunk map plus the per-class counts then (`NEOVM_GC_CHUNK_MAP`).
    /// See the claim arms for why a page-hit vector may be claimed without
    /// deferring its CURRENT backing, and why a claimed bytecode's children
    /// are gray-pushed by the arm itself.
    pub(super) pages: PageSnapshot,
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

impl ConcurrentClaimJob {
    /// The captured collection scope. Generation is tested before mark
    /// parity in every header claim arm.
    #[inline(always)]
    pub(super) const fn scope(&self) -> CollectionScope {
        if self.major {
            CollectionScope::Full
        } else {
            CollectionScope::Young
        }
    }
}

/// Owned worker result, sent only after the last heap/snapshot read.
/// Headers remain live and non-moving until the coordinator joins the worker;
/// addresses are never dereferenced after the old/young sweep starts.
#[derive(Debug, Default)]
pub(super) struct ConcurrentMarkResult {
    /// Marker-owned work that the stopped mutators trace after receiving this result.
    /// Only this worker appends; the completion channel transfers ownership.
    pub(super) deferred: Vec<MarkWord>,
    pub(super) promo: Vec<usize>,
    pub(super) symbols: Vec<SymId>,
}

/// Enabled-only claim state, boxed together with the original job at launch.
/// The snapshot remains owned until the worker's last read and result handoff.
pub(super) struct EnabledClaims {
    pub(super) leaves: chunk_map::LeafPageSnapshot,
    pub(super) leaf_claimed: Option<std::sync::Arc<AtomicUsize>>,
    pub(super) hashes: Option<std::sync::Arc<concurrent_hash::HashTableScanSnapshot>>,
    pub(super) hash_claimed: Option<std::sync::Arc<AtomicUsize>>,
}

pub(super) struct EnabledConcurrentMarkJob {
    pub(super) job: ConcurrentMarkJob,
    pub(super) claims: EnabledClaims,
}

/// This static policy isolates the original OFF logs and child classifier.
trait WorkerSymbolMarks: Default {
    const ENABLED: bool;
    fn note(&mut self, value: TaggedValue, symbols: &mut Vec<SymId>) -> bool;
}

impl WorkerSymbolMarks for FxHashSet<SymId> {
    const ENABLED: bool = false;
    #[inline]
    fn note(&mut self, value: TaggedValue, symbols: &mut Vec<SymId>) -> bool {
        let crate::tagged::value::ValueKind::Symbol(id) = value.gc_kind() else {
            return false;
        };
        if self.insert(id) {
            symbols.push(id);
        }
        true
    }
}

impl WorkerSymbolMarks for SymbolMarkBits {
    const ENABLED: bool = true;
    #[inline(always)]
    fn note(&mut self, value: TaggedValue, symbols: &mut Vec<SymId>) -> bool {
        // Exclude identities, never mutable intern membership or namesakes.
        if !value.is_bare_symbol() || value.bits() <= TaggedValue::UNBOUND.bits() {
            return false;
        }
        let id = value.xbare_symbol_id();
        if self.insert_if_absent(id) {
            symbols.push(id);
        }
        true
    }
}

/// Job-local logs. No worker writes to collector or mutator symbol state.
#[derive(Default)]
struct WorkerMarkLogs<S: WorkerSymbolMarks = FxHashSet<SymId>> {
    result: ConcurrentMarkResult,
    seen_symbols: S,
}

type EnabledWorkerMarkLogs = WorkerMarkLogs<SymbolMarkBits>;

impl<S: WorkerSymbolMarks> WorkerMarkLogs<S> {
    #[inline]
    fn note_symbol(&mut self, value: TaggedValue) -> bool {
        self.seen_symbols.note(value, &mut self.result.symbols)
    }

    /// Generation bytes do not change until this worker has exited.
    #[inline]
    fn note_promotion<const MAJOR: bool>(&mut self, header: *const GcHeader) {
        if MAJOR && !unsafe { (*header).tenured } {
            self.result.promo.push(header as usize);
        }
    }

    #[inline(always)]
    fn queue_child<const SYMBOLS: bool>(&mut self, value: TaggedValue, gray: &mut MarkStack) {
        if S::ENABLED {
            if value.is_heap_object() {
                gray.push(value);
            } else if SYMBOLS {
                self.note_symbol(value);
            }
        } else {
            // The original OFF major classifier and queue order.
            if SYMBOLS && self.note_symbol(value) {
                return;
            }
            if value.is_heap_object() {
                gray.push(value);
            }
        }
    }

    #[inline]
    fn queue_snapshot_child(&mut self, value: TaggedValue, gray: &mut MarkStack) {
        self.queue_child::<true>(value, gray);
    }
}

trait ClaimExtension {
    type Symbols: WorkerSymbolMarks;
    fn try_mark<const MAJOR: bool>(
        &self,
        ptr: *const VecLikeHeader,
        job: &ConcurrentClaimJob,
        gray: &mut MarkStack,
        logs: &mut WorkerMarkLogs<Self::Symbols>,
    ) -> Option<bool>;
}

struct LegacyClaims;

impl ClaimExtension for LegacyClaims {
    type Symbols = FxHashSet<SymId>;
    #[inline(always)]
    fn try_mark<const MAJOR: bool>(
        &self,
        _ptr: *const VecLikeHeader,
        _job: &ConcurrentClaimJob,
        _gray: &mut MarkStack,
        _logs: &mut WorkerMarkLogs<Self::Symbols>,
    ) -> Option<bool> {
        None
    }
}

/// A unit of work handed to the GC thread, plus a oneshot done-channel the GC
/// thread signals when finished so the mutator can resume.
///
/// The variant sizes differ (the mark job carries the per-cycle claim
/// snapshots). The original concurrent job stays inline and unchanged; the
/// enabled extension is boxed so it cannot widen the OFF channel payload.
#[allow(clippy::large_enum_variant)]
pub(super) enum GcRequest {
    /// Drain the gray queue (mark to a fixpoint) on the GC thread.
    MarkAll(HeapPtr, std::sync::mpsc::Sender<()>),
    /// Non-blocking concurrent mark (Phase 5): mark conses while the mutator
    /// runs; defer everything else to the termination handshake.
    ConcurrentMark(ConcurrentMarkJob),
    /// Extended ownership and symbol tracing are allocated only when enabled.
    ConcurrentMarkEnabled(Box<EnabledConcurrentMarkJob>),
}

// Ordinary-build worker sizes after moving the shared deferred Arc out of
// the job and adding its owned Vec to the result. The enabled extension stays
// boxed, so it cannot widen the legacy channel payload or its niche choice.
#[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
const _: () = {
    assert!(size_of::<PageSnapshot>() == 160);
    assert!(size_of::<ConcurrentClaimJob>() == 224);
    assert!(size_of::<ConcurrentMarkJob>() == 416);
    assert!(size_of::<GcRequest>() == 416);
    assert!(size_of::<WorkerMarkLogs>() == 104);
};

/// The name of every collector thread. Thread names need not be unique: a
/// process with one heap looks as it did with the single process collector,
/// and measurement tools keep matching it. The heap identity lives in the
/// registry and in the `neovm::gc` trace events.
pub(crate) const COLLECTOR_THREAD_NAME: &str = "neovm-gc";

/// One heap's collector: the request channel of its thread and the thread
/// itself. Each collector serves one heap in FIFO order, so a concurrent job
/// another heap leaves running cannot hold up this heap's requests (two
/// Worlds on one OS thread used to deadlock on the single process
/// collector).
struct Collector {
    requests: std::sync::mpsc::Sender<GcRequest>,
    thread: std::thread::JoinHandle<()>,
}

impl Collector {
    /// Spawn `heap`'s collector. Like the process collector it replaces, it
    /// is spawned by the heap's first GC request, so it inherits that
    /// mutator's CPU affinity mask and default thread settings.
    #[cold]
    #[inline(never)]
    fn spawn(heap: HeapIdentity) -> CollectorStart {
        #[cfg(test)]
        if let Some(error) = collector_test_support::injected_start_failure(heap) {
            return CollectorStart::Unavailable(error);
        }
        let (requests, rx) = std::sync::mpsc::channel::<GcRequest>();
        let thread = match std::thread::Builder::new()
            .name(COLLECTOR_THREAD_NAME.to_owned())
            .spawn(move || serve_gc_requests(rx))
        {
            Ok(thread) => thread,
            Err(error) => return CollectorStart::Unavailable(error),
        };
        tracing::debug!(target: "neovm::gc", heap = heap.get(), "collector spawned");
        CollectorStart::Thread(Self { requests, thread })
    }
}

/// Resource refusal is recoverable; a collector that started and then died is
/// still an invariant failure handled by the request send below.
enum CollectorStart {
    Thread(Collector),
    Unavailable(std::io::Error),
}

enum CollectorHandle {
    Thread(Collector),
    Inline,
}

enum StartAttempt {
    First,
    Retry,
}

/// Process-wide diagnostic count, not a heap field or a TLS word. Incremented
/// once per refused start, including retries; logging occurs only on transition
/// from an absent handle to Inline.
static COLLECTOR_START_FAILURES: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn collector_start_failures() -> u64 {
    COLLECTOR_START_FAILURES.load(std::sync::atomic::Ordering::Relaxed)
}

impl CollectorHandle {
    #[cold]
    fn start(heap: HeapIdentity, attempt: StartAttempt) -> Self {
        match Collector::spawn(heap) {
            CollectorStart::Thread(collector) => Self::Thread(collector),
            CollectorStart::Unavailable(error) => {
                COLLECTOR_START_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match attempt {
                    StartAttempt::First => {
                        tracing::warn!(target: "neovm::gc", heap = heap.get(), %error,
                            "collector thread unavailable; collecting on the requester");
                    }
                    StartAttempt::Retry => {}
                }
                Self::Inline
            }
        }
    }
}

/// The collector's loop: drain requests until the heap retires (its sender
/// is dropped).
fn serve_gc_requests(rx: std::sync::mpsc::Receiver<GcRequest>) {
    while let Ok(req) = rx.recv() {
        run_gc_request(req);
    }
}

/// One implementation of the request protocol for both possible executors.
fn run_gc_request(req: GcRequest) {
    match req {
        GcRequest::MarkAll(HeapPtr(p), done) => {
            // SAFETY: the requester either executes this directly with
            // its exclusive heap loan or blocks on done until the sole
            // worker signals. No other heap access overlaps marking.
            unsafe { (*p).mark_all() };
            let _ = done.send(());
        }
        GcRequest::ConcurrentMark(job) => {
            run_concurrent_mark(job);
        }
        GcRequest::ConcurrentMarkEnabled(job) => {
            run_concurrent_mark_enabled(*job);
        }
    }
}

/// The requester is the cycle's sole marker when no worker can start. Request
/// stop before entry: mandatory scans still run, residual gray is handed off,
/// and the normal loop cannot wait for a stop that only this requester can set.
/// WorkerMarkLogs and snapshots are job-owned; marking needs no collector TLS
/// or dedicated-thread identity. Keep the active SATB window until normal join.
#[cold]
#[inline(never)]
fn run_gc_request_inline(request: GcRequest) {
    let done = match &request {
        GcRequest::MarkAll(_, _) => None,
        GcRequest::ConcurrentMark(job) => Some(prepare_inline_mark(job)),
        GcRequest::ConcurrentMarkEnabled(job) => Some(prepare_inline_mark(&job.job)),
    };
    run_gc_request(request);
    if let Some(done) = done {
        // A quantum-stop bypasses the worker's drained store. Its owned result
        // is now queued, so safe-point polling must allow termination to join
        // and trace residual work rather than wait for an absent worker.
        done.store(true, std::sync::atomic::Ordering::Release);
    }
}

fn prepare_inline_mark(job: &ConcurrentMarkJob) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    job.stop.store(true, std::sync::atomic::Ordering::Release);
    job.done.clone()
}

/// The live collectors, keyed by the heap each serves. Cold: touched once per
/// GC request and at heap teardown, never by allocation. A side table rather
/// than a `TaggedHeap` field, so the allocation fast paths keep their field
/// displacements.
static COLLECTORS: std::sync::Mutex<FxHashMap<HeapIdentity, CollectorHandle>> =
    std::sync::Mutex::new(FxHashMap::with_hasher(rustc_hash::FxBuildHasher));

/// Lock the cold handle registry. A refused start leaves an Inline entry;
/// a poisoned lock remains usable for heap teardown.
fn collectors() -> std::sync::MutexGuard<'static, FxHashMap<HeapIdentity, CollectorHandle>> {
    COLLECTORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Hand `request` to `heap`'s collector, spawning the collector on the heap's
/// first request and retrying each later request while its handle is Inline.
pub(super) fn send_to_collector(heap: HeapIdentity, request: GcRequest) {
    {
        use std::collections::hash_map::Entry;
        let mut registry = collectors();
        let handle = match registry.entry(heap) {
            Entry::Vacant(entry) => entry.insert(CollectorHandle::start(heap, StartAttempt::First)),
            Entry::Occupied(entry) => {
                let handle = entry.into_mut();
                if let CollectorHandle::Inline = handle {
                    *handle = CollectorHandle::start(heap, StartAttempt::Retry);
                }
                handle
            }
        };
        match handle {
            CollectorHandle::Thread(collector) => {
                collector.requests.send(request).unwrap_or_else(|_| {
                    panic!("the neovm-gc thread of heap {} is gone", heap.get())
                });
                return;
            }
            CollectorHandle::Inline => {}
        }
    }
    // No registry lock spans heap marking, callbacks, or completion sends.
    run_gc_request_inline(request);
}

/// Remove `heap`'s collector at heap teardown and close its request channel:
/// the thread exits once its current job returns. `None` when the heap never
/// sent a GC request or only used Inline marking.
pub(super) fn retire_collector(heap: HeapIdentity) -> Option<RetiredCollector> {
    let Collector { requests, thread } = match collectors().remove(&heap)? {
        CollectorHandle::Thread(collector) => collector,
        CollectorHandle::Inline => return None,
    };
    drop(requests);
    Some(RetiredCollector { heap, thread })
}

/// Whether `heap` has a live collector.
#[cfg(test)]
pub(crate) fn collector_is_registered(heap: HeapIdentity) -> bool {
    matches!(collectors().get(&heap), Some(CollectorHandle::Thread(_)))
}

/// A collector whose heap retired: `join` it, or `detach` it when the heap
/// abandoned a running concurrent job.
#[must_use = "join or detach the retired collector"]
pub(super) struct RetiredCollector {
    heap: HeapIdentity,
    thread: std::thread::JoinHandle<()>,
}

impl RetiredCollector {
    /// Wait for the thread to exit. With no job in flight it is parked in
    /// `recv`, which fails at once now that the channel is closed. A panic
    /// on the collector already failed its heap's request; teardown only
    /// logs it.
    pub(super) fn join(self) {
        if self.thread.join().is_err() {
            tracing::error!(target: "neovm::gc", heap = self.heap.get(), "collector panicked");
        } else {
            tracing::debug!(target: "neovm::gc", heap = self.heap.get(), "collector joined");
        }
    }

    /// Let the thread finish on its own, for a heap that abandoned a running
    /// concurrent job (Drop never waits for a marker). The job exits once it
    /// sees `stop`; then `recv` fails on the closed channel and the thread
    /// ends. What the job may still read is retained by the abandoning heap
    /// (`TaggedHeap::drop`).
    pub(super) fn detach(self) {
        tracing::debug!(target: "neovm::gc", heap = self.heap.get(), "collector detached");
        drop(self.thread);
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
    unsafe { ConsBlockTrailer::from_block_base(base) }.try_mark(index)
}

/// CONCURRENT STRING MARKING: try to mark one discovered string on the GC
/// thread. Returns `true` when fully handled here (claimed now, or already
/// marked); `false` means the caller must park the value in `deferred` for the
/// STW termination exactly as before. Called at all three discovery sinks
/// (gray drain, obarray scan, vector-backing scan).
///
/// OWNERSHIP — THE START-HANDSHAKE PAGE SNAPSHOT (stage 3; replaces
/// float-v1's dump-span test): all owned strings live in STRING ARENA
/// PAGES, and `pages` knows every string page that existed at the
/// world-stopped launch (`PageSnapshot`, published with the job).
/// Snapshot-hit ⇒ this is an owned page string (a 64KB page
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
    pages: &PageSnapshot,
    parity: MarkParity,
    str_claimed: &AtomicUsize,
) -> bool {
    debug_assert!(val.is_string());
    let Some(ptr) = val.gc_string_ptr() else {
        return false; // malformed value — let the termination's mark_value decide
    };
    if !pages.contains(ChunkClass::String, ptr as usize) {
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

/// Enabled-major string claim: permanent generation first, interval refusal
/// before the claim, and P-all logging only after a successful young claim.
#[inline]
fn concurrent_try_mark_string_major<S: WorkerSymbolMarks>(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    logs: &mut WorkerMarkLogs<S>,
) -> bool {
    debug_assert!(val.is_string());
    let Some(ptr) = val.gc_string_ptr() else {
        return false;
    };
    if !job.pages.contains(ChunkClass::String, ptr as usize) {
        return false;
    }
    if unsafe { (*ptr).header.black_by_generation(job.scope()) } {
        return true;
    }
    // Read only the atomic pointer word, never the interval table.
    if !unsafe { (*ptr).data.intervals_ptr() }.is_null() {
        return false;
    }
    if unsafe { (*ptr).header.mark_claim_at(job.parity) } {
        job.str_claimed.fetch_add(1, Ordering::Relaxed);
        logs.note_promotion::<true>(ptr.cast());
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

// Existing direct claim tests keep their legacy three-argument surface.
#[cfg(test)]
#[inline]
pub(super) fn concurrent_try_mark_owned(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    gray: &mut Vec<TaggedValue>,
) -> bool {
    debug_assert!(!job.major);
    let mut stack = MarkStack::default();
    let handled = concurrent_try_mark_owned_logged::<false>(
        val,
        job,
        &mut stack,
        &mut WorkerMarkLogs::default(),
    );
    gray.extend(stack.into_values());
    handled
}

#[cfg(test)]
#[inline]
fn concurrent_try_mark_owned_logged<const MAJOR: bool>(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    gray: &mut MarkStack,
    logs: &mut WorkerMarkLogs,
) -> bool {
    concurrent_try_mark_owned_with_symbols::<MAJOR, MAJOR, LegacyClaims>(
        val,
        job,
        &LegacyClaims,
        gray,
        logs,
    )
}

#[cfg(test)]
#[inline]
fn concurrent_try_mark_owned_enabled<const MAJOR: bool>(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    claims: &EnabledClaims,
    gray: &mut Vec<TaggedValue>,
    logs: &mut EnabledWorkerMarkLogs,
) -> bool {
    let mut stack = MarkStack::default();
    let handled = concurrent_try_mark_owned_with_symbols::<MAJOR, true, EnabledClaims>(
        val, job, claims, &mut stack, logs,
    );
    gray.extend(stack.into_values());
    handled
}

#[inline]
fn concurrent_try_mark_owned_with_symbols<
    const MAJOR: bool,
    const SYMBOLS: bool,
    E: ClaimExtension,
>(
    val: TaggedValue,
    job: &ConcurrentClaimJob,
    extension: &E,
    gray: &mut MarkStack,
    logs: &mut WorkerMarkLogs<E::Symbols>,
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
        if MAJOR {
            return concurrent_try_mark_string_major(val, job, logs);
        }
        return concurrent_try_mark_string(val, &job.pages, job.parity, &job.str_claimed);
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
        if !job.pages.contains(ChunkClass::Float, ptr as usize) {
            // Snapshot MISS: mid-cycle page (born-at-parity anyway), mapped
            // (pdump) float (marks via the mutator-only side ranges), or a
            // residual Box float — DEFER, fail-safe.
            return false;
        }
        // TENURED short-circuit BEFORE the claim (H5): tenured ≡ permanently
        // black, never re-traced/re-swept — "handled, nothing owed" without
        // touching the frozen mark bit.
        if unsafe {
            (*ptr).header.black_by_generation(if MAJOR {
                CollectionScope::Full
            } else {
                CollectionScope::Young
            })
        } {
            return true;
        }
        // Young owned page float: claim at THIS cycle's parity. A failed
        // claim means it is already black (allocate-black or an earlier edge
        // this cycle) — equally done.
        if unsafe { (*ptr).header.mark_claim_at(job.parity) } {
            job.float_claimed.fetch_add(1, Ordering::Relaxed);
            logs.note_promotion::<MAJOR>(ptr.cast());
        }
        return true;
    }
    if val.is_veclike() {
        let Some(ptr) = val.gc_veclike_ptr() else {
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
        if job.pages.contains(ChunkClass::Vector, addr) {
            // Page vectors are 64-byte slots; a page-hit veclike value must
            // decode to a slot boundary (page-homogeneity argument above).
            debug_assert_eq!(
                (addr - base) % <VectorObj as PagedObject>::SLOT_BYTES,
                0,
                "page-hit veclike value does not address a vector slot",
            );
            // TENURED short-circuit BEFORE the claim (H5): permanently
            // black, never re-traced; frozen at the world-stopped promotion.
            if unsafe {
                (*ptr).gc.black_by_generation(if MAJOR {
                    CollectionScope::Full
                } else {
                    CollectionScope::Young
                })
            } {
                return true;
            }
            if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                job.vec_claimed.fetch_add(1, Ordering::Relaxed);
                logs.note_promotion::<MAJOR>(ptr.cast());
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
        //      `trace_veclike`'s ByteCode arm traces (the two `aref` slot
        //      objects, arglist, constants, env, doc_form, interactive,
        //      extra_slots and the Dynamic parameter child; Stack/Named
        //      parameters carry no heap children), and the drain traces them
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
        if job.pages.contains(ChunkClass::ByteCode, addr) {
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
            if unsafe {
                (*ptr).gc.black_by_generation(if MAJOR {
                    CollectionScope::Full
                } else {
                    CollectionScope::Young
                })
            } {
                return true;
            }
            if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                job.bc_claimed.fetch_add(1, Ordering::Relaxed);
                logs.note_promotion::<MAJOR>(ptr.cast());
                // Fresh claim: gray-push the children (coverage leg (a)).
                // Field reads are race-free per the immutability argument
                // above (a fresh claim proves the object pre-dates the
                // cycle, so construction completed before the snapshot).
                // The `aref` slot objects: the one part of a published
                // bytecode that is written after publication (at most once
                // per word, from nil, by `install_bytecode_slot_object`),
                // hence atomics read with Acquire. An object installed after
                // the snapshot was allocated this cycle (born black) or is a
                // prototype's code string reachable through the prototype;
                // pushing it is harmless either way.
                for child in unsafe { (*(ptr as *const ByteCodeObj)).slot_objects.children() } {
                    logs.queue_child::<SYMBOLS>(child, gray);
                }
                let data = unsafe { &(*(ptr as *const ByteCodeObj)).data };
                // Lazy pdump stubs are confined to the MAPPED image (the
                // arena/descriptor load fallback stays eager): this arm
                // reads fields with plain loads on the GC thread, which a
                // mid-materialize ~350-byte overwrite would tear.
                debug_assert!(
                    !data.is_pdump_stub(),
                    "arena bytecode must never be a lazy pdump stub"
                );
                logs.queue_child::<SYMBOLS>(data.arglist, gray);
                if let Some(child) = data.params.heap_child() {
                    logs.queue_child::<SYMBOLS>(child, gray);
                }
                for &c in &data.constants {
                    logs.queue_child::<SYMBOLS>(c, gray);
                }
                if let Some(env) = data.env {
                    logs.queue_child::<SYMBOLS>(env, gray);
                }
                if let Some(doc_form) = data.doc_form {
                    logs.queue_child::<SYMBOLS>(doc_form, gray);
                }
                if let Some(interactive) = data.interactive {
                    logs.queue_child::<SYMBOLS>(interactive, gray);
                }
                for &s in &data.extra_slots {
                    logs.queue_child::<SYMBOLS>(s, gray);
                }
            }
            // Already marked (lost race, earlier edge, or born-at-parity —
            // coverage leg (c)): equally handled, nothing further owed.
            return true;
        }
        if let Some(handled) = extension.try_mark::<MAJOR>(ptr, job, gray, logs) {
            return handled;
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

impl ClaimExtension for EnabledClaims {
    type Symbols = SymbolMarkBits;

    #[inline]
    fn try_mark<const MAJOR: bool>(
        &self,
        ptr: *const VecLikeHeader,
        job: &ConcurrentClaimJob,
        gray: &mut MarkStack,
        logs: &mut EnabledWorkerMarkLogs,
    ) -> Option<bool> {
        let addr = ptr as usize;
        let base = addr & !(OBJECT_PAGE_ALIGN - 1);
        if let Some(hashes) = &self.hashes {
            // Exact snapshot-map ownership before any Box header read.
            // Ineligible weak/pending, mapped and post-start owners miss this
            // map and remain on the ordinary mutator-side defer path.
            if let Some(entry) = hashes.get(addr) {
                if unsafe { (*ptr).gc.black_by_generation(job.scope()) } {
                    return Some(true);
                }
                #[cfg(test)]
                concurrent_hash_before_claim_hook_for_test();
                // Reader admission precedes the header claim. A first writer
                // may defer an unread table, but never an already claimed one.
                let Some(mut reader) = hashes.lock_reader(entry) else {
                    return Some(!entry.is_deferred());
                };
                if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                    self.hash_claimed
                        .as_ref()
                        .expect("Tier-H snapshot requires a cycle counter")
                        .fetch_add(1, Ordering::Relaxed);
                    logs.note_promotion::<MAJOR>(ptr.cast());
                    // Complete this snapshot scan even if stop arrives now:
                    // a claimed header cannot be sent back for retracing.
                    #[cfg(test)]
                    let mut first_word = true;
                    unsafe {
                        reader.scan(|child| {
                            logs.queue_snapshot_child(child, gray);
                            #[cfg(test)]
                            if first_word {
                                first_word = false;
                                concurrent_hash_scan_hook_for_test();
                            }
                        });
                    }
                }
                reader.finish();
                return Some(true);
            }
        }
        {
            // A class page hit is both the ownership and subtype proof.
            // Never inspect a live allocator bitmap or a payload on a miss.
            if let Some(class) = self.leaves.leaf_class(&job.pages, addr) {
                let stride = match class {
                    ChunkClass::Marker => <MarkerObj as PagedObject>::SLOT_BYTES,
                    ChunkClass::Bignum => <BignumObj as PagedObject>::SLOT_BYTES,
                    ChunkClass::SymbolWithPos => <SymbolWithPosObj as PagedObject>::SLOT_BYTES,
                    _ => unreachable!("leaf snapshot returned a non-leaf class"),
                };
                debug_assert_eq!((addr - base) % stride, 0, "misaligned leaf page value");
                if unsafe { (*ptr).gc.black_by_generation(job.scope()) } {
                    return Some(true);
                }
                if unsafe { (*ptr).gc.mark_claim_at(job.parity) } {
                    self.leaf_claimed
                        .as_ref()
                        .expect("enabled leaf claims require a cycle counter")
                        .fetch_add(1, Ordering::Relaxed);
                    logs.note_promotion::<MAJOR>(ptr.cast());
                    if class == ChunkClass::SymbolWithPos {
                        // These fields have no post-publication setter. A
                        // fresh claim excludes births at this cycle parity:
                        // construction happened before the start handshake
                        // and channel publication. Failed claims never read
                        // payloads, including reused slots born black.
                        let object = ptr as *const SymbolWithPosObj;
                        logs.queue_snapshot_child(unsafe { (*object).sym }, gray);
                        logs.queue_snapshot_child(unsafe { (*object).pos }, gray);
                    }
                    // Marker chains and bignum limb buffers are mutable host
                    // state, with no Lisp children: do not read either here.
                }
                return Some(true);
            }
        }
        None
    }
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
fn concurrent_trace_mapped_veclike<const MAJOR: bool, const SYMBOLS: bool, E: ClaimExtension>(
    snapshot: &MappedVeclikeScanSnapshot,
    job: &mut ConcurrentMarkJob,
    extension: &E,
    seen_symbols: &mut FxHashSet<usize>,
    logs: &mut WorkerMarkLogs<E::Symbols>,
) {
    let route = |child: TaggedValue,
                 job: &mut ConcurrentMarkJob,
                 seen_symbols: &mut FxHashSet<usize>,
                 logs: &mut WorkerMarkLogs<E::Symbols>| {
        if child.is_cons() {
            let addr = child.gc_cons_ptr() as usize;
            if addr < job.claims.dump_lo || addr >= job.claims.dump_hi {
                job.gray.push(child);
            }
        } else if child.is_bare_symbol() {
            if SYMBOLS {
                logs.note_symbol(child);
            } else if seen_symbols.insert(child.bits()) {
                logs.result.deferred.push(MarkWord::of(child));
            }
        } else if child.is_heap_object()
            && !concurrent_try_mark_owned_with_symbols::<MAJOR, SYMBOLS, E>(
                child,
                &job.claims,
                extension,
                &mut job.gray,
                logs,
            )
        {
            logs.result.deferred.push(MarkWord::of(child));
        }
    };
    // SAFETY: stopped-start capture admitted these same-heap initialized
    // spans. The owning job retains the image and every required backing
    // until this worker joins or the heap conservatively abandons storage.
    // Concurrent slot writes follow atomic pointee publication; no mutable
    // storage enum or growable live Vec header is borrowed here.
    unsafe {
        snapshot.scan(|item| match item {
            MappedVeclikeScanItem::Child(value) => route(value, job, seen_symbols, logs),
            MappedVeclikeScanItem::Deferred(owner) => {
                // Direct push bypasses the dispatcher, so its first-cycle
                // dump-child shortcut cannot discard termination-only owners.
                logs.result.deferred.push(owner);
            }
        })
    };
}

pub(super) fn run_concurrent_mark(job: ConcurrentMarkJob) {
    if job.claims.major {
        run_concurrent_mark_impl::<true, true, LegacyClaims>(job, LegacyClaims);
    } else {
        run_concurrent_mark_impl::<false, false, LegacyClaims>(job, LegacyClaims);
    }
}

pub(super) fn run_concurrent_mark_enabled(job: EnabledConcurrentMarkJob) {
    // ON requires transitive bare-symbol handoff even in GEN0. This election
    // happens once, outside every classification and child-routing loop.
    if job.job.claims.major {
        run_concurrent_mark_impl::<true, true, EnabledClaims>(job.job, job.claims);
    } else {
        run_concurrent_mark_impl::<false, true, EnabledClaims>(job.job, job.claims);
    }
}

#[inline(always)]
fn route_snapshot_child<const MAJOR: bool, const SYMBOLS: bool, E: ClaimExtension>(
    child: TaggedValue,
    job: &mut ConcurrentMarkJob,
    extension: &E,
    logs: &mut WorkerMarkLogs<E::Symbols>,
) {
    if <E::Symbols as WorkerSymbolMarks>::ENABLED {
        if child.is_cons() {
            job.gray.push(child);
        } else if child.is_heap_object() {
            if !concurrent_try_mark_owned_with_symbols::<MAJOR, SYMBOLS, E>(
                child,
                &job.claims,
                extension,
                &mut job.gray,
                logs,
            ) {
                logs.result.deferred.push(MarkWord::of(child));
            }
        } else if SYMBOLS {
            logs.note_symbol(child);
        }
    } else {
        // The original OFF snapshot route, including its immediate deferral.
        if SYMBOLS && logs.note_symbol(child) {
            return;
        }
        if child.is_cons() {
            job.gray.push(child);
        } else if !concurrent_try_mark_owned_with_symbols::<MAJOR, SYMBOLS, E>(
            child,
            &job.claims,
            extension,
            &mut job.gray,
            logs,
        ) {
            logs.result.deferred.push(MarkWord::of(child));
        }
    }
}

fn run_concurrent_mark_impl<const MAJOR: bool, const SYMBOLS: bool, E: ClaimExtension>(
    mut job: ConcurrentMarkJob,
    extension: E,
) {
    let mut logs = WorkerMarkLogs::<E::Symbols>::default();
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
        // SAFETY: this cycle's sole marker (worker or inline requester) owns
        // the scan; `snap` was captured at its world-stopped start
        // handshake; its chunk + seq pointers address the live, non-moving
        // obarray storage, retained through this job's completion.
        unsafe {
            if SYMBOLS {
                // This API only widens the child filter to include Symbols;
                // promotion still uses the independent MAJOR policy.
                snap.scan_for_major(|child| {
                    route_snapshot_child::<MAJOR, SYMBOLS, E>(
                        child, &mut job, &extension, &mut logs,
                    )
                });
            } else {
                snap.scan(|child| {
                    route_snapshot_child::<MAJOR, SYMBOLS, E>(
                        child, &mut job, &extension, &mut logs,
                    )
                });
            }
        }
    }
    // Stage 2 Tier B CONCURRENT VECTOR SCAN: trace the snapshotted vector
    // backings ONCE per cycle, routing children exactly as above.
    if let Some(snap) = job.vectors.take() {
        // SAFETY: this cycle's sole marker (worker or inline requester) owns
        // the scan; `snap` was captured at its world-stopped start
        // handshake; each entry's base/len addresses a live, immutable backing
        // (Mapped dump or retired-on-write Owned buffer) retained until join.
        unsafe {
            if SYMBOLS {
                snap.scan_for_major(|child| {
                    route_snapshot_child::<MAJOR, SYMBOLS, E>(
                        child, &mut job, &extension, &mut logs,
                    )
                });
            } else {
                snap.scan(|child| {
                    route_snapshot_child::<MAJOR, SYMBOLS, E>(
                        child, &mut job, &extension, &mut logs,
                    )
                });
            }
        }
    }
    // FIRST PARTITION CYCLE: scan stopped-start mapped backing descriptors
    // (see the job-field safety envelope; unsafe mutable backings defer).
    if let Some(snapshot) = job.mapped_veclikes.take() {
        let mut seen_symbols: FxHashSet<usize> = FxHashSet::default();
        concurrent_trace_mapped_veclike::<MAJOR, SYMBOLS, E>(
            &snapshot,
            &mut job,
            &extension,
            &mut seen_symbols,
            &mut logs,
        );
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
                        let addr = child.gc_cons_ptr() as usize;
                        if addr < job.claims.dump_lo || addr >= job.claims.dump_hi {
                            job.gray.push(child);
                        }
                    } else if child.is_bare_symbol() {
                        // Deduped symbol hand-off: `mark_symbol` is
                        // mutator-only, and an uninterned dumped symbol is
                        // reachable only through image data, so each UNIQUE
                        // symbol must reach the termination exactly once.
                        if SYMBOLS {
                            logs.note_symbol(child);
                        } else if seen_symbols.insert(child.bits()) {
                            logs.result.deferred.push(MarkWord::of(child));
                        }
                    } else if child.is_heap_object() {
                        // Same filter as `mark_or_push_child`: immediates
                        // (fixnums, chars) carry nothing to mark — routing
                        // them into `deferred` flooded the first termination
                        // with ~118K no-op entries.
                        if !concurrent_try_mark_owned_with_symbols::<MAJOR, SYMBOLS, E>(
                            child,
                            &job.claims,
                            &extension,
                            &mut job.gray,
                            &mut logs,
                        ) {
                            logs.result.deferred.push(MarkWord::of(child));
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
            if !<E::Symbols as WorkerSymbolMarks>::ENABLED && SYMBOLS && logs.note_symbol(val) {
                continue;
            }
            if val.is_cons() {
                let ptr = val.gc_cons_ptr();
                let addr = ptr as usize;
                if addr >= job.claims.dump_lo && addr < job.claims.dump_hi {
                    continue; // dump cons: permanent black, children via remembered set
                }
                if !job.claims.pages.contains(ChunkClass::Cons, addr) {
                    // Mapped (non-dump) or new-block cons — let the mutator's
                    // termination mark it through the full `mark_value` path.
                    logs.result.deferred.push(MarkWord::of(val));
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
                        logs.queue_child::<SYMBOLS>(car, &mut job.gray);
                        if !cdr.is_cons() {
                            logs.queue_child::<SYMBOLS>(cdr, &mut job.gray);
                            break;
                        }
                        since_stop_check += 1;
                        if since_stop_check >= STOP_CHECK_QUANTUM {
                            job.gray.push(cdr);
                            break;
                        }
                        let cptr = cdr.gc_cons_ptr();
                        let caddr = cptr as usize;
                        if caddr >= job.claims.dump_lo && caddr < job.claims.dump_hi {
                            break; // dump cons: permanent black
                        }
                        if !job.claims.pages.contains(ChunkClass::Cons, caddr) {
                            logs.result.deferred.push(MarkWord::of(cdr));
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
                if concurrent_try_mark_owned_with_symbols::<MAJOR, SYMBOLS, E>(
                    val,
                    &job.claims,
                    &extension,
                    &mut job.gray,
                    &mut logs,
                ) {
                    continue;
                }
                logs.result.deferred.push(MarkWord::of(val));
            } else if <E::Symbols as WorkerSymbolMarks>::ENABLED && SYMBOLS {
                logs.note_symbol(val);
            }
        }
        // Fold the mutator's SATB log (overwritten children) into gray.
        let batch = job.satb.take_all().unwrap();
        if batch.is_empty() {
            // Tentatively drained. Advertise done; exit if the mutator asked.
            // Only this worker writes done during a job. A repeated idle pass
            // can read it without taking ownership of the mutator-polled line.
            if !job.done.load(Ordering::Relaxed) {
                job.done.store(true, Ordering::Release);
            }
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
            if job.done.load(Ordering::Relaxed) {
                job.done.store(false, Ordering::Release);
            }
            for chunk in batch {
                job.gray.extend_words(chunk.into_words());
            }
        }
    }
    // Fix B: residual local gray (a mid-drain stop) joins the deferred
    // handoff; the termination fold routes both through the STW `mark_value`
    // drain. Empty on the normal (idle-stop) path.
    if !job.gray.is_empty() {
        logs.result.deferred.extend(job.gray.drain_words());
    }
    // Every exit, including either stop quantum, retains the owned logs.
    // No heap/snapshot access follows this handoff.
    let _ = job.exited.send(logs.result);
}

/// Set the thread-local tagged heap pointer from the heap's provenance root.
///
/// `root` must be a copy of the allocation's single root raw pointer (see
/// [`super::AliasableBox`]), not a pointer derived from a `&mut` borrow of the
/// heap. The TLS slot hands the pointer back out to `with_tagged_heap`, so it
/// must remain valid across every other access path the owner takes; a
/// `&mut`-derived tag is popped by the owning `&mut`'s next use (Stacked
/// Borrows) or disabled by it (Tree Borrows).
///
/// All production installers use this form. Root-owning test fixtures use it
/// too; the legacy borrow installer is restricted to test builds.
pub(crate) fn set_tagged_heap_root(root: std::ptr::NonNull<TaggedHeap>) {
    // SAFETY: the caller supplies the live allocation's root pointer.
    if !tagged_heap_is_current(unsafe { root.as_ref() }) {
        super::super::collection_reads::reset_recent_reads_for_heap_installation();
    }
    // Attach before deriving the temporary heap loan used by mirror refresh.
    unsafe { attach_store_protocol(root) };
    TAGGED_HEAP.with(|h| h.set(root.as_ptr()));
    // The mirrors below need a `&mut`. Deriving it from the root (rather than
    // from a caller's borrow) is what leaves the stored root tag intact: a
    // sibling reborrow pops only its predecessor, never the root itself.
    // SAFETY: the root is live and the caller has exclusive access to the
    // heap at installation time; no TLS-derived access is live here.
    unsafe { sync_heap_tls_mirrors(root.as_ptr()) };
}

/// Legacy borrow installer for test fixtures awaiting TestHeap migration.
///
/// The stored child pointer cannot survive sibling owner/root accesses, and
/// mirror synchronization can itself invalidate it. New fixtures must own an
/// AliasableBox root and install with `set_tagged_heap_root` (use TestHeap).
/// Restrict this legacy surface to unit-test builds while that inventory is
/// migrated; no production or integration-test caller can use it.
#[cfg(test)]
pub fn set_tagged_heap(heap: &mut TaggedHeap) {
    if !tagged_heap_is_current(heap) {
        super::super::collection_reads::reset_recent_reads_for_heap_installation();
    }
    // Legacy tests retain their existing borrow-derived heap contract.
    unsafe { attach_store_protocol(std::ptr::NonNull::from(&mut *heap)) };
    TAGGED_HEAP.with(|h| h.set(heap as *mut TaggedHeap));
    unsafe { sync_heap_tls_mirrors(heap as *mut TaggedHeap) };
}

/// Refresh the per-heap TLS mirrors after the allocation slot was stored.
unsafe fn sync_heap_tls_mirrors(root: *mut TaggedHeap) {
    // End each heap loan before an observation-owner access can resolve its
    // inline home through the saved allocation root. Never reuse a sibling
    // &mut heap loan across those accesses.
    let (owner, dump) = {
        let heap = unsafe { &*root };
        (
            heap._satb_layout_reserve.observations().clone(),
            heap.collection_dump_window(),
        )
    };
    owner.install(dump);
    let observed = super::super::collection_reads::compiled_observation_gate(dump);
    {
        let heap = unsafe { &mut *root };
        TAGGED_HEAP_ID.with(|identity| identity.set(Some(heap.identity())));
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.set(heap.write_tracking_mode()));
        TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.set(heap.partition_dump));
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(heap.concurrent_mark_running));
        TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|active| {
            active.set(
                heap.concurrent_mark_running
                    && heap.concurrent_claims()
                    && heap.concurrent_hash_snapshot().is_some(),
            );
        });
        TAGGED_HEAP_DUMP_SPAN.with(|s| s.set((heap.dump_addr_lo, heap.dump_addr_hi)));
        let window = heap.barrier_window();
        heap.set_compiled_barrier_window(window, observed);
        TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.set(window));
        TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(heap.cons_barrier_window(window)));
        // This is the legitimate current loan, not the retained sibling root.
        unsafe { heap.store.assume_init_ref() }
            .publish_windows(heap.cons_barrier_window(window), window);
        heap.current_mutator_gc_mut().remembered_cache.fill(0);
    }
    clear_barrier_cache(&TAGGED_HEAP_SATB_CACHE);
}

/// Clear the carrier's heap-pointer TLS after a retired job (peer review,
/// condition 1: defence in depth).
///
/// The World's heap stays alive; only this OS thread's slot and its hot-path
/// mirrors are reset to their fresh values. The next job's `new_green_owner`
/// installs the heap again, and the debug assertion at job start proves the
/// slot was empty, so a read before that install faults deterministically
/// instead of following a stale but plausible pointer.
pub(crate) fn clear_carrier_heap_tls() {
    TAGGED_HEAP.with(|slot| slot.set(std::ptr::null_mut()));
    TAGGED_HEAP_ID.with(|slot| slot.set(None));
    TAGGED_HEAP_WRITE_TRACKING_MODE.with(|slot| slot.set(WriteTrackingMode::Disabled));
    TAGGED_HEAP_PARTITION_ACTIVE.with(|slot| slot.set(false));
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|slot| slot.set(false));
    TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|slot| slot.set(false));
    TAGGED_HEAP_DUMP_SPAN.with(|slot| slot.set((usize::MAX, 0)));
    TAGGED_HEAP_BARRIER_WINDOW.with(|slot| slot.set(BarrierWindow::NONE));
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|slot| slot.set(BarrierWindow::NONE));
    clear_barrier_cache(&TAGGED_HEAP_SATB_CACHE);
}

/// Whether this carrier may start a Lisp job with its current heap slot
/// (job-prologue and park-reset debug assertion).
///
/// The slot must be empty: a stale World heap from the previous owner is the
/// use-after-free the carrier reset exists to prevent. Test builds add one
/// harness exception: reading lazily initialized TLS while no heap is
/// installed allocates the per-thread test fallback heap, and that heap is
/// not a stale owner heap. The strict release behaviour stays "null only".
///
/// Unconditional so the `debug_assert!` that calls it also type-checks in a
/// release build (the macro compiles its expression there and only folds the
/// check away).
#[inline(always)]
pub(crate) fn carrier_heap_is_acceptable_for_job_start() -> bool {
    let ptr = TAGGED_HEAP.with(|slot| slot.get());
    if ptr.is_null() {
        return true;
    }
    #[cfg(test)]
    {
        TEST_FALLBACK_TAGGED_HEAP.with(|fb| {
            fb.borrow()
                .as_ref()
                .is_some_and(|heap| heap.root().as_ptr() == ptr)
        })
    }
    #[cfg(not(test))]
    {
        false
    }
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
    heap.detach_store_protocol();
    clear_tagged_heap_root_if_installed(std::ptr::NonNull::from(heap));
}

/// Uninstall by address alone, without creating a heap reference. Admission
/// may hold an exclusive collector loan while clearing its root-derived TLS
/// alias; even a shared reference from that root would invalidate the loan.
/// This compares pointers and resets TLS mirrors, never dereferencing `root`.
pub(crate) fn clear_tagged_heap_root_if_installed(root: std::ptr::NonNull<TaggedHeap>) {
    let owned = root.as_ptr();
    let _ = TAGGED_HEAP.try_with(|h| {
        if h.get() == owned {
            super::super::collection_reads::clear_installed_store_heap(owned);
            super::super::collection_reads::reset_recent_reads_for_heap_installation();
            h.set(std::ptr::null_mut());
            let _ = TAGGED_HEAP_ID.try_with(|identity| identity.set(None));
            let _ = TAGGED_HEAP_WRITE_TRACKING_MODE
                .try_with(|mode| mode.set(WriteTrackingMode::Disabled));
            let _ = TAGGED_HEAP_PARTITION_ACTIVE.try_with(|p| p.set(false));
            let _ = TAGGED_HEAP_CONCURRENT_ACTIVE.try_with(|c| c.set(false));
            let _ = TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.try_with(|active| active.set(false));
            let _ = TAGGED_HEAP_DUMP_SPAN.try_with(|s| s.set((usize::MAX, 0)));
            let _ = TAGGED_HEAP_BARRIER_WINDOW.try_with(|w| w.set(BarrierWindow::NONE));
            let _ = TAGGED_HEAP_CONS_BARRIER_WINDOW.try_with(|w| w.set(BarrierWindow::NONE));
            let _ = TAGGED_HEAP_SATB_CACHE.try_with(|slots| {
                for slot in slots {
                    slot.set(0);
                }
            });
        }
    });
}

// SAFETY: caller supplies the allocation's stable provenance root, with no
// temporary heap reference retained across the protocol transfer.
unsafe fn attach_store_protocol(root: std::ptr::NonNull<TaggedHeap>) {
    let owner = unsafe { root.as_ref() }
        ._satb_layout_reserve
        .observations()
        .clone();
    let destination = unsafe { std::ptr::addr_of_mut!((*root.as_ptr()).store).cast() };
    unsafe { owner.attach(root, std::ptr::NonNull::new_unchecked(destination)) };
}

impl TaggedHeap {
    /// Called through a legitimate loan before collector admission or Drop.
    /// Displaced heaps also retire their inline home before storage is freed.
    pub(super) fn detach_store_protocol(&self) {
        let Some(owner) = self._satb_layout_reserve.existing_observations() else {
            return;
        };
        // SAFETY: this loan addresses the current (possibly moved) heap.
        // An unattached heap does not read its uninitialized tail.
        unsafe { owner.detach(self.store.as_ptr()) };
    }

    /// Pair a temporary quiescent loan with restoration only when its serial
    /// allocation slot is still installed. Era admission clears that slot and
    /// deliberately stays detached until ordinary owner activation resumes.
    pub(super) fn restore_store_after_quiescent_loan(&mut self) {
        let root = TAGGED_HEAP.with(Cell::get);
        if !std::ptr::eq(root, self) {
            return;
        }
        let Some(owner) = self._satb_layout_reserve.existing_observations().cloned() else {
            return;
        };
        let window = self.barrier_window();
        let cons = self.cons_barrier_window(window);
        // SAFETY: root is the installed allocation provenance root; writing
        // uses this legitimate exclusive loan. The saved root is not read.
        unsafe {
            let destination = std::ptr::addr_of_mut!((*root).store).cast();
            owner.attach_via(
                std::ptr::NonNull::new_unchecked(root),
                self.store.as_mut_ptr(),
                std::ptr::NonNull::new_unchecked(destination),
            );
            self.store.assume_init_ref().publish_windows(cons, window);
        }
    }

    /// Stable World-owned store protocol, retained by this heap's registry.
    /// Obtained on construction/activation, never selected from another World.
    pub(crate) fn store_state_root(
        &self,
    ) -> std::ptr::NonNull<super::super::collection_reads::StoreState> {
        self._satb_layout_reserve.observations().root()
    }
}

/// True when this thread has a tagged heap installed for allocation.
pub fn tagged_heap_is_installed() -> bool {
    TAGGED_HEAP.with(|h| !h.get().is_null())
}

/// Check the allocation view without reading the installed heap. A Context
/// moved to another thread may have left an inactive raw pointer here.
#[inline(always)]
pub(crate) fn tagged_heap_is_current(heap: &TaggedHeap) -> bool {
    TAGGED_HEAP.with(|h| std::ptr::eq(h.get(), heap))
        && TAGGED_HEAP_ID.with(|identity| identity.get() == Some(heap.identity()))
}

/// Return the current thread's tagged heap identity, if one is installed.
///
/// This is used only for runtime side tables that must avoid retaining Lisp
/// objects from a different evaluator heap. GNU keeps those object references
/// inside ordinary GC-managed structures; the heap identity preserves that
/// ownership boundary for Neomacs side tables.
pub(crate) fn current_tagged_heap_identity() -> Option<usize> {
    TAGGED_HEAP_ID.with(Cell::get)
}

/// Compile-time mode of the active mutator's heap, without installing one.
/// Heapless lowering preserves the legacy shape. Heap-store leaves must be
/// compiled on their intended heap; cached runtime leaves already obey this
/// identity contract, as do BLV leaves with baked cell addresses.
pub(crate) fn current_heap_generational_enabled() -> bool {
    #[cfg(all(test, debug_assertions, feature = "jit"))]
    HEAP_GENERATIONAL_MODE_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    TAGGED_HEAP.with(|slot| {
        // SAFETY: an installed heap is live while its mutator lowers code.
        unsafe { installed_heap_generational_enabled(slot.get()) }
    })
}

/// Compiler-only regression instrumentation, absent from production. This
/// counts mode captures, not mutator GC events or cached heap state.
#[cfg(all(test, debug_assertions, feature = "jit"))]
static HEAP_GENERATIONAL_MODE_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(all(test, debug_assertions, feature = "jit"))]
pub(crate) fn heap_generational_mode_reads_for_test() -> usize {
    HEAP_GENERATIONAL_MODE_READS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Read the frozen compile-time mode without creating a fallback heap. The
/// false target is an immutable promoted literal, not a mutator cache.
/// Rust stores select their protocol window without reading this flag.
///
/// SAFETY: `heap` is null or the current mutator's live, initialized heap.
#[inline(always)]
unsafe fn installed_heap_generational_enabled(heap: *const TaggedHeap) -> bool {
    let flag = if heap.is_null() {
        &false as *const bool
    } else {
        unsafe { std::ptr::addr_of!((*heap).generational.enabled) }
    };
    // Both selected targets are initialized and readable; the knob is immutable.
    unsafe { *flag }
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
                // Copy the root out first: holding the `RefCell` borrow across
                // `f` would both block reentrancy and hand out a `&mut` with a
                // shorter provenance than the slot needs.
                let root = {
                    let mut borrow = fb.borrow_mut();
                    if borrow.is_none() {
                        *borrow = Some(AliasableBox::new(TaggedHeap::new()));
                    }
                    borrow.as_ref().unwrap().root()
                };
                set_tagged_heap_root(root);
                f(unsafe { &mut *root.as_ptr() })
            })
        }
        #[cfg(not(test))]
        {
            panic!("no TaggedHeap set for this thread");
        }
    })
}

/// The shared store entry: one const-initialized heap TLS load, then inline
/// scalar fields at fixed offsets. A null installation is supported by native
/// symbol stores and admission tests; its outlined path never installs a heap.
#[inline(always)]
pub(crate) fn with_store_state<R>(
    f: impl FnOnce(&super::super::collection_reads::StoreState) -> R,
) -> R {
    let heap = TAGGED_HEAP.with(Cell::get);
    if heap.is_null() {
        return store_without_heap(f);
    }
    // SAFETY: installation initializes the inline protocol before publishing
    // TLS. No callback or safepoint changes installation during a slot store.
    f(unsafe { &*std::ptr::addr_of!((*heap).store).cast() })
}

#[cold]
#[inline(never)]
fn store_without_heap<R>(f: impl FnOnce(&super::super::collection_reads::StoreState) -> R) -> R {
    super::super::collection_reads::with_store(|store| {
        // An uninstalled heap has no GC barrier. Cold publication to a parked
        // heap can refresh its saved windows; the null entry still must not
        // infer a live GEN1 heap from such a cons-window mirror.
        store.heap.set(std::ptr::null_mut());
        store.publish_windows(BarrierWindow::NONE, BarrierWindow::NONE);
        f(store)
    })
}

/// Central mutation hook for bulk writes to the tagged heap.
///
/// `#[inline(always)]`, like the whole inline half of the barrier: this is
/// inlined into every heap store (`set_cons_car`, `set_vector_slot`, ...),
/// and when the barrier's outlined rejects lived inline it grew past LLVM's
/// threshold and every `setcar` made a call (elb inclist +9% instructions).
/// The `HeapWriteRecord` is built only on the cold path, never before the
/// gate.
#[inline(always)]
pub fn note_heap_write(owner: TaggedValue, kind: HeapWriteKind) {
    if barrier_gate(owner) {
        if owner.is_cons() {
            note_cons_write_slow(owner, kind, None, None);
        } else {
            note_heap_write_slow(owner, kind, None, None);
        }
    }
}

/// Central mutation hook for slot writes to the tagged heap.
#[inline(always)]
pub fn note_heap_slot_write(
    owner: TaggedValue,
    kind: HeapWriteKind,
    slot: usize,
    value: TaggedValue,
) {
    with_store_state(|store| note_heap_slot_write_in_store(store, owner, kind, slot, value));
}

#[inline(always)]
pub(crate) fn note_heap_slot_write_in_store(
    store: &super::super::collection_reads::StoreState,
    owner: TaggedValue,
    kind: HeapWriteKind,
    slot: usize,
    value: TaggedValue,
) {
    if owner.is_cons() {
        let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
        // The precomputed mirror comes first: in GEN0 it equals the real
        // window, so a miss is exactly pre-b125's plain store and the hot
        // path pays one address test. In GEN1 the mirror is ALL, so every
        // cons store enters the filter below — where main called the cons
        // slow path unconditionally.
        if !store.windows.cons_window.get().covers(addr) {
            return;
        }
        // A real-window hit takes the full barrier in both generations: an
        // immediate overwrite still needs its preimage during SATB. GEN0's
        // mirror equals the real window, so a GEN0 owner admitted above
        // always hits here; only GEN1 can miss.
        if store.windows.window.get().covers(addr) {
            note_heap_write_slow(owner, kind, Some(slot), Some(value));
            return;
        }
        // The missed real window and ALL mirror prove an installed GEN1
        // heap (a GEN0 mirror would have hit above). Uninstall clears both
        // mirrors. No allocation, callback or safe point can change the
        // installation. An immediate creates no old-to-young edge, so only
        // a pointer value reaches the owner check below.
        if value.is_fixnum() {
            return;
        }
        // Only the zero-old-count certificate stays inline: the old-owner
        // tail is outlined, because every inline store site carrying it
        // grew past the setters' outline thresholds and absorbed the whole
        // barrier instead (b122: specbind_cached +133 instructions, setcdr
        // call sites inlined away). Young-block stores — the hot
        // generational case — still return without any call.
        let ptr = store.heap.get();
        // SAFETY: the protocol proof above establishes a live installed
        // heap, borrowed only for non-allocating atomic metadata accesses.
        let heap = unsafe { &*ptr };
        if !heap.region_book.no_old_cons_block_contains(addr) {
            generation_only_old_cons_store(heap, owner, addr);
        }
        return;
    }
    if owner.is_heap_object()
        && (store
            .windows
            .window
            .get()
            .covers(owner.bits() & !crate::tagged::value::TAG_MASK)
            || unsafe {
                GcHeader::needs_remembering(
                    (owner.bits() & !crate::tagged::value::TAG_MASK) as *const GcHeader,
                )
            })
    {
        let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
        // Preserve GEN0's persistent owner bookkeeping. GEN0's mirror equals
        // the real window, so its length is never ALL: one length test and
        // the admitted GEN0 store reaches the full barrier, as before b125.
        // An ALL mirror proves GEN1 installed, where an immediate outside
        // the real window creates no old-to-young edge and needs no
        // bookkeeping; real-window writes retain it.
        if store.windows.cons_window.get().len() == usize::MAX {
            if value.is_fixnum() && !store.windows.window.get().covers(addr) {
                return;
            }
        }
        note_heap_write_slow(owner, kind, Some(slot), Some(value));
    }
}

/// Old-owner tail of the generation-only cons store: prove ownership, read
/// the age byte, mark the owner's card. Outlined from every store site —
/// reached only when GEN1 is installed and the canonical zero-old-count
/// certificate failed, so the owner's block may hold old cells. Scalar
/// arguments travel in registers; nothing here constructs a record.
#[cold]
#[inline(never)]
fn generation_only_old_cons_store(heap: &TaggedHeap, owner: TaggedValue, addr: usize) {
    if generation_only_cons_is_old(heap, owner) {
        heap.mark_owner_card(addr);
    }
}

/// Bulk stores and non-cons slot stores retain the legacy window/header
/// gate. Cons slot stores take their mirror-first path above instead.
#[inline(always)]
fn barrier_gate(owner: TaggedValue) -> bool {
    if !owner.is_heap_object() {
        return false;
    }
    let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
    let window = if owner.is_cons() {
        TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.get())
    } else {
        TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.get())
    };
    if window.covers(addr) {
        return true;
    }
    // SAFETY: a non-cons heap value points at a live GcHeader-prefixed
    // object. Promotion is stop-the-world; remember_owner claims the logged
    // byte atomically. Cons owners never read a header.
    !owner.is_cons() && unsafe { GcHeader::needs_remembering(addr as *const GcHeader) }
}

/// Test an owned cons's atomic age byte after the real window refused it.
/// Slot stores already require a live, well-formed ConsCell: they dereference
/// this same owner immediately after the barrier. Foreign conses are valid
/// inputs, so ownership must still precede every trailer access. This query
/// does not record a Lisp read; setters own that policy. No arm safepoints.
#[inline(always)]
fn generation_only_cons_is_old(heap: &TaggedHeap, owner: TaggedValue) -> bool {
    let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
    // Reuse the canonical allocator's zero-old-count certificate from the
    // current grant. Its young-work flag alone is insufficient: mixed blocks
    // contain old owners too. All collector entries close this certificate
    // before changing ages/counts, even when survivors remain young. No
    // owner memory, registry or allocator mutex is read on this fast path.
    if heap.region_book.no_old_cons_block_contains(addr) {
        return false;
    }
    let cur = heap.jit.cons_cur.get();
    let ptr = addr as *const ConsCell;
    let base = ConsBlock::block_base_for_ptr(ptr);
    // An open region's cursor belongs to a live owned block, even when it
    // equals its limit (the payload ends before the block boundary). Every
    // collector entry closes regions before releasing storage. This proves
    // ownership, not youth: old cells can share the region's block.
    if cur == 0 || ConsBlock::block_base_for_ptr(cur as *const ConsCell) != base {
        let owned = match heap.chunk_map.as_ref() {
            Some(map) => map.get(ptr as usize).is(ChunkClass::Cons),
            None => return generation_only_registered_cons_is_old(heap, ptr),
        };
        if !owned {
            return false;
        }
    }
    // A live cell in this owned block necessarily has payload-cell geometry.
    // Foreign eight-aligned cells fail ownership above, before any trailer read.
    #[cfg(debug_assertions)]
    debug_assert!(generation_only_cons_cell_offset(addr).is_some());
    // SAFETY: ownership plus the slot store's live ConsCell contract proves
    // the payload index. Release and promotion require mutator exclusion,
    // and this leaf has no safe point.
    let trailer = unsafe { ConsBlockTrailer::from_block_base(base) };
    trailer.age_is_old((addr & (CONS_BLOCK_ALIGN - 1)) / size_of::<ConsCell>())
}

/// Debug/test witness for the owned slot-store input invariant.
#[cfg(any(debug_assertions, test))]
#[inline(always)]
fn generation_only_cons_cell_offset(addr: usize) -> Option<usize> {
    let offset = addr & (CONS_BLOCK_ALIGN - 1);
    if offset < CONS_CELLS_BYTES && offset & (size_of::<ConsCell>() - 1) == 0 {
        Some(offset)
    } else {
        None
    }
}

/// Keep registry lookup temporaries inside the map-off fallback. Carrying
/// its base and index across `contains_key` otherwise adds callee-saved
/// registers to every setter, including the GEN0 immediate fast path.
#[cold]
#[inline(never)]
fn generation_only_registered_cons_is_old(heap: &TaggedHeap, ptr: *const ConsCell) -> bool {
    let base = ConsBlock::block_base_for_ptr(ptr);
    if !heap.cons_block_index_by_base.contains_key(&base) {
        return false;
    }
    let addr = ptr as usize;
    #[cfg(debug_assertions)]
    debug_assert!(generation_only_cons_cell_offset(addr).is_some());
    // SAFETY: the registry proves ownership; the caller's live ConsCell input
    // proves payload geometry. This leaf cannot safepoint.
    let trailer = unsafe { ConsBlockTrailer::from_block_base(base) };
    trailer.age_is_old((addr & (CONS_BLOCK_ALIGN - 1)) / size_of::<ConsCell>())
}

/// Dispatch cons writes admitted by the protocol mirror. A real-window hit
/// retains the full barrier, including immediate-overwrite preimage capture.
/// Otherwise admission proves GEN1: its mirror is ALL, whereas GEN0's mirror
/// equals the real window. A generation-only store marks its owner's card
/// and stops there, before constructing a record or reading census,
/// tracking or concurrent state.
#[cold]
#[inline(never)]
fn note_cons_write_slow(
    owner: TaggedValue,
    kind: HeapWriteKind,
    slot: Option<usize>,
    value: Option<TaggedValue>,
) {
    let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
    if !TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.get().covers(addr)) {
        // GEN1 outside the window: an old owner marks its card, as compiled
        // stores do, with one Relaxed byte store. Characters share fixnum
        // encoding and create no edge; a symbol's liveness rides on the card
        // like any heap value.
        if !value.is_some_and(TaggedValue::is_fixnum) {
            with_tagged_heap(|heap| {
                if heap
                    .old_cons_trailer(owner)
                    .is_some_and(|(trailer, index)| trailer.age_is_old(index))
                {
                    heap.mark_owner_card(addr);
                }
            });
        }
        return;
    }
    note_heap_write_slow(owner, kind, slot, value);
}

/// The barrier's outlined part: build the record and ask the heap.
#[cold]
#[inline(never)]
fn note_heap_write_slow(
    owner: TaggedValue,
    kind: HeapWriteKind,
    slot: Option<usize>,
    value: Option<TaggedValue>,
) {
    #[cfg(test)]
    BARRIER_SLOW_CALLS.with(|calls| calls.set(calls.get() + 1));
    let record = HeapWriteRecord {
        owner,
        kind,
        slot,
        value,
    };
    if census_remset_probe_on() {
        with_tagged_heap(|heap| heap.census_note_write(record));
    }
    let disabled =
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.get()) == WriteTrackingMode::Disabled;
    let concurrent = TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get());
    if barrier_needs_heap(record, disabled, concurrent) {
        with_tagged_heap(|heap| heap.record_heap_write(record));
    }
}

/// Whether a write that passed the flag test must reach `record_heap_write`.
#[inline(always)]
pub(super) fn barrier_needs_heap(
    record: HeapWriteRecord,
    disabled: bool,
    concurrent: bool,
) -> bool {
    let bits = record.owner.bits();
    if disabled && !concurrent {
        // The Rust cons mirror is ALL under generations; compiled cons
        // stores arrive through the real window or their inline bitmap.
        // Reject irrelevant generation-only writes before claiming the owner.
        // Tracking and concurrent marking use the legacy barrier below and
        // must not pay for a generational heap lookup.
        let gen_decision = with_tagged_heap(|heap| {
            if !heap.generational_enabled() {
                return None;
            }
            // Characters have fixnum encoding. Symbols (including nil/t) and
            // all pointer/handle tags remain conservative: symbols have GC
            // side-table liveness, and SymbolWithPos is a traced heap object.
            if record.value.is_some_and(TaggedValue::is_fixnum) {
                return Some(false);
            }
            let needs_log = if heap.owner_is_mapped(record.owner) {
                !heap
                    .current_mutator_gc()
                    .r_mapped_seen
                    .contains(&record.owner.bits())
            } else if record.owner.is_cons() {
                heap.old_cons_trailer(record.owner)
                    .is_some_and(|(trailer, index)| trailer.age_is_old(index))
            } else {
                TaggedHeap::value_heap_addr(record.owner).is_some_and(|addr| unsafe {
                    GcHeader::needs_remembering(addr as *const GcHeader)
                })
            };
            Some(needs_log)
        });
        if let Some(needs_log) = gen_decision {
            return needs_log;
        }
        // Partition-only path: the barrier's sole job is the append-only dump
        // remembered set (see `record_heap_write`), whose only effect here is
        // inserting a mapped or tenured owner, so two cheap
        // rejects apply. (1) An owner already inserted has nothing to add —
        // its entry is permanent. (2) An owner outside the dump span that is
        // not tenured is not inserted: a cons never is (`value_is_tenured` is
        // false for cons), and any other heap value points at a `GcHeader`
        // whose `tenured` byte is exactly what `value_is_tenured` reads.
        let cached = TAGGED_HEAP.with(|current| {
            let heap = current.get();
            // SAFETY: an installed heap is live until its owner uninstalls it.
            // Only this mutator accesses the cache, and this check is reached
            // through the outlined store barrier.
            !heap.is_null()
                && unsafe {
                    (*heap).current_mutator_gc().remembered_cache[barrier_cache_slot(bits)] == bits
                }
        });
        if cached {
            return false;
        }
        if let Some(addr) = TaggedHeap::value_heap_addr(record.owner) {
            let (lo, hi) = TAGGED_HEAP_DUMP_SPAN.with(|s| s.get());
            if (addr < lo || addr >= hi)
                && (record.owner.is_cons() || !unsafe { (*(addr as *const GcHeader)).tenured })
            {
                return false;
            }
        }
    } else if disabled
        && !record.owner.is_cons()
        && TAGGED_HEAP_SATB_CACHE.with(|slots| slots[barrier_cache_slot(bits)].get()) == bits
    {
        // Concurrent mark, owner tracking off: this owner's pre-image is
        // already logged this cycle (see `TAGGED_HEAP_SATB_CACHE`).
        return false;
    }
    true
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
    if !pre_image.is_heap_object()
        && !matches!(
            pre_image.gc_kind(),
            crate::tagged::value::ValueKind::Symbol(_)
        )
    {
        return;
    }
    if !TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get()) {
        return;
    }
    with_tagged_heap(|heap| heap.note_root_overwrite_value(pre_image));
}

/// [`note_root_overwrite`] for a caller that has just read
/// [`concurrent_mark_active`] and found it true, so the thread-local gate is
/// not read a second time.
#[inline]
pub(crate) fn note_root_overwrite_while_marking(pre_image: TaggedValue) {
    if !pre_image.is_heap_object()
        && !matches!(
            pre_image.gc_kind(),
            crate::tagged::value::ValueKind::Symbol(_)
        )
    {
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

/// The installed mutator has an active Tier-H snapshot. Claims-OFF and
/// stopped-world mutations reject with one TLS load, without touching a heap.
#[inline(always)]
pub(crate) fn concurrent_hash_mutation_active() -> bool {
    TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|active| active.get())
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
/// as `push_value_children_to_satb`'s owner dedup. The caller has
/// already checked `concurrent_mark_active()`.
pub(crate) fn note_string_interval_preimage(
    string_addr: usize,
    table: &crate::buffer::text_props::TextPropertyTable,
) {
    with_tagged_heap(|heap| {
        if !heap
            .current_mutator_satb_mut()
            .satb_string_preimage_addrs
            .insert(string_addr)
        {
            return; // this string's full pre-image was already logged this cycle
        }
        let major = heap.generational.major_in_progress;
        let (mutator, handoff) = heap.satb_parts();
        table.for_each_root(|value| {
            if value.is_heap_object()
                || (major && matches!(value.gc_kind(), crate::tagged::value::ValueKind::Symbol(_)))
            {
                mutator.satb.push(MarkWord::of(value), handoff);
            }
        });
    });
}

#[cfg(test)]
#[path = "tests/canonical_cons_block_barrier_test.rs"]
mod canonical_cons_block_barrier_tests;

#[cfg(test)]
#[path = "tests/cons_payload_alignment_test.rs"]
mod cons_payload_alignment_tests;

#[cfg(test)]
#[path = "tests/inline_cons_owner_test.rs"]
mod inline_cons_owner_tests;

#[cfg(test)]
#[path = "tests/inline_slot_barrier_test.rs"]
mod inline_slot_barrier_tests;

#[cfg(test)]
#[path = "tests/foreign_cons_footer_test.rs"]
mod foreign_cons_footer_tests;

#[cfg(test)]
#[path = "tests/cons_real_window_first_test.rs"]
mod cons_real_window_first_tests;

#[cfg(test)]
#[path = "tests/once_survived_young_cons_test.rs"]
mod once_survived_young_cons_tests;

#[cfg(test)]
#[path = "tests/concurrent_major_worker_test.rs"]
mod concurrent_major_worker_tests;

#[cfg(test)]
#[path = "tests/chartable_concurrent_write_test.rs"]
mod chartable_concurrent_write_tests;

#[cfg(test)]
#[path = "tests/concurrent_leaf_claim_tests.rs"]
mod concurrent_leaf_claim_tests;

#[cfg(test)]
#[path = "tests/concurrent_hash_worker_tests.rs"]
mod concurrent_hash_worker_tests;

#[cfg(test)]
mod worker_symbol_classification_tests {
    use super::*;
    use crate::emacs_core::intern::{intern, intern_uninterned};
    use crate::heap_types::LispString;

    #[test]
    fn worker_symbol_classification_preserves_namesakes_and_deduplicates_ids() {
        let mut heap = TaggedHeap::new();
        let heap_values = [
            heap.alloc_cons(TaggedValue::NIL, TaggedValue::T),
            heap.alloc_vector(vec![TaggedValue::fixnum(7)]),
            heap.alloc_string(LispString::from_utf8("worker symbol classifier")),
            heap.alloc_float(4.25),
        ];
        let ignored = [
            TaggedValue::NIL,
            TaggedValue::T,
            TaggedValue::UNBOUND,
            TaggedValue::fixnum(0),
            TaggedValue::fixnum(-1),
            heap_values[0],
            heap_values[1],
            heap_values[2],
            heap_values[3],
        ];
        let mut logs = EnabledWorkerMarkLogs::default();
        for value in ignored {
            assert!(!logs.note_symbol(value), "unexpected side mark: {value:?}");
        }
        assert!(logs.result.symbols.is_empty());

        let ids = [
            intern("u35-worker-symbol-classification"),
            intern_uninterned("u35-worker-symbol-classification"),
            intern_uninterned("nil"),
            intern_uninterned("t"),
            intern_uninterned("unbound"),
        ];
        assert_ne!(ids[0], ids[1], "same name does not imply same identity");
        for _ in 0..2 {
            for id in ids {
                let value = TaggedValue::from_sym_id(id);
                assert!(matches!(
                    value.kind(),
                    crate::tagged::value::ValueKind::Symbol(_)
                ));
                assert!(logs.note_symbol(value));
            }
        }
        assert_eq!(logs.result.symbols, ids);
        assert_eq!(logs.seen_symbols.count(), ids.len());

        let mut gray = MarkStack::default();
        for value in ignored {
            logs.queue_child::<true>(value, &mut gray);
        }
        assert_eq!(
            gray.into_values(),
            heap_values,
            "only real heap objects join gray"
        );
        assert_eq!(logs.result.symbols, ids);

        let mut legacy: WorkerMarkLogs = WorkerMarkLogs::default();
        let mut legacy_gray = MarkStack::default();
        for value in ignored {
            legacy.queue_child::<false>(value, &mut legacy_gray);
        }
        for id in ids {
            legacy.queue_child::<false>(TaggedValue::from_sym_id(id), &mut legacy_gray);
        }
        assert_eq!(legacy_gray.into_values(), heap_values);
        assert!(legacy.result.symbols.is_empty());
        assert_eq!(legacy.seen_symbols.len(), 0);

        // A fresh worker/cycle has its own bits. Snapshot routing preserves
        // first-encounter order and deduplication even after repeated symbols.
        let mut next_cycle = EnabledWorkerMarkLogs::default();
        let mut snapshot_gray = MarkStack::default();
        for value in ignored {
            next_cycle.queue_snapshot_child(value, &mut snapshot_gray);
        }
        for _ in 0..3 {
            for id in ids.into_iter().rev() {
                next_cycle.queue_snapshot_child(TaggedValue::from_sym_id(id), &mut snapshot_gray);
            }
        }
        assert_eq!(snapshot_gray.into_values(), heap_values);
        assert_eq!(
            next_cycle.result.symbols,
            ids.into_iter().rev().collect::<Vec<_>>()
        );
        assert_eq!(next_cycle.seen_symbols.count(), ids.len());
    }
}

/// Carrier park: reset the GC's per-thread helper state (cold path).
pub(crate) fn reset_carrier_tls() {
    cold_gc::reset_carrier_tls();
}

/// Debug-only carrier TLS fingerprint: the heap slot and its mirrors, then
/// the cold-GC helper state.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    crate::tls_fingerprint::mix(h, TAGGED_HEAP.with(|slot| slot.get() as usize as u64));
    crate::tls_fingerprint::mix(h, TAGGED_HEAP_ID.with(|slot| slot.get().is_some() as u64));
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|slot| slot.get() as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_PARTITION_ACTIVE.with(|slot| slot.get() as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|slot| slot.get() as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.with(|slot| slot.get() as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_DUMP_SPAN.with(|slot| {
            let (lo, hi) = slot.get();
            (lo as u64) ^ ((hi as u64) << 1)
        }),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_BARRIER_WINDOW.with(|slot| (slot.get() == BarrierWindow::NONE) as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|slot| (slot.get() == BarrierWindow::NONE) as u64),
    );
    crate::tls_fingerprint::mix(
        h,
        TAGGED_HEAP_SATB_CACHE.with(|cells| {
            cells
                .iter()
                .map(|c| c.get() as u64)
                .fold(0u64, |a, b| a.wrapping_add(b))
        }),
    );
    cold_gc::carrier_tls_fingerprint(h);
}

#[cfg(test)]
#[path = "tests/collector_test_support.rs"]
mod collector_test_support;

#[cfg(test)]
#[path = "tests/collector_start_test.rs"]
mod collector_start_tests;
