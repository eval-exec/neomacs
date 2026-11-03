//! Repeated region grants reuse only a successful canonical epoch record.

use super::*;
use crate::tagged::gc::cons_allocator::{ConsAllocatorFault, ConsAllocatorFaultGuard};
use crate::tagged::gc::scan_contract::SingleMutatorWorld;
use crate::tagged::mutate::set_cons_car;
use crate::tagged::value::TAG_MASK;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn new_heap() -> Box<TaggedHeap> {
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_generational_enabled(true);
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.gc_threshold = usize::MAX / 4;
    set_tagged_heap(&mut heap);
    heap
}

fn address(value: TaggedValue) -> usize {
    value.bits() & !TAG_MASK
}

fn base(value: TaggedValue) -> usize {
    address(value) & !(CONS_BLOCK_ALIGN - 1)
}

fn counts(heap: &TaggedHeap) -> (u64, usize, usize, usize) {
    (
        heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()],
        heap.allocated_count(),
        heap.bytes_since_gc_exact(),
        heap.cons_live_count_exact(),
    )
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

fn promote(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    for _ in 0..3 {
        if roots
            .iter()
            .all(|&root| heap.cons_age_is_old_for_test(root))
        {
            return;
        }
        minor(heap, roots);
    }
    assert!(
        roots
            .iter()
            .all(|&root| heap.cons_age_is_old_for_test(root))
    );
}

fn logged(heap: &mut TaggedHeap) -> Vec<ConsBlockBase> {
    heap._satb_layout_reserve
        .current_young_log_mut()
        .unwrap()
        .blocks()
        .to_vec()
}

fn panic_text(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_owned()
    } else {
        panic!("expected a canonical refusal message")
    }
}

fn exhaust_tail(heap: &mut TaggedHeap) -> Vec<TaggedValue> {
    let cells: Vec<_> = (0..CONS_REGION_MAX_CELLS)
        .map(|index| heap.alloc_cons(TaggedValue::fixnum(index as i64), TaggedValue::NIL))
        .collect();
    assert_eq!(heap.jit.cons_cur.get(), heap.jit.cons_lim.get());
    assert!(
        heap.region_book
            .no_old_cons_block_contains(address(cells[0]))
    );
    cells
}

fn fragmented_old_block(heap: &mut TaggedHeap) -> Vec<TaggedValue> {
    let cells: Vec<_> = (0..9)
        .map(|index| heap.alloc_cons(TaggedValue::fixnum(index), TaggedValue::NIL))
        .collect();
    let roots: Vec<_> = cells.iter().step_by(2).copied().collect();
    promote(heap, &roots);
    assert_eq!(heap.cons_allocator.free_count(), 4);
    roots
}

#[test]
fn recording_cache_word_distinguishes_zero_full_bit_mixed_and_inactive() {
    let mut book = RegionBook::new();
    let mask = CONS_BLOCK_ALIGN - 1;
    let mut bases = vec![0, CONS_BLOCK_ALIGN, usize::MAX & !mask];
    for bit in 16..usize::BITS {
        bases.push(1usize << bit);
    }
    for block in bases {
        for color in [RegionColor::White, RegionColor::Black] {
            for age in [ConsCarveAge::NoOldOwners, ConsCarveAge::MayHaveOldOwners] {
                book.set_cons_recording_and_color(Some((block, age)), color);
                assert_eq!(
                    RegionBook::recorded_cons_age(book.cons_block_and_color, block),
                    Some(age)
                );
                assert_eq!(
                    RegionBook::recorded_cons_age(
                        book.cons_block_and_color,
                        block ^ CONS_BLOCK_ALIGN
                    ),
                    None
                );
                assert_eq!(book.cons_color(), color);
                for offset in [0, 1, 4, 16, mask] {
                    assert_eq!(
                        book.no_old_cons_block_contains(block | offset),
                        age.no_old_owners()
                    );
                }
                book.clear_cons_block();
                assert_eq!(
                    RegionBook::recorded_cons_age(book.cons_block_and_color, block),
                    None
                );
                assert!(!book.no_old_cons_block_contains(block));
                assert_eq!(book.cons_color(), color);
            }
        }
    }
}

