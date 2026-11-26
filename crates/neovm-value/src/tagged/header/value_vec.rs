//! Exclusive in-place bulk views over owned `Vec<AtomicTaggedValue>` storage.
//!
//! `OwnedValueVec` never moves the `Vec` out via `std::mem::take`: the
//! allocation stays in place inside the heap object at all times, and
//! `&[TaggedValue]` / `&mut [TaggedValue]` are projected directly over its
//! buffer while exclusive `&mut` access guarantees no concurrent marker
//! snapshot is reading it.

use super::{AtomicTaggedValue, TaggedValue};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

#[inline(always)]
pub(super) fn into_cells(values: Vec<TaggedValue>) -> Vec<AtomicTaggedValue> {
    let mut values = ManuallyDrop::new(values);
    // SAFETY: both word types have identical size/alignment and no drop glue.
    // AtomicTaggedValue transparently wraps an UnsafeCell of this same Value.
    // Ownership is transferred, so no old Value references remain live.
    unsafe { Vec::from_raw_parts(values.as_mut_ptr().cast(), values.len(), values.capacity()) }
}

/// A sole-owner in-place bulk view over a `Vec<AtomicTaggedValue>`.
///
/// Captured backings must be retired before this view is constructed.
/// Nothing is moved out of `destination`, so the backing remains live and
/// rooted in place across every operation and unwind without requiring `Drop`.
pub struct OwnedValueVec<'a> {
    destination: &'a mut Vec<AtomicTaggedValue>,
}

impl<'a> OwnedValueVec<'a> {
    #[inline(always)]
    pub(super) fn new(destination: &'a mut Vec<AtomicTaggedValue>) -> Self {
        Self { destination }
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[TaggedValue] {
        // SAFETY: `&self` holds exclusive ownership of `destination` (captured
        // marker snapshots were retired before constructing `OwnedValueVec`),
        // and `AtomicTaggedValue` is `#[repr(transparent)]` over
        // `UnsafeCell<TaggedValue>`.
        unsafe {
            &*(self.destination.as_slice() as *const [AtomicTaggedValue] as *const [TaggedValue])
        }
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [TaggedValue] {
        // SAFETY: `&mut self` holds exclusive access to `destination` (no
        // concurrent marker snapshot), and `AtomicTaggedValue` has identical
        // size, alignment, and bit validity to `TaggedValue`.
        unsafe {
            &mut *(self.destination.as_mut_slice() as *mut [AtomicTaggedValue]
                as *mut [TaggedValue])
        }
    }

    #[inline(always)]
    pub fn push(&mut self, value: TaggedValue) {
        self.destination.push(AtomicTaggedValue::new(value));
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.destination.clear();
    }

    #[inline(always)]
    pub fn truncate(&mut self, len: usize) {
        self.destination.truncate(len);
    }

    #[inline(always)]
    pub fn replace(&mut self, values: Vec<TaggedValue>) {
        *self.destination = into_cells(values);
    }
}

impl Deref for OwnedValueVec<'_> {
    type Target = [TaggedValue];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for OwnedValueVec<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}
