//! The tier spine's countdown and cold C7/C9 policy (P2.1 C6/C7/C9 + W1).
//!
//! A T1 leaf under `NEOVM_JIT_TIER2=on` has a per-leaf budget: entry takes
//! one, an entered cold back-edge poll takes `T2_LOOP_CREDIT`, and a mapping
//! builtin's profiling shim takes `len / 64` only during the profile window.
//! The existing 255-taken-back-edge quit-poll cadence and its hot code stay
//! unchanged. Optional lazy-table imports call the cold request without Lisp
//! allocation or a safepoint; the current activation continues in T1.
//!
//! The first request captures feedback and requires one stable work window.
//! Later requests compare exact lattice snapshots, reserve the compile budget,
//! and choose full-allocator T1' or the existing backend with feedback. Upgrade
//! jobs retain the working T1 until installation. A feedback upgrade retains T1
//! as its fallback; a conclusive/repeated T2 deopt widens the existing source
//! policy before reverting to T1. The bounded source ban survives cache evictions.
//! Off, profiling emission and the heat-driven re-tier stay unchanged.
//!
//! # Threading
//!
//! Leaves and their plain `Cell`/`RefCell` policy state belong to one mutator.
//! The scoped compile publication is per compiler thread, never a cache of
//! Lisp values, and is restored after nested compiles. Shared source feedback
//! and bans use their existing atomic accessors; exact snapshots are conservative
//! observations, not transactional reads. All generated type/epoch/source guards
//! remain in force if another mutator widens feedback. Per-mutator budget TLS
//! contains resource counters only, and workers return CPU costs to their owner.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use super::RuntimeState;
use super::compile::{CompiledLeaf, LeafObs, Tier2Knob, jit_tier2};

/// What a second compile of a source produces.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum T2Upgrade {
    /// T1': the same body rebuilt with the full register allocator, with
    /// no profiling code (the former heat-driven re-tier of a
    /// fast-allocator leaf).
    Retier,
    /// T2: the existing backend consumes the source's stable feedback until
    /// the optimizing IR is available. Its T1 leaf remains available to C9.
    Feedback,
}

/// What a compile is in the tier spine (a [`super::compile::CompileRequest`]
/// field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompileTier {
    /// A tier-up's first compile of a source: a T1 leaf, which profiles
    /// under `NEOVM_JIT_TIER2`.
    T1,
    /// A second compile the tier spine decided.
    Upgrade(T2Upgrade),
    /// Outside the spine (a direct compile, the AOT drain): never profiles.
    Plain,
}

/// How a leaf came to be, for the tier spine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum T2Origin {
    /// No countdown: the knob was off, or an OSR, AOT or direct compile.
    Unprofiled,
    /// A T1 entry leaf with the countdown.
    Profiling,
    /// Built by an upgrade; carries no profiling code.
    Upgrade(T2Upgrade),
}

/// What a profiling leaf's request decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum T2State {
    /// No request yet, or no countdown at all.
    Idle,
    /// Kept as it is: nothing better to compile it to (yet).
    Kept,
    /// The request found the leaf no longer its source's current one
    /// (retired, or built outside the cache): nothing to upgrade.
    Stale,
    /// The upgrade is due: the next entry through a seam compiles it.
    Due(T2Upgrade),
    /// The upgrade replaced this leaf.
    Upgraded(T2Upgrade),
}

/// A disarmed countdown: no entry, poll or credit can bring it to zero.
pub(crate) const DISARMED: i64 = i64::MAX;

/// Divisor of the mapping-builtin credit: `len / 64` per call, the
/// interpreter's loop weighting of 256 iterations per call scaled to the
/// poll credit's 64 per 255 back edges.
pub(crate) const HOF_CREDIT_DIVISOR: usize = 64;

