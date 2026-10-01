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
//! | `on` | `Threaded` | entry tier-ups, first-sight and OSR compiles hand their package to a worker thread (`bg::worker`); the function stays interpreted until its leaf is installed. Under `NEOVM_JIT_TIER2=on`, upgrades use a worker too, and the old native leaf keeps serving calls until install. Other re-tiers and the AOT drain run as under `sync`. x86-64 Linux; elsewhere `on` is `sync` |
//! | `auto` | `Threaded` or `Sync` | `on` when the process's affinity has at least two CPUs, else `sync` (a worker on the eval thread's only CPU just competes with it) |
//!
//! The companions -- worker count, queue caps, the classes that defer, worker
//! placement and the stress soak -- are in the knob table of `jit/mod.rs`.
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
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use super::RuntimeState;
use super::compile::shared::split::JobPayload;
use super::compile::{CompileError, CompiledLeaf};
use super::stats::CompileOrigin;
use super::stats::asm_dump::{self, PendingAsm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;

mod queue;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod worker;

/// How JIT compiles run (`NEOVM_JIT_BG`). Exhaustive matches only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum BgMode {
    /// The persistent per-thread module defines in place (the B4 path).
    Legacy,
    /// The front/backend split, run in line on the eval thread.
    Sync,
    /// The split, with tier-up backends on worker threads.
    #[strum(serialize = "on")]
    Threaded,
}

/// Whether this target can run backends on a worker: the code arena that
/// seals a worker's code and the install's instruction-stream serialization
/// are x86-64 Linux's.
pub(crate) const fn workers_supported() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
}

/// The mode a `NEOVM_JIT_BG` value selects; anything unrecognised (or
/// unset) is [`BgMode::Legacy`]. `auto` is `on` when this process may run
/// on at least two CPUs (its affinity), else `sync`: on one CPU a worker
/// only competes with the eval thread.
pub(crate) fn parse_mode(value: Option<&str>) -> BgMode {
    match value.map(str::trim) {
        Some("sync") => BgMode::Sync,
        Some("on") if workers_supported() => BgMode::Threaded,
        Some("on") => BgMode::Sync,
        Some("auto") if workers_supported() && available_cpus() >= 2 => BgMode::Threaded,
        Some("auto") => BgMode::Sync,
        _ => BgMode::Legacy,
    }
}

/// CPUs this process may run on (its affinity mask, on Linux).
fn available_cpus() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// A `NEOVM_JIT_BG_*` count, or `default` when unset or unreadable.
fn knob_count(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
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
        BgMode::Sync | BgMode::Threaded => true,
    }
}

/// Worker threads to start: `NEOVM_JIT_BG_THREADS` (default 1), at least
/// one and at most `min(4, CPUs - 1)`.
pub(crate) fn worker_threads() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        let most = available_cpus().saturating_sub(1).clamp(1, 4);
        knob_count("NEOVM_JIT_BG_THREADS", 1).clamp(1, most)
    })
}

/// Jobs the queue holds before it drops or refuses (`NEOVM_JIT_BG_QUEUE`,
/// default 256).
pub(crate) fn queue_cap() -> usize {
    #[cfg(test)]
    if let Some(cap) = QUEUE_CAP_TEST.with(Cell::get) {
        return cap;
    }
    static CAP: OnceLock<usize> = OnceLock::new();
    *CAP.get_or_init(|| knob_count("NEOVM_JIT_BG_QUEUE", 256).max(1))
}

/// CLIF instructions the queue holds before it drops or refuses
/// (`NEOVM_JIT_BG_QUEUE_INSTS`, default 1,000,000).
pub(crate) fn queue_insts_cap() -> u64 {
    static CAP: OnceLock<u64> = OnceLock::new();
    *CAP.get_or_init(|| knob_count("NEOVM_JIT_BG_QUEUE_INSTS", 1_000_000).max(1) as u64)
}

/// Pending compiles one eval thread may hold before it refuses more.
pub(crate) const PENDING_CAP: usize = 512;

/// `NEOVM_JIT_BG_NICE`: the workers' nice value (unset: the process's).
pub(crate) fn worker_nice() -> Option<i32> {
    std::env::var("NEOVM_JIT_BG_NICE")
        .ok()
        .and_then(|v| v.trim().parse().ok())
}

