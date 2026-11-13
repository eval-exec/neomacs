//! Exclusive bulk views transfer ownership of the allocation; they never cast
//! a reference between Vec instantiations or provide marker provenance.

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

#[inline(always)]
fn into_values(cells: Vec<AtomicTaggedValue>) -> Vec<TaggedValue> {
    let mut cells = ManuallyDrop::new(cells);
    // SAFETY: identical allocation layout, initialized words and no drop glue.
    // The caller exclusively owns this uncaptured allocation.
    unsafe { Vec::from_raw_parts(cells.as_mut_ptr().cast(), cells.len(), cells.capacity()) }
}

/// A sole-owner bulk view that restores typed storage even if its caller unwinds.
/// Captured backings must be retired before this view is constructed.
pub struct OwnedValueVec<'a> {
    destination: &'a mut Vec<AtomicTaggedValue>,
    values: ManuallyDrop<Vec<TaggedValue>>,
}

impl<'a> OwnedValueVec<'a> {
    #[inline(always)]
    pub(super) fn new(destination: &'a mut Vec<AtomicTaggedValue>) -> Self {
        let cells = std::mem::take(destination);
        Self {
            destination,
            values: ManuallyDrop::new(into_values(cells)),
        }
    }
}

impl Deref for OwnedValueVec<'_> {
    type Target = Vec<TaggedValue>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for OwnedValueVec<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl Drop for OwnedValueVec<'_> {
    #[inline(always)]
    fn drop(&mut self) {
        // SAFETY: the guard owns this Vec, and Drop runs exactly once. Nothing
        // after the take can panic before the cell allocation is restored.
        let values = unsafe { ManuallyDrop::take(&mut self.values) };
        *self.destination = into_cells(values);
    }
}
