//! Cold, local withdrawal of the legacy source cache, not nmjit admission.
//! Relocation roots must already be captured or pinned, and the source remains
//! inaccessible. No leaf, pending job, or executable allocation is destroyed.

use super::{
    COMPILED, COMPILED_HEAP, COMPILED_OBARRAY, DenseCache, INLINE_DEPS, INLINE_EVICTIONS,
    OSR_CACHE, OSR_PENDING, OsrEntry, PendingOsr, REDEFINED_PINS, REJECTION_EPOCH, RedefinedPin,
    bump_leaf_slot_epoch,
};
use crate::emacs_core::eval::Context;
use neovm_value::intern::SymId;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cell::Cell;
use std::mem::ManuallyDrop;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ContextOwner(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObarrayGeneration(u64);

/// The source whose legacy side tables this withdrawal may take.
///
/// The heap word is the same plain identity `COMPILED_HEAP` and redefined pins
/// store. `HeapIdentity::get` stays crate-private in the value layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceCacheOwner {
    heap: usize,
    owner: ContextOwner,
    obarray: ObarrayGeneration,
}

impl SourceCacheOwner {
    pub(crate) fn for_context(source: &Context) -> Self {
        Self {
            heap: source.tagged_heap.tagged_heap_identity(),
            owner: ContextOwner(source.context_instance_id()),
            obarray: ObarrayGeneration(source.obarray.generation()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceCacheSlot {
    Compiled,
    InlineDeps,
    InlineEvictions,
    Osr,
    PendingOsr,
    RedefinedPins,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceCacheError {
    HeapInstalled,
    NativeActivation,
    PendingJobs,
    WrongHeap,
    WrongObarray,
    MissingHeapOwner,
    Borrowed(SourceCacheSlot),
}

/// Owns all original cache payloads on the source OS thread. Backend and
/// pending-flow TLS are not included; they require a separate withdrawal.
/// Every owning payload is retained independently if this receipt is abandoned.
#[must_use]
pub(crate) struct DetachedSourceCache {
    owner: SourceCacheOwner,
    compiled: ManuallyDrop<DenseCache>,
    inline_deps: ManuallyDrop<HashMap<SymId, HashSet<u64>>>,
    inline_evictions: ManuallyDrop<HashMap<SymId, u32>>,
    osr: ManuallyDrop<HashMap<(u64, usize), Option<OsrEntry>>>,
    pending_osr: ManuallyDrop<HashMap<(u64, usize), PendingOsr>>,
    pins: ManuallyDrop<Vec<RedefinedPin>>,
    recorded_heap: Option<usize>,
    recorded_obarray: Option<ObarrayGeneration>,
}

impl std::fmt::Debug for DetachedSourceCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never format leaf code, relocation words, or pinned functions.
        formatter
            .debug_struct("DetachedSourceCache")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(DetachedSourceCache: Send, Sync, Clone, Copy);

impl Drop for DetachedSourceCache {
    fn drop(&mut self) {
        // Retain every original payload. Taking a ManuallyDrop would free
        // executable and relocation backing while a suspended frame may still
        // name it. Forget and unwind use this same path.
        let _retained = (
            &self.compiled,
            &self.inline_deps,
            &self.inline_evictions,
            &self.osr,
            &self.pending_osr,
            &self.pins,
            self.recorded_heap,
            self.recorded_obarray,
        );
    }
}

impl DetachedSourceCache {
    /// # Safety
    ///
    /// Only the actual inaccessible sole-source shell may call this after its
    /// final census and drain. No native, VM, or compile frame can execute
    /// this source, including frames not covered by debug-only native depth
    /// accounting. No collection may occur after cached root sources are
    /// removed until their captured words are published or explicitly traced.
    /// The caller retains the original Context and backing, including on
    /// error, drop, forget, and unwind. This local effect is not permission
    /// to send a heap or run nmjit code.
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn withdraw(owner: SourceCacheOwner) -> Result<Self, SourceCacheError> {
        if neovm_value::tagged::gc::tagged_heap_is_installed() {
            return Err(SourceCacheError::HeapInstalled);
        }
        if super::native_depth() != 0 {
            return Err(SourceCacheError::NativeActivation);
        }
        if super::super::bg::pending_count() != 0 {
            return Err(SourceCacheError::PendingJobs);
        }
        COMPILED.with(|compiled| {
            INLINE_DEPS.with(|deps| {
                INLINE_EVICTIONS.with(|evictions| {
                    OSR_CACHE.with(|osr| {
                        OSR_PENDING.with(|pending| {
                            REDEFINED_PINS.with(|pins| {
                                // Acquire every real slot before changing any one.
                                // Refusal never runs lazy cache sync or clear, and
                                // never consumes a pending job.
                                let mut compiled = compiled.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::Compiled)
                                })?;
                                let mut deps = deps.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::InlineDeps)
                                })?;
                                let mut evictions = evictions.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::InlineEvictions)
                                })?;
                                let mut osr = osr.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::Osr)
                                })?;
                                let mut pending = pending.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::PendingOsr)
                                })?;
                                let mut pins = pins.try_borrow_mut().map_err(|_| {
                                    SourceCacheError::Borrowed(SourceCacheSlot::RedefinedPins)
                                })?;
                                if !pending.is_empty()
                                    || compiled.values().any(|entry| entry.pending_job().is_some())
                                {
                                    return Err(SourceCacheError::PendingJobs);
                                }
                                let recorded_heap = match COMPILED_HEAP.with(Cell::get) {
                                    Some(heap) if heap == owner.heap => Some(owner.heap),
                                    Some(_) => return Err(SourceCacheError::WrongHeap),
                                    None => {
                                        if compiled.values().next().is_some()
                                            || !compiled.retired.is_empty()
                                            || !osr.is_empty()
                                        {
                                            return Err(SourceCacheError::MissingHeapOwner);
                                        }
                                        None
                                    }
                                };
                                if pins.iter().any(|pin| pin.heap != owner.heap) {
                                    return Err(SourceCacheError::WrongHeap);
                                }
                                let recorded_obarray = match COMPILED_OBARRAY.with(Cell::get) {
                                    Some(generation) if generation == owner.obarray.0 => {
                                        Some(owner.obarray)
                                    }
                                    Some(_) => return Err(SourceCacheError::WrongObarray),
                                    None => None,
                                };
                                // Source slots cannot retain a callable pointer into
                                // another mutator's local cache. Reuse the existing
                                // cold epochs. No dispatch, VM, or JIT shim shape
                                // and no TLS field is introduced.
                                bump_leaf_slot_epoch();
                                REJECTION_EPOCH.fetch_add(1, Ordering::Relaxed);
                                let detached = Self {
                                    owner,
                                    compiled: ManuallyDrop::new(std::mem::take(&mut *compiled)),
                                    inline_deps: ManuallyDrop::new(std::mem::take(&mut *deps)),
                                    inline_evictions: ManuallyDrop::new(std::mem::take(
                                        &mut *evictions,
                                    )),
                                    osr: ManuallyDrop::new(std::mem::take(&mut *osr)),
                                    pending_osr: ManuallyDrop::new(std::mem::take(&mut *pending)),
                                    pins: ManuallyDrop::new(std::mem::take(&mut *pins)),
                                    recorded_heap,
                                    recorded_obarray,
                                };
                                COMPILED_HEAP.with(|slot| slot.set(None));
                                COMPILED_OBARRAY.with(|slot| slot.set(None));
                                Ok(detached)
                            })
                        })
                    })
                })
            })
        })
    }
}

#[cfg(test)]
#[path = "tests/source_detachment_test.rs"]
mod tests;
