//! Phase 1c zero-sized heap capability tokens (`HeapRef<'h>`, `HeapMut<'h>`).
//!
//! These tokens tie heap payload borrows and bulk mutations to the owning
//! [`TaggedHeap`] (or [`Context`] / `EditHost`) without adding runtime size or
//! register traffic:
//! - [`HeapRef<'h>`] is `Copy`, zero-sized, `!Send`, `!Sync`. A payload borrow
//!   derived from it lives at most `'h`, so any `&mut Context` safepoint,
//!   [`TaggedHeap::shutdown`], `drop(heap)`, or [`HeapMut`] bulk mutation
//!   conflicts at compile time (`E0502` / `E0505`).
//! - [`HeapMut<'h>`] is non-`Copy`, zero-sized, `!Send`, `!Sync`. Taking
//!   `&mut HeapMut<'_>` statically excludes both live [`HeapRef`] payload
//!   borrows (`E0502`) and re-entrant `with_*_mut_in` calls (`E0499`).

use std::marker::PhantomData;

use super::gc::TaggedHeap;

/// Shared capability token proving no GC sweep, heap teardown, or bulk payload
/// replacement can occur during `'h`.
///
/// Zero-sized, `Copy`, `!Send`, `!Sync`, covariant in `'h`.
#[derive(Clone, Copy, Debug)]
pub struct HeapRef<'h> {
    _borrow: PhantomData<&'h TaggedHeap>,
    _confined: PhantomData<*const ()>,
}

/// Exclusive capability token permitting bulk payload mutation (`with_*_mut_in`,
/// `replace_*_in`, `clear_*_in`) while proving no [`HeapRef`] payload borrow is
/// live and no nested bulk mutation is active.
///
/// Zero-sized, non-`Copy`, `!Send`, `!Sync`.
#[derive(Debug)]
pub struct HeapMut<'h> {
    _borrow: PhantomData<&'h mut TaggedHeap>,
    _confined: PhantomData<*const ()>,
}

const _: () = {
    assert!(std::mem::size_of::<HeapRef<'_>>() == 0);
    assert!(std::mem::size_of::<HeapMut<'_>>() == 0);
};
static_assertions::assert_not_impl_any!(HeapRef<'_>: Send, Sync);
static_assertions::assert_not_impl_any!(HeapMut<'_>: Send, Sync);

impl<'h> HeapRef<'h> {
    #[inline(always)]
    pub(crate) fn from_heap(_heap: &'h TaggedHeap) -> Self {
        Self {
            _borrow: PhantomData,
            _confined: PhantomData,
        }
    }
}

impl<'h> From<&'h TaggedHeap> for HeapRef<'h> {
    #[inline(always)]
    fn from(heap: &'h TaggedHeap) -> Self {
        Self::from_heap(heap)
    }
}

impl<'h> HeapMut<'h> {
    #[inline(always)]
    pub(crate) fn from_heap(_heap: &'h mut TaggedHeap) -> Self {
        Self {
            _borrow: PhantomData,
            _confined: PhantomData,
        }
    }

    /// Derive a shared [`HeapRef`] tied to this borrow of `self`.
    ///
    /// While the returned [`HeapRef`] (or any payload borrow derived from it)
    /// is live, `&mut self` cannot be taken for a bulk mutation (`E0502`).
    #[inline(always)]
    pub fn as_ref(&self) -> HeapRef<'_> {
        HeapRef {
            _borrow: PhantomData,
            _confined: PhantomData,
        }
    }

    /// Reborrow this exclusive token for a shorter scope.
    #[inline(always)]
    pub fn reborrow(&mut self) -> HeapMut<'_> {
        HeapMut {
            _borrow: PhantomData,
            _confined: PhantomData,
        }
    }
}

impl<'h> From<&'h mut TaggedHeap> for HeapMut<'h> {
    #[inline(always)]
    fn from(heap: &'h mut TaggedHeap) -> Self {
        Self::from_heap(heap)
    }
}

impl TaggedHeap {
    /// Mint a shared [`HeapRef`] tied to `&self`.
    #[inline(always)]
    pub fn heap_ref(&self) -> HeapRef<'_> {
        HeapRef::from_heap(self)
    }

    /// Mint an exclusive [`HeapMut`] tied to `&mut self`.
    #[inline(always)]
    pub fn heap_mut(&mut self) -> HeapMut<'_> {
        HeapMut::from_heap(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn heap_tokens_are_zero_sized() {
        assert_eq!(std::mem::size_of::<HeapRef<'_>>(), 0);
        assert_eq!(std::mem::size_of::<HeapMut<'_>>(), 0);
    }

    /// Miri SB+TB test modeling `special_forms.rs:1451`
    /// (`sf_byte_code_literal_value` -> `make_byte_code_from_slots`): a live
    /// `&'h LispValueSlice` derived from `vector_data_in(heap.heap_ref())`
    /// remains valid across token-free allocations through the TLS heap
    /// (`with_tagged_heap`), including arena page growth, and sequential
    /// `HeapMut` replacement + `shutdown` stay sound.
    #[test]
    fn miri_heap_ref_vector_slice_survives_allocation() {
        // Production heaps are owned by `AliasableBox<TaggedHeap>` (see
        // `tagged/gc/aliasable.rs`): one raw provenance root shared by the
        // owner's `WorldView` and the TLS allocation slot.
        let mut heap = crate::tagged::gc::AliasableBox::new(TaggedHeap::new());
        // SAFETY: `heap.root()` is the single AliasableBox provenance root;
        // `TaggedHeap::drop` clears the TLS slot before freeing the heap.
        unsafe { crate::tagged::gc::set_tagged_heap_root(heap.root()) };

        let v = Value::vector(vec![
            Value::fixnum(10),
            Value::fixnum(20),
            Value::fixnum(30),
            Value::fixnum(40),
        ]);

        {
            let h = heap.heap_ref();
            let items = v.vector_data_in(h).expect("vector");
            assert_eq!(items.len(), 4);
            assert_eq!(items[0], Value::fixnum(10));

            // Perform token-free allocations through the TLS heap while `items`
            // is live (mirroring `make_byte_code_from_slots(items)`).
            let copy_vec = Value::vector(items.to_vec());
            let pair = Value::cons(items[0], items[3]);
            let s = Value::string("allocated while vector slice live");
            let mut extra = Vec::with_capacity(64);
            for i in 0..64_i64 {
                extra.push(Value::vector(vec![Value::fixnum(i), items[1]]));
            }

            // Re-read the original slice after all allocations.
            assert_eq!(
                items.as_slice(),
                &[
                    Value::fixnum(10),
                    Value::fixnum(20),
                    Value::fixnum(30),
                    Value::fixnum(40),
                ]
            );
            assert_eq!(
                copy_vec.vector_data_in(h).expect("copy").as_slice(),
                items.as_slice()
            );
            assert_eq!(pair.cons_car(), Value::fixnum(10));
            assert_eq!(pair.cons_cdr(), Value::fixnum(40));
            assert_eq!(
                s.lisp_string_in(h).and_then(|ls| ls.as_utf8_str()),
                Some("allocated while vector slice live")
            );
            assert_eq!(extra.len(), 64);
        }

        // Sequential bulk mutation via `HeapMut` after the `HeapRef` borrow ends.
        {
            let mut hm = heap.heap_mut();
            assert!(v.replace_vector_data_in(&mut hm, vec![Value::fixnum(99)]));
            let updated = v.vector_data_in(hm.as_ref()).expect("updated vector");
            assert_eq!(updated.as_slice(), &[Value::fixnum(99)]);
        }

        drop(heap);
    }
}