/// One leaf's tier-spine state (see the module docs). Lives in the leaf's
/// boxed [`LeafObs`], so `budget` has a stable address the generated code
/// bakes.
#[derive(Debug)]
pub(crate) struct T2Cells {
    /// What produced the leaf.
    pub(crate) origin: T2Origin,
    /// The countdown: the prologue takes 1 per entry, the poll the loop
    /// credit per tick, a mapping builtin `len / 64`; at or below zero the
    /// leaf requests. Signed so a credit may overshoot zero. [`DISARMED`]
    /// for a leaf without a countdown and after the request.
    pub(crate) budget: Cell<i64>,
    /// Back-edge poll ticks, counted (with `LeafObs::entries`) when entry
    /// counting was on at compile time: the work a loop leaf does.
    pub(crate) polls: Cell<u64>,
    /// What the request decided.
    pub(crate) state: Cell<T2State>,
    /// `(entries, polls)` when the request fired.
    pub(crate) at_request: Cell<(u64, u64)>,
    /// The source whose interpreter leaf slot an upgrade request disarms;
    /// retained across unstable and deferred windows; never holds a Lisp value.
    pub(crate) source: RefCell<Option<Arc<RuntimeState>>>,
    /// Cold policy state owned by this leaf's mutator, never sent to a worker.
    pub(crate) policy: RefCell<policy::PolicyState>,
    /// CPU budget reserved for its in-flight upgrade; charged/released by the
    /// owning mutator at installation, failure or cancellation.
    pub(crate) reserved_us: Cell<u64>,
}

impl T2Cells {
    /// A leaf without a countdown.
    pub(crate) fn unprofiled() -> Self {
        Self::with_origin(T2Origin::Unprofiled, DISARMED, None)
    }

    fn with_origin(origin: T2Origin, budget: i64, source: Option<Arc<RuntimeState>>) -> Self {
        T2Cells {
            origin,
            budget: Cell::new(budget),
            polls: Cell::new(0),
            state: Cell::new(T2State::Idle),
            at_request: Cell::new((0, 0)),
            source: RefCell::new(source),
            policy: RefCell::new(policy::PolicyState::default()),
            reserved_us: Cell::new(0),
        }
    }

    /// Whether the generated code takes from the countdown.
    pub(crate) fn profiling(&self) -> bool {
        self.origin == T2Origin::Profiling
    }

    /// The upgrade a request decided and no seam has compiled yet.
    #[inline]
    pub(crate) fn due(&self) -> Option<T2Upgrade> {
        match self.state.get() {
            T2State::Due(kind) => Some(kind),
            _ => None,
        }
    }

    /// A report-time copy.
    pub(crate) fn snapshot(&self) -> T2Snapshot {
        let (entries_at_request, polls_at_request) = self.at_request.get();
        T2Snapshot {
            origin: self.origin.into(),
            state: self.state.get().into(),
            polls: self.polls.get(),
            requested: !matches!(self.state.get(), T2State::Idle),
            entries_at_request,
            polls_at_request,
            upgraded_leaf: matches!(self.origin, T2Origin::Upgrade(_)),
        }
    }
}

/// A copy of one leaf's [`T2Cells`] for the exit report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct T2Snapshot {
    pub(crate) origin: &'static str,
    pub(crate) state: &'static str,
    pub(crate) polls: u64,
    /// Whether the leaf's request fired.
    pub(crate) requested: bool,
    pub(crate) entries_at_request: u64,
    pub(crate) polls_at_request: u64,
    /// Whether an upgrade built this leaf.
    pub(crate) upgraded_leaf: bool,
}

// ---------------------------------------------------------------------------
// The compile in progress.
// ---------------------------------------------------------------------------

/// What the leaf builders of the compile in progress give their leaf.
#[derive(Clone)]
struct ActiveBuild {
    origin: T2Origin,
    source: Option<Arc<RuntimeState>>,
    window: u32,
    ops_len: usize,
    loop_or_recursive: bool,
}

thread_local! {
    /// The tier-spine role of the compile in progress (scoped by
    /// [`BuildScope`]; `None` outside a cache compile, which builds
    /// unprofiled leaves).
    static ACTIVE_BUILD: RefCell<Option<ActiveBuild>> = const { RefCell::new(None) };
}

/// Publishes a compile's tier-spine role for its leaf builders, restoring
/// the outer compile's on drop.
pub(crate) struct BuildScope(Option<ActiveBuild>);

