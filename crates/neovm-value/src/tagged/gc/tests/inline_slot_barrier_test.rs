//! Slot-barrier fast rejects must retain the real window's obligations.
//! Performance is measured separately; these tests exercise the actual stores
//! and their SATB, tracking and remembered-set effects without new TLS counters.

use super::fake_image::FakeImage;
use super::knobs::set_chunk_map_for_test;
use super::*;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr, set_vector_slot};
use crate::tagged::value::TAG_MASK;

fn installed_heap(generational: bool) -> Box<TaggedHeap> {
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_generational_enabled(generational);
    set_tagged_heap(&mut heap);
    heap
}

fn installed_generational_heap_with_chunk_map(enabled: bool) -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(enabled));
    let heap = installed_heap(true);
    set_chunk_map_for_test(None);
    assert_eq!(heap.chunk_map.is_some(), enabled);
    heap
}

fn make_old_cons(heap: &mut TaggedHeap, owner: TaggedValue, other_roots: &[TaggedValue]) {
    // Keep the canonical young-block logs and cached old counts consistent.
    // Root other live fixtures through every minor needed by the age policy.
    for _ in 0..3 {
        if heap.cons_age_is_old_for_test(owner) {
            break;
        }
        heap.begin_minor_collection().unwrap();
        heap.seed_root(owner);
        for &root in other_roots {
            heap.seed_root(root);
        }
        heap.complete_minor_collection();
        heap.finish_incremental_sweep_now();
    }
    assert!(heap.cons_age_is_old_for_test(owner));
    assert_eq!(heap.cons_region_for_test(), (0, 0));
}

fn make_tenured_vector(owner: TaggedValue) {
    // SAFETY: every caller owns this live vector, with no collector running.
    unsafe { (*(owner.as_veclike_ptr().unwrap() as *mut GcHeader)).tenured = true };
}

#[test]
fn generational_inline_slot_immediates_leave_old_cards_and_logs_clean() {
    let mut heap = installed_heap(true);
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    make_old_cons(&mut heap, cons, &[vector]);
    make_tenured_vector(vector);

    for value in [TaggedValue::fixnum(-17), TaggedValue::char('λ')] {
        assert!(set_cons_car(cons, value));
        assert!(set_cons_cdr(cons, value));
        assert!(set_vector_slot(vector, 0, value));
        assert_eq!(cons.cons_car(), value);
        assert_eq!(cons.cons_cdr(), value);
        assert_eq!(vector.as_vector_data().unwrap().as_slice()[0], value);
        assert!(!heap.owner_card_is_dirty_for_test(cons));
        assert!(heap.current_mutator_gc().remset.is_empty());
    }

    let young_cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let young_vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    assert!(set_cons_car(young_cons, vector));
    assert!(set_vector_slot(young_vector, 0, cons));
    assert!(!heap.owner_card_is_dirty_for_test(young_cons));
    assert!(heap.current_mutator_gc().remset.is_empty());
}

#[test]
fn generational_inline_slot_symbols_still_dirty_cards_and_remember_vectors() {
    for symbol in [TaggedValue::NIL, TaggedValue::T] {
        let mut heap = installed_heap(true);
        let cons = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::fixnum(2));
        let vector = heap.alloc_vector(vec![TaggedValue::fixnum(3)]);
        make_old_cons(&mut heap, cons, &[vector]);
        make_tenured_vector(vector);
        assert!(set_cons_car(cons, symbol));
        assert!(set_cons_cdr(cons, symbol));
        assert!(set_vector_slot(vector, 0, symbol));
        assert!(heap.owner_card_is_dirty_for_test(cons));
        assert_eq!(heap.current_mutator_gc().remset, [vector]);
    }
}

#[test]
fn inline_slot_immediate_overwrites_capture_cons_and_vector_satb_preimages() {
    for generational in [false, true] {
        for value in [TaggedValue::fixnum(23), TaggedValue::char('界')] {
            let mut heap = installed_heap(generational);
            let car_child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            let cdr_child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            let vector_child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            let cons = heap.alloc_cons(car_child, cdr_child);
            let vector = heap.alloc_vector(vec![vector_child]);
            heap.set_concurrent_active_for_test(true);

            assert!(set_cons_car(cons, value));
            assert!(set_cons_cdr(cons, value));
            assert!(set_vector_slot(vector, 0, value));
            let preimages = heap.take_satb_shared_for_test();
            for child in [car_child, cdr_child, vector_child] {
                assert!(
                    preimages.contains(&child),
                    "lost {child:?}, GEN1={generational}"
                );
            }
            heap.set_concurrent_active_for_test(false);
        }
    }
}

