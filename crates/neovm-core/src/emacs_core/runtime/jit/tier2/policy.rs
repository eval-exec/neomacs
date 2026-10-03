//! Cold tier policy. Each leaf and its budget ledger belong to one mutator;
//! shared feedback is read through its existing atomic lattice accessors.
//! Comparing exact lattice snapshots adds no instructions to feedback recording.
use super::super::compile::{LeafTier, jit_tier2_policy};
use super::*;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::feedback::CallTarget;

#[derive(Clone, Debug)]
// A weak hold preserves pointer identity against allocator reuse without
// keeping the target source live. This state belongs only to this leaf's owner.
struct HeldSource(std::sync::Weak<RuntimeState>);
impl PartialEq for HeldSource {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Weak::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for HeldSource {}
#[derive(Clone, Debug, PartialEq, Eq)]
enum TargetVersion {
    Uninit,
    Symbol(SymId),
    Sources(Vec<HeldSource>),
    Mega,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Version {
    numeric: Vec<NumericFeedback>,
    calls: Vec<TargetVersion>,
    retreat: Vec<u16>,
    prefix: usize,
    level: super::super::ReoptLevel,
}
impl Version {
    fn read(source: &RuntimeState, ops_len: usize) -> Self {
        Self {
            numeric: (0..ops_len)
                .map(|pc| source.feedback.numeric_at(pc))
                .collect(),
            calls: source.call_sites().map_or_else(Vec::new, |sites| {
                sites
                    .sites()
                    .iter()
                    .map(|site| match site.target() {
                        CallTarget::Uninit => TargetVersion::Uninit,
                        CallTarget::Sym(sym) => TargetVersion::Symbol(sym),
                        CallTarget::Sources(sources) => TargetVersion::Sources(
                            sources
                                .iter()
                                .map(|source| HeldSource(Arc::downgrade(source)))
                                .collect(),
                        ),
                        CallTarget::Mega => TargetVersion::Mega,
                    })
                    .collect()
            }),
            retreat: (0..ops_len)
                .map(|pc| {
                    source
                        .site_retreat
                        .get(pc)
                        .map_or(0, |s| s.bits().fold(0, |bits, b| bits | b as u16))
                })
                .collect(),
            prefix: source.patched_prefix(),
            level: source.reopt_level(),
        }
    }
}
/// Cold state is per leaf, never sent to a backend worker. It holds no Lisp values.
#[derive(Debug, Default)]
pub(crate) struct PolicyState {
    version: Option<Version>,
    attempts: u32,
    pub(crate) ops_len: usize,
    pub(crate) loop_or_recursive: bool,
}
/// Per-mutator accounting only; this TLS contains resource counters, no Lisp state.
#[derive(Clone, Copy, Default)]
struct Ledger {
    spent: u64,
    reserved: u64,
}
thread_local! { static LEDGER: Cell<Ledger> = const { Cell::new(Ledger { spent: 0, reserved: 0 }) }; }

/// A scalar OS thread CPU clock, also usable by the backend worker (no Lisp).
pub(crate) fn cpu_time_us() -> u64 {
    #[cfg(unix)]
    {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: valid writable timespec and no retained pointer.
        if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0 {
            return (ts.tv_sec as u64)
                .saturating_mul(1_000_000)
                .saturating_add(ts.tv_nsec as u64 / 1_000);
        }
    }
    0
}
pub(crate) fn charge_compile(elapsed: std::time::Duration) {
    // Cache/job destructors may run after this thread's ledger was destroyed.
    // No later compile can consume its budget, so teardown drops the charge.
    let _ = LEDGER.try_with(|c| {
        let mut l = c.get();
        l.spent = l
            .spent
            .saturating_add(elapsed.as_micros().min(u128::from(u64::MAX)) as u64);
        c.set(l);
    });
}
fn affordable(
    spent: u64,
    reserved: u64,
    estimate: u64,
    cpu_us: u64,
    pct: u32,
    floor_ms: u32,
) -> bool {
    pct == 0
        || spent.saturating_add(reserved).saturating_add(estimate)
            <= cpu_us.saturating_mul(u64::from(pct)) / 100 + u64::from(floor_ms) * 1_000
}
fn estimate_us(leaf: &CompiledLeaf) -> u64 {
    u64::from(leaf.obs.compile_us.get())
        .saturating_mul(3)
        .div_ceil(2)
        .max(1)
}
fn budget_allows(leaf: &CompiledLeaf) -> bool {
    let k = jit_tier2_policy();
    LEDGER.with(|c| {
        let l = c.get();
        affordable(
            l.spent,
            l.reserved,
            estimate_us(leaf),
            cpu_time_us(),
            k.budget_pct,
            k.floor_ms,
        )
    })
}
/// Reserve only at the owning mutator's compile seam, not while a Due
/// leaf is waiting for an entry that may never happen. Pending jobs retain
/// this reservation until install/cancellation and charge this same ledger.
pub(crate) fn reserve_compile(leaf: &CompiledLeaf) -> bool {
    let k = jit_tier2_policy();
    let estimate = estimate_us(leaf);
    LEDGER.with(|c| {
        let mut l = c.get();
        if !affordable(
            l.spent,
            l.reserved,
            estimate,
            cpu_time_us(),
            k.budget_pct,
            k.floor_ms,
        ) {
            bump_stats(|s| s.budget_denied += 1);
            return false;
        }
        l.reserved = l.reserved.saturating_add(estimate);
        c.set(l);
        leaf.obs.t2.reserved_us.set(estimate);
        true
    })
}
pub(crate) fn release(leaf: &CompiledLeaf) {
    // Releasing from a cache destructor remains safe after ledger teardown.
    let reservation = leaf.obs.t2.reserved_us.replace(0);
    let _ = LEDGER.try_with(|c| {
        let mut l = c.get();
        l.reserved = l.reserved.saturating_sub(reservation);
        c.set(l);
    });
}

/// Re-arm the retained T1 against widened feedback. The next request compares
/// that exact snapshot after one stable work window (entries + cold poll credit).
pub(crate) fn rearm_fallback(source: &RuntimeState, old: &CompiledLeaf) {
    release(old);
    let t2 = &old.obs.t2;
    if !t2.profiling() {
        return;
    }
    let mut p = t2.policy.borrow_mut();
    p.version = Some(Version::read(source, p.ops_len));
    p.attempts = 0;
    t2.state.set(T2State::Idle);
    t2.budget.set(i64::from(jit_tier2_policy().stable));
}
pub(crate) fn upgrade_deferred(source: &RuntimeState, old: &CompiledLeaf) {
    if old.tier() == LeafTier::Aot {
        // AOT has no emitted profiling window to re-arm. Keep its native
        // code without retrying a refused upgrade on every entry.
        release(old);
        old.obs.t2.state.set(T2State::Kept);
        old.obs.t2.budget.set(DISARMED);
    } else {
        rearm_fallback(source, old);
    }
    bump_stats(|s| s.deferred += 1);
}
pub(crate) fn upgrade_failed(old: &CompiledLeaf) {
    release(old);
    old.obs.t2.state.set(T2State::Kept);
    old.obs.t2.budget.set(DISARMED);
    bump_stats(|s| s.failed += 1);
}

/// A budget denial is temporary. Keep the stable snapshot and retry
/// admission after another work window without reserving CPU in the meantime.
fn admit_request(leaf: &CompiledLeaf, kind: T2Upgrade) -> Option<T2Decision> {
    if !budget_allows(leaf) {
        leaf.obs.t2.budget.set(i64::from(jit_tier2_policy().stable));
        bump_stats(|s| s.budget_denied += 1);
        return None;
    }
    Some(T2Decision::Upgrade(kind))
}

/// Stability and admissibility, followed by budget; no Lisp and no GC.
pub(crate) fn request_decision(leaf: &CompiledLeaf, source: &RuntimeState) -> Option<T2Decision> {
    if leaf.tier() == LeafTier::Aot {
        // AOT has no T1 feedback window. Check affordability without
        // reserving until the compile seam, as every tier-spine job does.
        return Some(match decide(leaf) {
            T2Decision::Upgrade(kind) if budget_allows(leaf) => T2Decision::Upgrade(kind),
            T2Decision::Upgrade(_) => {
                bump_stats(|s| s.budget_denied += 1);
                T2Decision::Keep
            }
            T2Decision::Keep => T2Decision::Keep,
        });
    }
    let k = jit_tier2_policy();
    let t2 = &leaf.obs.t2;
    let banned = source.t2_reopts.load(std::sync::atomic::Ordering::Relaxed) >= k.max_reopt
        || source.reopt_level() >= super::super::ReoptLevel::BaselineOnly;
    if banned {
        return match decide(leaf) {
            T2Decision::Upgrade(kind) => admit_request(leaf, kind),
            T2Decision::Keep => Some(T2Decision::Keep),
        };
    }
    let mut p = t2.policy.borrow_mut();
    let version = Version::read(source, p.ops_len);
    let unstable = p.version.as_ref() != Some(&version);
    if unstable && p.attempts < k.attempts {
        p.version = Some(version);
        p.attempts += 1;
        t2.budget.set(i64::from(k.stable));
        bump_stats(|s| s.unstable += 1);
        return None;
    }
    let fallback = decide(leaf);
    let dynamic_worth = source.call_sites().is_some_and(|sites| {
        sites.sites().iter().any(|s| {
            s.count() >= jit_tier2().window / 8
                && matches!(s.target(), CallTarget::Sym(_) | CallTarget::Sources(_))
        })
    });
    let kind = if let T2Decision::Upgrade(kind) = fallback {
        Some(kind)
    } else if !unstable
        && !banned
        && (p.loop_or_recursive || leaf.tier() == LeafTier::Mir || dynamic_worth)
    {
        Some(T2Upgrade::Feedback)
    } else {
        None
    };
    drop(p);
    // An opt compile replaces code rather than merely changing its allocator:
    // use the T2 lifecycle so the existing T1 fallback remains rooted and can
    // be restored on a counted deopt. The legacy decision is unchanged.
    let kind = kind.map(|kind| {
        if !banned
            && !unstable
            && super::super::compile::jit_opt_mode() == super::super::compile::OptMode::Opt
        {
            T2Upgrade::Feedback
        } else {
            kind
        }
    });
    let Some(kind) = kind else {
        bump_stats(|s| s.not_worth += 1);
        return Some(T2Decision::Keep);
    };
    admit_request(leaf, kind)
}

/// Called only for a counted, conclusive/repeated deopt, after its existing
/// widening/retreat policy. C9 reuses that policy and the persistent site table.
pub(crate) fn revert_if_t2(
    func: &crate::emacs_core::bytecode::ByteCodeFunction,
    leaf: &CompiledLeaf,
    pc: Option<usize>,
    floor: super::super::ReoptLevel,
) -> bool {
    use super::super::{ReoptLevel, cache, reopt::LeafOrigin, retreat::RetreatBit};
    use std::sync::atomic::Ordering;
    if !matches!(leaf.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Feedback))
        || floor == ReoptLevel::Interpreter
        || !cache::leaf_is_current(func, leaf, LeafOrigin::Entry)
    {
        return false;
    }
    let source = func.jit_runtime();
    if let Some(pc) = pc {
        if let Some(s) = source.site_retreat.site(pc, func.executable_ops().len()) {
            s.set(RetreatBit::Invalidated);
        }
    }
    source.reopt_level.fetch_max(floor as u8, Ordering::Relaxed);
    source.reopen_numeric_feedback();
    source
        .t2_reopts
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        })
        .ok();
    // Metadata is visible before cache rearm captures its stable-window snapshot.
    if !cache::revert_t2_to_t1(func, leaf) {
        return false;
    }
    bump_stats(|s| s.reverted += 1);
    true
}

#[cfg(test)]
#[path = "tests/policy_test.rs"]
mod tests;
