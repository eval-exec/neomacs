//! Compile origins and the per-phase split of every compile stall (P0.6 L0,
//! P2.4 B0).
//!
//! Every compile is timed as a whole ([`super::CompileStats::total_us`]);
//! this module says WHY it ran ([`CompileOrigin`]) and WHERE its time went
//! ([`CompilePhase`]): the front (gates, MIR, fuser, CLIF construction) that
//! must stay on the eval thread, and the backend (module setup, Cranelift
//! codegen, finalize) that a persistent module, a code arena or a background
//! worker can shrink or move. The split is what gates those levers: a
//! per-compile fixed cost that does not fall shows up here as the phase that
//! kept it.
//!
//! Cost: the origin row is three adds per compile (always on, like the other
//! compile aggregates). The phase split takes two `Instant` reads per phase
//! switch, and only under [`super::summary_enabled`]
//! (`NEOVM_JIT_COMPILE_STATS=1` or `NEOVM_JIT_STATS_FILE`): with the knob off
//! a phase guard is one relaxed load and a branch, and no clock is read.
//!
//! Attribution is exclusive: entering a phase pauses the one it interrupts,
//! and leaving it resumes that one, so the phases of one compile sum exactly
//! to its stall ([`CompilePhase::Other`] takes the time no guard claimed).

use std::cell::Cell;
use std::time::{Duration, Instant};

use strum::EnumCount;

/// Why a compile ran. Indexes [`super::CompileStats::origins`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum CompileOrigin {
    /// `dispatch_sized` found the body hot (`cache::try_run_compiled`).
    Dispatch,
    /// A profitability deferral ran out: the gate-bypassed re-attempt.
    DeferralExpired,
    /// A fast-allocator leaf rebuilt with the full allocator once hot.
    Retier,
    /// A speculated call site's first call into an uncompiled callee
    /// (`cache::resolve_compiled_leaf_ptr`, no heat check).
    FirstSight,
    /// An on-stack-replacement entry at a hot loop header.
    Osr,
    /// The AOT-PGO drain staging a JIT leaf (`cache::compile_and_cache_jit_leaf`).
    AotDrain,
    /// A compile outside the cache (`compile::compile_bytecode_function_with`).
    Direct,
}

/// Where a compile's time went. Indexes [`super::CompileStats::phase_ns`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum CompilePhase {
    /// Body scans and admission gates: call-heavy and self-recursion
    /// verdicts, feedback publication, profitability, OSR eligibility.
    Gate,
    /// MIR build, the pure-callee inliner and the tier plan.
    MirBuild,
    /// The bytecode fuser (`inline::fuse_calls`).
    Fuse,
    /// Baseline analyses (CFG, known-fixnum facts, spec sites, relocs) and
    /// CLIF construction in either tier.
    Lower,
    /// JIT module, ISA and shim-table setup.
    Setup,
    /// `define_function`: Cranelift's passes, instruction selection, register
    /// allocation, emission and the copy into code memory.
    Codegen,
    /// `finalize_definitions`: relocations and page protection.
    Finalize,
    /// Everything no guard claimed.
    Other,
}

/// One origin's compile count, successes and stall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OriginStats {
    pub(crate) count: u64,
    pub(crate) ok: u64,
    pub(crate) us: u64,
}

/// The running split of the compile in progress.
#[derive(Clone, Copy, Debug)]
struct ClockState {
    current: CompilePhase,
    since: Instant,
    acc_ns: [u64; CompilePhase::COUNT],
}

impl ClockState {
    /// Charge the time since the last switch to the current phase and make
    /// `next` current.
    fn switch(&mut self, now: Instant, next: CompilePhase) -> CompilePhase {
        self.acc_ns[self.current as usize] +=
            now.saturating_duration_since(self.since).as_nanos() as u64;
        self.since = now;
        std::mem::replace(&mut self.current, next)
    }
}

thread_local! {
    /// The split of the compile in progress on this thread; `None` outside a
    /// compile or when the split is off.
    static PHASE_CLOCK: Cell<Option<ClockState>> = const { Cell::new(None) };
}

