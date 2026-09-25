//! Background compilation (P2.4, design `p2-4-background-compile`).
//!
//! A JIT compile splits into a FRONT that stays on the eval thread and a
//! BACKEND that needs nothing of the Lisp heap (`compile::shared::split`).
//! The front is today's pipeline up to the finished Cranelift function; it
//! reads the heap, the obarray and the eval thread's knobs, and it is
//! unchanged. The backend is Cranelift alone.
//!
//! `NEOVM_JIT_BG` ([`mode`], read once per process; tests override it per
//! thread) picks where the backend runs:
//!
//! | value | [`BgMode`] | what runs where |
//! |---|---|---|
//! | `legacy` (default) | `Legacy` | the persistent per-thread module defines each leaf in place (the B4 path, unchanged) |
//! | `sync` | `Sync` | the split, run in line: the eval thread's backend compiles each packaged function at once. Deterministic |
//!
//! Whatever the mode, the CLIF and the machine code are the same, and so is
//! everything Lisp can observe: the mode moves where code is produced, never
//! what it does.
//!
//! # A deferred compile's life
//!
//! A compile the cache allows to defer (a [`DeferScope`] around it) whose
//! backend runs elsewhere leaves a [`PendingJob`] in its cache entry: the
//! front's whole leaf (every box its code bakes an address of) with a null
//! entry, and the [`JobCell`] its backend publishes into. The function runs
//! in the interpreter meanwhile. The eval thread probes the cell where it
//! would have run the leaf, and installs a finished one there: the leaf
//! gets its entry and becomes the cache's `Compiled` entry. Until then:
//!
//! - GC: the pending leaf's reloc vector (every heap constant its code will
//!   load) is rooted through the cache entry, exactly as a compiled leaf's
//!   (`cache::collect_jit_reloc_gc_roots`).
//! - Invalidation: the leaf's inline dependencies are registered when the
//!   front finishes, so a redefinition of an inlined callee removes the
//!   pending entry as it would a compiled leaf; every other removal (a
//!   `make-closure` prefix widening, a deopt invalidation, a heap change)
//!   drops the pending entry, which cancels its job.
//! - Dispatch: a probe that finds the job unfinished holds the function in
//!   the interpreter for the next 32, 64, ... 1024 calls
//!   (`RuntimeState::defer_tier_up`), so the dispatcher does not probe per
//!   call.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use super::RuntimeState;
use super::compile::{CompileError, CompiledLeaf};
use super::stats::CompileOrigin;
use super::stats::asm_dump::{self, PendingAsm};

/// How JIT compiles run (`NEOVM_JIT_BG`). Exhaustive matches only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum BgMode {
    /// The persistent per-thread module defines in place (the B4 path).
    Legacy,
    /// The front/backend split, run in line on the eval thread.
    Sync,
}

/// The mode a `NEOVM_JIT_BG` value selects; anything unrecognised (or
/// unset) is [`BgMode::Legacy`].
pub(crate) fn parse_mode(value: Option<&str>) -> BgMode {
    match value.map(str::trim) {
        Some("sync") => BgMode::Sync,
        _ => BgMode::Legacy,
    }
}

/// This process's (or, under a test override, this thread's) mode.
pub(crate) fn mode() -> BgMode {
    #[cfg(test)]
    if let Some(mode) = MODE_TEST_OVERRIDE.with(Cell::get) {
        return mode;
    }
    static MODE: OnceLock<BgMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let mode = parse_mode(std::env::var("NEOVM_JIT_BG").ok().as_deref());
        if mode != BgMode::Legacy {
            let name: &'static str = mode.into();
            tracing::info!(target: "neovm::jit::knobs", "NEOVM_JIT_BG={name} is on in this process");
        }
        mode
    })
}

/// Whether compiles take the front/backend split (every mode but
/// [`BgMode::Legacy`]). Read once per compile, never per call.
pub(crate) fn split_enabled() -> bool {
    match mode() {
        BgMode::Legacy => false,
        BgMode::Sync => true,
    }
}

/// Why a compile was requested; the order is its priority (lower = sooner).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    strum::EnumCount,
    strum::EnumIter,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[repr(u8)]
pub(crate) enum JobClass {
    /// A `dispatch_sized` tier-up, or the re-attempt after a deferral.
    Entry = 0,
}

