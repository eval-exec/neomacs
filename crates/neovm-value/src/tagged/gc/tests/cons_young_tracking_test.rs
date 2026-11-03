//! Young-block recording, stopped-world flag roles, and verifier mutations.

use super::cons_allocator::*;
use super::*;
use crate::heap_types::LispString;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn verified_heap() -> TaggedHeap {
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

fn captured_bases(heap: &TaggedHeap) -> FxHashSet<usize> {
    heap.cons_allocator
        .mark_work_snapshot()
        .into_iter()
        .map(ConsBlockBase::addr)
        .collect()
}

fn complete_minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection().unwrap();
    complete_minor(heap, roots);
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        panic!("expected a verifier assertion message")
    }
}

fn check_both_allocator_modes(check: fn(TaggedHeap)) {
    check(verified_heap());
    let mut admitted = verified_heap();
    // SAFETY: this fresh fixture has one stopped owner, no open regions,
    // and no concurrent or unfinished collection before admission.
    unsafe { admitted.admit_threads_world_stopped() };
    check(admitted);
}

#[test]
fn cons_young_tracking_records_fresh_bump_and_free_list_region_carves() {
    let mut heap = verified_heap();
    let allocated: Vec<_> = (0..CONS_REGION_MAX_CELLS + 1)
        .map(|_| heap.alloc_cons(TaggedValue::T, TaggedValue::NIL))
        .collect();
    assert_eq!(heap.region_stats.cons_from_fresh_block, 1);
    assert!(heap.region_stats.cons_from_block_tail > 0);
    let expected: FxHashSet<_> = allocated.iter().copied().map(base).collect();
    heap.begin_minor_collection().unwrap();
    assert_eq!(captured_bases(&heap), expected);
    // Keep one old cell so the block remains available for free-list reuse.
    complete_minor(&mut heap, &allocated[..1]);
    assert!(heap.cons_allocator.free_count() > 0);

    let reused = heap.alloc_cons(TaggedValue::fixnum(251), TaggedValue::NIL);
    assert!(heap.region_stats.cons_from_free_list > 0);
    assert_eq!(base(reused), base(allocated[0]));
    heap.begin_minor_collection().unwrap();
    assert_eq!(captured_bases(&heap), FxHashSet::from_iter([base(reused)]));
    complete_minor(&mut heap, &[allocated[0], reused]);
    assert!(heap.value_is_old_for_test(reused));
}

#[test]
fn cons_young_verifier_rejects_missing_carve_record_at_minor_begin() {
    let mut heap = verified_heap();
    {
        let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::SkipCarveRecording);
        heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    }
    let panic = catch_unwind(AssertUnwindSafe(|| heap.begin_minor_collection()))
        .expect_err("begin's production verifier must detect an unrecorded young block");
    assert!(panic_message(panic).contains("allocated young cell missing flag"));
    // The verifier panics while holding the allocator metadata lock. The
    // poisoned fixture is intentionally discarded; do not continue collection.
}

#[test]
fn cons_young_verifier_rejects_flag_retained_after_promotion() {
    let mut heap = verified_heap();
    let root = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    heap.begin_minor_collection().unwrap();
    heap.seed_root(root);
    let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::KeepFlagAfterPromotion);
    let panic = catch_unwind(AssertUnwindSafe(|| heap.complete_minor_collection()))
        .expect_err("post-promotion production verification must reject an uncleared young flag");
    assert!(panic_message(panic).contains("young flag/log mismatch"));
}

#[test]
fn cons_young_verifier_rejects_corrupt_available_index_at_minor_begin() {
    let mut heap = verified_heap();
    let allocated: Vec<_> = (0..2 * CONS_REGION_MAX_CELLS + 1)
        .map(|_| heap.alloc_cons(TaggedValue::T, TaggedValue::NIL))
        .collect();
    minor(&mut heap, &allocated[..1]);
    // More than one region of free cells remains. The injected duplicate thus
    // follows a genuine indexed entry even after reserving the next region.
    assert!(heap.cons_allocator.free_count() > CONS_REGION_MAX_CELLS);
    {
        let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::CorruptAvailable);
        heap.alloc_cons(TaggedValue::fixnum(257), TaggedValue::NIL);
    }
    let panic = catch_unwind(AssertUnwindSafe(|| heap.begin_minor_collection()))
        .expect_err("begin's production verifier must check blocks_with_free indexing");
    assert!(panic_message(panic).contains("duplicate available block"));
}

#[test]
fn cons_young_verifier_allows_stale_marks_on_old_cells_of_non_young_blocks() {
    let mut heap = verified_heap();
    let root = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    minor(&mut heap, &[root]);
    assert!(heap.value_is_old_for_test(root));
    let pointer = root.as_cons_ptr().unwrap();
    let block = &heap.cons_blocks[heap.cons_block_index_by_base[&base(root)]];
    assert!(
        block.is_marked_ptr(pointer),
        "promotion intentionally preserves old marks"
    );
    // No carve occurred since promotion, so this old block is not young.
    // Begin's real verifier must accept its marked old cell.
    heap.begin_minor_collection().unwrap();
    assert!(captured_bases(&heap).is_empty());
    complete_minor(&mut heap, &[root]);
}

