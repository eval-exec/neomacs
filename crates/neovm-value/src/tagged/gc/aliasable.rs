//! An owning, address-stable box whose allocation has ONE provenance root.
//!
//! The tagged heap is reached through several paths that must all alias the
//! same allocation:
//!
//! - the owner's own accessors (the `Context`'s heap view, the `World`'s
//!   exclusive teardown path);
//! - the thread-local raw pointer installed by `set_tagged_heap`, which
//!   `with_tagged_heap` re-derives `&mut TaggedHeap` from; and
//! - the GC constructors that move the heap into the `World`.
//!
//! A `Box<T>` cannot serve as that shared root: a `Box` is a uniqueness
//! assertion, so a pointer derived from a `&mut T` borrowed out of it is a
//! *child* tag. Under Stacked Borrows the next use of the owning `&mut`
//! pops that child; under Tree Borrows the child is disabled. Re-deriving
//! `&mut` from the stored child is then undefined behavior — which is
//! exactly what the pre-existing Miri finding in `gc_thread.rs` was.
//!
//! [`AliasableBox`] instead converts the `Box` into a raw pointer **once**
//! (`Box::into_raw`), stores it as a [`NonNull`], and never derives access
//! from anything but that pointer:
//!
//! - [`AliasableBox::root`] hands out a copy of the raw root for the TLS
//!   slot and for view construction;
//! - [`AliasableBox::get_mut`] (and `DerefMut`) derive `&mut T` *from the
//!   root*, so every sibling reborrow pops only its predecessor, never the
//!   root; and
//! - `Drop` reclaims with `Box::from_raw` exactly once, after the heap's own
//!   `Drop` has uninstalled the TLS pointer.
//!
//! The consequence every caller must respect: a `&mut TaggedHeap` derived
//! from this root must not be *live* across an access derived from the TLS
//! slot (or another root copy). A later use of that earlier `&mut` is the
//! sibling-reborrow violation the design exists to prevent. In practice this
//! means calling heap accessors freshly (`view.method()`), not binding
//! `let heap = &mut view;` and then crossing a `with_tagged_heap` call.
//!
//! This type is crate-private: it adds no public unsafe surface.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub(crate) struct AliasableBox<T> {
    /// The single provenance root of the allocation, from `Box::into_raw`.
    ptr: NonNull<T>,
    /// `NonNull` alone would let `AliasableBox<T>` auto-derive `Send`/`Sync`
    /// from its payload in future refactors; the box owns `T` outright, so
    /// it keeps the box's ownership transfer semantics.
    _owns: PhantomData<Box<T>>,
}

// SAFETY: this is the ownership of a `Box<T>`; it is `Send` exactly when
// `T: Send`, as `Box<T>` is. It is never `Sync` (no shared mutation path is
// exposed through `&self`), like a boxed value behind `&mut`.
unsafe impl<T: Send> Send for AliasableBox<T> {}

impl<T> AliasableBox<T> {
    /// Take ownership of `value` on the heap.
    pub(crate) fn new(value: T) -> Self {
        Self::from_box(Box::new(value))
    }

    /// Take ownership of an existing box, preserving its allocation and
    /// address.
    pub(crate) fn from_box(value: Box<T>) -> Self {
        // `Box::into_raw` is the allocation's root raw pointer: every later
        // access is derived from it, so no sibling `&mut` can invalidate it.
        let ptr = NonNull::new(Box::into_raw(value)).expect("Box allocation is never null");
        Self {
            ptr,
            _owns: PhantomData,
        }
    }

    /// The one provenance root. Copying this pointer is how views and the
    /// thread-local slot name the allocation; nothing else may be stored.
    #[inline(always)]
    pub(crate) fn root(&self) -> NonNull<T> {
        self.ptr
    }

    /// Exclusive access, derived from the root.
    #[inline(always)]
    pub(crate) fn get_mut(&mut self) -> &mut T {
        // SAFETY: `&mut self` proves no other `&mut T` derived from this
        // owner is live; the root pointer is valid for the whole `self`
        // lifetime, and this keys off `self`'s exclusive borrow in the same
        // way `Box::deref_mut` does.
        unsafe { self.ptr.as_mut() }
    }

    /// Shared access, derived from the root.
    #[inline(always)]
    pub(crate) fn get(&self) -> &T {
        // SAFETY: the root pointer is live for the whole `self` lifetime.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Deref for AliasableBox<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T> DerefMut for AliasableBox<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

impl<T> Drop for AliasableBox<T> {
    fn drop(&mut self) {
        // SAFETY: this owner constructed the pointer with `Box::into_raw` and
        // is its only owner; `Drop` runs once. Any raw copies handed out
        // (the TLS slot) must already be uninstalled by `T`'s own `Drop`
        // (`TaggedHeap::drop` calls `clear_tagged_heap_if_installed`).
        unsafe { drop(Box::from_raw(self.ptr.as_ptr())) }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for AliasableBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AliasableBox")
            .field("ptr", &self.ptr)
            .finish_non_exhaustive()
    }
}
