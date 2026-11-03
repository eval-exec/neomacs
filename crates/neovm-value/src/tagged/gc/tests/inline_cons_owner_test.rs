//! Allocation-cursor ownership proofs must retain age and barrier semantics.

use super::knobs::set_chunk_map_for_test;
use super::*;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr};
use crate::tagged::value::TAG_MASK;

fn heap_with_chunk_map(enabled: bool) -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(enabled));
    let mut heap = Box::new(TaggedHeap::new());
    set_chunk_map_for_test(None);
    heap.set_generational_enabled(true);
    set_tagged_heap(&mut heap);
    assert_eq!(heap.chunk_map.is_some(), enabled);
    heap
}

fn address(owner: TaggedValue) -> usize {
    owner.bits() & !TAG_MASK
}

fn block_base(address: usize) -> usize {
    address & !(CONS_BLOCK_BYTES - 1)
}

fn cons(heap: &mut TaggedHeap) -> TaggedValue {
    heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL)
}

fn promoted_cons(heap: &mut TaggedHeap) -> TaggedValue {
    let owner = cons(heap);
    // Exercise canonical promotion under either one- or two-minor policies.
    // Admission closes the allocation region before changing any age state.
    for _ in 0..3 {
        if heap.cons_age_is_old_for_test(owner) {
            break;
        }
        heap.begin_minor_collection().unwrap();
        heap.seed_root(owner);
        heap.complete_minor_collection();
        heap.finish_incremental_sweep_now();
    }
    assert!(heap.cons_age_is_old_for_test(owner));
    assert_eq!(heap.cons_region_for_test(), (0, 0));
    owner
}

fn store_and_check_age(heap: &TaggedHeap, owner: TaggedValue, old: bool) {
    // Symbols take the nonfixnum barrier without reopening an allocation
    // region. Separate age fixtures keep dirty-card aliases from hiding bugs.
    assert!(set_cons_car(owner, TaggedValue::T));
    assert!(set_cons_cdr(owner, TaggedValue::NIL));
    assert_eq!(owner.cons_car(), TaggedValue::T);
    assert_eq!(owner.cons_cdr(), TaggedValue::NIL);
    assert_eq!(heap.owner_card_is_dirty_for_test(owner), old);
    assert!(heap.current_mutator_gc().remset.is_empty());
}

#[test]
fn cons_cursor_barrier_exhausted_region_still_identifies_live_block() {
    for chunk_map in [false, true] {
        for old in [false, true] {
            let mut heap = heap_with_chunk_map(chunk_map);
            let owner = if old {
                let owner = promoted_cons(&mut heap);
                cons(&mut heap);
                owner
            } else {
                cons(&mut heap)
            };
            let (_, limit) = heap.cons_region_for_test();
            while heap.cons_region_for_test().0 < limit {
                cons(&mut heap);
            }
            let (cursor, end) = heap.cons_region_for_test();
            let base = block_base(address(owner));
            assert_ne!(cursor, 0);
            assert_eq!(cursor, limit);
            assert_eq!(cursor, end);
            assert_eq!(block_base(cursor), base);
            assert!(cursor < base + CONS_BLOCK_SIZE * size_of::<ConsCell>());
            store_and_check_age(&heap, owner, old);
        }
    }
}

#[test]
fn cons_cursor_barrier_payload_end_is_not_the_next_block() {
    for chunk_map in [false, true] {
        for old in [false, true] {
            let mut heap = heap_with_chunk_map(chunk_map);
            let owner = if old {
                promoted_cons(&mut heap)
            } else {
                cons(&mut heap)
            };
            let base = block_base(address(owner));
            let payload_end = base + CONS_BLOCK_SIZE * size_of::<ConsCell>();
            while heap.cons_region_for_test().0 != payload_end {
                cons(&mut heap);
            }
            assert_eq!(heap.cons_blocks.len(), 1);
            assert_eq!(heap.cons_region_for_test(), (payload_end, payload_end));
            assert_eq!(block_base(payload_end), base);
            store_and_check_age(&heap, owner, old);
        }
    }
}

#[test]
fn cons_cursor_barrier_closed_region_keeps_owned_fallbacks() {
    for chunk_map in [false, true] {
        for old in [false, true] {
            let mut heap = heap_with_chunk_map(chunk_map);
            let owner = if old {
                promoted_cons(&mut heap)
            } else {
                cons(&mut heap)
            };
            heap.close_alloc_regions();
            assert_eq!(heap.cons_region_for_test(), (0, 0));
            store_and_check_age(&heap, owner, old);
            assert_eq!(heap.cons_region_for_test(), (0, 0));
        }
    }
}

#[test]
fn cons_cursor_barrier_current_block_can_hold_old_and_young_cells() {
    for chunk_map in [false, true] {
        let mut heap = heap_with_chunk_map(chunk_map);
        let old = promoted_cons(&mut heap);
        let young = cons(&mut heap);
        let (cursor, _) = heap.cons_region_for_test();
        assert_ne!(cursor, 0);
        assert_eq!(block_base(cursor), block_base(address(old)));
        assert_eq!(block_base(cursor), block_base(address(young)));
        assert!(heap.cons_age_is_old_for_test(old));
        assert!(!heap.cons_age_is_old_for_test(young));
        // Check the young owner before dirtying the shared card through old.
        store_and_check_age(&heap, young, false);
        store_and_check_age(&heap, old, true);
    }
}

#[test]
fn cons_cursor_barrier_older_block_uses_ownership_fallback() {
    for chunk_map in [false, true] {
        for old in [false, true] {
            let mut heap = heap_with_chunk_map(chunk_map);
            let owner = if old {
                promoted_cons(&mut heap)
            } else {
                cons(&mut heap)
            };
            while heap.cons_blocks.len() == 1 {
                cons(&mut heap);
            }
            let (cursor, _) = heap.cons_region_for_test();
            assert_ne!(cursor, 0);
            assert_ne!(block_base(cursor), block_base(address(owner)));
            store_and_check_age(&heap, owner, old);
        }
    }
}

#[test]
fn cons_cursor_barrier_open_region_does_not_bypass_satb_window() {
    for chunk_map in [false, true] {
        for immediate in [TaggedValue::fixnum(71), TaggedValue::char('λ')] {
            let mut heap = heap_with_chunk_map(chunk_map);
            // Arming after allocation would close the region. Allocate after
            // arming so the owner window and cursor proof are both eligible.
            heap.set_concurrent_active_for_test(true);
            let child = cons(&mut heap);
            let owner = heap.alloc_cons(child, TaggedValue::NIL);
            let (cursor, _) = heap.cons_region_for_test();
            assert_ne!(cursor, 0);
            assert_eq!(block_base(cursor), block_base(address(owner)));
            assert!(set_cons_car(owner, immediate));
            assert_eq!(owner.cons_car(), immediate);
            assert!(heap.take_satb_shared_for_test().contains(&child));
            heap.set_concurrent_active_for_test(false);
        }
    }
}