#[test]
fn cons_young_verifier_rejects_marked_free_cell_in_non_young_block() {
    let mut heap = verified_heap();
    let root = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let doomed = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    minor(&mut heap, &[root]);
    assert!(!heap.owns_heap_value_for_test(doomed));
    let pointer = doomed.as_cons_ptr().unwrap();
    let block = &heap.cons_blocks[heap.cons_block_index_by_base[&base(doomed)]];
    let index = ConsBlock::index_of_ptr(pointer);
    assert!(!block.trailer().is_old(index));
    // SAFETY: the block stays owned and no mutator or marker is running.
    // This is a bitmap-only mutation; the free-list union remains untouched.
    assert!(block.trailer().mark_cell(index));
    let panic = catch_unwind(AssertUnwindSafe(|| heap.begin_minor_collection()))
        .expect_err("stale marks are legal only for old cells in non-young blocks");
    assert!(panic_message(panic).contains("non-young block has marked non-old cell"));
}

#[test]
fn cons_young_tracking_preserves_mid_sweep_carve_for_the_next_minor() {
    check_both_allocator_modes(
        check_cons_young_tracking_preserves_mid_sweep_carve_for_the_next_minor,
    );
}

fn check_cons_young_tracking_preserves_mid_sweep_carve_for_the_next_minor(mut heap: TaggedHeap) {
    let mut roots = Vec::new();
    for index in 0..2 * CONS_BLOCK_SIZE + 1 {
        let value = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        if index % CONS_BLOCK_SIZE == 0 {
            roots.push(value);
        }
    }
    assert_eq!(heap.cons_blocks.len(), 3);
    let first_base = base(roots[0]);
    heap.begin_minor_collection().unwrap();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    // Sweep exactly the first block while the remaining blocks keep the
    // current collection active. No synthetic late-sweep hook is required.
    assert!(!heap.incremental_sweep_slice(1));
    assert!(heap.sweep_in_progress());
    assert_eq!(heap.sweep_cons_cursor, 1);
    let rooted = heap.alloc_cons(TaggedValue::fixnum(263), TaggedValue::NIL);
    let unrooted = heap.alloc_cons(TaggedValue::fixnum(269), TaggedValue::NIL);
    assert_eq!(base(rooted), first_base);
    assert_eq!(base(unrooted), first_base);
    assert!(heap.region_stats.cons_from_free_list > 0);
    assert!(!heap.value_is_old_for_test(rooted));
    assert!(!heap.value_is_old_for_test(unrooted));
    heap.finish_incremental_sweep_now();
    assert!(heap.owns_heap_value_for_test(rooted));
    assert!(heap.owns_heap_value_for_test(unrooted));

    heap.begin_minor_collection().unwrap();
    assert_eq!(captured_bases(&heap), FxHashSet::from_iter([first_base]));
    roots.push(rooted);
    complete_minor(&mut heap, &roots);
    assert!(heap.owns_heap_value_for_test(rooted));
    assert!(heap.value_is_old_for_test(rooted));
    assert!(!heap.owns_heap_value_for_test(unrooted));
    assert_eq!(rooted.cons_car(), TaggedValue::fixnum(263));
}

#[test]
fn cons_young_verifier_compares_dead_sentinel_bits_for_heap_payloads() {
    let mut heap = verified_heap();
    let payload = heap.alloc_string(LispString::from_utf8("young-block-payload"));
    let root = heap.alloc_cons(payload, TaggedValue::NIL);
    // DEAD has a pointer tag but no object behind it. Verifying allocation
    // status must compare bits without invoking Lisp structural equality.
    minor(&mut heap, &[root]);
    assert!(heap.value_is_old_for_test(root));
    assert!(heap.owns_heap_value_for_test(payload));
    assert_eq!(root.cons_car().bits(), payload.bits());
    heap.begin_minor_collection().unwrap();
    assert!(captured_bases(&heap).is_empty());
    complete_minor(&mut heap, &[root]);
    assert!(heap.owns_heap_value_for_test(payload));
}

#[test]
fn cons_young_verifier_rejects_next_minor_flag_cleared_at_sweep_completion() {
    check_both_allocator_modes(
        check_cons_young_verifier_rejects_next_minor_flag_cleared_at_sweep_completion,
    );
}

fn check_cons_young_verifier_rejects_next_minor_flag_cleared_at_sweep_completion(
    mut heap: TaggedHeap,
) {
    let mut roots = Vec::new();
    for index in 0..2 * CONS_BLOCK_SIZE + 1 {
        let value = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
        if index % CONS_BLOCK_SIZE == 0 {
            roots.push(value);
        }
    }
    heap.begin_minor_collection().unwrap();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    assert!(!heap.incremental_sweep_slice(1));
    let newborn = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    assert_eq!(base(newborn), base(roots[0]));
    {
        let _fault =
            ConsAllocatorFaultGuard::inject(ConsAllocatorFault::ClearNextFlagAtSweepCompletion);
        heap.finish_incremental_sweep_now();
    }
    let owner_index = heap.cons_block_index_by_base[&base(newborn)];
    let owner_base = ConsBlockBase::of(&heap.cons_blocks[owner_index]);
    assert!(heap.cons_allocator.free_count_for(owner_base).unwrap() > 0);
    let panic = catch_unwind(AssertUnwindSafe(|| heap.begin_minor_collection()))
        .expect_err("next minor must detect the rejected late-clear race");
    assert!(panic_message(panic).contains("young flag/log mismatch"));
}
