use super::*;
use crate::heap_types::LispMarker;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
use crate::tagged::header::MarkerObj;

fn alloc_marker_for_test(heap: &mut TaggedHeap) -> *mut MarkerObj {
    let tv = heap.alloc_marker(LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: None,
        bytepos: 0,
        charpos: 0,
        last_position_valid: false,
        next_marker: std::ptr::null_mut(),
    });
    tv.as_veclike_ptr().unwrap() as *mut MarkerObj
}

#[test]
fn chain_splice_at_head_and_walk() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let bt = BufferText::new();
    let m1 = alloc_marker_for_test(&mut heap);
    let m2 = alloc_marker_for_test(&mut heap);
    let m3 = alloc_marker_for_test(&mut heap);

    bt.chain_splice_at_head(m1);
    bt.chain_splice_at_head(m2);
    bt.chain_splice_at_head(m3);

    let walked = bt.chain_walk_collect();
    assert_eq!(walked, vec![m3, m2, m1]);
}

#[test]
fn chain_unlink_front_middle_back() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let bt = BufferText::new();
    let m1 = alloc_marker_for_test(&mut heap);
    let m2 = alloc_marker_for_test(&mut heap);
    let m3 = alloc_marker_for_test(&mut heap);
    bt.chain_splice_at_head(m1);
    bt.chain_splice_at_head(m2);
    bt.chain_splice_at_head(m3);

    bt.chain_unlink(m2);
    assert_eq!(bt.chain_walk_collect(), vec![m3, m1]);

    bt.chain_unlink(m3);
    assert_eq!(bt.chain_walk_collect(), vec![m1]);

    bt.chain_unlink(m1);
    assert_eq!(bt.chain_walk_collect(), vec![]);
}

#[test]
fn chain_unlink_absent_is_noop() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let bt = BufferText::new();
    let m1 = alloc_marker_for_test(&mut heap);
    let absent = alloc_marker_for_test(&mut heap);
    bt.chain_splice_at_head(m1);

    bt.chain_unlink(absent);
    assert_eq!(bt.chain_walk_collect(), vec![m1]);
}