/// `NEOVM_JIT_BG_AFFINITY=<cpu>[,<cpu>...]`: pin the workers (measurement).
pub(crate) fn worker_affinity() -> Option<Vec<usize>> {
    let value = std::env::var("NEOVM_JIT_BG_AFFINITY").ok()?;
    let cpus: Vec<usize> = value
        .split(',')
        .filter_map(|c| c.trim().parse().ok())
        .collect();
    (!cpus.is_empty()).then_some(cpus)
}

/// `NEOVM_JIT_BG_CLASSES=<class>[,<class>...]` (`osr`, `first_sight`,
/// `entry`, `upgrade`; default all): the classes whose compiles may defer, the others
/// running in line (measurement: `entry` alone is P2.4 B7).
fn class_enabled(class: JobClass) -> bool {
    static MASK: OnceLock<u32> = OnceLock::new();
    let mask = *MASK.get_or_init(|| match std::env::var("NEOVM_JIT_BG_CLASSES") {
        Ok(value) => {
            use strum::IntoEnumIterator;
            JobClass::iter()
                .filter(|c| {
                    let name: &'static str = (*c).into();
                    value.split(',').any(|v| v.trim() == name)
                })
                .fold(0, |mask, c| mask | 1 << c as u32)
        }
        Err(_) => u32::MAX,
    });
    mask & 1 << class as u32 != 0
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
    /// An on-stack-replacement leaf: a hot loop interprets until it lands.
    Osr = 0,
    /// A speculated call site's first call into an uncompiled callee: native
    /// callers take the strict path until it lands.
    FirstSight = 1,
    /// A `dispatch_sized` tier-up, or the re-attempt after a deferral.
    Entry = 2,
    /// A tier-spine upgrade: its existing native leaf serves calls while
    /// the backend runs. Its priority follows interpreted entry compiles.
    Upgrade = 3,
}

impl JobClass {
    /// The class a compile of `origin` defers under, or `None` when that
    /// compile always runs in line. Countdown upgrades may defer only
    /// under the tier-spine knob; legacy heat re-tiers keep their path.
    pub(crate) fn for_origin(origin: CompileOrigin) -> Option<JobClass> {
        JobClass::for_origin_any(origin).filter(|class| class_enabled(*class))
    }