/// Charges the time until it drops to one phase, then resumes the phase it
/// interrupted. Inert when no split is running.
#[must_use = "a phase lasts until its guard drops"]
pub(crate) struct PhaseGuard {
    prev: Option<CompilePhase>,
}

impl PhaseGuard {
    const INERT: PhaseGuard = PhaseGuard { prev: None };
}

/// Attribute the time until the returned guard drops to `phase`.
#[inline]
pub(crate) fn enter_phase(phase: CompilePhase) -> PhaseGuard {
    if !super::summary_enabled() {
        return PhaseGuard::INERT;
    }
    enter_phase_tracked(phase)
}

#[inline(never)]
fn enter_phase_tracked(phase: CompilePhase) -> PhaseGuard {
    PHASE_CLOCK.with(|c| match c.get() {
        Some(mut state) => {
            let prev = state.switch(Instant::now(), phase);
            c.set(Some(state));
            PhaseGuard { prev: Some(prev) }
        }
        None => PhaseGuard::INERT,
    })
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        let Some(prev) = self.prev else {
            return;
        };
        PHASE_CLOCK.with(|c| {
            if let Some(mut state) = c.get() {
                state.switch(Instant::now(), prev);
                c.set(Some(state));
            }
        });
    }
}

/// Times one compile from start to [`CompileClock::finish`], and runs its
/// phase split when the split is on. A compile nested inside another (not
/// expected: compiles run under the cache borrow) saves and restores the
/// outer split.
pub(crate) struct CompileClock {
    origin: CompileOrigin,
    started: Instant,
    tracking: bool,
    outer: Option<ClockState>,
    finished: bool,
}

impl CompileClock {
    /// Start timing a compile of `origin`.
    pub(crate) fn start(origin: CompileOrigin) -> CompileClock {
        let started = Instant::now();
        let tracking = super::summary_enabled();
        let outer = if tracking {
            PHASE_CLOCK.with(|c| {
                c.replace(Some(ClockState {
                    current: CompilePhase::Other,
                    since: started,
                    acc_ns: [0; CompilePhase::COUNT],
                }))
            })
        } else {
            None
        };
        CompileClock {
            origin,
            started,
            tracking,
            outer,
            finished: false,
        }
    }

    /// Stop the clock: fold this compile's origin row (and its phase split,
    /// when tracked) into the thread's aggregates, and return the stall.
    pub(crate) fn finish(mut self, ok: bool) -> Duration {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.started);
        let split = if self.tracking {
            PHASE_CLOCK.with(|c| {
                let state = c.replace(self.outer.take());
                state.map(|mut state| {
                    state.switch(now, CompilePhase::Other);
                    state.acc_ns
                })
            })
        } else {
            None
        };
        self.finished = true;
        super::record_origin_and_phases(self.origin, ok, elapsed, split.as_ref());
        elapsed
    }
}

impl Drop for CompileClock {
    fn drop(&mut self) {
        // An unwinding compile (a contained panic) must not leave its split
        // installed for the next compile to fold in.
        if !self.finished && self.tracking {
            PHASE_CLOCK.with(|c| c.set(self.outer.take()));
        }
    }
}

/// Render `acc` (ns per phase) as `phase=µs` pairs in phase order.
pub(crate) fn render_phase_us(acc_ns: &[u64; CompilePhase::COUNT]) -> String {
    use strum::IntoEnumIterator;
    CompilePhase::iter()
        .map(|p| {
            let name: &'static str = p.into();
            format!("{name}={}", acc_ns[p as usize] / 1_000)
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Render the origin rows as `origin=count/ok/µs`, skipping empty ones.
pub(crate) fn render_origins(origins: &[OriginStats; CompileOrigin::COUNT]) -> String {
    use strum::IntoEnumIterator;
    let rows: Vec<String> = CompileOrigin::iter()
        .filter(|o| origins[*o as usize].count > 0)
        .map(|o| {
            let name: &'static str = o.into();
            let s = origins[o as usize];
            format!("{name}={}/{}/{}", s.count, s.ok, s.us)
        })
        .collect();
    if rows.is_empty() {
        "-".to_string()
    } else {
        rows.join(",")
    }
}
