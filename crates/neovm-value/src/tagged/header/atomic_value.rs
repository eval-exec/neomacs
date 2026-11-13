//! Typed storage for a GC-visible value word.

use super::{ConsWord, TaggedValue};
use std::sync::atomic::AtomicUsize;

/// A value slot whose single owner writes while the concurrent marker reads.
///
/// Reuses the cons word's interior-mutability and publication contract. Owner
/// reads are plain loads, marker reads acquire, and publications release. A
/// copied value is returned for owner reads: no shared reference to an ordinary
/// value word is exposed by this type.
#[repr(transparent)]
pub struct AtomicTaggedValue(ConsWord);

static_assertions::assert_eq_size!(AtomicTaggedValue, TaggedValue, AtomicUsize);
static_assertions::assert_eq_align!(AtomicTaggedValue, TaggedValue, AtomicUsize);
static_assertions::assert_not_impl_any!(AtomicTaggedValue: Send, Sync);
const _: () = assert!(!std::mem::needs_drop::<AtomicTaggedValue>());

impl AtomicTaggedValue {
    #[inline(always)]
    pub const fn new(value: TaggedValue) -> Self {
        Self(ConsWord::new(value))
    }

    /// Plain read by the sole writer. The marker only reads this slot.
    #[inline(always)]
    pub fn get(&self) -> TaggedValue {
        self.0.get()
    }

    /// Acquire the owner's release publication, through the actual cell.
    #[inline(always)]
    pub fn load_atomic(&self) -> TaggedValue {
        self.0.load_atomic()
    }

    /// Publish through a shared cell borrow, without creating a unique slot
    /// reference that would invalidate the marker's shared provenance.
    #[inline(always)]
    pub fn store_atomic(&self, value: TaggedValue) {
        self.0.store_atomic(value);
    }
}

impl Clone for AtomicTaggedValue {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

impl From<TaggedValue> for AtomicTaggedValue {
    #[inline(always)]
    fn from(value: TaggedValue) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Debug for AtomicTaggedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}