impl JobClass {
    /// The class a compile of `origin` defers under, or `None` when that
    /// compile always runs in line (re-tiers keep the leaf they replace
    /// only once B11's upgrade jobs exist; AOT drains and direct compiles
    /// want their leaf at once).
    pub(crate) fn for_origin(origin: CompileOrigin) -> Option<JobClass> {
        match origin {
            CompileOrigin::Dispatch | CompileOrigin::DeferralExpired => Some(JobClass::Entry),
            CompileOrigin::Retier
            | CompileOrigin::FirstSight
            | CompileOrigin::Osr
            | CompileOrigin::AotDrain
            | CompileOrigin::Direct => None,
        }
    }
}

/// Why a pending compile never became a leaf.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum DiscardReason {
    /// Its cache entry was removed or replaced first (an inlined callee's
    /// redefinition, a `make-closure` prefix widening, a deopt invalidation,
    /// a cache clear).
    Superseded,
    /// The tagged heap it was compiled against is gone.
    HeapChanged,
    /// Its backend failed (the body becomes `NotCompilable`, as a failed
    /// in-line compile would).
    Failed,
}

/// A backend job's progress, as its [`JobCell`] records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum JobState {
    Queued = 0,
    Done = 1,
}

/// What a backend hands back: the finalized entry's address, or the error
/// an in-line compile would have returned.
pub(crate) struct BackendOut {
    pub(crate) result: Result<usize, CompileError>,
    /// Backend time: module setup, codegen, finalize.
    pub(crate) backend_us: u64,
    /// From the front's hand-over to the backend's start.
    pub(crate) queue_wait_us: u64,
    /// The disassembly, under `NEOVM_JIT_DUMP_ASM`.
    pub(crate) asm: Option<PendingAsm>,
}

/// The rendezvous of one backend job: the backend publishes its
/// [`BackendOut`] and then marks it done (release); the eval thread reads
/// the mark (acquire) before it takes the result.
pub(crate) struct JobCell {
    state: AtomicU8,
    cancelled: AtomicBool,
    out: Mutex<Option<BackendOut>>,
}

impl JobCell {
    pub(crate) fn new() -> Arc<JobCell> {
        Arc::new(JobCell {
            state: AtomicU8::new(JobState::Queued as u8),
            cancelled: AtomicBool::new(false),
            out: Mutex::new(None),
        })
    }

    /// Tell the backend the result is no longer wanted (advisory: a job not
    /// yet started is skipped, a running one finishes and is dropped).
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Publish the backend's result.
    pub(crate) fn publish(&self, out: BackendOut) {
        *self.out.lock().unwrap_or_else(|p| p.into_inner()) = Some(out);
        self.state.store(JobState::Done as u8, Ordering::Release);
    }

    /// Whether the result is published (acquire: a `true` makes the result
    /// and the code it names visible to this thread).
    pub(crate) fn is_done(&self) -> bool {
        self.state.load(Ordering::Acquire) == JobState::Done as u8
    }

