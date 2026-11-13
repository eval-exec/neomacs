//! THE MARKER-SPECIFIC INVARIANT (moved from value by the module-cluster
//! ruling; the GC-side unchain proof needs the edit-layer BufferText that owns
//! the chain head): markers are intrusively linked into per-buffer chains by
//! raw pointer. A dead marker still on a chain must be spliced out by
//! `unchain_dead_markers` BEFORE the sweep frees its slot, so that reusing the
//! slot for a fresh marker can never leave a chain link pointing at recycled
//! memory.

use crate::heap_types::LispMarker;
use crate::tagged::gc::{TaggedHeap, TaggedValue, set_tagged_heap};
use crate::tagged::header::MarkerObj;
use neovm_edit::test_support::authority::chain_splice_at_head_for_test;
use neovm_value::value::positions::CharPos0;

/// A detached marker whose positions carry `id` as its identity.
fn mk(heap: &mut TaggedHeap, id: u64) -> TaggedValue {
    heap.alloc_marker(LispMarker::new(
        crate::heap_types::MarkerBuffer::DETACHED,
        CharPos0::new(id as usize),
        crate::heap_types::byte_positions::EmacsBytePos::new(id as usize),
        false,
        true,
    ))
}
fn mk_id(v: TaggedValue) -> usize {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const MarkerObj) };
    obj.data.charpos.get()
}

/// THE MARKER-SPECIFIC INVARIANT (the one place the other arenas offer no
/// precedent): markers are intrusively linked into per-buffer chains by raw
/// pointer. A dead marker still on a chain must be spliced out by
/// `unchain_dead_markers` BEFORE the sweep frees its slot, so that reusing
/// the slot for a fresh marker can never leave a chain link pointing at
/// recycled memory.
#[test]
fn dead_chained_marker_is_unchained_before_its_slot_is_freed_and_reused() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let bt = crate::buffer::buffer_text::BufferText::new();
    let live = mk(&mut heap, 1);
    let dead = mk(&mut heap, 2);
    let live_ptr = live.as_veclike_ptr().unwrap() as *mut MarkerObj;
    let dead_ptr = dead.as_veclike_ptr().unwrap() as *mut MarkerObj;
    // SAFETY: both markers are live in this heap and not yet chained; the
    // serial fixture holds the heap and text exclusively.
    unsafe {
        chain_splice_at_head_for_test(&bt, live_ptr);
        chain_splice_at_head_for_test(&bt, dead_ptr);
    }
    assert_eq!(bt.chain_walk_collect(), vec![dead_ptr, live_ptr]);
    // What the command loop does each cycle: hand the GC every buffer's
    // chain-head slot so `unchain_dead_markers` can splice the dead out.
    unsafe { heap.set_marker_chain_head_slots(vec![bt.markers_head_slot_raw()]) };
    heap.collect_exact(std::iter::once(live));
    assert!(
        !heap.marker_arena_owns_for_test(dead_ptr as *const u8),
        "the unrooted marker's slot must be freed",
    );
    assert!(heap.marker_arena_owns_for_test(live_ptr as *const u8));
    assert_eq!(
        bt.chain_walk_collect(),
        vec![live_ptr],
        "the dead marker must leave the chain before its slot is freed",
    );
    // Reuse the freed slot for a fresh marker: the chain must not see it.
    let fresh = mk(&mut heap, 3);
    let fresh_ptr = fresh.as_veclike_ptr().unwrap() as *mut MarkerObj;
    assert_eq!(
        fresh_ptr, dead_ptr,
        "the fresh marker reuses the freed slot"
    );
    assert!(
        unsafe { (*fresh_ptr).data.next_marker_ptr().is_null() },
        "a reused slot must be fully rewritten — no stale chain link",
    );
    assert_eq!(mk_id(fresh), 3);
    assert_eq!(
        bt.chain_walk_collect(),
        vec![live_ptr],
        "the chain must not resurrect the recycled slot",
    );
    heap.assert_object_arenas_coherent();
}
