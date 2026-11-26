//! String allocation charges and completed-GC live bytes are distinct.
//!
//! GNU alloc.c:1701/1741/1818 charges the string header and allocated sdata
//! extent (including NUL and padding). Its sweep census at1862-1863 counts
//! logical string bytes; total_bytes_of_live_objects adds live headers.

use super::*;
use crate::tagged::collection_reads::StoreState;

impl TaggedHeap {
    /// The logical collectible object census excludes spare byte capacity
    /// and the arena slot's free-list link, as GNU's sweep census does.
    pub(super) fn string_object_bytes(obj: &StringObj) -> usize {
        size_of::<StringObj>().saturating_add(obj.data.byte_len())
    }

    /// Charge a newly allocated string's actual slot and owned byte extent.
    /// Borrowed image/static bytes allocate no payload in this process.
    pub(super) fn string_allocation_bytes(obj: &StringObj) -> usize {
        <StringObj as super::arena_pages::PagedObject>::SLOT_BYTES
            .saturating_add(obj.data.owned_capacity())
    }
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum StringBacking {
    Borrowed,
    Owned { data: usize, capacity: usize },
}

#[cfg(any(test, feature = "test-support"))]
impl StringBacking {
    fn snapshot(string: &crate::heap_types::LispString) -> Self {
        match string.owned_storage_identity() {
            None => Self::Borrowed,
            Some((data, capacity)) => Self::Owned { data, capacity },
        }
    }
}

/// Charge the allocation counter without borrowing the heap's StoreState
/// tail or constructing a mutable view of the whole heap.
///
/// # Safety
/// A non-null heap is the live, serialized mutator that owns this allocation.
/// No callback, collection, or mutator retirement overlaps the scalar store.
pub(super) unsafe fn charge_backing(heap: *mut TaggedHeap, bytes: usize) {
    if heap.is_null() || bytes == 0 {
        return;
    }
    // SAFETY: the owner remains live and serialized. Project only this word,
    // disjoint from the shared StoreState tail and the external string slot.
    // This is the same single counter as TaggedHeap::note_allocation_bytes.
    unsafe {
        let counter = std::ptr::addr_of_mut!((*heap).mutator_gc.bytes_since_gc);
        *counter += bytes;
    }
}

#[cold]
#[inline(never)]
pub(crate) fn note_string_backing_allocation_in_store(store: &StoreState, bytes: usize) {
    // SAFETY: the installed store carries this serialized owner's provenance;
    // the byte store has no callback or safepoint. A detached store is null
    // and must not create a fallback heap just to account for the allocation.
    unsafe { charge_backing(store.heap.get(), bytes) };
}

/// Fixture-only observer for unrestricted published edits. Production string
/// operations bill their allocation branches without backing snapshots.
#[cfg(any(test, feature = "test-support"))]
pub(crate) struct SerialStringStorageCharge {
    string: *const StringObj,
    before: StringBacking,
    heap: *mut TaggedHeap,
}

#[cfg(any(test, feature = "test-support"))]
impl SerialStringStorageCharge {
    /// # Safety
    /// The initialized string and current owner outlive the guard. The
    /// caller holds serial mutation access, with no collection, callback,
    /// owner retirement, or escaping payload loan before the guard drops.
    pub(crate) unsafe fn new(string: *const StringObj) -> Self {
        Self {
            string,
            // SAFETY: the caller retains the initialized published string;
            // this copied observation ends before the mutable payload loan.
            before: unsafe { StringBacking::snapshot(&(*string).data) },
            heap: with_store_state(|store| store.heap.get()),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for SerialStringStorageCharge {
    fn drop(&mut self) {
        // SAFETY: the caller's payload loan has ended, including on unwind,
        // and its serial owner keeps this string initialized and live.
        let after = unsafe { StringBacking::snapshot(&(*self.string).data) };
        if after != self.before
            && let StringBacking::Owned { capacity, .. } = after
        {
            // GNU allocate_string_data charges the full new extent, unlike
            // hash-array resizing that refunds the old arrays. Keep spare
            // capacity out of the separate completed-GC logical census.
            // SAFETY: the captured owner outlives the same mutation guard.
            // Re-project its counter now, after any intervening heap borrow.
            unsafe { charge_backing(self.heap, capacity) };
        }
    }
}