impl BuildScope {
    /// The scope of a compile of `source` as `tier`: a T1 compile under
    /// `NEOVM_JIT_TIER2` builds a profiling leaf, an upgrade an upgraded
    /// one, anything else an unprofiled one.
    pub(crate) fn enter(tier: CompileTier, source: &super::Runtime) -> Self {
        Self::enter_for(tier, source, 0, false)
    }

    /// The cache compile supplies its executable body shape for the cold
    /// worth-it test. `enter` remains available to runtime-free emission tests.
    pub(crate) fn enter_for(
        tier: CompileTier,
        source: &super::Runtime,
        ops_len: usize,
        loop_or_recursive: bool,
    ) -> Self {
        let knob = jit_tier2();
        let build = match tier {
            CompileTier::T1 if knob.on => Some(ActiveBuild {
                origin: T2Origin::Profiling,
                source: Some(source.share_state()),
                window: knob.window,
                ops_len,
                loop_or_recursive,
            }),
            CompileTier::Upgrade(kind) => Some(ActiveBuild {
                origin: T2Origin::Upgrade(kind),
                source: None,
                window: 0,
                ops_len,
                loop_or_recursive,
            }),
            CompileTier::T1 | CompileTier::Plain => None,
        };
        BuildScope(ACTIVE_BUILD.with(|a| std::mem::replace(&mut *a.borrow_mut(), build)))
    }
}

impl Drop for BuildScope {
    fn drop(&mut self) {
        let outer = self.0.take();
        ACTIVE_BUILD.with(|a| *a.borrow_mut() = outer);
    }
}

/// The tier-spine cells for a whole-function JIT leaf built now (never an
/// OSR or AOT leaf: their builders do not ask).
pub(crate) fn cells_for_build() -> T2Cells {
    ACTIVE_BUILD.with(|a| match a.borrow().as_ref() {
        Some(build) => match build.origin {
            T2Origin::Profiling => {
                let cells = T2Cells::with_origin(
                    T2Origin::Profiling,
                    i64::from(build.window),
                    build.source.clone(),
                );
                {
                    let mut policy = cells.policy.borrow_mut();
                    policy.ops_len = build.ops_len;
                    policy.loop_or_recursive = build.loop_or_recursive;
                }
                cells
            }
            origin => T2Cells::with_origin(origin, DISARMED, None),
        },
        None => T2Cells::unprofiled(),
    })
}

/// What the generated code of a profiling leaf bakes: the countdown's
/// address, the leaf's `LeafObs` (the request's argument) and the poll
/// credit. Plain addresses: JIT code only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct T2Emit {
    pub(crate) budget: usize,
    pub(crate) obs: usize,
    pub(crate) loop_credit: u32,
}

impl T2Emit {
    /// The emission for `obs`'s leaf, when it profiles.
    pub(crate) fn of(obs: &LeafObs) -> Option<Self> {
        obs.t2.profiling().then(|| T2Emit {
            budget: obs.t2.budget.as_ptr() as usize,
            obs: std::ptr::from_ref(obs) as usize,
            loop_credit: jit_tier2().loop_credit,
        })
    }
}

// ---------------------------------------------------------------------------
// The request.
// ---------------------------------------------------------------------------

/// What [`decide`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum T2Decision {
    /// Keep the leaf, its countdown disarmed.
    Keep,
    /// Compile this upgrade at the next seam entry.
    Upgrade(T2Upgrade),
}

/// Existing allocator-upgrade admissibility, used as the C7 policy's
/// fallback decision. A fast leaf that is not call-heavy uses the full
/// allocator (the old re-tier set, with its two vetoes: a forced allocator
/// would rebuild `Fast` forever, and `NEOVM_JIT_RETIER_FACTOR=0` turns the
/// re-tier off). Other leaves need the feedback policy's separate admission.
pub(crate) fn decide(leaf: &CompiledLeaf) -> T2Decision {
    use super::compile::lowering::{RegallocChoice, forced_regalloc};
    if leaf.regalloc == RegallocChoice::Fast
        && !leaf.call_heavy
        && forced_regalloc().is_none()
        && super::retier_factor() != 0
    {
        T2Decision::Upgrade(T2Upgrade::Retier)
    } else {
        T2Decision::Keep
    }
}

