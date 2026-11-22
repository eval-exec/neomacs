//! Source-only background compiler closure. Unfinished work is never cancelled.

use super::{DEFER_SCOPE, DEFERRED, EPOCH_DISCARDS, PENDING_COUNT, PENDING_IDS};
use crate::emacs_core::jit::source_suspension::{SourceScopeRefusal, SourceScopeSlot};
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;

/// These payloads contain ids and counts only. Their allocations stay for
/// the later explicit return protocol and confer no heap or root permission.
#[must_use]
pub(crate) struct DetachedBgMetadata {
    pending_ids: ManuallyDrop<Vec<u64>>,
    epoch_discards: ManuallyDrop<rustc_hash::FxHashMap<u64, u8>>,
    _local: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for DetachedBgMetadata {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DetachedBgMetadata")
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(DetachedBgMetadata: Send, Sync, Clone, Copy);

impl Drop for DetachedBgMetadata {
    fn drop(&mut self) {
        // Retain the id vector and the epoch map. Taking either ManuallyDrop
        // would free metadata the return protocol has not yet accepted.
        let _retained = (&self.pending_ids, &self.epoch_discards, &self._local);
    }
}

impl DetachedBgMetadata {
    #[cold]
    #[inline(never)]
    pub(crate) fn withdraw() -> Result<Self, SourceScopeRefusal> {
        if DEFER_SCOPE.with(Cell::get).is_some() {
            return Err(SourceScopeRefusal::DeferredScope);
        }
        if PENDING_COUNT.with(Cell::get) != 0 {
            return Err(SourceScopeRefusal::PendingJobs);
        }
        DEFERRED.with(|deferred| {
            PENDING_IDS.with(|ids| {
                EPOCH_DISCARDS.with(|epochs| {
                    let deferred = deferred
                        .try_borrow_mut()
                        .map_err(|_| SourceScopeRefusal::Borrowed(SourceScopeSlot::DeferredCode))?;
                    let mut ids = ids
                        .try_borrow_mut()
                        .map_err(|_| SourceScopeRefusal::Borrowed(SourceScopeSlot::PendingIds))?;
                    let mut epochs = epochs.try_borrow_mut().map_err(|_| {
                        SourceScopeRefusal::Borrowed(SourceScopeSlot::EpochDiscards)
                    })?;
                    // A deferred result precedes PendingJob and its counter.
                    // Refuse it without taking, cancelling, or dropping it.
                    if deferred.is_some() {
                        return Err(SourceScopeRefusal::DeferredCode);
                    }
                    Ok(Self {
                        pending_ids: ManuallyDrop::new(std::mem::take(&mut *ids)),
                        epoch_discards: ManuallyDrop::new(std::mem::take(&mut *epochs)),
                        _local: PhantomData,
                    })
                })
            })
        })
    }
}

#[cfg(test)]
#[path = "tests/source_suspension_test.rs"]
mod tests;
