//! Read-only, cold Opt admission evidence from the existing OSR cache.
//!
//! Threading: each mutator owns its existing !Send OSR leaves. The query reads
//! source atomics, cache-owner scalars and fully published registry metadata;
//! only a boolean escapes. It adds no TLS cache, Lisp handle or runtime state.

use super::{COMPILED_HEAP, COMPILED_OBARRAY, OSR_CACHE};
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::compile::opt_census::SelectedTier;

/// Installed Opt code at one of this source's original qualifying headers.
/// A ready pending job is insufficient until its owning mutator installs it.
/// Missing ownership or a reentrant cache borrow declines conservatively.
/// Cache synchronization is deliberately excluded: the entry compiler can
/// already hold COMPILED.borrow_mut, and this predicate must not clear it.
#[cold]
#[inline(never)]
pub(crate) fn has_ready_opt_osr(
    source: &RuntimeState,
    generation: Option<u64>,
    mut original_headers: impl Iterator<Item = usize>,
) -> bool {
    let Some(id) = source.compiled_id() else {
        return false;
    };
    let Some(generation) = generation else {
        return false;
    };
    let Some(heap) = crate::tagged::gc::current_tagged_heap_identity() else {
        return false;
    };
    if !COMPILED_HEAP.with(|owner| owner.get() == Some(heap))
        || !COMPILED_OBARRAY.with(|owner| owner.get() == Some(generation))
    {
        return false;
    }
    let level = source.reopt_level();
    let prefix = source.patched_prefix();
    OSR_CACHE.with(|cache| {
        let Ok(cache) = cache.try_borrow() else {
            return false;
        };
        original_headers.any(|header| {
            let Some(Some(entry)) = cache.get(&(id, header)) else {
                return false;
            };
            let leaf = &entry.leaf;
            !leaf.retired.get()
                && !leaf.entry.is_null()
                && leaf.obs.id == id
                && leaf.obs.osr_pc.is_some_and(|pc| pc as usize == header)
                && leaf.compiled_level == level
                && leaf.dynamic_prefix as usize == prefix
                && leaf.selected_tier() == SelectedTier::Opt
        })
    })
}

#[cfg(test)]
#[path = "tests/opt_evidence.rs"]
mod tests;
