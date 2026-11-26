//! Phase 1c zero-sized heap capability tokens (`HeapRef<'h>`, `HeapMut<'h>`).
//!
//! These tokens tie heap payload borrows and bulk mutations to the lifetime
//! `'h` of a borrowed [`TaggedHeap`] (via `TaggedHeap`, `Context`, or
//! `EditHost`) without adding runtime size or register traffic:
//! - [`HeapRef<'h>`] is `Copy`, zero-sized, `!Send`, `!Sync`. A payload borrow
//!   derived from it lives at most `'h`, so borrowing `&mut Context` for a
//!   safepoint, moving `TaggedHeap` into [`TaggedHeap::shutdown`] or `drop`,
//!   or taking `&mut HeapMut` from the same owner conflicts at compile time
//!   (`E0502` / `E0505`).
//! - [`HeapMut<'h>`] is non-`Copy`, zero-sized, `!Send`, `!Sync`. Taking
//!   `&mut HeapMut<'_>` conflicts at compile time with live [`HeapRef`]
//!   borrows derived from the same owner (`E0502`) and with nested
//!   `with_*_mut_in` calls on the same `HeapMut` (`E0499`).
//!
//! ## Not yet enforced
//!
//! Today (Phase 1c.0) these tokens do **not** yet close every safe-code path:
//! 1. **Heap identity / leaked heaps (Phase 2/3)**: readers do not verify that
//!    a `Value` belongs to the `TaggedHeap` that minted the token. Because
//!    `TaggedHeap::new()` is safe, an unrelated or `Box::leak`ed `TaggedHeap`
//!    can mint a `HeapRef` (including `HeapRef<'static>`) whose lifetime is
//!    independent of the `Value`'s real owning `Context`. Single-active-Context
//!    enforcement belongs to Phase 2 and generative `'heap` provenance branding
//!    belongs to Phase 3.
//! 2. **Legacy tokenless bulk mutators (Phase 1c.1)**: `replace_*`, `clear_*`,
//!    and `with_*_mut` remain callable without `&mut HeapMut` until Phase 1c.1
//!    migrates their callers and bans them. (`with_tagged_heap` is already
//!    `unsafe fn` and guarded against minting `HeapRef`/`HeapMut`.)
//! 3. **Single-slot atomic stores vs `LispValueSlice` (closed in Phase 1b)**:
//!    `LispValueSlice` is `#[repr(transparent)]` over `[AtomicTaggedValue]`
//!    and exposes only relaxed atomic loads (`get`, `at`, `iter`, `to_vec`)
//!    with `as_slice()` and `Deref<Target = [TaggedValue]>` withdrawn, so
//!    token-free single-slot atomic stores (`set_vector_slot`, etc.) do not
//!    alias immutable `&[TaggedValue]` references under Stacked/Tree Borrows.
//! 4. **Rooting discipline (out of scope for lifetimes)**: an unrooted `Copy`
//!    `Value` word parked in a Rust local or struct across a safepoint is not
//!    tracked by borrowck and requires explicit rooting (`InFlightRoots` /
//!    `GcRoot`).

use std::marker::PhantomData;

use super::gc::TaggedHeap;

/// Shared capability token tying payload borrows to the lifetime `'h` of a
/// borrowed [`TaggedHeap`] / `Context` / `EditHost`.
///
/// Zero-sized, `Copy`, `!Send`, `!Sync`, covariant in `'h`. See the module
/// docs for the invariants that are not yet enforced in Phase 1c.0.
#[derive(Clone, Copy, Debug)]
pub struct HeapRef<'h> {
    _borrow: PhantomData<&'h TaggedHeap>,
    _confined: PhantomData<*const ()>,
}