#[test]
fn inline_slot_immediates_preserve_owner_tracking_and_exact_write_records() {
    for generational in [false, true] {
        let mut heap = installed_heap(generational);
        let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
        heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
        let values = [TaggedValue::fixnum(31), TaggedValue::char('x')];
        for value in values {
            assert!(set_cons_car(cons, value));
            assert!(set_cons_cdr(cons, value));
            assert!(set_vector_slot(vector, 0, value));
        }
        assert!(heap.is_dirty_owner(cons));
        assert!(heap.is_dirty_owner(vector));
        let records = heap.dirty_writes();
        assert_eq!(records.len(), 6);
        for (records, value) in records.chunks_exact(3).zip(values) {
            for (record, (owner, kind, slot)) in records.iter().zip([
                (cons, HeapWriteKind::ConsCar, 0),
                (cons, HeapWriteKind::ConsCdr, 1),
                (vector, HeapWriteKind::VectorSlot, 0),
            ]) {
                assert_eq!(record.owner, owner);
                assert_eq!(record.kind, kind);
                assert_eq!(record.slot, Some(slot));
                assert_eq!(record.value, Some(value));
            }
        }
    }
}

#[test]
fn gen0_inline_slot_mapped_immediates_keep_persistent_owner_bookkeeping() {
    for value in [TaggedValue::fixnum(41), TaggedValue::char('m')] {
        let mut heap = installed_heap(false);
        let image = FakeImage::leak(false);
        let cons = image.register_cons(&mut heap);
        let vector = image.register_vector(&mut heap);
        assert!(set_cons_car(cons, value));
        assert!(set_cons_cdr(cons, value));
        assert!(set_vector_slot(vector, 0, value));
        assert!(heap.mapped_remembered.contains(&cons.bits()));
        assert!(heap.mapped_remembered.contains(&vector.bits()));
        assert!(heap.current_mutator_gc().remset.is_empty());
    }
}

#[test]
fn gen0_inline_slot_tenured_immediates_outside_window_keep_bookkeeping() {
    let mut heap = installed_heap(false);
    let image = FakeImage::leak(false);
    image.register_cons(&mut heap);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL]);
    // GEN0 has no old generation for runtime allocations: the only tenured
    // owner there is a loader-stamped image resident. Stamp the row and let
    // the first partition cycle make it permanent; it stays outside the dump
    // span, so the inline store still takes the remembered gate.
    assert_eq!(heap.note_image_object(vector), ImageStamp::Stamped);
    heap.collect_exact(std::iter::once(vector));
    assert!(heap.is_tenured_for_test(vector));
    assert!(!heap.is_remembered_for_test(vector));
    assert!(!heap.barrier_window().covers(vector.bits() & !TAG_MASK));
    assert!(set_vector_slot(vector, 0, TaggedValue::fixnum(47)));
    assert!(heap.mapped_remembered.contains(&vector.bits()));
    assert!(unsafe { &*(vector.as_veclike_ptr().unwrap() as *const GcHeader) }.is_remembered());
    assert!(heap.current_mutator_gc().remset.is_empty());
}

#[test]
fn inline_slot_heap_switch_rederives_generation_and_real_window_obligations() {
    let mut generational = installed_heap(true);
    let cons = generational.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    make_old_cons(&mut generational, cons, &[]);
    assert!(set_cons_car(cons, TaggedValue::fixnum(53)));
    assert!(!generational.owner_card_is_dirty_for_test(cons));

    let mut legacy = installed_heap(false);
    let image = FakeImage::leak(false);
    let mapped = image.register_cons(&mut legacy);
    assert!(set_cons_car(mapped, TaggedValue::char('s')));
    assert!(legacy.mapped_remembered.contains(&mapped.bits()));

    set_tagged_heap(&mut generational);
    let child = generational.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    assert!(set_cons_car(cons, child));
    generational.set_concurrent_active_for_test(true);
    set_tagged_heap(&mut legacy);
    set_tagged_heap(&mut generational);
    assert!(set_cons_car(cons, TaggedValue::fixnum(59)));
    assert!(generational.take_satb_shared_for_test().contains(&child));
    generational.set_concurrent_active_for_test(false);
}

