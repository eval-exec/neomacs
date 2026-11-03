//! Actual pointer stores must preserve real-window obligations before GEN1's
//! ALL-length sentinel is considered.

use super::*;
use crate::tagged::gc::barrier_window::{published_barrier_window, published_cons_barrier_window};
use crate::tagged::gc::fake_image::FakeImage;
use crate::tagged::gc::knobs::set_chunk_map_for_test;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr};
use crate::tagged::value::TAG_MASK;

fn installed_heap(generational: bool, chunk_map: bool) -> Box<TaggedHeap> {
    set_chunk_map_for_test(Some(chunk_map));
    let mut heap = Box::new(TaggedHeap::new());
    set_chunk_map_for_test(None);
    heap.set_generational_enabled(generational);
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.gc_threshold = usize::MAX / 4;
    assert_eq!(heap.chunk_map.is_some(), chunk_map);
    set_tagged_heap(&mut heap);
    heap
}

fn slow_calls() -> usize {
    BARRIER_SLOW_CALLS.with(Cell::get)
}

fn address(value: TaggedValue) -> usize {
    value.bits() & !TAG_MASK
}

fn assert_mirrors(heap: &TaggedHeap) {
    let ordinary = heap.barrier_window();
    assert_eq!(published_barrier_window(), ordinary);
    assert_eq!(
        published_cons_barrier_window(),
        if heap.generational.enabled {
            BarrierWindow::ALL
        } else {
            ordinary
        }
    );
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
    // Real promotion keeps canonical block counts/logs coherent. The owner
    // retains every payload through each policy-dependent minor.
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
fn cons_real_window_first_pointer_overwrites_capture_preimages_with_open_young_region() {
    for generational in [false, true] {
        for chunk_map in [false, true] {
            let mut heap = installed_heap(generational, chunk_map);
            heap.set_concurrent_active_for_test(true);
            let old_car = heap.alloc_cons(TaggedValue::fixnum(101), TaggedValue::NIL);
            let old_cdr = heap.alloc_cons(TaggedValue::fixnum(103), TaggedValue::NIL);
            let new_car = heap.alloc_cons(TaggedValue::fixnum(107), TaggedValue::NIL);
            let new_cdr = heap.alloc_cons(TaggedValue::fixnum(109), TaggedValue::NIL);
            let owner = heap.alloc_cons(old_car, old_cdr);
            assert_ne!(heap.jit.cons_cur.get(), 0);
            if generational {
                assert!(heap.region_book.no_old_cons_block_contains(address(owner)));
            }
            assert_eq!(published_barrier_window(), BarrierWindow::ALL);
            assert_eq!(published_cons_barrier_window(), BarrierWindow::ALL);
            let before = slow_calls();
            assert!(set_cons_car(owner, new_car));
            assert!(set_cons_cdr(owner, new_cdr));
            assert_eq!(slow_calls(), before + 2);
            assert_eq!(owner.cons_car(), new_car);
            assert_eq!(owner.cons_cdr(), new_cdr);
            let preimages = heap.take_satb_shared_for_test();
            assert!(preimages.contains(&old_car));
            assert!(preimages.contains(&old_cdr));
            heap.set_concurrent_active_for_test(false);
            assert_mirrors(&heap);
        }
    }
}

#[test]
fn cons_real_window_first_pointer_stores_follow_span_tracking_heap_switch_and_uninstall() {
    // Construct BOTH heaps before the final uninstall. After uninstall this
    // test inspects mirrors/identity only: no store, fallback or heap creation.
    let mut heaps = [installed_heap(false, true), installed_heap(true, true)];
    for heap in &mut heaps {
        set_tagged_heap(heap);
        assert_eq!(published_barrier_window(), BarrierWindow::NONE);
        assert_mirrors(heap);
        let child = heap.alloc_cons(TaggedValue::fixnum(211), TaggedValue::NIL);
        let owner = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert_eq!(slow_calls(), before);
        assert_eq!(owner.cons_car(), child);

        let image = FakeImage::leak(false);
        let mapped = image.register_cons(heap);
        let span = heap.barrier_window();
        assert_ne!(span, BarrierWindow::NONE);
        assert_ne!(span, BarrierWindow::ALL);
        assert!(span.covers(address(mapped)));
        assert!(!span.covers(address(owner)));
        assert_mirrors(heap);
        let before = slow_calls();
        assert!(set_cons_car(mapped, child));
        assert_eq!(slow_calls(), before + 1);
        assert_eq!(mapped.cons_car(), child);
        if heap.generational.enabled {
            assert!(
                heap.current_mutator_gc()
                    .r_mapped_seen
                    .contains(&mapped.bits())
            );
            assert!(heap.current_mutator_gc().remset.contains(&mapped));
        } else {
            assert!(heap.mapped_remembered.contains(&mapped.bits()));
        }

        heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
        assert_eq!(published_barrier_window(), BarrierWindow::ALL);
        assert_mirrors(heap);
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert!(set_cons_cdr(owner, child));
        assert_eq!(slow_calls(), before + 2);
        assert_eq!(owner.cons_car(), child);
        assert_eq!(owner.cons_cdr(), child);
        assert!(heap.is_dirty_owner(owner));
        let records = heap.dirty_writes();
        assert_eq!(records.len(), 2);
        for (record, (kind, slot)) in records
            .iter()
            .zip([(HeapWriteKind::ConsCar, 0), (HeapWriteKind::ConsCdr, 1)])
        {
            assert_eq!(record.owner, owner);
            assert_eq!(record.kind, kind);
            assert_eq!(record.slot, Some(slot));
            assert_eq!(record.value, Some(child));
        }

        heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
        assert_eq!(published_barrier_window(), span);
        assert_mirrors(heap);
        assert!(heap.dirty_writes().is_empty());
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert_eq!(slow_calls(), before);
        assert_eq!(owner.cons_car(), child);
    }
    // Reinstall each already-created heap. Their real spans and generation
    // protocol must be re-derived from the selected owner, not retained TLS.
    for heap in &mut heaps {
        set_tagged_heap(heap);
        assert_mirrors(heap);
        assert_eq!(current_tagged_heap_identity(), Some(heap.identity()));
    }
    clear_tagged_heap_if_installed(&heaps[0]);
    assert_mirrors(&heaps[1]);
    clear_tagged_heap_if_installed(&heaps[1]);
    assert!(!tagged_heap_is_installed());
    assert_eq!(current_tagged_heap_identity(), None);
    assert_eq!(published_barrier_window(), BarrierWindow::NONE);
    assert_eq!(published_cons_barrier_window(), BarrierWindow::NONE);
}

#[test]
fn cons_real_window_first_old_pointer_card_survives_two_minors_after_all_disarms() {
    for chunk_map in [false, true] {
        let mut heap = installed_heap(true, chunk_map);
        let old_child = heap.alloc_cons(TaggedValue::fixnum(307), TaggedValue::NIL);
        let owner = heap.alloc_cons(old_child, TaggedValue::NIL);
        promote_owner(&mut heap, owner);
        assert!(!heap.owner_card_is_dirty_for_test(owner));

        heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
        assert_eq!(published_barrier_window(), BarrierWindow::ALL);
        // Use a YOUNG owner during ALL so an already dirty old-owner card
        // cannot conceal a missing card mark after the real window disarms.
        let probe_child = heap.alloc_cons(TaggedValue::fixnum(311), TaggedValue::NIL);
        let probe = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
        let before = slow_calls();
        assert!(set_cons_car(probe, probe_child));
        assert_eq!(slow_calls(), before + 1);
        assert!(heap.is_dirty_owner(probe));
        assert!(!heap.owner_card_is_dirty_for_test(owner));
        heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
        assert_eq!(published_barrier_window(), BarrierWindow::NONE);
        assert_eq!(published_cons_barrier_window(), BarrierWindow::ALL);

        let child = heap.alloc_cons(TaggedValue::fixnum(313), TaggedValue::NIL);
        assert!(!heap.cons_age_is_old_for_test(child));
        let before = slow_calls();
        assert!(set_cons_car(owner, child));
        assert_eq!(slow_calls(), before);
        assert_eq!(owner.cons_car(), child);
        assert!(heap.owner_card_is_dirty_for_test(owner));
        for _ in 0..2 {
            // Only the old owner is rooted. No extra store can repair the
            // edge between these minors if survivors stay young for a cycle.
            minor(&mut heap, &[owner]);
            assert_eq!(owner.cons_car(), child);
            assert_eq!(child.cons_car(), TaggedValue::fixnum(313));
            assert_eq!(child.cons_cdr(), TaggedValue::NIL);
        }
    }
}