/// Exclusive capability token for `*_in` bulk payload mutators
/// (`with_*_mut_in`, `replace_*_in`, `clear_*_in`), tied to the lifetime `'h`
/// of a mutable [`TaggedHeap`] / `Context` / `EditHost` borrow.
///
/// Zero-sized, non-`Copy`, `!Send`, `!Sync`. See the module docs for the
/// invariants that are not yet enforced in Phase 1c.0.
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
    #[allow(clippy::should_implement_trait)]
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
    use crate::tagged::header::{LispValueSlice, VectorObj};
    use crate::value::Value;

    /// Strict-provenance helper: tie a raw `VectorObj` arena slot pointer to a
    /// `HeapRef<'h>` without going through `TaggedValue`'s integer-to-pointer
    /// tag cast (`as_veclike_ptr`).
    fn borrow_vector_slot<'h>(slot: *const VectorObj, _heap: HeapRef<'h>) -> &'h LispValueSlice {
        // SAFETY: `slot` is an initialized `VectorObj` allocated from the same
        // heap that minted `_heap`, and no sweep or bulk mutation occurs
        // during `'h`.
        unsafe { (*slot).data.as_value_slice() }
    }

    #[test]
    fn heap_tokens_are_zero_sized() {
        assert_eq!(std::mem::size_of::<HeapRef<'_>>(), 0);
        assert_eq!(std::mem::size_of::<HeapMut<'_>>(), 0);
    }

    /// Strict-provenance Miri SB+TB test modeling `special_forms.rs:1451`
    /// (`sf_byte_code_literal_value` -> `make_byte_code_from_slots`):
    /// - Borrows a `&'h LispValueSlice` directly from a `VectorObj` arena slot
    ///   (no integer-to-pointer cast, so Stacked Borrows and Tree Borrows check
    ///   strict non-wildcard provenance).
    /// - Allocates >1,024 vectors through `with_tagged_heap` (a 64 KiB vector
    ///   page holds 1,024 64-byte slots), asserting `vector_arena_page_count`
    ///   strictly increases (1 -> >=2) while `items` is live.
    /// - Re-reads `items` after real arena page growth.
    ///
    /// Note: substitutes `TaggedHeap::alloc_vector` for `neovm-eval`'s bytecode
    /// allocator, which is not in `neovm-value`.
    #[test]
    fn miri_heap_ref_strict_provenance_survives_page_growth() {
        let heap = crate::tagged::gc::AliasableBox::new(TaggedHeap::new());
        // SAFETY: `heap.root()` is the single AliasableBox provenance root;
        // `TaggedHeap::drop` clears the TLS slot before freeing the heap.
        unsafe { crate::tagged::gc::set_tagged_heap_root(heap.root()) };

        // Allocate the initial vector via `alloc_vector_raw` so we retain its
        // strict-provenance `*mut VectorObj` (no integer-to-pointer cast).
        let slot: *mut VectorObj = unsafe {
            crate::tagged::gc::with_tagged_heap(|h| {
                h.alloc_vector_raw(vec![
                    Value::fixnum(10),
                    Value::fixnum(20),
                    Value::fixnum(30),
                    Value::fixnum(40),
                ])
            })
        };

        let pages_before =
            unsafe { crate::tagged::gc::with_tagged_heap(|h| h.vector_arena_page_count()) };
        assert_eq!(pages_before, 1);

        {
            let h = heap.heap_ref();
            let items = borrow_vector_slot(slot, h);
            assert_eq!(items.len(), 4);
            assert_eq!(items.at(0), Value::fixnum(10));

            // 64 KiB / 64 B = 1,024 vector slots per page; 1,100 allocations
            // force at least one new vector arena page to be opened while
            // `items` stays borrowed.
            let mut allocated = 0usize;
            for i in 0..1100_i64 {
                let _v = unsafe {
                    crate::tagged::gc::with_tagged_heap(|th| {
                        th.alloc_vector(vec![Value::fixnum(i), items.at(1)])
                    })
                };
                allocated += 1;
            }
            assert_eq!(allocated, 1100);

            let pages_after =
                unsafe { crate::tagged::gc::with_tagged_heap(|th| th.vector_arena_page_count()) };
            assert!(
                pages_after > pages_before,
                "expected vector arena page growth (>1024 slots): before={pages_before}, after={pages_after}"
            );

            // Re-read the strict-provenance slice after page growth.
            assert_eq!(
                items,
                &[
                    Value::fixnum(10),
                    Value::fixnum(20),
                    Value::fixnum(30),
                    Value::fixnum(40),
                ]
            );
        }

        drop(heap);
    }

    /// Strict-provenance Phase 1b Miri SB+TB test: interleaves a live
    /// `&'h LispValueSlice` borrow (with zero integer-to-pointer cast) with
    /// single-slot atomic stores (`LispValueVec::store_atomic`) on the same
    /// vector, proving `LispValueSlice([AtomicTaggedValue])` + relaxed loads
    /// never alias immutable `&[TaggedValue]` under Stacked or Tree Borrows.
    #[test]
    fn miri_heap_ref_strict_provenance_interleaved_slot_store() {
        let heap = crate::tagged::gc::AliasableBox::new(TaggedHeap::new());
        // SAFETY: `heap.root()` is the single AliasableBox provenance root;
        // `TaggedHeap::drop` clears the TLS slot before freeing the heap.
        unsafe { crate::tagged::gc::set_tagged_heap_root(heap.root()) };

        let slot: *mut VectorObj = unsafe {
            crate::tagged::gc::with_tagged_heap(|h| {
                h.alloc_vector_raw(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)])
            })
        };

        {
            let h = heap.heap_ref();
            let items = borrow_vector_slot(slot, h);
            assert_eq!(items.at(0), Value::fixnum(1));
            assert_eq!(items.get(1), Some(Value::fixnum(2)));

            // Interleave single-slot atomic stores while `items` stays live.
            // SAFETY: `slot` is live and `store_atomic` publishes through the
            // interior-mutable `AtomicTaggedValue` cell without replacing the
            // backing `Vec`.
            assert!(unsafe { (*slot).data.store_atomic(0, Value::fixnum(99)) });
            assert!(unsafe { (*slot).data.store_atomic(2, Value::fixnum(77)) });

            // Re-read through the same live `LispValueSlice` via relaxed loads.
            assert_eq!(items.at(0), Value::fixnum(99));
            assert_eq!(items.get(1), Some(Value::fixnum(2)));
            assert_eq!(items.last(), Some(Value::fixnum(77)));
            assert_eq!(
                items.iter().collect::<Vec<_>>(),
                vec![Value::fixnum(99), Value::fixnum(2), Value::fixnum(77)]
            );
        }

        drop(heap);
    }

    /// Public `Value` API test for `vector_data_in`, interleaved single-slot
    /// `set_vector_slot`, token-free vector/cons/string allocations, and
    /// sequential `HeapMut` `replace_vector_data_in`.
    ///
    /// Note: `Value::vector_data_in` recovers the pointer from a tagged `usize`
    /// (`as_veclike_ptr`), so Miri assigns wildcard provenance on that read.
    /// Strict non-wildcard SB/TB aliasing across page growth and interleaved
    /// slot stores is tested by
    /// [`miri_heap_ref_strict_provenance_survives_page_growth`] and
    /// [`miri_heap_ref_strict_provenance_interleaved_slot_store`].
    #[test]
    fn miri_heap_ref_vector_slice_survives_allocation() {
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
            assert_eq!(items.at(0), Value::fixnum(10));

            // Phase 1b: single-slot store while `items: &LispValueSlice` is live.
            assert!(v.set_vector_slot(1, Value::fixnum(25)));
            assert_eq!(items.at(1), Value::fixnum(25));

            let copy_vec = Value::vector(items.to_vec());
            let pair = Value::cons(items.at(0), items.at(3));
            let s = Value::string("allocated while vector slice live");

            assert_eq!(
                items,
                &[
                    Value::fixnum(10),
                    Value::fixnum(25),
                    Value::fixnum(30),
                    Value::fixnum(40),
                ]
            );
            assert_eq!(copy_vec.vector_data_in(h).expect("copy"), items);
            assert_eq!(pair.cons_car(), Value::fixnum(10));
            assert_eq!(pair.cons_cdr(), Value::fixnum(40));
            assert_eq!(
                s.lisp_string_in(h).and_then(|ls| ls.as_utf8_str()),
                Some("allocated while vector slice live")
            );
        }

        {
            let mut hm = heap.heap_mut();
            assert!(v.replace_vector_data_in(&mut hm, vec![Value::fixnum(99)]));
            let updated = v.vector_data_in(hm.as_ref()).expect("updated vector");
            assert_eq!(updated, &[Value::fixnum(99)]);
        }

        drop(heap);
    }
}