#[test]
fn generational_inline_cons_age_marks_only_old_owners_with_chunk_map_on_and_off() {
    for chunk_map in [false, true] {
        // Use separate heaps so an old cell's dirty card cannot hide an
        // erroneous young-owner mark on another cell of that same card.
        for old in [false, true] {
            let mut heap = installed_generational_heap_with_chunk_map(chunk_map);
            let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
            if old {
                make_old_cons(&mut heap, owner, &[]);
            }
            let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            assert!(set_cons_car(owner, child));
            assert!(set_cons_cdr(owner, TaggedValue::T));
            assert_eq!(owner.cons_car(), child);
            assert_eq!(owner.cons_cdr(), TaggedValue::T);
            assert_eq!(heap.owner_card_is_dirty_for_test(owner), old);
            assert!(heap.current_mutator_gc().remset.is_empty());
        }
    }
}

// A foreign allocation deliberately has the same granule alignment as an
// owned block. Its first cell passes the block offset/alignment checks, but
// the chunk map and registry do not own it. Nonzero trailing bytes make an
// unproven age-byte read look old, exposing an erroneous card mark. The
// second cell has only the eight-byte alignment promised by from_cons_ptr.
#[repr(C, align(65536))]
struct ForeignConses {
    aligned: ConsCell,
    spacer: u64,
    eight_aligned: ConsCell,
    tail: [u8; CONS_BLOCK_BYTES - 2 * size_of::<ConsCell>() - size_of::<u64>()],
}

impl ForeignConses {
    fn new() -> Box<Self> {
        // `ConsCell` words are interior-mutable, so the cell is not `Copy`;
        // build one per fixture slot.
        let cell = || ConsCell {
            car: crate::tagged::header::ConsWord::new(TaggedValue::NIL),
            cdr_or_next: crate::tagged::header::ConsCdrWord::new(TaggedValue::NIL),
        };
        Box::new(Self {
            aligned: cell(),
            spacer: 0,
            eight_aligned: cell(),
            tail: [1; CONS_BLOCK_BYTES - 2 * size_of::<ConsCell>() - size_of::<u64>()],
        })
    }

    fn owners(&mut self) -> [TaggedValue; 2] {
        // SAFETY: derive writable raw pointers from this exclusive borrow.
        // Both fields remain live for this fixture's lifetime and satisfy
        // ConsCell's eight-byte alignment requirement.
        unsafe {
            [
                TaggedValue::from_cons_ptr(std::ptr::addr_of_mut!(self.aligned).cast_const()),
                TaggedValue::from_cons_ptr(std::ptr::addr_of_mut!(self.eight_aligned).cast_const()),
            ]
        }
    }
}

impl Drop for ForeignConses {
    fn drop(&mut self) {
        // Foreign storage has no ConsBlock destructor. Clear only these
        // exact sticky observation bits before their addresses can be reused.
        for owner in self.owners() {
            let (word, mask) = cons_collection_observed_word(owner.bits());
            if word.fetch_and(!mask, Ordering::Release) & mask != 0 {
                advance_collection_observation_epoch();
            }
        }
    }
}

#[test]
fn generational_inline_cons_age_rejects_foreign_boxed_owners_before_trailer_access() {
    assert_eq!(size_of::<ForeignConses>(), CONS_BLOCK_BYTES);
    for chunk_map in [false, true] {
        let mut heap = installed_generational_heap_with_chunk_map(chunk_map);
        let mut cells = ForeignConses::new();
        let owners = cells.owners();
        assert!(ConsBlock::ptr_is_cell_aligned(owners[0].xcons_ptr()));
        assert_eq!((owners[1].bits() & !TAG_MASK) % 16, 8);
        assert!(!ConsBlock::ptr_is_cell_aligned(owners[1].xcons_ptr()));
        let child = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        for owner in owners {
            assert!(!heap.owner_is_mapped(owner));
            assert!(heap.old_cons_trailer(owner).is_none());
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