    fn for_origin_any(origin: CompileOrigin) -> Option<JobClass> {
        match origin {
            CompileOrigin::Dispatch | CompileOrigin::DeferralExpired => Some(JobClass::Entry),
            CompileOrigin::FirstSight => Some(JobClass::FirstSight),
            CompileOrigin::Osr => Some(JobClass::Osr),
            CompileOrigin::Retier if super::compile::jit_tier2().on => Some(JobClass::Upgrade),
            CompileOrigin::Retier | CompileOrigin::AotDrain | CompileOrigin::Direct => None,
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
    /// A guarded MIR inline armed at a function epoch that has moved: its
    /// inline-entry guards would deopt on every call. Requested again (at
    /// most [`MAX_EPOCH_DISCARDS`] times per function, then in line).
    EpochMoved,
    /// Its backend failed (the body becomes `NotCompilable`, as a failed
    /// in-line compile would).
    Failed,
    /// The full queue dropped it for a job of a higher class; its function
    /// asks again once its heat doubles.
    Dropped,
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
    /// Backend CPU time, excluding queue wait and scheduler preemption,
    /// with wall elapsed as a conservative fallback if the CPU clock is
    /// unavailable. Charged to the compile budget only for upgrades.
    pub(crate) backend_cpu_us: u64,
    /// From the front's hand-over to the backend's start.
    pub(crate) queue_wait_us: u64,
    /// The disassembly, under `NEOVM_JIT_DUMP_ASM`.
    pub(crate) asm: Option<PendingAsm>,
    /// The queue dropped the job before a backend ran it.
    pub(crate) dropped: bool,
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

    /// Publish that the full queue dropped the job unrun.
    pub(crate) fn publish_dropped(&self) {
        self.publish(BackendOut {
            result: Err(CompileError::Backend(super::backend::BackendError::Define(
                "dropped from the full background queue".into(),
            ))),
            backend_us: 0,
            backend_cpu_us: 0,
            queue_wait_us: 0,
            asm: None,
            dropped: true,
        });
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
    /// An inline backend ran inside the frontend CPU interval, so its
    /// cost is already charged by the mutator at compile return. This
    /// scalar remains mutator-owned alongside DeferredCode.
    backend_charged_in_front: bool,
    enqueued_at: Instant,
    /// The job's sequence number (the stress soak's seed).
    seq: u64,
    /// `(slot, symbol, expected function bits)` of the leaf's
    /// epoch-validated spec sites ([`note_spec_bindings`]).
    spec_bindings: Box<[(usize, SymId, u64)]>,
}

/// Epoch-moved discards of one function after which its compile runs in
/// line (the front then sees the live epoch).
pub(crate) const MAX_EPOCH_DISCARDS: u8 = 2;

/// Probes that find a job unfinished hold its function in the interpreter
/// for this many calls at first, doubling up to [`MAX_BACKOFF`].
pub(crate) const FIRST_BACKOFF: u32 = 32;
/// The longest hold between two probes of one pending job.
pub(crate) const MAX_BACKOFF: u32 = 1024;

/// A compile whose backend runs elsewhere, as its cache entry holds it
/// (see the module docs). Lives on the eval thread; only its
/// [`JobCell`] is shared. Each mutator owns its own cache and pending jobs;
/// workers see plain backend payloads and the Release/Acquire rendezvous,
/// never this structure or its Lisp state.
pub(crate) struct PendingJob {
    /// The function's `compiled_id`.
    id: u64,
    /// The front's leaf, entry still null. `None` once installed.
    leaf: Option<CompiledLeaf>,
    /// See [`DeferredCode::spec_bindings`].
    spec_bindings: Box<[(usize, SymId, u64)]>,
    cell: Arc<JobCell>,
    class: JobClass,
    /// The tagged heap the front compiled against.
    heap: Option<usize>,
    /// The function's tier-up deferral before this job's hold replaced it.
    saved_hold: u32,
    backoff: Cell<u32>,
    requested_heat: u32,
    /// The upgrade's source runtime, so an idle drain can restore its hold
    /// and disarm its leaf slot. Owned by this mutator's job; never sent
    /// to the worker, which only sees JobCell and plain backend payloads.
    upgrade_runtime: Option<Arc<RuntimeState>>,
    /// Reserved compile estimate for an upgrade cancelled before its
    /// backend result can be measured. Charging this on cancellation
    /// conservatively accounts for work already running on the worker.
    /// Completed jobs charge their measured backend CPU instead.
    cancel_estimate_us: u64,
    /// See DeferredCode::backend_charged_in_front. A ready inline backend
    /// must not be charged again at install or cancellation.
    backend_charged_in_front: bool,
    enqueued_at: Instant,
    /// Installed, or discarded with its reason counted.
    settled: Cell<bool>,
    /// Probes that must still find it running (`NEOVM_JIT_BG_STRESS`).
    stress_skips: Cell<u8>,
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
            id,
            leaf: Some(leaf),
            spec_bindings: code.spec_bindings,
            cell: code.cell,
            class: code.class,
            heap: crate::tagged::gc::current_tagged_heap_identity(),
            saved_hold,
            backoff: Cell::new(FIRST_BACKOFF.saturating_mul(2)),
            requested_heat: heat,
            upgrade_runtime: None,
            cancel_estimate_us: 0,
            backend_charged_in_front: code.backend_charged_in_front,
            enqueued_at: code.enqueued_at,
            settled: Cell::new(false),
            stress_skips: Cell::new(stress_probe_skips(code.seq)),
        })
    }

    /// The pending entry for an OSR leaf: no tier-up hold (the loop's own
    /// back-edge wraps pace the probes) and no drain list (the OSR cache
    /// drains its own).
    pub(crate) fn new_osr(id: u64, leaf: CompiledLeaf, code: DeferredCode) -> Box<PendingJob> {
        PENDING_COUNT.with(|c| c.set(c.get() + 1));
        Box::new(PendingJob {
            id,
            leaf: Some(leaf),
            spec_bindings: code.spec_bindings,
            cell: code.cell,
            class: code.class,
            heap: crate::tagged::gc::current_tagged_heap_identity(),
            saved_hold: 0,
            backoff: Cell::new(0),
            requested_heat: 0,
            upgrade_runtime: None,
            cancel_estimate_us: 0,
            backend_charged_in_front: code.backend_charged_in_front,
            enqueued_at: code.enqueued_at,
            settled: Cell::new(false),
            stress_skips: Cell::new(stress_probe_skips(code.seq)),
        })
    }