/// The request a profiling leaf makes when its countdown runs out (see the
/// module docs). Called through the optional lazy shim table from the
/// prologue and poll block, and directly by a profiling shim; returns to the leaf, which
/// continues in T1.
///
/// SAFETY: `obs` is the calling leaf's own `LeafObs`, alive as long as the
/// leaf.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: only ever called from generated code.
#[unsafe(no_mangle)]
#[cold]
#[inline(never)]
pub(crate) extern "C" fn neovm_jit_tier_request(obs: *const LeafObs) {
    // SAFETY: see the function docs.
    request(unsafe { &*obs });
}

/// The mapping-builtin credit (W1-HOF): `len / 64` of `seq` while the
/// countdown runs, then the request if it ran out. A list's length is
/// counted only as far as the countdown needs, so the walk is bounded by
/// what the request would cost the loop it stands for.
///
/// SAFETY: as [`neovm_jit_tier_request`]; `seq` is a tagged value the
/// caller holds (read only, never retained).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: only ever called from generated code.
pub(crate) extern "C" fn neovm_jit_t2_hof_credit(obs: *const LeafObs, seq: i64) {
    // SAFETY: see the function docs.
    let obs = unsafe { &*obs };
    let t2 = &obs.t2;
    let budget = t2.budget.get();
    if t2.state.get() != T2State::Idle || budget == DISARMED {
        return;
    }
    let cap = usize::try_from(budget)
        .unwrap_or(0)
        .saturating_add(1)
        .saturating_mul(HOF_CREDIT_DIVISOR);
    let len = bounded_length(
        crate::emacs_core::value::Value::from_bits(seq as usize),
        cap,
    );
    let credit = (len / HOF_CREDIT_DIVISOR) as i64;
    if credit == 0 {
        return;
    }
    bump_stats(|s| s.hof_credits += 1);
    let left = budget.saturating_sub(credit);
    t2.budget.set(left);
    if left <= 0 {
        request(obs);
    }
}

/// The length of a list (counted to at most `cap`), vector or string; 0
/// for anything else. Never signals, allocates or follows more than `cap`
/// conses (a circular list stops there).
fn bounded_length(seq: crate::emacs_core::value::Value, cap: usize) -> usize {
    use crate::emacs_core::value::{ValueKind, VecLikeType};
    match seq.kind() {
        ValueKind::Cons => {
            let mut n = 0;
            let mut tail = seq;
            while n < cap && tail.is_cons() {
                n += 1;
                tail = tail.cons_cdr();
            }
            n
        }
        ValueKind::String => seq.as_lisp_string().map_or(0, |s| s.schars()),
        ValueKind::Veclike(VecLikeType::Vector) => seq.as_vector_data().map_or(0, |v| v.len()),
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            crate::emacs_core::boolvec::bool_vector_length(&seq).unwrap_or(0) as usize
        }
        ValueKind::Veclike(VecLikeType::Lambda) => seq.closure_slots().map_or(0, |s| s.len()),
        ValueKind::Veclike(VecLikeType::ByteCode) => seq
            .bytecode_data_if_materialized()
            .map_or(0, |bc| bc.observable_closure_slot_count()),
        _ => 0,
    }
}

