//! A Context field whose state GNU keeps global (round 3w, plan-3w.md).
//!
//! The value lives in the World ([`super::super::global_state::GlobalState`]);
//! every owner's Context reaches it through its [`WorldSlot`], the one access
//! path for that state. The slot is the size and alignment of the value it
//! replaces, so no other Context field moves (plan-3w.md section 3, option
//! A): it holds the cell's address in its first word and reserves the rest.
//!
//! No `AsRef`/`AsMut`: they would shadow the value's own `as_ref`/`as_mut`
//! (`Option::as_ref`) at every call site.

use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

/// A Context field of GNU-global state; see the module documentation.
#[repr(C)]
#[doc(hidden)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) union WorldSlot<T> {
    cell: NonNull<T>,
    /// Keeps the slot the size and alignment of `T` (plan-3w.md section 3).
    _reserve: ManuallyDrop<MaybeUninit<T>>,
}

impl<T> WorldSlot<T> {
    /// A slot is in place of a value at least a pointer wide and as aligned.
    pub(crate) const FITS: () = assert!(
        std::mem::size_of::<T>() >= std::mem::size_of::<NonNull<T>>()
            && std::mem::align_of::<T>() == std::mem::align_of::<NonNull<T>>()
    );

    /// The slot over CELL.
    ///
    /// # Safety
    /// CELL is a value in the World the slot's Context holds a lease on, so
    /// it outlives the slot. Its accesses follow the World's view rules: one
    /// owner touches World state at a time (the sole owner, or the Green
    /// Baton holder; design-r3 section 12).
    #[inline(always)]
    #[doc(hidden)]
    pub unsafe fn over(cell: NonNull<T>) -> Self {
        let () = Self::FITS;
        Self { cell }
    }

    /// The World cell this slot reaches.
    #[inline(always)]
    #[doc(hidden)]
    pub fn cell(&self) -> NonNull<T> {
        // SAFETY: only `over` constructs a slot, which writes `cell`.
        unsafe { self.cell }
    }
}

impl<T> std::fmt::Debug for WorldSlot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldSlot")
            .field("cell", &self.cell())
            .finish()
    }
}

impl<T> Deref for WorldSlot<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        // SAFETY: `over`'s contract: a live World cell, touched by one owner
        // at a time; this shared borrow excludes a mutable one through the
        // same slot.
        unsafe { self.cell().as_ref() }
    }
}

impl<T> DerefMut for WorldSlot<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as `deref`, with this slot borrowed exclusively.
        unsafe { self.cell().as_mut() }
    }
}

impl<'a, T> IntoIterator for &'a WorldSlot<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&**self).into_iter()
    }
}

impl<'a, T> IntoIterator for &'a mut WorldSlot<T>
where
    &'a mut T: IntoIterator,
{
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;
    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        (&mut **self).into_iter()
    }
}
