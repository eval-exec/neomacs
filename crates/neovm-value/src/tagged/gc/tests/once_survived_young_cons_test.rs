//! A cons that survived one minor and stayed young.
//!
//! Two-minor aging leaves the age byte young and the block old-count
//! unchanged after the first survival, and sets only the mark bit.
//! The store filter must keep treating that cell as young: the no-old-block
//! certificate still covers it, a mixed block still trusts the age byte, and
//! an open real barrier window still logs the overwritten preimage.

use super::*;
use crate::tagged::gc::knobs::set_chunk_map_for_test;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr};
use crate::tagged::value::TAG_MASK;
use std::cell::Cell;

fn installed_heap(chunk_map: bool) -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(chunk_map));
    let mut heap = Box::new(TaggedHeap::new());
    set_chunk_map_for_test(None);
    heap.set_generational_enabled(true);
    heap.gc_threshold = usize::MAX / 4;
    set_tagged_heap(&mut heap);
    assert_eq!(heap.chunk_map.is_some(), chunk_map);
    heap
}

fn address(value: TaggedValue) -> usize {
    value.bits() & !TAG_MASK
}

fn block_base(value: TaggedValue) -> usize {
    ConsBlock::block_base_for_ptr(value.xcons_ptr())
}

fn slow_calls() -> usize {
    BARRIER_SLOW_CALLS.with(Cell::get)
}

/// Collector order for a first survival that does not promote: close the
/// region, then mark the live cell. Age and the canonical old-count stay
/// young. This is the store-path image of two-minor aging, not a second
/// promotion policy.
fn model_once_survived_young(heap: &mut TaggedHeap, owner: TaggedValue) {
    heap.close_alloc_regions();
    let ptr = owner.xcons_ptr();
    let base = ConsBlock::block_base_for_ptr(ptr);
    let index = heap.cons_block_index_by_base[&base];
    assert!(
        heap.cons_blocks[index].mark_cell_offset(ConsBlock::ptr_offset(ptr)),
        "first survival sets a clear mark"
    );
    assert!(heap.cons_blocks[index].is_marked_ptr(ptr));
    assert!(!heap.cons_age_is_old_for_test(owner));
    assert!(!heap.value_is_old_for_test(owner));
    assert_eq!(heap.cons_region_for_test(), (0, 0));
    assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection().unwrap();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    heap.gc_threshold = usize::MAX / 4;
}

fn promote_owner(heap: &mut TaggedHeap, owner: TaggedValue) {
    for _ in 0..3 {
        if heap.cons_age_is_old_for_test(owner) {
            break;
        }
        minor(heap, &[owner]);
    }
    assert!(heap.cons_age_is_old_for_test(owner));
    assert_eq!(heap.cons_region_for_test(), (0, 0));
}

#[test]
fn once_survived_young_cons_keeps_the_no_old_certificate() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let owner = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        let base = block_base(owner);
        model_once_survived_young(&mut heap, owner);
        let fresh = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
        assert_eq!(block_base(fresh), base);
        assert!(heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(heap.region_book.no_old_cons_block_contains(address(fresh)));
        let child = heap.alloc_cons(TaggedValue::fixnum(11), TaggedValue::NIL);
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert!(set_cons_cdr(owner, TaggedValue::fixnum(5)));
        assert_eq!(slow_calls(), before);
        assert!(!heap.owner_card_is_dirty_for_test(owner));
        assert!(!heap.cons_age_is_old_for_test(owner));
        assert!(
            heap.cons_blocks[heap.cons_block_index_by_base[&base]].is_marked_ptr(owner.xcons_ptr())
        );
        assert_eq!(owner.cons_car(), child);
        assert_eq!(owner.cons_cdr(), TaggedValue::fixnum(5));
    }
}

#[test]
fn once_survived_young_cons_outside_the_open_block_stays_young() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let base = block_base(owner);
        model_once_survived_young(&mut heap, owner);
        let mut later = owner;
        while block_base(later) == base {
            later = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
        }
        assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(heap.region_book.no_old_cons_block_contains(address(later)));
        let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        let before = slow_calls();
        assert!(set_cons_cdr(owner, child));
        assert_eq!(slow_calls(), before);
        assert!(!heap.owner_card_is_dirty_for_test(owner));
        assert!(!heap.cons_age_is_old_for_test(owner));
        assert_eq!(owner.cons_cdr(), child);
    }
}

#[test]
fn once_survived_young_cons_in_a_mixed_block_uses_its_age_byte() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        let old = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        promote_owner(&mut heap, old);
        let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        assert_eq!(block_base(owner), block_base(old));
        model_once_survived_young(&mut heap, owner);
        let reopened = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::NIL);
        assert_eq!(block_base(reopened), block_base(old));
        assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(!heap.region_book.no_old_cons_block_contains(address(old)));
        let child = heap.alloc_cons(TaggedValue::fixnum(13), TaggedValue::NIL);
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert_eq!(slow_calls(), before);
        assert!(!heap.owner_card_is_dirty_for_test(owner));
        assert!(!heap.cons_age_is_old_for_test(owner));
        assert!(heap.cons_age_is_old_for_test(old));
        assert!(set_cons_cdr(old, child));
        assert!(heap.owner_card_is_dirty_for_test(old));
        assert_eq!(owner.cons_car(), child);
        assert_eq!(old.cons_cdr(), child);
    }
}

#[test]
fn once_survived_young_cons_still_logs_an_open_window_preimage() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(chunk_map);
        // Survive while allocating white, so the mark is the survival bit
        // and not allocate-black. Then arm the real window and reopen the
        // no-old certificate; the overwrite must still log its preimage.
        let child = heap.alloc_cons(TaggedValue::fixnum(17), TaggedValue::NIL);
        let owner = heap.alloc_cons(child, TaggedValue::NIL);
        let base = block_base(owner);
        model_once_survived_young(&mut heap, owner);
        heap.set_concurrent_active_for_test(true);
        let reopened = heap.alloc_cons(TaggedValue::fixnum(19), TaggedValue::NIL);
        assert_eq!(block_base(reopened), base);
        assert!(heap.region_book.no_old_cons_block_contains(address(owner)));
        assert!(set_cons_car(owner, TaggedValue::fixnum(23)));
        assert!(heap.take_satb_shared_for_test().contains(&child));
        assert!(!heap.cons_age_is_old_for_test(owner));
        assert!(
            heap.cons_blocks[heap.cons_block_index_by_base[&base]].is_marked_ptr(owner.xcons_ptr())
        );
        assert_eq!(owner.cons_car(), TaggedValue::fixnum(23));
        heap.set_concurrent_active_for_test(false);
    }
}
