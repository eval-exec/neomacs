//! Normal-entry admission that waits for its optimized OSR compile to settle.
//!
//! A selected normal frontend needs installed Opt OSR evidence
//! (`opt_profit::ready_osr_front`). With background backends that OSR leaf can
//! still be compiling when the entry first asks; deciding then would cache a
//! baseline entry that a synchronous compile never chooses. The entry compile
//! instead waits on that exact pending job: the job leaving `OSR_PENDING`
//! (installed, refused, stale or cancelled) releases the wait, and the next
//! call compiles against the settled outcome -- the decision a synchronous
//! OSR compile would have produced.
//!
//! Threading: owned by one mutator, stored in its `OSR_PENDING` entry. Only
//! the source's atomic `RuntimeState` is retained; no Lisp value, Context or
//! cache handle, and the worker never sees it.

use std::sync::Arc;

use super::{CacheEntry, DeferReason, EntryLeaves, OSR_PENDING, install_osr_pending};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{self, CompileRequest};
use crate::emacs_core::jit::tier2::CompileTier;
use crate::emacs_core::jit::{RuntimeState, bg};
use crate::emacs_core::symbol::Obarray;

/// The tier-up hold of a normal entry whose compile waits on a pending
/// optimized OSR job. Dropping it -- with that job, by any route -- lets the
/// source's next call compile. The hold itself only bounds how long a job
/// that finished but was never installed (its loop stopped wrapping) goes
/// unnoticed: an expired hold re-asks [`await_opt_osr`], which installs it.
pub(super) struct EntryWait {
    source: Arc<RuntimeState>,
    /// The deferral heat this wait set; another owner's hold is never undone.
    hold: u32,
    /// Calls until the next re-ask, doubling like a pending entry's probes.
    backoff: u32,
}

impl EntryWait {
    fn new(source: Arc<RuntimeState>) -> Self {
        let mut wait = Self {
            source,
            hold: 0,
            backoff: bg::FIRST_BACKOFF,
        };
        wait.extend();
        wait
    }

    /// Hold the source in the interpreter for the next `backoff` calls.
    fn extend(&mut self) {
        self.hold = self.source.heat().saturating_add(self.backoff).max(1);
        self.backoff = self.backoff.saturating_mul(2).min(bg::MAX_BACKOFF);
        self.source.defer_tier_up(self.hold);
    }
}

impl Drop for EntryWait {
    fn drop(&mut self) {
        // The awaited job settled: an expired hold sends the next call to
        // the compile, which reads the installed (or absent) evidence.
        if self.source.deferred_heat() == self.hold {
            self.source.defer_tier_up(self.source.heat().max(1));
        }
    }
}

/// Wait for the pending optimized OSR compile a normal compile of `func`
/// depends on: `Some` is the entry to cache meanwhile, `None` compiles now.
/// A finished job is installed here first, so the compile sees its outcome.
/// Runs inside the entry cache's borrow, for an id whose slot holds no leaf
/// or job; the installation therefore skips that slot's inline dependencies.
#[cold]
#[inline(never)]
pub(super) fn await_opt_osr(
    id: u64,
    func: &ByteCodeFunction,
    obarray: Option<&Obarray>,
    request: CompileRequest,
) -> Option<CacheEntry> {
    if request.tier != CompileTier::T1 {
        return None;
    }
    let source = func.jit_runtime();
    let pending = OSR_PENDING.with(|p| {
        let p = p.try_borrow().ok()?;
        let headers: Vec<usize> = p
            .iter()
            .filter(|((fid, _), osr)| {
                let leaf = osr.job.leaf();
                *fid == id
                    && leaf.selected_tier() == SelectedTier::Opt
                    && leaf.compiled_level == source.reopt_level()
                    && leaf.dynamic_prefix as usize == source.patched_prefix()
            })
            .map(|((_, header), _)| *header)
            .collect();
        (!headers.is_empty()).then_some(headers)
    })?;
    if !compile::front_requires_osr_evidence(func, request) {
        return None;
    }
    let mut headers = compile::opt_profit::osr_evidence_headers(func);
    if super::has_ready_opt_osr(source, obarray.map(Obarray::generation), headers.clone()) {
        return None;
    }
    let key = (id, headers.find(|h| pending.contains(h))?);
    let ready = OSR_PENDING.with(|p| p.borrow().get(&key).map(|osr| osr.job.is_ready()))?;
    if ready {
        install_osr_pending(key, None, EntryLeaves::Compiling);
        return None;
    }
    OSR_PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let osr = p.get_mut(&key).expect("the awaited job is pending");
        match &mut osr.wait {
            Some(wait) => wait.extend(),
            None => osr.wait = Some(EntryWait::new(source.share_state())),
        }
    });
    Some(CacheEntry::Deferred(DeferReason::AwaitingOsr))
}

#[cfg(test)]
#[path = "tests/osr_wait.rs"]
mod tests;