    /// An upgrade of a running native leaf. Unlike an entry compile, it
    /// leaves the runtime's hold unchanged: callers continue entering the
    /// old leaf. The mutator's drain list installs it even if calls stop.
    pub(crate) fn new_upgrade(
        id: u64,
        leaf: CompiledLeaf,
        code: DeferredCode,
        rt: &super::Runtime,
    ) -> Box<PendingJob> {
        let saved_hold = rt.deferred_heat();
        let mut job = Self::new_osr(id, leaf, code);
        job.saved_hold = saved_hold;
        job.requested_heat = rt.heat();
        job.upgrade_runtime = Some(rt.share_state());
        PENDING_IDS.with(|ids| ids.borrow_mut().push(id));
        job
    }

    /// An upgrade's runtime for cache install/drain, when the caller has
    /// no live function object. Entry and OSR jobs do not hold one here.
    pub(crate) fn upgrade_runtime(&self) -> Option<Arc<RuntimeState>> {
        self.upgrade_runtime.as_ref().map(Arc::clone)
    }

    /// The running T1's reservation, copied by the cache when the job
    /// becomes CompiledUpgrading. Plain resource accounting, no Lisp state.
    pub(crate) fn set_cancel_estimate(&mut self, estimate_us: u64) {
        self.cancel_estimate_us = estimate_us;
    }

    /// The front's leaf (entry null): its reloc constants are GC roots.
    pub(crate) fn leaf(&self) -> &CompiledLeaf {
        self.leaf.as_ref().expect("a pending job holds its leaf")
    }

