//! Normal-entry admission that waits for its optimized OSR compile to settle.
//!
//! A selected normal frontend needs installed Opt OSR evidence
//! (`opt_profit::ready_osr_front`). With background backends that OSR leaf can
//! still be compiling when the entry first asks; deciding then would cache a
//! baseline entry that a synchronous compile never chooses. The entry compile
//! instead waits on that exact pending job: the source is parked on the job
//! (`bg::PendingJob::park_waiter`), and the backend's publication -- or the
//! job leaving `OSR_PENDING` (installed, refused, stale or cancelled) --
//! releases it. Its next call asks again, installs a finished job here first
//! and compiles against the settled outcome: the decision a synchronous OSR
//! compile would have produced.
//!
//! Threading: the park is owned by one mutator through its `OSR_PENDING`
//! entry. The job's cell retains only the source's atomic `RuntimeState`; no
//! Lisp value, Context or cache handle, and the worker only releases it.

use super::{CacheEntry, DeferReason, EntryLeaves, OSR_PENDING, install_osr_pending};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{self, CompileRequest};
use crate::emacs_core::jit::tier2::CompileTier;
use crate::emacs_core::symbol::Obarray;

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
        p.borrow()
            .get(&key)
            .expect("the awaited job is pending")
            .job
            .park_waiter(source.share_state());
    });
    Some(CacheEntry::Deferred(DeferReason::AwaitingOsr))
}

#[cfg(test)]
#[path = "tests/osr_wait.rs"]
mod tests;
