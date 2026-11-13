//! Store-local offset validation preserves the original non-dereferencing gate.

use super::{
    CONS_BLOCK_ALIGN, CONS_CELLS_BYTES, ConsBlock, ConsCell, generation_only_cons_cell_offset,
};
use std::mem::size_of;

#[test]
fn cons_payload_alignment_offset_matches_original_gate_at_every_granule_offset() {
    let mask = CONS_BLOCK_ALIGN - 1;
    let mut bases = vec![
        0,
        CONS_BLOCK_ALIGN,
        (1usize << (usize::BITS - 1)) & !mask,
        usize::MAX & !mask,
    ];
    bases.sort_unstable();
    bases.dedup();
    for base in bases {
        for offset in 0..CONS_BLOCK_ALIGN {
            let addr = base + offset;
            let ptr = addr as *const ConsCell;
            // Both original methods do integer metadata arithmetic only;
            // no candidate, canonical or otherwise, is dereferenced.
            let original = ConsBlock::ptr_is_cell_aligned(ptr);
            let expected = (offset < CONS_CELLS_BYTES && offset % size_of::<ConsCell>() == 0)
                .then_some(offset);
            let actual = generation_only_cons_cell_offset(addr);
            assert_eq!(actual.is_some(), original, "addr={addr:#x}");
            assert_eq!(actual, expected, "base={base:#x}, offset={offset:#x}");
            if let Some(cell_offset) = actual {
                assert_eq!(
                    cell_offset / size_of::<ConsCell>(),
                    ConsBlock::index_of_ptr(ptr)
                );
            }
        }
    }
}