#[test]
fn recording_cache_same_block_tail_grants_keep_canonical_capture_complete() {
    for admitted in [false, true] {
        let mut heap = new_heap();
        if admitted {
            // SAFETY: this fixture exclusively owns an idle heap. Admission
            // closes all regions before enabling the shared allocator facade.
            unsafe { heap.admit_threads_world_stopped() };
            assert!(heap.threads_admitted());
        }
        let cells: Vec<_> = (0..3 * CONS_REGION_MAX_CELLS)
            .map(|index| heap.alloc_cons(TaggedValue::fixnum(index as i64), TaggedValue::NIL))
            .collect();
        let block = ConsBlockBase::of_cell(cells[0].as_cons_ptr().unwrap());
        assert!(cells.iter().all(|&value| base(value) == block.addr()));
        assert_eq!(heap.region_stats.cons_refills, 3);
        assert_eq!(logged(&mut heap), vec![block]);
        assert_eq!(
            RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, block.addr()),
            Some(ConsCarveAge::NoOldOwners)
        );
        // Real capture must retain the entire set of handed roots, including
        // prefixes whose later grants reused the first successful recording.
        minor(&mut heap, &cells);
        assert_eq!(heap.cons_live_count_exact(), cells.len());
        for (index, cell) in cells.iter().enumerate() {
            assert_eq!(cell.cons_car(), TaggedValue::fixnum(index as i64));
        }
    }
}

fn assert_loan_refusal_refunds_cached_grant(heap: &mut TaggedHeap, prior: &[TaggedValue]) {
    assert_eq!(heap.jit.cons_cur.get(), heap.jit.cons_lim.get());
    let block = base(prior[0]);
    assert!(RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, block).is_some());
    let before = counts(heap);
    let free_before = heap.cons_allocator.free_count();
    let used_before: usize = heap
        .cons_blocks
        .iter()
        .map(|block| block.next_index as usize)
        .sum();
    let loan = {
        // SAFETY: this fixture is the exclusive parked owner; no collector or
        // callbacks occur. Deliberately keep the exhausted region certificate.
        let mut world = unsafe { SingleMutatorWorld::from_heap(heap) };
        let id = world.current_young_registration_id();
        world.take_young_log_loan(id).unwrap()
    };
    let refused = catch_unwind(AssertUnwindSafe(|| {
        heap.alloc_cons(TaggedValue::fixnum(7001), TaggedValue::NIL)
    }));
    let after = counts(heap);
    let free_after = heap.cons_allocator.free_count();
    let used_after: usize = heap
        .cons_blocks
        .iter()
        .map(|block| block.next_index as usize)
        .sum();
    // Return the actual loan before assertions can unwind the fixture.
    {
        // SAFETY: the same stopped owner retains every block and the loan.
        let mut world = unsafe { SingleMutatorWorld::from_heap(heap) };
        world.return_young_log_loan(loan).unwrap();
    }
    let refused =
        panic_text(refused.expect_err("a cached record cannot bypass a current log loan"));
    assert!(refused.contains("LogLoaned"), "{refused}");
    assert_eq!(after, before);
    assert_eq!(free_after, free_before + used_after - used_before);
    assert!(!heap.alloc_regions_open());
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, block),
        None
    );
    let recovered = heap.alloc_cons(TaggedValue::fixnum(7003), TaggedValue::NIL);
    assert_eq!(base(recovered), block);
    assert_eq!(heap.region_book.cons, ConsRegionSource::FreeListRun);
    let mut roots = prior.to_vec();
    roots.push(recovered);
    minor(heap, &roots);
    assert_eq!(recovered.cons_car(), TaggedValue::fixnum(7003));
}

#[test]
fn recording_cache_tail_hit_still_refuses_loan_and_refunds() {
    let mut heap = new_heap();
    let prior = exhaust_tail(&mut heap);
    assert!(matches!(
        heap.region_book.cons,
        ConsRegionSource::BlockTail { .. }
    ));
    assert_loan_refusal_refunds_cached_grant(&mut heap, &prior);
}

