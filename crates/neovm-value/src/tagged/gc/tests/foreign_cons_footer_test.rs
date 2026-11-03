//! Valid foreign cells need no owned-block payload geometry.

use super::*;
use crate::tagged::gc::knobs::set_chunk_map_for_test;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr};
use crate::tagged::value::TAG_MASK;
use std::sync::atomic::Ordering;

// Both live cells lie beyond the owned allocator's payload limit. Reading
// an owned age byte before proving ownership would use an invalid index.
// One cell has only the eight-byte alignment from_cons_ptr promises.
#[repr(C, align(65536))]
struct ForeignFooterConses {
    prefix: [u8; CONS_BLOCK_BYTES - 3 * size_of::<ConsCell>() - size_of::<u64>()],
    eight_aligned: ConsCell,
    spacer: u64,
    aligned: ConsCell,
    tail: [u8; size_of::<ConsCell>()],
}

impl ForeignFooterConses {
    fn new() -> Box<Self> {
        // `ConsCell` words are interior-mutable, so the cell is not `Copy`;
        // build one per fixture slot.
        let cell = || ConsCell {
            car: crate::tagged::header::ConsWord::new(TaggedValue::NIL),
            cdr_or_next: crate::tagged::header::ConsCdrWord::new(TaggedValue::NIL),
        };
        Box::new(Self {
            prefix: [1; CONS_BLOCK_BYTES - 3 * size_of::<ConsCell>() - size_of::<u64>()],
            eight_aligned: cell(),
            spacer: 0,
            aligned: cell(),
            tail: [1; size_of::<ConsCell>()],
        })
    }

    fn owners(&mut self) -> [TaggedValue; 2] {
        // SAFETY: derive writable raw pointers from this exclusive borrow.
        // The fields stay live and eight-byte aligned for the fixture's
        // lifetime; neither belongs to a GC-owned ConsBlock.
        unsafe {
            [
                TaggedValue::from_cons_ptr(std::ptr::addr_of_mut!(self.eight_aligned).cast_const()),
                TaggedValue::from_cons_ptr(std::ptr::addr_of_mut!(self.aligned).cast_const()),
            ]
        }
    }
}

impl Drop for ForeignFooterConses {
    fn drop(&mut self) {
        // Foreign storage does not run ConsBlock's observation cleanup.
        // Retire only these objects' sticky bits before address reuse.
        for owner in self.owners() {
            let (word, mask) = cons_collection_observed_word(owner.bits());
            if word.fetch_and(!mask, Ordering::Release) & mask != 0 {
                advance_collection_observation_epoch();
            }
        }
    }
}

#[test]
fn generational_slot_barrier_rejects_foreign_footer_cells_before_age_access() {
    assert_eq!(size_of::<ForeignFooterConses>(), CONS_BLOCK_BYTES);
    for chunk_map in [false, true] {
        for closed_region in [false, true] {
            set_chunk_map_for_test(Some(chunk_map));
            let mut heap = Box::new(TaggedHeap::new());
            set_chunk_map_for_test(None);
            heap.set_generational_enabled(true);
            set_tagged_heap(&mut heap);
            assert_eq!(heap.chunk_map.is_some(), chunk_map);
            let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            if closed_region {
                heap.close_alloc_regions();
            }

            let mut cells = ForeignFooterConses::new();
            let owners = cells.owners();
            assert_eq!((owners[0].bits() & !TAG_MASK) % 16, 8);
            assert_eq!((owners[1].bits() & !TAG_MASK) % 16, 0);
            for owner in owners {
                let addr = owner.bits() & !TAG_MASK;
                assert!(addr & (CONS_BLOCK_ALIGN - 1) >= CONS_CELLS_BYTES);
                assert!(!heap.owner_is_mapped(owner));
                assert!(!heap.owner_card_is_dirty_for_test(owner));
                assert!(set_cons_car(owner, child));
                assert!(set_cons_cdr(owner, TaggedValue::T));
                assert_eq!(owner.cons_car(), child);
                assert_eq!(owner.cons_cdr(), TaggedValue::T);
                assert!(!heap.owner_card_is_dirty_for_test(owner));
                assert!(heap.current_mutator_gc().remset.is_empty());
            }
        }
    }
}
