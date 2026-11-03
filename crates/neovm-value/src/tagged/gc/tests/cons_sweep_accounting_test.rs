//! GEN1 publication-time accounting, sweep-window births, and conditional release.

use super::cons_allocator::*;
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};
use strum::IntoEnumIterator;

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum DeferredCycle {
    Minor,
    Major,
}

fn heap() -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.set_generational_enabled(true);
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.set_gc_threshold(usize::MAX);
    heap
}

fn base(value: TaggedValue) -> usize {
    ConsBlock::block_base_for_ptr(value.as_cons_ptr().unwrap())
}

fn begin_deferred(heap: &mut TaggedHeap, cycle: DeferredCycle, roots: &[TaggedValue]) {
    match cycle {
        DeferredCycle::Minor => {
            heap.begin_minor_collection().unwrap();
            for &root in roots {
                heap.seed_root(root);
            }
            heap.complete_minor_collection();
        }
        DeferredCycle::Major => {
            // SAFETY: this fixture owns the sole heap writer and all roots;
            // no callback or allocation safepoint runs through capture.
            let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
            for &root in roots {
                capture.heap_mut().seed_root(root);
            }
            // An unpublished major still uses the real full-mark/promotion
            // and deferred sweep path, without a timing-dependent worker.
            drop(capture);
            heap.incremental_drain_all();
            heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
        }
    }
    assert!(heap.sweep_in_progress());
}

fn finish(heap: &mut TaggedHeap, expected: usize) {
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
    assert_eq!(heap.cons_live_count_exact(), expected);
    assert_eq!(heap.allocated_count(), expected);
    assert_eq!(heap.cons_live_count, expected);
    let census: usize = heap
        .cons_blocks
        .iter()
        .map(ConsBlock::count_live_generational)
        .sum();
    assert_eq!(census, expected);
}

#[test]
fn cons_sweep_accounting_subtracts_newly_freed_cells_not_existing_free_cells() {
    for cycle in DeferredCycle::iter() {
        let mut heap = heap();
        let first: Vec<_> = (0..8)
            .map(|_| heap.alloc_cons(TaggedValue::T, TaggedValue::NIL))
            .collect();
        begin_deferred(&mut heap, DeferredCycle::Minor, &[first[0], first[7]]);
        finish(&mut heap, 2);
        assert_eq!(heap.cons_allocator.free_count(), 6);

        let young: Vec<_> = (0..3)
            .map(|_| heap.alloc_cons(TaggedValue::T, TaggedValue::NIL))
            .collect();
        heap.close_alloc_regions();
        assert_eq!(heap.cons_live_count_exact(), 5);
        assert_eq!(heap.cons_allocator.free_count(), 3);
        // The minor preserves both existing old cells; the major drops the
        // unrooted old cell in addition to the two unrooted young cells.
        let expected = match cycle {
            DeferredCycle::Minor => 3,
            DeferredCycle::Major => 2,
        };
        begin_deferred(&mut heap, cycle, &[first[0], young[0]]);
        finish(&mut heap, expected);
        assert_eq!(heap.generational.old_cons_count, expected);
        assert!(heap.owns_heap_value_for_test(first[0]));
        assert!(heap.owns_heap_value_for_test(young[0]));
        assert!(!heap.owns_heap_value_for_test(young[1]));
        assert!(!heap.owns_heap_value_for_test(young[2]));
        assert_eq!(
            heap.owns_heap_value_for_test(first[7]),
            matches!(cycle, DeferredCycle::Minor)
        );
    }
}

