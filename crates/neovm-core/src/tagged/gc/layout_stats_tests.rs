use super::*;
use crate::heap_types::LispString;

#[test]
fn layout_stats_report_exact_page_occupancy_and_payload_capacity() {
    let mut heap = TaggedHeap::new();
    let _cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let _string = heap.alloc_string(LispString::from_utf8("abc"));
    let _vector = heap.alloc_vector(vec![TaggedValue::NIL; 3]);

    let stats = heap.layout_stats();
    assert_eq!(stats.allocated_objects, 3);
    assert_eq!(stats.cons.pages, 1);
    assert_eq!(stats.cons.live_slots, 1);
    assert_eq!(stats.cons.reclaimed_slots, 0);
    assert_eq!(stats.cons.occupied_bytes, size_of::<ConsCell>());

    let string = stats
        .arenas
        .iter()
        .find(|arena| arena.class == "string")
        .unwrap();
    assert_eq!(string.pages, 1);
    assert_eq!(string.allocated_slots, 1);
    assert_eq!(string.young_slots, 1);
    assert_eq!(string.payload_logical_bytes, 4); // "abc" + trailing NUL
    assert!(string.payload_capacity_bytes >= 4);
    assert_eq!(string.owned_payloads, 1);

    let vector = stats
        .arenas
        .iter()
        .find(|arena| arena.class == "vector")
        .unwrap();
    assert_eq!(vector.pages, 1);
    assert_eq!(vector.allocated_slots, 1);
    assert_eq!(vector.payload_logical_bytes, 3 * size_of::<TaggedValue>());
    assert!(vector.payload_capacity_bytes >= vector.payload_logical_bytes);
    assert_eq!(
        stats.page_backing_bytes,
        3 * 64 * 1024,
        "one cons, string, and vector page should be resident",
    );
}

#[test]
fn completed_sweep_releases_empty_cons_blocks_and_rebuilds_free_list() {
    let mut heap = TaggedHeap::new();
    let survivor = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
    for i in 0..CONS_BLOCK_SIZE {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
    }
    assert_eq!(heap.cons_blocks.len(), 2);

    heap.collect_exact(std::iter::once(survivor));

    assert_eq!(heap.cons_blocks.len(), 1);
    assert_eq!(heap.cons_block_index_by_base.len(), 1);
    assert_eq!(
        unsafe { (*survivor.xcons_ptr()).load_car() }.as_fixnum(),
        Some(7)
    );

    for i in 0..(CONS_BLOCK_SIZE - 1) {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL);
    }
    assert_eq!(
        heap.cons_blocks.len(),
        1,
        "the rebuilt free list must reuse every dead cell in the survivor block",
    );
    let _ = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    assert_eq!(heap.cons_blocks.len(), 2);
}