#[test]
fn recording_cache_fragmented_free_list_hit_still_refuses_loan_and_refunds() {
    let mut heap = new_heap();
    let mut roots = fragmented_old_block(&mut heap);
    let first = heap.alloc_cons(TaggedValue::fixnum(7011), TaggedValue::NIL);
    assert_eq!(heap.region_book.cons, ConsRegionSource::FreeListRun);
    assert_eq!(base(first), base(roots[0]));
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(first)),
        Some(ConsCarveAge::MayHaveOldOwners)
    );
    roots.push(first);
    assert_loan_refusal_refunds_cached_grant(&mut heap, &roots);
}

#[test]
fn recording_cache_minor_and_admission_close_invalidate_recording() {
    let mut heap = new_heap();
    let owner = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    promote(&mut heap, &[owner]);
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(owner)),
        None
    );
    assert!(logged(&mut heap).is_empty());
    let first = heap.alloc_cons(TaggedValue::fixnum(7021), TaggedValue::NIL);
    assert_eq!(base(first), base(owner));
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(owner)),
        Some(ConsCarveAge::MayHaveOldOwners)
    );
    assert_eq!(logged(&mut heap).len(), 1);
    // Admission is a real epoch close even when no ages or flags change.
    // SAFETY: no marker or residual collector is active, and this fixture owns
    // the sole Context; admission itself closes its partially used region.
    unsafe { heap.admit_threads_world_stopped() };
    assert!(heap.threads_admitted());
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(owner)),
        None
    );
    let second = heap.alloc_cons(TaggedValue::fixnum(7027), TaggedValue::NIL);
    assert_eq!(base(second), base(owner));
    assert_eq!(logged(&mut heap).len(), 1);
    minor(&mut heap, &[owner, first, second]);
    assert_eq!(first.cons_car(), TaggedValue::fixnum(7021));
    assert_eq!(second.cons_car(), TaggedValue::fixnum(7027));
}

#[test]
fn recording_cache_mixed_grants_remember_old_to_young_edge_across_minors() {
    let mut heap = new_heap();
    let roots = fragmented_old_block(&mut heap);
    let owner = roots[0];
    let first = heap.alloc_cons(TaggedValue::fixnum(7031), TaggedValue::NIL);
    let child = heap.alloc_cons(TaggedValue::fixnum(7039), TaggedValue::NIL);
    assert_eq!(base(first), base(child));
    assert_eq!(base(owner), base(child));
    assert_eq!(heap.region_book.cons, ConsRegionSource::FreeListRun);
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(child)),
        Some(ConsCarveAge::MayHaveOldOwners)
    );
    assert!(!heap.region_book.no_old_cons_block_contains(address(owner)));
    assert_eq!(logged(&mut heap).len(), 1);
    assert!(set_cons_car(owner, child));
    assert!(heap.owner_card_is_dirty_for_test(owner));
    for _ in 0..2 {
        minor(&mut heap, &roots);
        assert_eq!(owner.cons_car(), child);
        assert_eq!(child.cons_car(), TaggedValue::fixnum(7039));
    }
}