    /// Whether the backend has published (install it now).
    pub(crate) fn is_ready(&self) -> bool {
        if !self.cell.is_done() {
            return false;
        }
        // NEOVM_JIT_BG_STRESS: a finished job still reads as running for a
        // few more probes.
        let skips = self.stress_skips.get();
        if skips > 0 {
            self.stress_skips.set(skips - 1);
            return false;
        }
        true
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
            if self.class == JobClass::Upgrade && !self.backend_charged_in_front {
                let cost = self
                    .cell
                    .take_out()
                    .map_or(self.cancel_estimate_us, |out| out.backend_cpu_us);
                super::tier2::charge_compile(std::time::Duration::from_micros(cost));
            }
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
/// drain without it lets the hold run out instead. `ctx` (the running
/// Context, when there is one) validates against the live obarray: a
/// guarded inline armed at a moved epoch is discarded, and a spec slot
/// whose binding held is re-stamped at the live epoch. Eval thread only;
/// no Lisp allocation, no safepoint.
#[cold]
#[inline(never)]
pub(crate) fn install(
    mut job: Box<PendingJob>,
    rt: Option<&RuntimeState>,
    ctx: Option<&Context>,
) -> Result<CompiledLeaf, Discard> {
    let out = job.cell.take_out().expect("install only a ready job");
    if job.class == JobClass::Upgrade && !job.backend_charged_in_front {
        super::tier2::charge_compile(std::time::Duration::from_micros(out.backend_cpu_us));
    }
    if out.dropped {
        // Asked again only once the heat doubled: bounds the thrash of a
        // backlog that keeps dropping it.
        if job.class != JobClass::Upgrade
            && let Some(rt) = rt
        {
            let heat = rt.heat();
            rt.defer_tier_up(heat.saturating_mul(2).max(heat.saturating_add(1)));
        }
        job.settle(Settled::Discarded(DiscardReason::Dropped));
        return Err(Discard::Stale(DiscardReason::Dropped));
    }
    if let Some(rt) = rt {
        rt.defer_tier_up(job.saved_hold);
        let calls = u64::from(rt.heat().saturating_sub(job.requested_heat));
        bump_stats(|s| {
            if job.class == JobClass::Upgrade {
                s.native_calls_while_pending += calls;
            } else {
                s.interp_calls_while_pending += calls;
            }
        });
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
    if let Some(ctx) = ctx {
        let epoch = ctx.obarray.function_epoch();
        let front = job.leaf();
        // A precise MIR leaf that inlined a callee guards every inline
        // entry against the epoch its front armed (`emit_mir_inline_entry_
        // guard`); once the epoch moved, every call would deopt there.
        if front.tier() == super::compile::LeafTier::Mir
            && front.has_side_effects
            && front.inline_epoch().is_some_and(|armed| armed != epoch)
        {
            let id = job.id;
            EPOCH_DISCARDS.with(|m| *m.borrow_mut().entry(id).or_default() += 1);
            job.settle(Settled::Discarded(DiscardReason::EpochMoved));
            return Err(Discard::Stale(DiscardReason::EpochMoved));
        }
        // The proven-fresh re-arm: a slot the front armed at an older epoch
        // whose binding still is the one it speculated on is current. The
        // same test `call_spec_slow` makes on the first call; a compiler
        // override (`cl-letf' of a subr) makes the subr shims refuse, so
        // nothing is re-stamped under one.
        if !job.spec_bindings.is_empty() && !ctx.compiler_function_overrides_active() {
            let mut restamped = 0u64;
            for &(slot, sym, expected) in job.spec_bindings.iter() {
                if ctx
                    .obarray
                    .symbol_function_id(sym)
                    .is_some_and(|binding| binding.bits() as u64 == expected)
                    && front.spec_slots[slot].restamp(epoch)
                {
                    restamped += 1;
                }
            }
            bump_stats(|s| s.spec_restamps += restamped);
        }
    }
    let _ = EPOCH_DISCARDS.try_with(|m| m.borrow_mut().remove(&job.id));
    let mut leaf = job.leaf.take().expect("a pending job holds its leaf");
    serialize_instruction_stream();
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
    /// Hot back-edge wraps that found their OSR leaf still compiling (and
    /// interpreted on without latching).
    pub(crate) osr_waits: u64,
    /// Heat a function gained between its request and its install (the
    /// calls it interpreted meanwhile), over the installs that know it.
    pub(crate) interp_calls_while_pending: u64,
    /// Calls the source gained while an upgrade's old native leaf remained
    /// available; counted at installs that know the live runtime.
    pub(crate) native_calls_while_pending: u64,
    /// Spec slots an install re-armed at the live epoch (their binding held
    /// while a redefinition elsewhere moved it).
    pub(crate) spec_restamps: u64,
    /// Compiles refused before their front (the queue or this thread's
    /// pending entries full): their function asks again once its heat
    /// doubles.
    pub(crate) refused: u64,
}

/// The `[neovm-jit-final-bg]` line: the eval thread's counters, the
/// workers' and what was still pending at exit.
#[derive(Clone, Debug, Default)]
pub(crate) struct BgReport {
    pub(crate) mode: &'static str,
    pub(crate) workers: usize,
    pub(crate) stats: BgStats,
    /// Pending entries this thread held at exit (their jobs are dropped).
    pub(crate) in_flight_at_exit: usize,
    pub(crate) worker_jobs: u64,
    pub(crate) worker_skipped: u64,
    pub(crate) worker_panics: u64,
    pub(crate) worker_code_bytes: u64,
    pub(crate) worker_backend_max_us: u64,
}

impl BgReport {
    /// This thread's report, or `None` under the legacy path (no line).
    pub(crate) fn collect() -> Option<BgReport> {
        let mode = mode();
        if mode == BgMode::Legacy {
            return None;
        }
        Some(BgReport {
            mode: mode.into(),
            workers: queue::pool().workers(),
            stats: stats_snapshot(),
            in_flight_at_exit: pending_count(),
            worker_jobs: WORKER_STATS.jobs.load(Ordering::Relaxed),
            worker_skipped: WORKER_STATS.skipped.load(Ordering::Relaxed),
            worker_panics: WORKER_STATS.panics.load(Ordering::Relaxed),
            worker_code_bytes: WORKER_STATS.code_bytes.load(Ordering::Relaxed),
            worker_backend_max_us: WORKER_STATS.backend_max_us.load(Ordering::Relaxed),
        })
    }

    pub(crate) fn render(&self) -> String {
        use strum::IntoEnumIterator;
        let by_class = |counts: &[u64]| {
            JobClass::iter()
                .map(|c| {
                    let name: &'static str = c.into();
                    format!("{name}:{}", counts[c as usize])
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let discarded = DiscardReason::iter()
            .map(|r| {
                let name: &'static str = r.into();
                format!("{name}:{}", self.stats.discarded[r as usize])
            })
            .collect::<Vec<_>>()
            .join(",");
        let latency = self
            .stats
            .latency_us
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "mode={} workers={} enqueued={} installed={} discarded={discarded} backend_us={} \
             backend_max_us={} queue_wait_us={} latency_hist_us[<100,<250,<500,<1ms,<2.5ms,<5ms,<10ms,>=10ms]={latency} \
             pending_probes={} osr_waits={} refused={} interp_calls_while_pending={} native_calls_while_pending={} spec_restamps={} in_flight_at_exit={} worker_jobs={} \
             worker_skipped={} worker_panics={} worker_code_bytes={} worker_backend_max_us={}",
            self.mode,
            self.workers,
            by_class(&self.stats.enqueued),
            by_class(&self.stats.installed),
            self.stats.backend_us,
            self.stats.backend_max_us,
            self.stats.queue_wait_us,
            self.stats.pending_probes,
            self.stats.osr_waits,
            self.stats.refused,
            self.stats.interp_calls_while_pending,
            self.stats.native_calls_while_pending,
            self.stats.spec_restamps,
            self.in_flight_at_exit,
            self.worker_jobs,
            self.worker_skipped,
            self.worker_panics,
            self.worker_code_bytes,
            self.worker_backend_max_us,
        )
    }
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
    /// Epoch-moved discards per `compiled_id` since its last install.
    static EPOCH_DISCARDS: RefCell<rustc_hash::FxHashMap<u64, u8>> =
        RefCell::new(rustc_hash::FxHashMap::default());
}

/// Whether a compile of `class` must be refused before its front runs:
/// this thread already holds [`PENDING_CAP`] pending compiles, or the full
/// queue would drop the job itself (it is the lowest class queued). Only
/// with worker threads; a refusal is counted.
pub(crate) fn refuses(class: JobClass) -> bool {
    let refuse = match mode() {
        BgMode::Legacy | BgMode::Sync => false,
        BgMode::Threaded => pending_count() >= PENDING_CAP || !queue::pool().admits(class),
    };
    if refuse {
        bump_stats(|s| s.refused += 1);
    }
    refuse
}

/// Whether a compile of `id` may defer: not after [`MAX_EPOCH_DISCARDS`]
/// epoch-moved discards, which a compile in line cannot repeat.
pub(crate) fn may_defer(id: u64) -> bool {
    EPOCH_DISCARDS.with(|m| m.borrow().get(&id).copied().unwrap_or(0) < MAX_EPOCH_DISCARDS)
}

/// Attach the leaf's epoch-validated spec bindings to the job its compile
/// just deferred (the leaf builders, when their define was deferred).
pub(crate) fn note_spec_bindings(bindings: impl Iterator<Item = (usize, SymId, u64)>) {
    DEFERRED.with(|d| {
        if let Some(code) = d.borrow_mut().as_mut() {
            code.spec_bindings = bindings.collect();
        }
    });
}

fn bump_stats(f: impl FnOnce(&mut BgStats)) {
    let _ = STATS.try_with(|s| {
        let mut stats = s.get();
        f(&mut stats);
        s.set(stats);
    });
}

/// A hot back-edge found its OSR leaf still compiling.
pub(crate) fn note_osr_wait() {
    bump_stats(|s| s.osr_waits += 1);
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
/// [`defer_route`]); restores the enclosing scope on drop.
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

/// Where a deferred leaf's backend runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeferRoute {
    /// A worker thread ([`enqueue`]).
    Worker,
    /// In line, the install still waiting for the next probe (the
    /// deferred-install test mode).
    InLine,
}

/// The class and route the leaf being built may hand its backend under, or
/// `None` to compile it in line and install it at once. Taking it closes
/// the scope: one compile defers at most one leaf.
pub(crate) fn defer_route() -> Option<(JobClass, DeferRoute)> {
    let class = DEFER_SCOPE.with(|c| c.take())?;
    if deferred_install_for_test() {
        return Some((class, DeferRoute::InLine));
    }
    match mode() {
        BgMode::Legacy | BgMode::Sync => None,
        BgMode::Threaded => Some((class, DeferRoute::Worker)),
    }
}

/// Start worker `index` for `pool` (x86-64 Linux only; see
/// [`workers_supported`]).
fn spawn_worker(index: usize, pool: &'static queue::Pool) -> std::io::Result<()> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return worker::spawn(index, pool);
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (index, pool);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "background JIT backends need x86-64 Linux",
        ))
    }
}

/// Process-unique job sequence numbers (FIFO order within a class).
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Hand `payload` to a worker under `class` and record the job for the
/// compile's cache entry. `Err` gives the payload back when no worker can
/// run it: the caller compiles it in line.
pub(crate) fn enqueue(class: JobClass, payload: JobPayload) -> Result<(), JobPayload> {
    let cell = JobCell::new();
    let enqueued_at = Instant::now();
    let insts = payload.func.dfg.num_insts() as u64;
    let seq = NEXT_SEQ.fetch_add(1, Ordering::Relaxed);
    let job = queue::BackendJob {
        payload,
        class,
        seq,
        enqueued_at,
        cell: Arc::clone(&cell),
        insts,
    };
    match queue::pool().push(job) {
        Ok(()) => {
            stash_deferred(cell, class, enqueued_at, seq, false);
            Ok(())
        }
        Err(job) => Err(job.payload),
    }
}

/// Counters the worker threads keep (process-wide).
pub(crate) struct WorkerStats {
    /// Jobs compiled (or failed).
    pub(crate) jobs: AtomicU64,
    /// Jobs skipped because they were cancelled before they started.
    pub(crate) skipped: AtomicU64,
    /// Backend panics contained.
    pub(crate) panics: AtomicU64,
    /// Machine code bytes produced.
    pub(crate) code_bytes: AtomicU64,
    /// The longest backend run, µs.
    pub(crate) backend_max_us: AtomicU64,
}

pub(crate) static WORKER_STATS: WorkerStats = WorkerStats {
    jobs: AtomicU64::new(0),
    skipped: AtomicU64::new(0),
    panics: AtomicU64::new(0),
    code_bytes: AtomicU64::new(0),
    backend_max_us: AtomicU64::new(0),
};

/// A test asked the next worker job to panic.
static FORCED_PANIC: AtomicBool = AtomicBool::new(false);

/// Whether the backend job starting now must panic (consumes the request;
/// only a test makes one).
pub(crate) fn take_forced_panic() -> bool {
    FORCED_PANIC.swap(false, Ordering::Relaxed)
}

/// Make the next job a worker starts panic inside its backend (tests).
#[cfg(test)]
pub(crate) fn force_backend_panic_for_test() {
    FORCED_PANIC.store(true, Ordering::Relaxed);
}

/// Wait until no job is queued or running (tests); whether it got there
/// within `timeout`.
#[cfg(test)]
pub(crate) fn quiesce_for_test(timeout: std::time::Duration) -> bool {
    queue::pool().quiesce(timeout)
}

/// Keep the workers from taking jobs until the guard drops (tests): jobs
/// queue up, so a test can act while they are known not to have started.
#[cfg(test)]
pub(crate) fn hold_workers_for_test() -> WorkerHold {
    queue::pool().set_held(true);
    WorkerHold(())
}

#[cfg(test)]
pub(crate) struct WorkerHold(());

#[cfg(test)]
impl Drop for WorkerHold {
    fn drop(&mut self) {
        queue::pool().set_held(false);
    }
}

/// Make code another core wrote visible to this core's instruction fetch:
/// the backend sealed it and published with a release store that this
/// thread read with an acquire load; a serializing instruction before the
/// first execution completes the cross-modifying-code protocol (Intel SDM
/// vol. 3 §8.1.3; Cranelift's own flush is a no-op on x86-64).
#[inline(never)]
fn serialize_instruction_stream() {
    // `cpuid` (leaf 0) is architecturally serializing and has no other
    // effect.
    #[cfg(target_arch = "x86_64")]
    let _ = core::arch::x86_64::__cpuid(0);
}

/// Record the job the compile in progress deferred (the sink's side).
fn stash_deferred(
    cell: Arc<JobCell>,
    class: JobClass,
    enqueued_at: Instant,
    seq: u64,
    backend_charged_in_front: bool,
) {
    let code = DeferredCode {
        cell,
        class,
        backend_charged_in_front,
        enqueued_at,
        seq,
        spec_bindings: Box::default(),
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
    let seq = NEXT_SEQ.fetch_add(1, Ordering::Relaxed);
    let cpu_started = (class == JobClass::Upgrade).then(super::tier2::cpu_time_us);
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
    let backend_us = enqueued_at.elapsed().as_micros() as u64;
    let out = BackendOut {
        result,
        backend_us,
        backend_cpu_us: cpu_started.map_or(0, |started| {
            let finished = super::tier2::cpu_time_us();
            if started == 0 || finished == 0 {
                backend_us
            } else {
                finished.saturating_sub(started)
            }
        }),
        queue_wait_us: 0,
        asm,
        dropped: false,
    };
    #[cfg(test)]
    if HOLD_PUBLISH_TEST.with(Cell::get) {
        HELD_TEST.with(|h| h.borrow_mut().push((Arc::clone(&cell), out)));
        stash_deferred(cell, class, enqueued_at, seq, true);
        return;
    }
    cell.publish(out);
    stash_deferred(cell, class, enqueued_at, seq, true);
}

/// `NEOVM_JIT_BG_STRESS=1`: drive every interleaving of the soak -- each
/// worker job starts 0-2 ms late, and each finished job is found running
/// by 1-4 more probes, from a seeded mix of its sequence number. Read once
/// (tests override it process-wide).
pub(crate) fn stress_enabled() -> bool {
    match STRESS_TEST.load(Ordering::Relaxed) {
        1 => return false,
        2 => return true,
        _ => {}
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = matches!(
            std::env::var("NEOVM_JIT_BG_STRESS").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        );
        if on {
            tracing::info!(target: "neovm::jit::knobs", "NEOVM_JIT_BG_STRESS=1 is on in this process");
        }
        on
    })
}

/// Test override of [`stress_enabled`]: 0 = the knob, 1 = off, 2 = on.
static STRESS_TEST: AtomicU8 = AtomicU8::new(0);

/// Force the stress soak on or off for the whole process (tests).
#[cfg(test)]
pub(crate) fn force_stress_for_test(on: Option<bool>) {
    STRESS_TEST.store(
        match on {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        },
        Ordering::Relaxed,
    );
}

/// SplitMix64: a well-mixed word from a job's sequence number.
pub(crate) fn stress_mix(seq: u64) -> u64 {
    let mut z = seq.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Probes a finished job of `seq` still reads as running under stress.
fn stress_probe_skips(seq: u64) -> u8 {
    if stress_enabled() {
        1 + (stress_mix(seq) % 4) as u8
    } else {
        0
    }
}

#[cfg(test)]
thread_local! {
    static MODE_TEST_OVERRIDE: Cell<Option<BgMode>> = const { Cell::new(None) };
    static DEFERRED_INSTALL_TEST: Cell<bool> = const { Cell::new(false) };
    static FAIL_BACKEND_TEST: Cell<bool> = const { Cell::new(false) };
    static QUEUE_CAP_TEST: Cell<Option<usize>> = const { Cell::new(None) };
    static HOLD_PUBLISH_TEST: Cell<bool> = const { Cell::new(false) };
    static HELD_TEST: RefCell<Vec<(Arc<JobCell>, BackendOut)>> = const { RefCell::new(Vec::new()) };
}

/// Jobs the workers took, in order: `(class, seq, cancelled)` (tests).
#[cfg(test)]
static SERVED_TEST: Mutex<Vec<(JobClass, u64, bool)>> = Mutex::new(Vec::new());

#[cfg(test)]
pub(crate) fn note_served_for_test(class: JobClass, seq: u64, cancelled: bool) {
    SERVED_TEST
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push((class, seq, cancelled));
}

/// Take the order the workers took jobs in (tests).
#[cfg(test)]
pub(crate) fn take_served_for_test() -> Vec<(JobClass, u64, bool)> {
    std::mem::take(&mut *SERVED_TEST.lock().unwrap_or_else(|p| p.into_inner()))
}

/// Cap the queue at `cap` jobs for pushes and admissions from this thread
/// (tests only); `None` returns to the knob.
#[cfg(test)]
pub(crate) fn force_queue_cap_for_test(cap: Option<usize>) {
    QUEUE_CAP_TEST.with(|c| c.set(cap));
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
/// to the environment's. A forced mode also pins `NEOVM_JIT_BG_STRESS` off
/// (unless a test forced it itself), so a test's installs happen where it
/// expects them whatever soak the environment runs.
#[cfg(test)]
pub(crate) fn force_mode_for_test(mode: Option<BgMode>) {
    MODE_TEST_OVERRIDE.with(|c| c.set(mode));
    let (from, to) = if mode.is_some() { (0, 1) } else { (1, 0) };
    let _ = STRESS_TEST.compare_exchange(from, to, Ordering::Relaxed, Ordering::Relaxed);
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

#[cfg(test)]
#[path = "bg/tests/invalidation_test.rs"]
mod invalidation_tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "bg/tests/worker_test.rs"]
mod worker_tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "bg/tests/first_sight_test.rs"]
mod first_sight_tests;

#[cfg(test)]
#[path = "bg/tests/osr_pending_test.rs"]
mod osr_pending_tests;

#[cfg(test)]
#[path = "bg/tests/osr_best_test.rs"]
mod osr_best_tests;

#[cfg(test)]
#[path = "bg/tests/upgrade_test.rs"]
mod upgrade_tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "bg/tests/queue_test.rs"]
mod queue_tests;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "bg/tests/soak_test.rs"]
mod soak_tests;