#[test]
fn cons_sweep_accounting_keeps_reused_empty_candidate_and_then_releases_it() {
    for cycle in DeferredCycle::iter() {
        let mut heap = heap();
        let mut last = TaggedValue::NIL;
        let mut first_base = 0;
        for index in 0..CONS_BLOCK_SIZE + 1 {
            let cell = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            if index == 0 {
                first_base = base(cell);
            }
            last = cell;
        }
        begin_deferred(&mut heap, cycle, &[last]);
        assert!(!heap.incremental_sweep_slice(1));
        assert_eq!(heap.sweep_cons_cursor, 1);
        let newborn = heap.alloc_cons(TaggedValue::fixnum(277), TaggedValue::NIL);
        assert_eq!(base(newborn), first_base);
        assert!(heap.region_stats.cons_from_free_list > 0);
        // One cell was handed out; the rest of its prepaid region must be
        // refunded before the final freed-cell delta corrects the live count.
        assert!(heap.cons_live_count > heap.cons_live_count_exact());
        finish(&mut heap, 2);
        assert!(heap.cons_block_index_by_base.contains_key(&first_base));
        assert!(heap.owns_heap_value_for_test(newborn));
        assert!(!heap.value_is_old_for_test(newborn));
        assert_eq!(heap.cons_blocks.len(), 2);

        // Next minor can really empty the former candidate. Its stable base
        // is removed only after its final live inventory is again zero.
        begin_deferred(&mut heap, DeferredCycle::Minor, &[last]);
        finish(&mut heap, 1);
        assert!(!heap.cons_block_index_by_base.contains_key(&first_base));
        assert!(!heap.owns_heap_value_for_test(newborn));
        assert_eq!(heap.cons_blocks.len(), 1);
    }
}

#[test]
fn cons_sweep_accounting_includes_new_sweep_window_blocks_and_refunds_unused_tails() {
    for cycle in DeferredCycle::iter() {
        let mut heap = heap();
        let mut roots = Vec::new();
        for index in 0..CONS_BLOCK_SIZE + 1 {
            let cell = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
            if index % CONS_BLOCK_SIZE == 0 {
                roots.push(cell);
            }
        }
        begin_deferred(&mut heap, cycle, &roots);
        assert_eq!(heap.cons_blocks.len(), 2);
        // All current-work blocks are withheld, including the partial tail
        // of the newest one, so this production carve must create a block.
        let newborn = heap.alloc_cons(TaggedValue::fixnum(281), TaggedValue::NIL);
        assert_eq!(heap.cons_blocks.len(), 3);
        assert!(heap.region_stats.cons_from_fresh_block > 0);
        assert!(heap.cons_live_count > heap.cons_live_count_exact());
        finish(&mut heap, 3);
        assert!(heap.owns_heap_value_for_test(newborn));
        assert!(!heap.value_is_old_for_test(newborn));
        let newborn_block = &heap.cons_blocks[heap.cons_block_index_by_base[&base(newborn)]];
        assert_eq!(newborn_block.next_index, 1);
        roots.push(newborn);
        begin_deferred(&mut heap, DeferredCycle::Minor, &roots);
        finish(&mut heap, 3);
        assert!(heap.value_is_old_for_test(newborn));
    }
}

#[test]
fn cons_sweep_accounting_empty_minor_preserves_old_only_inventory() {
    let mut heap = heap();
    let root = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    begin_deferred(&mut heap, DeferredCycle::Minor, &[root]);
    finish(&mut heap, 1);
    let blocks = heap.cons_blocks.len();
    begin_deferred(&mut heap, DeferredCycle::Minor, &[root]);
    assert_eq!(
        heap.sweep_cons_end, 0,
        "old-only blocks are not minor sweep work"
    );
    finish(&mut heap, 1);
    assert_eq!(heap.cons_blocks.len(), blocks);
    assert_eq!(heap.generational.old_cons_count, 1);
}

#[test]
fn cons_sweep_accounting_verifier_rejects_corrupt_completed_freed_count() {
    let mut heap = heap();
    let root = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    begin_deferred(&mut heap, DeferredCycle::Minor, &[root]);
    let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::CorruptCompletedSweepFreed);
    let panic = catch_unwind(AssertUnwindSafe(|| heap.finish_incremental_sweep_now()))
        .expect_err("the slow verify census must reject a wrong arithmetic recount");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap();
    assert!(message.contains("cons recount from sweep differs from bitmap census"));
}