#[test]
fn recording_cache_black_grants_log_only_handed_prefixes_and_refund_marks() {
    let mut heap = new_heap();
    heap.begin_collection();
    heap.mark_in_progress = true;
    heap.set_concurrent_active_for_test(true);
    let before = counts(&heap);
    let cells: Vec<_> = (0..2 * CONS_REGION_MAX_CELLS + 1)
        .map(|index| heap.alloc_cons(TaggedValue::fixnum(index as i64), TaggedValue::NIL))
        .collect();
    let block = base(cells[0]);
    assert_eq!(heap.region_book.cons_color(), RegionColor::Black);
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, block),
        Some(ConsCarveAge::NoOldOwners)
    );
    let cur = heap.jit.cons_cur.get();
    let lim = heap.jit.cons_lim.get();
    assert!(cur < lim);
    let marked = |heap: &TaggedHeap, addr: usize| {
        let index = heap.cons_block_index_by_base[&block];
        heap.cons_blocks[index].is_marked_ptr(addr as *const ConsCell)
    };
    assert!(
        (cur..lim)
            .step_by(size_of::<ConsCell>())
            .all(|addr| marked(&heap, addr))
    );
    heap.close_alloc_regions();
    assert!(
        (cur..lim)
            .step_by(size_of::<ConsCell>())
            .all(|addr| !marked(&heap, addr))
    );
    assert!(cells.iter().all(|&cell| marked(&heap, address(cell))));
    let logged_births: Vec<_> = heap
        .current_mutator_gc()
        .black_born_regions
        .iter()
        .flat_map(|region| region.cons_owners())
        .collect();
    assert_eq!(logged_births, cells);
    assert_eq!(heap.current_mutator_gc().black_born_regions.len(), 3);
    assert_eq!(
        counts(&heap),
        (
            before.0 + cells.len() as u64,
            before.1 + cells.len(),
            before.2 + cells.len() * size_of::<ConsCell>(),
            before.3 + cells.len()
        )
    );
    assert_eq!(heap.region_book.cons_color(), RegionColor::Black);
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, block),
        None
    );
    heap.set_concurrent_active_for_test(false);
    heap.complete_collection();
    heap.finish_incremental_sweep_now();
    assert!(
        cells
            .iter()
            .all(|&cell| heap.cons_age_is_old_for_test(cell))
    );
}

#[test]
fn recording_cache_hit_still_rejects_wrong_heap_before_publication() {
    let mut heap = new_heap();
    let prior = exhaust_tail(&mut heap);
    let before = counts(&heap);
    let mut foreign = TaggedHeap::new();
    // Temporarily cross two actual canonical registry slots; restore them
    // before any assertion or collection can unwind either heap.
    std::mem::swap(
        &mut heap._satb_layout_reserve,
        &mut foreign._satb_layout_reserve,
    );
    let refused = catch_unwind(AssertUnwindSafe(|| {
        heap.alloc_cons(TaggedValue::fixnum(7043), TaggedValue::NIL)
    }));
    std::mem::swap(
        &mut heap._satb_layout_reserve,
        &mut foreign._satb_layout_reserve,
    );
    let refused =
        panic_text(refused.expect_err("a cached record cannot authorize a foreign canonical log"));
    assert!(refused.contains("WrongHeap"), "{refused}");
    assert_eq!(counts(&heap), before);
    assert!(!heap.alloc_regions_open());
    let recovered = heap.alloc_cons(TaggedValue::fixnum(7049), TaggedValue::NIL);
    let mut roots = prior;
    roots.push(recovered);
    minor(&mut heap, &roots);
    assert_eq!(recovered.cons_car(), TaggedValue::fixnum(7049));
}

#[test]
fn recording_cache_faults_cannot_be_hidden_by_prior_recording() {
    let mut heap = new_heap();
    let prior = exhaust_tail(&mut heap);
    {
        let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::SkipCarveRecording);
        heap.alloc_cons(TaggedValue::fixnum(7057), TaggedValue::NIL);
    }
    assert_eq!(
        RegionBook::recorded_cons_age(heap.region_book.cons_block_and_color, base(prior[0])),
        None
    );
    // The original successful grant is still logged, but the deliberately
    // skipped second grant must not publish a reusable recording certificate.
    assert_eq!(logged(&mut heap).len(), 1);
    minor(&mut heap, &prior);

    let mut corrupt = new_heap();
    let _prior = exhaust_tail(&mut corrupt);
    {
        let _fault = ConsAllocatorFaultGuard::inject(ConsAllocatorFault::CorruptAvailable);
        corrupt.alloc_cons(TaggedValue::fixnum(7069), TaggedValue::NIL);
    }
    let rejected = catch_unwind(AssertUnwindSafe(|| corrupt.begin_minor_collection()));
    assert!(
        rejected.is_err(),
        "the repeated grant must still reach the real canonical fault site"
    );
}