    fn take_out(&self) -> Option<BackendOut> {
        if !self.is_done() {
            return None;
        }
        self.out.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

/// A job the front just handed to a backend, on its way from the sink
/// (`split::define_split`) to the cache entry that will own it.
pub(crate) struct DeferredCode {
    cell: Arc<JobCell>,
    class: JobClass,
    enqueued_at: Instant,
}

/// Probes that find a job unfinished hold its function in the interpreter
/// for this many calls at first, doubling up to [`MAX_BACKOFF`].
pub(crate) const FIRST_BACKOFF: u32 = 32;
/// The longest hold between two probes of one pending job.
pub(crate) const MAX_BACKOFF: u32 = 1024;

/// A compile whose backend runs elsewhere, as its cache entry holds it
/// (see the module docs). Lives on the eval thread; only its
/// [`JobCell`] is shared.
pub(crate) struct PendingJob {
    /// The front's leaf, entry still null. `None` once installed.
    leaf: Option<CompiledLeaf>,
    cell: Arc<JobCell>,
    class: JobClass,
    /// The tagged heap the front compiled against.
    heap: Option<usize>,
    /// The function's tier-up deferral before this job's hold replaced it.
    saved_hold: u32,
    backoff: Cell<u32>,
    requested_heat: u32,
    enqueued_at: Instant,
    /// Installed, or discarded with its reason counted.
    settled: Cell<bool>,
}

impl PendingJob {
    /// The pending entry for `leaf`, whose code `code` is compiling; holds
    /// the function (`rt`) in the interpreter for the first backoff.
    pub(crate) fn new(
        id: u64,
        leaf: CompiledLeaf,
        code: DeferredCode,
        rt: &RuntimeState,
    ) -> Box<PendingJob> {
        let saved_hold = rt.deferred_heat();
        let heat = rt.heat();
        rt.defer_tier_up(heat.saturating_add(FIRST_BACKOFF));
        PENDING_COUNT.with(|c| c.set(c.get() + 1));
        PENDING_IDS.with(|ids| ids.borrow_mut().push(id));
        Box::new(PendingJob {
            leaf: Some(leaf),
            cell: code.cell,
            class: code.class,
            heap: crate::tagged::gc::current_tagged_heap_identity(),
            saved_hold,
            backoff: Cell::new(FIRST_BACKOFF.saturating_mul(2)),
            requested_heat: heat,
            enqueued_at: code.enqueued_at,
            settled: Cell::new(false),
        })
    }

    /// The front's leaf (entry null): its reloc constants are GC roots.
    pub(crate) fn leaf(&self) -> &CompiledLeaf {
        self.leaf.as_ref().expect("a pending job holds its leaf")
    }

    /// Whether the backend has published (install it now).
    pub(crate) fn is_ready(&self) -> bool {
        self.cell.is_done()
    }

    /// The function's tier-up deferral before this job held it.
    pub(crate) fn saved_hold(&self) -> u32 {
        self.saved_hold
    }

    /// A probe found the backend still running: interpret the next
    /// `backoff` calls of the function without probing.
    pub(crate) fn wait(&self, rt: &RuntimeState) {
        let backoff = self.backoff.get();
        self.backoff.set(backoff.saturating_mul(2).min(MAX_BACKOFF));
        rt.defer_tier_up(rt.heat().saturating_add(backoff));
        bump_stats(|s| s.pending_probes += 1);
    }

    fn settle(&self, outcome: Settled) {
        self.settled.set(true);
        let class = self.class as usize;
        let latency_us = self.enqueued_at.elapsed().as_micros() as u64;
        bump_stats(|s| match outcome {
            Settled::Installed => {
                s.installed[class] += 1;
                s.latency_us[super::stats::bucket_index(latency_us)] += 1;
            }
            Settled::Discarded(reason) => s.discarded[reason as usize] += 1,
        });
    }
}

#[derive(Clone, Copy)]
enum Settled {
    Installed,
    Discarded(DiscardReason),
}

impl Drop for PendingJob {
    fn drop(&mut self) {
        let _ = PENDING_COUNT.try_with(|c| c.set(c.get().saturating_sub(1)));
        if !self.settled.get() {
            self.cell.cancel();
            self.settle(Settled::Discarded(DiscardReason::Superseded));
        }
    }
}

/// Why a ready job did not install.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Discard {
    /// The backend failed: the body is `NotCompilable`, as after a failed
    /// in-line compile.
    Rejected,
    /// The leaf is stale: drop the entry; the next hot call compiles again.
    Stale(DiscardReason),
}

/// Install a ready job: its leaf with the backend's entry, or why not.
/// `rt` (the function's runtime, when the caller has it) gets its tier-up
/// deferral back, so the dispatcher enters the leaf from the next call; a
/// drain without it lets the hold run out instead. Eval thread only; no
/// Lisp allocation, no safepoint.
#[cold]
#[inline(never)]
pub(crate) fn install(
    mut job: Box<PendingJob>,
    rt: Option<&RuntimeState>,
) -> Result<CompiledLeaf, Discard> {
    let out = job.cell.take_out().expect("install only a ready job");
    if let Some(rt) = rt {
        rt.defer_tier_up(job.saved_hold);
        let calls = u64::from(rt.heat().saturating_sub(job.requested_heat));
        bump_stats(|s| s.interp_calls_while_pending += calls);
    }
    bump_stats(|s| {
        s.backend_us += out.backend_us;
        s.backend_max_us = s.backend_max_us.max(out.backend_us);
        s.queue_wait_us += out.queue_wait_us;
    });
    let entry = match out.result {
        Ok(entry) => entry,
        Err(err) => {
            tracing::debug!(target: "neovm_jit::bg", %err, "background backend failed");
            job.settle(Settled::Discarded(DiscardReason::Failed));
            return Err(Discard::Rejected);
        }
    };
    if job.heap != crate::tagged::gc::current_tagged_heap_identity() {
        job.settle(Settled::Discarded(DiscardReason::HeapChanged));
        return Err(Discard::Stale(DiscardReason::HeapChanged));
    }
    let mut leaf = job.leaf.take().expect("a pending job holds its leaf");
    leaf.entry = entry as *const u8;
    job.settle(Settled::Installed);
    if let Some(asm) = out.asm {
        asm_dump::restash(asm);
        let tier = match leaf.obs.osr_pc {
            Some(pc) => super::stats::perf_map::LabelTier::Osr(pc as usize),
            None => match leaf.tier() {
                super::compile::LeafTier::Mir => super::stats::perf_map::LabelTier::Mir,
                super::compile::LeafTier::Baseline | super::compile::LeafTier::Aot => {
                    super::stats::perf_map::LabelTier::Baseline
                }
            },
        };
        let default_name = match tier {
            super::stats::perf_map::LabelTier::Mir => "__neovm_mir_leaf",
            super::stats::perf_map::LabelTier::Baseline
            | super::stats::perf_map::LabelTier::Osr(_) => "__neovm_jit_leaf",
        };
        asm_dump::flush(&asm_dump::AsmLeafInfo {
            tier,
            entry_name: leaf.obs.label.as_deref().unwrap_or(default_name),
            entry: leaf.entry,
            regalloc: leaf.regalloc.name(),
            clif_insts: leaf.clif_insts,
        });
    }
    Ok(leaf)
}

/// The eval thread's background-compile counters (`[neovm-jit-final-bg]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BgStats {
    /// Jobs handed to a backend elsewhere, by class.
    pub(crate) enqueued: [u64; <JobClass as strum::EnumCount>::COUNT],
    /// Of those, installed as leaves.
    pub(crate) installed: [u64; <JobClass as strum::EnumCount>::COUNT],
    /// Of those, never installed, by reason.
    pub(crate) discarded: [u64; <DiscardReason as strum::EnumCount>::COUNT],
    /// Backend µs of the installed and failed jobs (summed, and the max).
    pub(crate) backend_us: u64,
    pub(crate) backend_max_us: u64,
    /// µs the installed and failed jobs waited for a backend.
    pub(crate) queue_wait_us: u64,
    /// Enqueue-to-install latency, bucketed like the compile stalls
    /// (`stats::bucket_index`).
    pub(crate) latency_us: [u64; 8],
    /// Probes that found a job still running.
    pub(crate) pending_probes: u64,
    /// Heat a function gained between its request and its install (the
    /// calls it interpreted meanwhile), over the installs that know it.
    pub(crate) interp_calls_while_pending: u64,
}

