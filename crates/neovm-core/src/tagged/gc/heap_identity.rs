//! Process-unique identity of one tagged heap lifetime.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The identity of one [`TaggedHeap`](super::TaggedHeap) lifetime.
///
/// Identities come from a process-wide counter that starts at one and are
/// never reused, so a stale identity cannot name a later heap that happens to
/// occupy the same address. Side tables that carry GC-managed values key on
/// it; a value word recorded under one identity is never materialized for
/// another heap.
///
/// The word is deliberately a plain `usize`, not a `NonZeroUsize`: the heap
/// stores this type, and a niche lets rustc reorder `TaggedHeap`'s fields,
/// which moved the JIT-addressed `jit` field off ABI 32's offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct HeapIdentity(usize);

static_assertions::assert_impl_all!(HeapIdentity: Send, Sync, Copy, std::fmt::Debug);
static_assertions::assert_eq_size!(HeapIdentity, usize);

/// The next identity to issue; zero means "no heap" and is never issued.
static NEXT_HEAP_IDENTITY: AtomicUsize = AtomicUsize::new(1);

impl HeapIdentity {
    /// Issue the identity of a new heap lifetime.
    pub(super) fn issue() -> Self {
        Self(NEXT_HEAP_IDENTITY.fetch_add(1, Ordering::Relaxed))
    }

    /// Re-type a plain identity word from a legacy side table or the thread's
    /// installed-heap view; `None` for zero (no heap).
    #[inline]
    pub(crate) const fn from_legacy_word(word: usize) -> Option<Self> {
        if word == 0 { None } else { Some(Self(word)) }
    }

    /// The identity as the plain word legacy side tables still store.
    #[inline]
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}
