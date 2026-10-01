//! Minor sweeps retain old cells with white marks, reclaim only young
//! garbage, and leave allocations made during deferred sweeping young.

use super::*;
use std::collections::BTreeSet;

fn fill(block: &mut ConsBlock, count: usize) -> usize {
    let (first, granted) = block.reserve_tail(count);
    assert_eq!(granted, count);
    for index in first..first + count {
        // Fresh cells contain only immediates, so the test never retains an
        // unrooted heap Value across allocation or invokes Lisp allocation.
        unsafe {
            std::ptr::write(
                block.cells_ptr().add(index),
                ConsCell {
                    car: TaggedValue::fixnum(index as i64),
                    cdr_or_next: ConsCdrOrNext {
                        cdr: TaggedValue::NIL,
                    },
                },
            );
        }
    }
    first
}

fn free_indices(block: &ConsBlock, mut next: *mut ConsCell) -> BTreeSet<usize> {
    let mut indices = BTreeSet::new();
    while !next.is_null() {
        assert_eq!(ConsBlock::block_base_for_ptr(next), block.base_addr());
        assert!(
            indices.insert(ConsBlock::index_of_ptr(next)),
            "free-list cycle"
        );
        assert_eq!(unsafe { (*next).car }, TaggedValue::DEAD);
        next = unsafe { (*next).free_next() };
    }
    indices
}

#[test]
fn minor_sweep_keeps_unmarked_old_cells_and_marked_young_cells() {
    crate::test_utils::init_test_tracing();
    let mut block = ConsBlock::new();
    fill(&mut block, 132);
    for index in [0, 64, 131] {
        block.trailer().set_old(index);
        block.trailer().set_unlogged(index);
    }
    // The overlapping mark/old bit must count only once.
    for index in [0, 1, 65] {
        block.mark_cell_offset(index * size_of::<ConsCell>());
    }
    assert_eq!(block.count_marked(), 3);
    assert_eq!(block.count_live_generational(), 5);
    let mut free_list = std::ptr::null_mut();
    assert_eq!(block.sweep_generational(&mut free_list), 5);
    let survivors = BTreeSet::from([0, 1, 64, 65, 131]);
    let expected_free: BTreeSet<_> = (0..132).filter(|i| !survivors.contains(i)).collect();
    assert_eq!(free_indices(&block, free_list), expected_free);
    for index in survivors {
        let cell = unsafe { &*block.cells_ptr().add(index) };
        assert_eq!(cell.car, TaggedValue::fixnum(index as i64));
        assert_eq!(unsafe { cell.cdr() }, TaggedValue::NIL);
    }
    for index in expected_free {
        assert!(!block.trailer().is_old(index));
        assert!(!block.trailer().is_unlogged(index));
    }
}

#[test]
fn bitmap_promotion_counts_each_new_old_cell_once_across_words() {
    crate::test_utils::init_test_tracing();
    let mut block = ConsBlock::new();
    fill(&mut block, CONS_BLOCK_SIZE);
    for index in [1, 65] {
        block.trailer().set_old(index);
    }
    for index in [1, 64, CONS_BLOCK_SIZE - 1] {
        block.mark_cell_offset(index * size_of::<ConsCell>());
    }
    // This test owns the entire block and has no live workers or mutator
    // closures: the exclusive extent required for promotion/reset.
    assert_eq!(block.promote_marked_world_stopped(), 2);
    assert_eq!(block.count_live_generational(), 4);
    assert!(block.trailer().try_claim_unlogged(1));
    assert_eq!(block.promote_marked_world_stopped(), 0);
    assert!(block.trailer().is_unlogged(1));
    block.clear_marks();
    assert_eq!(block.count_marked(), 0);
    assert_eq!(block.count_live_generational(), 4);
    for index in [1, 64, 65, CONS_BLOCK_SIZE - 1] {
        assert!(block.trailer().is_old(index));
        assert!(block.trailer().is_unlogged(index));
    }
}

#[test]
fn sweep_window_allocations_survive_the_sweep_and_remain_young() {
    crate::test_utils::init_test_tracing();
    let mut block = ConsBlock::new();
    fill(&mut block, 2);
    block.mark_cell_offset(0);
    assert_eq!(block.promote_marked_world_stopped(), 1);
    let newborn = fill(&mut block, 1);
    assert_eq!(newborn, 2);
    block.mark_cell_offset(newborn * size_of::<ConsCell>());
    assert!(!block.trailer().is_old(newborn));
    let mut free_list = std::ptr::null_mut();
    assert_eq!(block.sweep_generational(&mut free_list), 2);
    assert_eq!(free_indices(&block, free_list), BTreeSet::from([1]));
    assert!(!block.trailer().is_old(newborn));
    // A later minor can reclaim that newborn if it became unreachable.
    block.clear_marks();
    free_list = std::ptr::null_mut();
    assert_eq!(block.sweep_generational(&mut free_list), 1);
    assert_eq!(free_indices(&block, free_list), BTreeSet::from([1, 2]));
    assert_eq!(block.count_live_generational(), 1);
}
