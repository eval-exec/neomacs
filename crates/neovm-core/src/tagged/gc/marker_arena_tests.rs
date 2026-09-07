use super::*;
use crate::heap_types::LispMarker;
fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    heap.extend_dump_span(4096, 16);
}
fn run_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}
/// A detached marker whose `marker_id`/positions carry `id` as its identity.
fn mk(heap: &mut TaggedHeap, id: u64) -> TaggedValue {
    heap.alloc_marker(LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: Some(id),
        bytepos: id as usize,
        charpos: id as usize,
        last_position_valid: true,
        next_marker: std::ptr::null_mut(),
    })
}
fn mk_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}
fn mk_id(v: TaggedValue) -> Option<u64> {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const MarkerObj) };
    obj.data.marker_id
}
/// Marker is POD (no Drop): `LispMarker` holds no Values and its chain link
/// is a raw pointer, so the generic sweep/teardown `drop_in_place` walk
/// compiles out exactly as for floats and symbols-with-pos.
#[test]
fn marker_is_pod() {
    assert!(
        !std::mem::needs_drop::<MarkerObj>(),
        "MarkerObj must stay POD (no Drop) — if this fails a Drop-worthy \
         field was added and the sweep must drop_in_place it",
    );
}
/// (a) PAGE-SPAN ORACLE EXACTNESS for the 128B class + cross-class
/// no-collision with the same-stride macro arena.
#[test]
fn marker_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let keep = mk(&mut heap, 1);
    let dead = mk(&mut heap, 2);
    let keep2 = mk(&mut heap, 3);
    let dead_addr = mk_ptr(dead) as usize;
    // Page-only: no marker ever enters the residual addr-set.
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.marker_arena.owns(mk_ptr(dead)));
    let m = heap.alloc_macro(vec![TaggedValue::fixnum(1); 4]);
    heap.collect_exact([keep, keep2, m].into_iter());
    let b_addr = mk_ptr(keep) as usize;
    assert!(heap.marker_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    assert!(!heap.marker_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    assert!(!heap.marker_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.marker_arena.owns((b_addr + 64) as *const u8));
    assert!(!heap.marker_arena.owns((b_addr + 1) as *const u8));
    let page_base = ObjectPage::<MarkerObj>::page_base_for_ptr(b_addr as *const MarkerObj);
    let beyond_bump = page_base + (MARKER_PAGE_SLOTS - 12) * <MarkerObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.marker_arena.owns(beyond_bump as *const u8));
    // Same-stride sibling arena (macro, 128B) never collides.
    let m_addr = m.as_veclike_ptr().unwrap() as usize;
    assert!(!heap.marker_arena.owns(m_addr as *const u8));
    assert!(!heap.macro_arena.owns(b_addr as *const u8));
    heap.assert_object_arenas_coherent();
}
/// (g) ownership-index-tracks-sweep; addr-set empty; identity intact.
#[test]
fn marker_ownership_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let live = mk(&mut heap, 10);
    let dead = mk(&mut heap, 20);
    let live_ptr = mk_ptr(live);
    let dead_ptr = mk_ptr(dead);
    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    heap.collect_exact(std::iter::once(live));
    assert!(heap.marker_arena.owns(live_ptr));
    assert!(!heap.marker_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert_eq!(mk_id(live), Some(10));
    heap.assert_object_arenas_coherent();
}
/// THE POINT OF THE ARENA: a freed marker slot is handed back to the very
/// next allocation (class free list), so `save-excursion`'s make/free churn
/// recycles a few cache-warm slots instead of scattering fresh `Box`es.
#[test]
fn marker_freed_slot_is_reused_by_the_next_allocation() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let transient = mk(&mut heap, 1);
    let transient_ptr = mk_ptr(transient);
    let keep = mk(&mut heap, 2);
    heap.collect_exact(std::iter::once(keep));
    assert!(!heap.marker_arena.owns(transient_ptr));
    let next = mk(&mut heap, 3);
    assert_eq!(
        mk_ptr(next),
        transient_ptr,
        "the next marker must reuse the freed slot — the locality win",
    );
    assert_eq!(mk_id(next), Some(3), "reused slot must be fully rewritten");
    assert_eq!(mk_id(keep), Some(2));
    heap.assert_object_arenas_coherent();
}
/// (b) Parity two-cycle survival/reclaim under the concurrent collector.
fn parity_two_cycle_marker_survival_and_reclaim_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
    }
    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    let b = mk(&mut heap, 25);
    let b_ptr = mk_ptr(b);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "allocate-black marker must survive its birth cycle",
    );
    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_eq!(mk_id(b), Some(25));
    let g1 = mk(&mut heap, 91);
    let g1_ptr = mk_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = mk(&mut heap, 92);
    let g2_ptr = mk_ptr(g2);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(b);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage must be reclaimed by the next cycle",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage floats through its birth cycle",
    );
    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage must be reclaimed by the SECOND cycle",
    );
    assert_eq!(mk_id(b), Some(25));
    heap.assert_object_arenas_coherent();
}
#[test]
fn parity_two_cycle_marker_survival_and_reclaim() {
    parity_two_cycle_marker_survival_and_reclaim_body(false);
}
#[test]
fn parity_two_cycle_marker_survival_and_reclaim_verified() {
    parity_two_cycle_marker_survival_and_reclaim_body(true);
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
    bt.chain_splice_at_head(live_ptr);
    bt.chain_splice_at_head(dead_ptr);
    assert_eq!(bt.chain_walk_collect(), vec![dead_ptr, live_ptr]);
    // What the command loop does each cycle: hand the GC every buffer's
    // chain-head slot so `unchain_dead_markers` can splice the dead out.
    unsafe { heap.set_marker_chain_head_slots(vec![bt.markers_head_slot_raw()]) };
    heap.collect_exact(std::iter::once(live));
    assert!(
        !heap.marker_arena.owns(dead_ptr as *const u8),
        "the unrooted marker's slot must be freed",
    );
    assert!(heap.marker_arena.owns(live_ptr as *const u8));
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
        unsafe { (*fresh_ptr).data.next_marker.is_null() },
        "a reused slot must be fully rewritten — no stale chain link",
    );
    assert_eq!(mk_id(fresh), Some(3));
    assert_eq!(
        bt.chain_walk_collect(),
        vec![live_ptr],
        "the chain must not resurrect the recycled slot",
    );
    heap.assert_object_arenas_coherent();
}