thread_local! {
    static STATS: Cell<BgStats> = Cell::new(BgStats::default());
    /// The class the compile in progress may defer under ([`DeferScope`]).
    static DEFER_SCOPE: Cell<Option<JobClass>> = const { Cell::new(None) };
    /// The job the compile in progress deferred, for its cache entry.
    static DEFERRED: RefCell<Option<DeferredCode>> = const { RefCell::new(None) };
    /// Pending jobs this thread's caches hold.
    static PENDING_COUNT: Cell<usize> = const { Cell::new(0) };
    /// The `compiled_id`s given a pending entry, for the drain (an id whose
    /// entry has moved on is dropped when the drain next looks).
    static PENDING_IDS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn bump_stats(f: impl FnOnce(&mut BgStats)) {
    let _ = STATS.try_with(|s| {
        let mut stats = s.get();
        f(&mut stats);
        s.set(stats);
    });
}

/// This thread's counters.
pub(crate) fn stats_snapshot() -> BgStats {
    STATS.with(Cell::get)
}

/// Pending jobs this thread's caches hold (the drain's fast exit).
pub(crate) fn pending_count() -> usize {
    PENDING_COUNT.with(Cell::get)
}

/// The ids that were given a pending entry since the last drain, handed to
/// the drain; `keep` says which still are (they go back on the list).
pub(crate) fn drain_pending_ids(mut visit: impl FnMut(u64) -> bool) {
    let ids = PENDING_IDS.with(|ids| std::mem::take(&mut *ids.borrow_mut()));
    let kept: Vec<u64> = ids.into_iter().filter(|&id| visit(id)).collect();
    PENDING_IDS.with(|ids| ids.borrow_mut().extend(kept));
}

/// Lets the compile inside it defer its backend under a class (see
/// [`defer_class`]); restores the enclosing scope on drop.
#[must_use = "the scope lasts until the guard drops"]
pub(crate) struct DeferScope(Option<JobClass>);

impl DeferScope {
    pub(crate) fn enter(class: Option<JobClass>) -> DeferScope {
        DeferScope(DEFER_SCOPE.with(|c| c.replace(class)))
    }
}

impl Drop for DeferScope {
    fn drop(&mut self) {
        DEFER_SCOPE.with(|c| c.set(self.0));
    }
}

/// The class the leaf being built may hand to a backend elsewhere, or
/// `None` to compile it in line. Taking it closes the scope: one compile
/// defers at most one leaf.
pub(crate) fn defer_class() -> Option<JobClass> {
    let class = DEFER_SCOPE.with(|c| c.take())?;
    deferred_install_for_test().then_some(class)
}

/// Record the job the compile in progress deferred (the sink's side).
pub(crate) fn stash_deferred(cell: Arc<JobCell>, class: JobClass, enqueued_at: Instant) {
    let code = DeferredCode {
        cell,
        class,
        enqueued_at,
    };
    bump_stats(|s| s.enqueued[class as usize] += 1);
    let previous = DEFERRED.with(|d| d.borrow_mut().replace(code));
    debug_assert!(previous.is_none(), "one deferred leaf per compile");
    if let Some(previous) = previous {
        previous.cell.cancel();
    }
}

/// Take the job the compile that just returned deferred, if it did (the
/// cache's side). A failed compile's job is cancelled.
pub(crate) fn take_deferred() -> Option<DeferredCode> {
    DEFERRED.with(|d| d.borrow_mut().take())
}

impl DeferredCode {
    /// The compile failed after its leaf was handed over: nobody will
    /// install the code.
    pub(crate) fn cancel(self) {
        self.cell.cancel();
    }
}

/// Run `backend` in line as if a backend elsewhere had: publish its result
/// into a fresh cell and hand the cell to the compile's cache entry, which
/// installs it at its next probe (the deferred-install test mode, and a
/// worker's fallback).
pub(crate) fn defer_in_line(
    class: JobClass,
    backend: impl FnOnce() -> Result<usize, CompileError>,
) {
    let cell = JobCell::new();
    let enqueued_at = Instant::now();
    #[cfg(test)]
    let result = if FAIL_BACKEND_TEST.with(|c| c.replace(false)) {
        Err(CompileError::Backend(super::backend::BackendError::Define(
            "forced by a test".into(),
        )))
    } else {
        backend()
    };
    #[cfg(not(test))]
    let result = backend();
    let asm = asm_dump::take_stashed();
    let out = BackendOut {
        result,
        backend_us: enqueued_at.elapsed().as_micros() as u64,
        queue_wait_us: 0,
        asm,
    };
    #[cfg(test)]
    if HOLD_PUBLISH_TEST.with(Cell::get) {
        HELD_TEST.with(|h| h.borrow_mut().push((Arc::clone(&cell), out)));
        stash_deferred(cell, class, enqueued_at);
        return;
    }
    cell.publish(out);
    stash_deferred(cell, class, enqueued_at);
}

#[cfg(test)]
thread_local! {
    static MODE_TEST_OVERRIDE: Cell<Option<BgMode>> = const { Cell::new(None) };
    static DEFERRED_INSTALL_TEST: Cell<bool> = const { Cell::new(false) };
    static FAIL_BACKEND_TEST: Cell<bool> = const { Cell::new(false) };
    static HOLD_PUBLISH_TEST: Cell<bool> = const { Cell::new(false) };
    static HELD_TEST: RefCell<Vec<(Arc<JobCell>, BackendOut)>> = const { RefCell::new(Vec::new()) };
}

/// Make the next in-line deferred backend on this thread fail (tests only).
#[cfg(test)]
pub(crate) fn fail_next_backend_for_test() {
    FAIL_BACKEND_TEST.with(|c| c.set(true));
}

/// Keep the results of this thread's in-line deferred backends unpublished
/// until [`publish_held_for_test`] (tests only): the jobs read as still
/// running.
#[cfg(test)]
pub(crate) fn hold_publish_for_test(on: bool) {
    HOLD_PUBLISH_TEST.with(|c| c.set(on));
}

/// Publish every result [`hold_publish_for_test`] held back (tests only).
#[cfg(test)]
pub(crate) fn publish_held_for_test() -> usize {
    let held = HELD_TEST.with(|h| std::mem::take(&mut *h.borrow_mut()));
    let n = held.len();
    for (cell, out) in held {
        cell.publish(out);
    }
    n
}

/// Force the mode for compiles on this thread (tests only); `None` returns
/// to the environment's.
#[cfg(test)]
pub(crate) fn force_mode_for_test(mode: Option<BgMode>) {
    MODE_TEST_OVERRIDE.with(|c| c.set(mode));
}

/// Deferred install without a worker (tests only): a deferrable split
/// compile on this thread runs its backend in line but leaves its leaf
/// pending until the next probe installs it (`BgTestMode::DeferredInstall`
/// in the design).
#[cfg(test)]
pub(crate) fn force_deferred_install_for_test(on: bool) {
    DEFERRED_INSTALL_TEST.with(|c| c.set(on));
}

fn deferred_install_for_test() -> bool {
    #[cfg(test)]
    {
        DEFERRED_INSTALL_TEST.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

#[cfg(test)]
#[path = "bg/tests/split_test.rs"]
mod split_tests;

#[cfg(test)]
#[path = "bg/tests/pending_test.rs"]
mod pending_tests;
