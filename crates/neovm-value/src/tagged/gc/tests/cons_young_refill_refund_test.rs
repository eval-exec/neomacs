//! #[cfg(test)] #[path = "tests/cons_young_refill_refund_test.rs"]
//! mod cons_young_refill_refund_test;
//! Requires canonical registry/caller migration and the refusal-refund follow-up.

use super::scan_contract::SingleMutatorWorld;
use super::young_registry::YoungRegistryError;
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Clone, Copy, Debug)]
enum RefusedGrantSource {
    Fresh,
    Tail,
    FreeList,
}

#[derive(Clone, Copy, Debug)]
enum RefusedGrantAccess {
    Sole,
    Admitted,
}

fn refill_refund_panic_text(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_owned()
    } else {
        panic!("expected the canonical-log invariant panic text")
    }
}

fn refill_refund_source_counts(heap: &TaggedHeap) -> (u64, u64, u64) {
    (
        heap.region_stats.cons_from_fresh_block,
        heap.region_stats.cons_from_block_tail,
        heap.region_stats.cons_from_free_list,
    )
}

fn refill_refund_total_used(heap: &TaggedHeap) -> usize {
    heap.cons_blocks
        .iter()
        .map(|block| block.next_index as usize)
        .sum()
}

fn check_refused_cons_grant_refunded(source: RefusedGrantSource, access: RefusedGrantAccess) {
    let mut heap = TaggedHeap::new();
    heap.set_generational_enabled(true);
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.set_gc_threshold(usize::MAX);
    match access {
        RefusedGrantAccess::Sole => {}
        RefusedGrantAccess::Admitted => {
            // SAFETY: this fresh owner has no regions, marker, or aliases.
            unsafe { heap.admit_threads_world_stopped() };
        }
    }

    let prior_count = match source {
        RefusedGrantSource::Fresh => 0,
        RefusedGrantSource::Tail => CONS_REGION_MAX_CELLS,
        RefusedGrantSource::FreeList => 1,
    };
    let prior: Vec<_> = (0..prior_count)
        .map(|index| heap.alloc_cons(TaggedValue::fixnum(index as i64), TaggedValue::NIL))
        .collect();
    heap.close_alloc_regions();
    assert!(!heap.alloc_regions_open());
    assert_eq!((heap.jit.cons_cur.get(), heap.jit.cons_lim.get()), (0, 0));
    if matches!(source, RefusedGrantSource::FreeList) {
        // Closing the original bump region rewinds its unused suffix instead
        // of publishing it as a free list. Explicitly reserve that unhanded
        // tail and return it through the allocator to create a genuine list.
        let block = &mut heap.cons_blocks[0];
        let (first, count) = block.reserve_tail(CONS_REGION_MAX_CELLS - 1);
        assert_eq!((first, count), (1, CONS_REGION_MAX_CELLS - 1));
        // SAFETY: this closed, exclusively owned heap retains the registered
        // block. reserve_tail grants this entire previously unused, unmarked,
        // young range; no cell was charged, handed out, or linked as free.
        unsafe {
            heap.cons_allocator
                .return_unused(block.cells_ptr().add(first), count);
        }
    }
    let free_before = heap.cons_allocator.free_count();
    match source {
        RefusedGrantSource::Fresh => {
            assert!(heap.cons_blocks.is_empty());
            assert_eq!(free_before, 0);
        }
        RefusedGrantSource::Tail => {
            assert_eq!(heap.cons_blocks.len(), 1);
            assert_eq!(refill_refund_total_used(&heap), CONS_REGION_MAX_CELLS);
            assert_eq!(free_before, 0, "an exhausted region refunds no suffix");
        }
        RefusedGrantSource::FreeList => {
            assert_eq!(heap.cons_blocks.len(), 1);
            assert_eq!(free_before, CONS_REGION_MAX_CELLS - 1);
        }
    }
    let live_before = heap.cons_live_count;
    let allocated_before = heap.current_mutator_gc().allocated_count;
    let used_before = refill_refund_total_used(&heap);
    let sources_before = refill_refund_source_counts(&heap);

    // Transfer only the current young payload. The heap, its SATB buffer and
    // registry control remain confined here. No worker thread is running.
    let loan = {
        // SAFETY: this fixture is the stopped exclusive owner; no callbacks,
        // TLS aliases, collector, or marker reads occur in this extent.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
        let id = world.current_young_registration_id();
        world.take_young_log_loan(id).unwrap()
    };
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _unhanded = heap.alloc_cons(TaggedValue::fixnum(1009), TaggedValue::NIL);
    }))
    .expect_err("a loaned current canonical log must refuse before cons handout");
    let panic = refill_refund_panic_text(panic);
    assert!(
        panic.contains("canonical young recording refused an unhanded cons grant")
            && panic.contains("LogLoaned"),
        "unexpected refusal for {source:?}/{access:?}: {panic}"
    );

    assert_eq!(heap.cons_live_count, live_before, "{source:?}/{access:?}");
    assert_eq!(heap.current_mutator_gc().allocated_count, allocated_before);
    assert!(!heap.alloc_regions_open());
    assert_eq!((heap.jit.cons_cur.get(), heap.jit.cons_lim.get()), (0, 0));
    let used_after = refill_refund_total_used(&heap);
    let free_after = heap.cons_allocator.free_count();
    let new_capacity = used_after.checked_sub(used_before).unwrap();
    assert_eq!(
        free_after,
        free_before + new_capacity,
        "the whole unhanded grant must return, including fresh/tail high-water growth: {source:?}/{access:?}"
    );
    let (fresh, tail, free) = sources_before;
    let expected_sources = match source {
        RefusedGrantSource::Fresh => (fresh + 1, tail, free),
        RefusedGrantSource::Tail => (fresh, tail + 1, free),
        RefusedGrantSource::FreeList => (fresh, tail, free + 1),
    };
    assert_eq!(refill_refund_source_counts(&heap), expected_sources);
    {
        // SAFETY: same confined owner, no collector or worker has started.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
        assert_eq!(
            world.ensure_young_logs_stopped(),
            Err(YoungRegistryError::OutstandingLoans(1)),
            "the failure must not silently return or lose the actual loan"
        );
        world.return_young_log_loan(loan).unwrap();
        world.ensure_young_logs_stopped().unwrap();
    }

    // Exercise the actual allocation path after restoring the protocol, before
    // an empty fresh block could be released by a collection. The refunded
    // grant is now the allocator's free-list source in ALL three cases.
    let free_sources_before = heap.region_stats.cons_from_free_list;
    let recovered = heap.alloc_cons(TaggedValue::fixnum(1013), TaggedValue::NIL);
    assert_eq!(
        heap.region_stats.cons_from_free_list,
        free_sources_before + 1
    );
    heap.close_alloc_regions();
    assert_eq!(heap.cons_live_count, live_before + 1);
    assert_eq!(
        heap.current_mutator_gc().allocated_count,
        allocated_before + 1
    );

    // Production begin/capture/promote/sweep and GEN1 verification must all
    // remain usable: neither a sole nor admitted allocator was poisoned, and
    // every pre-existing live cell plus the recovered newborn survives.
    heap.begin_minor_collection().unwrap();
    for &root in &prior {
        heap.seed_root(root);
    }
    heap.seed_root(recovered);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    assert!(!heap.mark_in_progress);
    assert!(!heap.sweep_in_progress);
    assert!(!heap.alloc_regions_open());
    assert_eq!(heap.cons_live_count, prior.len() + 1);
    assert!(heap.value_is_old_for_test(recovered));
    for root in prior {
        assert!(heap.value_is_old_for_test(root));
    }
}

#[test]
fn cons_young_refill_refunds_fresh_grant_when_current_log_is_loaned() {
    for access in [RefusedGrantAccess::Sole, RefusedGrantAccess::Admitted] {
        check_refused_cons_grant_refunded(RefusedGrantSource::Fresh, access);
    }
}

#[test]
fn cons_young_refill_refunds_tail_grant_when_current_log_is_loaned() {
    for access in [RefusedGrantAccess::Sole, RefusedGrantAccess::Admitted] {
        check_refused_cons_grant_refunded(RefusedGrantSource::Tail, access);
    }
}

#[test]
fn cons_young_refill_refunds_free_list_grant_when_current_log_is_loaned() {
    for access in [RefusedGrantAccess::Sole, RefusedGrantAccess::Admitted] {
        check_refused_cons_grant_refunded(RefusedGrantSource::FreeList, access);
    }
}
