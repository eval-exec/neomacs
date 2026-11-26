//! Sealed `Send` wrapper for sending a `*mut TaggedHeap` across a synchronous
//! collector handshake (value-gc B5, G1).
//!
//! Keeping the raw pointer private and gating construction behind `unsafe fn`
//! ensures safe code in `tagged::gc` cannot forge a `HeapPtr` from an arbitrary
//! pointer or send it without upholding the blocking-requester exclusivity
//! contract.

use super::TaggedHeap;

/// A raw `*mut TaggedHeap` that can cross to the GC thread for a synchronous
/// handshake while the requester blocks until completion.
pub(super) struct HeapPtr(*mut TaggedHeap);

// SAFETY: constructing `HeapPtr` via `HeapPtr::new` requires exclusive access
// to the pointed-to `TaggedHeap` and synchronous blocking until the receiver
// finishes using it, so the heap is never accessed concurrently.
unsafe impl Send for HeapPtr {}

impl HeapPtr {
    /// Wrap `heap` for transfer across a synchronous collector handshake.
    ///
    /// # Safety
    /// `heap` must be a non-null pointer to a live `TaggedHeap` exclusively
    /// borrowed by the caller, and the caller must block synchronously until
    /// the collector finishes and signals completion before touching or
    /// dropping the heap again.
    #[inline]
    pub(super) unsafe fn new(heap: *mut TaggedHeap) -> Self {
        debug_assert!(!heap.is_null());
        Self(heap)
    }

    /// Consume this wrapper and return the raw heap pointer.
    #[inline]
    pub(super) fn into_raw(self) -> *mut TaggedHeap {
        self.0
    }
}
