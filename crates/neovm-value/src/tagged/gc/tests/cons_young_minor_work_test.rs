//! Minor work stays bounded by blocks that served a region since promotion.
use super::*;

#[test]
fn cons_young_minor_preserves_old_only_marks_and_visits_only_new_carve_block() {
    let mut heap = TaggedHeap::new();
    heap.set_generational_enabled(true);
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.set_gc_threshold(usize::MAX);
    let roots: Vec<_> = (0..CONS_BLOCK_SIZE)
        .map(|_| heap.alloc_cons(TaggedValue::T, TaggedValue::NIL))
        .collect();
    heap.begin_minor_collection().unwrap();
    for &root in &roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    let old_base = ConsBlock::block_base_for_ptr(roots[0].xcons_ptr());
    assert_eq!(heap.cons_blocks.len(), 1);
    assert_eq!(heap.generational.old_cons_count, CONS_BLOCK_SIZE);
    assert!(heap.cons_blocks[0].is_marked_ptr(roots[0].xcons_ptr()));

    let young = heap.alloc_cons(TaggedValue::T, TaggedValue::NIL);
    let young_base = ConsBlock::block_base_for_ptr(young.xcons_ptr());
    assert_ne!(old_base, young_base, "old block is full");
    heap.begin_minor_collection().unwrap();
    let old_index = heap.cons_block_index_by_base[&old_base];
    assert!(
        heap.cons_blocks[old_index].is_marked_ptr(roots[0].xcons_ptr()),
        "minor clear must leave old-only mark words untouched"
    );
    heap.seed_root(young);
    heap.complete_minor_collection();
    assert_eq!(
        heap.sweep_cons_end, 1,
        "minor sweep includes only the new carve block"
    );
    heap.finish_incremental_sweep_now();
    assert_eq!(heap.sweep_cons_blocks_swept, 1);
    assert_eq!(heap.generational.old_cons_count, CONS_BLOCK_SIZE + 1);
    assert_eq!(heap.cons_live_count_exact(), CONS_BLOCK_SIZE + 1);
    assert_eq!(
        heap.live_bytes,
        (CONS_BLOCK_SIZE + 1) * size_of::<ConsCell>()
    );
    assert!(heap.value_is_old_for_test(young));
    assert!(
        roots
            .iter()
            .all(|&value| heap.owns_heap_value_for_test(value))
    );
}
