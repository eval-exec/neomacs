//! Generational heap invariants, built up with each stage of P3.1 G2.

use super::*;

#[test]
fn cons_trailer_layout_and_zeroed_generation_bitmaps() {
    assert_eq!(CONS_BLOCK_SIZE, 4001);
    assert_eq!(CONS_MARK_WORDS, 63);
    let mut block = ConsBlock::new();
    assert_eq!(block.base_addr() % CONS_BLOCK_BYTES, 0);
    let (first, count) = block.reserve_tail(CONS_BLOCK_SIZE);
    assert_eq!((first, count), (0, 4001));
    assert_eq!(block.reserve_tail(1).1, 0);
    for i in 0..CONS_BLOCK_SIZE {
        assert!(ConsBlock::ptr_is_cell_aligned(unsafe {
            block.cells_ptr().add(i)
        }));
    }
    assert!(!ConsBlock::ptr_is_cell_aligned(unsafe {
        block.cells_ptr().add(CONS_BLOCK_SIZE)
    }));
    block.mark_cell_offset((CONS_BLOCK_SIZE - 1) * size_of::<ConsCell>());
    assert_eq!(block.count_marked(), 1);
    // The mark bitmap ends before the two zeroed, unused generation maps.
    for i in 0..CONS_MARK_WORDS {
        assert_eq!(block.trailer().old_word(i), 0);
        assert_eq!(block.trailer().unlogged_word(i), 0);
    }
    block.clear_marks();
    assert_eq!(block.count_marked(), 0);
}
