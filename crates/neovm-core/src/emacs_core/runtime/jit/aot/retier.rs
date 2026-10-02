//! AOT's entry credit and the shared tier-spine upgrade job (P4.2 A5).
//!
//! Threading: existing leaf cells and their source hold belong to one mutator.
//! Source exclusion is a monotone atomic flag shared by mutators, not a code
//! publication. No new cache of Lisp state or worker-owned Lisp pointer exists.

use std::sync::atomic::Ordering;

use super::super::compile::{CompiledLeaf, LeafObs, LeafTier, jit_aot_retier_on};
use super::super::tier2::{DISARMED, T2State, T2Upgrade};
use super::super::{Runtime, RuntimeState};

/// The old heat crossing, also available when T2 owns JIT-leaf requests.
pub(crate) fn heat() -> Option<u32> {
    #[cfg(test)]
    if let Some(at) = HEAT_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return (at != 0).then_some(at);
    }
    let factor = super::super::retier_factor();
    (factor != 0).then(|| super::super::hot_threshold().saturating_mul(factor))
}

/// Initialize before cache publication. Dispatch has already credited the
/// current entry, hence the extra one in the native-entry residual. Native
/// spec-slot hits don't advance source heat, so they consume this credit too.
#[cold]
#[inline(never)]
pub(crate) fn prepare(leaf: &mut CompiledLeaf, source: &Runtime) {
    if !jit_aot_retier_on() {
        return;
    }
    let Some(at) = heat() else { return };
    if super::super::compile::lowering::forced_regalloc().is_some() {
        return;
    }
    leaf.obs
        .t2
        .budget
        .set(i64::from(at.saturating_sub(source.heat())) + 1);
    *leaf.obs.t2.source.borrow_mut() = Some(source.share_state());
}

/// A source that earned JIT code never reloads AOT after a cache eviction.
/// This cold consult also avoids loading AOT for an already-hot source.
#[cold]
#[inline(never)]
pub(crate) fn may_load(source: &RuntimeState) -> bool {
    !jit_aot_retier_on()
        || super::super::compile::lowering::forced_regalloc().is_some()
        || (!source.aot_retiered.load(Ordering::Relaxed)
            && heat().is_none_or(|at| source.heat() < at))
}

/// A hot AOT member keeps its earned native admission across mutator caches.
/// Cached spec entries can leave source heat low even after earning the JIT;
/// a cold miss must retain that credit alongside the shared AOT exclusion.
#[cold]
#[inline(never)]
pub(crate) fn earned_native_admission(source: &RuntimeState) -> bool {
    jit_aot_retier_on()
        && super::super::compile::lowering::forced_regalloc().is_none()
        && source.aot_retiered.load(Ordering::Relaxed)
}

/// Only the existing AOT sidecar branch calls this helper. Off, its credit is
/// disarmed. JIT entry/prologue emission is unchanged in either configuration.
#[inline]
pub(crate) fn entry(obs: &LeafObs) {
    let budget = obs.t2.budget.get();
    if budget != DISARMED {
        let remaining = budget - 1;
        obs.t2.budget.set(remaining);
        if remaining <= 0 {
            request(obs);
        }
    }
}

#[cold]
#[inline(never)]
fn request(obs: &LeafObs) {
    if let Some(source) = obs.t2.source.borrow().as_ref() {
        source.aot_retiered.store(true, Ordering::Relaxed);
    }
    super::super::tier2::request(obs);
}

/// The legacy heat trigger decides inside its existing cache seam. T2-on
/// admission instead happens in `tier2::request` after native entry credit.
#[inline]
pub(crate) fn legacy_request(leaf: &CompiledLeaf, source: &RuntimeState) -> Option<T2Upgrade> {
    if leaf.tier() != LeafTier::Aot
        || leaf.obs.t2.state.get() != T2State::Idle
        || !jit_aot_retier_on()
        || super::super::compile::jit_tier2().on
        || super::super::compile::lowering::forced_regalloc().is_some()
        || !heat().is_some_and(|at| source.heat() >= at)
    {
        return None;
    }
    source.aot_retiered.store(true, Ordering::Relaxed);
    leaf.obs.t2.budget.set(DISARMED);
    leaf.obs.t2.state.set(T2State::Due(T2Upgrade::Retier));
    Some(T2Upgrade::Retier)
}

#[cfg(test)]
thread_local! {
    // Scalar configuration only; no mutator Lisp state.
    static HEAT_TEST_OVERRIDE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_heat_for_test(at: Option<u32>) {
    HEAT_TEST_OVERRIDE.with(|setting| setting.set(at));
}

#[cfg(test)]
#[path = "tests/retier_test.rs"]
mod tests;