/// [`neovm_jit_tier_request`]'s body.
#[cold]
#[inline(never)]
pub(crate) fn request(obs: &LeafObs) {
    let t2 = &obs.t2;
    t2.budget.set(DISARMED);
    if t2.state.get() != T2State::Idle {
        return;
    }
    t2.at_request.set((obs.entries.get(), t2.polls.get()));
    // Retain the source across every unstable/deferred window. Taking it
    // here would leave the eventual Due request unable to disarm its slot.
    let source = t2
        .source
        .try_borrow()
        .ok()
        .and_then(|source| source.clone());
    let leaf = super::cache::current_leaf_of(obs);
    let decision = match (&leaf, &source) {
        (Some(leaf), Some(source)) => policy::request_decision(leaf, source),
        _ => Some(T2Decision::Keep),
    };
    bump_stats(|s| s.requests += 1);
    // A changed snapshot has rearmed the budget. The activation continues
    // in this T1 and every caller may keep using its slots during the window.
    let Some(decision) = decision else {
        return;
    };
    let state = match (&leaf, decision) {
        (None, _) => T2State::Stale,
        (Some(_), T2Decision::Keep) => T2State::Kept,
        (Some(_), T2Decision::Upgrade(kind)) => T2State::Due(kind),
    };
    t2.state.set(state);
    bump_stats(|s| match state {
        T2State::Kept => s.kept += 1,
        T2State::Stale => s.stale += 1,
        T2State::Due(_) => s.due += 1,
        T2State::Idle | T2State::Upgraded(_) => {}
    });
    if let (T2State::Due(kind), Some(leaf)) = (state, leaf) {
        // Every way into the leaf now leads to a seam that compiles the
        // upgrade: the interpreter's direct entry re-arms through
        // `try_run_compiled`, and an unlinked spec slot re-resolves through
        // `resolve_compiled_leaf_ptr`.
        if let Some(source) = &source {
            source.disarm_leaf_slot();
        }
        let unlinked = super::cache::unlink_spec_slots(std::rc::Rc::as_ptr(&leaf));
        tracing::debug!(
            target: "neovm_jit::tier2",
            id = obs.id,
            upgrade = <&'static str>::from(kind),
            unlinked,
            "upgrade requested"
        );
    }
}

/// Mark `old`, which the seam just replaced with its `kind` upgrade.
pub(crate) fn note_upgraded(old: &CompiledLeaf, kind: T2Upgrade) {
    policy::release(old);
    old.obs.t2.state.set(T2State::Upgraded(kind));
    bump_stats(|s| s.upgraded += 1);
}

// ---------------------------------------------------------------------------
// Counters.
// ---------------------------------------------------------------------------

/// This thread's tier-spine counters (the `[neovm-jit-final-t2]` line).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct T2Stats {
    /// Countdowns that ran out.
    pub(crate) requests: u64,
    /// Requests decided [`T2State::Kept`].
    pub(crate) kept: u64,
    /// Requests that found their leaf no longer current.
    pub(crate) stale: u64,
    /// Requests that decided an upgrade.
    pub(crate) due: u64,
    /// Upgrades a seam compiled.
    pub(crate) upgraded: u64,
    /// Mapping-builtin credits taken.
    pub(crate) hof_credits: u64,
    /// Requests rearmed because their feedback snapshot changed.
    pub(crate) unstable: u64,
    /// Upgrade requests denied by the compile CPU budget.
    pub(crate) budget_denied: u64,
    /// Stable leaves whose body/feedback offers no admissible improvement.
    pub(crate) not_worth: u64,
    /// Upgrade jobs deferred or superseded while retaining T1.
    pub(crate) deferred: u64,
    /// Failed upgrades whose T1 remains native.
    pub(crate) failed: u64,
    /// Feedback upgrades reverted to their retained T1.
    pub(crate) reverted: u64,
}

thread_local! {
    static T2_STATS: Cell<T2Stats> = const {
        Cell::new(T2Stats { requests: 0, kept: 0, stale: 0, due: 0, upgraded: 0, hof_credits: 0, unstable: 0, budget_denied: 0, not_worth: 0, deferred: 0, failed: 0, reverted: 0 })
    };
}

fn bump_stats(f: impl FnOnce(&mut T2Stats)) {
    let _ = T2_STATS.try_with(|c| {
        let mut s = c.get();
        f(&mut s);
        c.set(s);
    });
}

/// This thread's counters.
pub(crate) fn stats() -> T2Stats {
    T2_STATS.with(Cell::get)
}

/// The knobs in force (re-exported for the report and tests).
pub(crate) fn knob() -> Tier2Knob {
    jit_tier2()
}

#[cfg(test)]
#[path = "tier2/tests/tier2_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tier2/tests/reach_test.rs"]
mod reach_tests;

mod policy;
pub(crate) use policy::{
    charge_compile, cpu_time_us, rearm_fallback, release, revert_if_t2, upgrade_deferred,
    upgrade_failed,
};
