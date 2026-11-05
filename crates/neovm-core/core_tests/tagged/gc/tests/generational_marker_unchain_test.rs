//! The generational minor-collection marker-chain invariant, split out of
//! `value`'s `tagged/gc/tests/generational_test.rs` by the module-cluster
//! ruling (the test needs the edit-layer `BufferText` that owns the chain
//! head, so it moves to core).
//!
//! A young dead marker chained into a buffer's marker chain must be spliced
//! out by the minor collection while the tenured chain node survives, and the
//! old node's raw mark must stay white at rest across full cycles.

use crate::heap_types::byte_positions::EmacsBytePos;
use crate::heap_types::{LispMarker, MarkerBuffer};
use crate::tagged::gc::{TaggedHeap, TaggedValue, set_tagged_heap};
use crate::tagged::header::{GcHeader, MarkerObj, UNMARKED_AT_REST};
use neovm_edit::test_support::authority::chain_splice_at_head_for_test;
use neovm_value::test_support::authority::{
    publish_barrier_window_for_test, set_generational_enabled_for_test,
};
use neovm_value::value::positions::CharPos0;

struct ScratchRoots(usize);
impl ScratchRoots {
    fn new() -> Self {
        Self(crate::emacs_core::eval::save_scratch_gc_roots())
    }
    fn keep(&self, value: TaggedValue) {
        crate::emacs_core::eval::push_scratch_gc_root(value);
    }
}
impl Drop for ScratchRoots {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
    }
}

fn heap_with_generations(on: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    // SAFETY: serial fixture setup; no young-generation objects or
    // remembered-set state exist under the new mode.
    unsafe { set_generational_enabled_for_test(&mut heap, on) };
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { publish_barrier_window_for_test(&mut heap) };
    heap
}

fn header(value: TaggedValue) -> &'static mut GcHeader {
    unsafe { &mut *(TaggedHeap::value_heap_addr_for_test(value).unwrap() as *mut GcHeader) }
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection().unwrap();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

#[test]
fn minor_unchains_young_dead_marker_and_keeps_old_chain_node() {
    let mut heap = heap_with_generations(true);
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let marker_data = || {
        LispMarker::new(
            MarkerBuffer::DETACHED,
            CharPos0::new(0),
            EmacsBytePos::new(0),
            false,
            false,
        )
    };
    let old = heap.alloc_marker(marker_data());
    roots.keep(old);
    minor(&mut heap, &[old]);
    let young = heap.alloc_marker(marker_data());
    let text = crate::buffer::buffer_text::BufferText::new();
    let old_ptr = old.as_veclike_ptr().unwrap().cast::<MarkerObj>() as *mut MarkerObj;
    let young_ptr = young.as_veclike_ptr().unwrap().cast::<MarkerObj>() as *mut MarkerObj;
    // SAFETY: both markers are live in this heap and not yet chained; the
    // serial fixture holds the heap and text exclusively.
    unsafe {
        chain_splice_at_head_for_test(&text, old_ptr);
        chain_splice_at_head_for_test(&text, young_ptr);
    }
    // SAFETY: the slots point into this live BufferText and the fixture holds
    // the heap and buffer exclusively for the cycle.
    unsafe { heap.set_marker_chain_head_slots(vec![text.markers_head_slot_raw()]) };
    minor(&mut heap, &[]);
    assert_eq!(text.chain_walk_collect(), [old_ptr]);
    assert!(heap.marker_arena_owns_for_test(old_ptr.cast()));
    assert!(!heap.marker_arena_owns_for_test(young_ptr.cast()));
    assert_eq!(header(old).raw_mark(), UNMARKED_AT_REST);
    // Current full cycles still retain ordinary old nodes, and their raw
    // mark stays white even when a parity repeats (C2.6 supplies major reclaim).
    let mut parities = Vec::new();
    for _ in 0..3 {
        heap.collect_exact(std::iter::once(old));
        parities.push(heap.mark_parity_for_test());
        assert_eq!(header(old).raw_mark(), UNMARKED_AT_REST);
    }
    assert_eq!(parities[0], parities[2]);
}
