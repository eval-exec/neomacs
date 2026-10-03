//! Keep the string payload's native-store gate consistent with its owner mark.
//!
//! The owner and its payload stay live and exclusively borrowed for the entire
//! mutation. The existing heap-mutation closure contract excludes Lisp calls,
//! allocation safepoints and concurrent payload access. Shared mark publication
//! uses Acquire/Release; this does not extend mutator-local certificate coherence.

use crate::heap_types::LispString;
use crate::tagged::header::StringObj;

pub(super) struct StringStorageObservationGuard {
    owner: *const StringObj,
}

impl StringStorageObservationGuard {
    /// The caller retains the owner through this guard, including unwinding,
    /// and holds its only payload mutation borrow.
    #[inline]
    pub(super) unsafe fn new(owner: *const StringObj) -> Self {
        Self { owner }
    }
}

impl Drop for StringStorageObservationGuard {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: construction retains the live owner through normal return or
        // unwind; the payload mutation has ended before this guard is dropped.
        unsafe { synchronize_string_storage_observation(self.owner) };
    }
}

/// Synchronize AFTER publishing owned storage, so an observation that saw the
/// old borrowed capacity still reaches this acquire check. Never encode a mark
/// in a zero capacity: the existing OFF/GEN1 ownership predicate depends on it.
#[cold]
#[inline(never)]
pub(super) unsafe fn synchronize_string_storage_observation(owner: *const StringObj) {
    // SAFETY: the caller retains an exclusively mutated, live StringObj.
    let owner = unsafe { &*owner };
    if owner.header.collection_observed() {
        owner.data.mark_owned_storage_collection_observed();
    }
}

#[cold]
#[inline(never)]
pub(super) unsafe fn observe_materialized_string_storage(
    owner: *const StringObj,
    storage: &LispString,
) {
    // SAFETY: the byte setter retains the owner. Borrow only its disjoint
    // header while the setter lends the payload to this shared callback.
    let header = unsafe { &*std::ptr::addr_of!((*owner).header) };
    if header.collection_observed() {
        storage.mark_owned_storage_collection_observed();
    }
}
