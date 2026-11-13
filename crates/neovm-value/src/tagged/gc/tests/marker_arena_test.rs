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
/// A detached marker whose positions carry `id` as its identity.
fn mk(heap: &mut TaggedHeap, id: u64) -> TaggedValue {
    heap.alloc_marker(LispMarker::new(
        crate::heap_types::MarkerBuffer::DETACHED,
        crate::value::positions::CharPos0::new(id as usize),
        crate::heap_types::byte_positions::EmacsBytePos::new(id as usize),
        false,
        true,
    ))
}
fn mk_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}
fn mk_id(v: TaggedValue) -> usize {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const MarkerObj) };
    obj.data.charpos.get()
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
/// (a) PAGE-SPAN ORACLE EXACTNESS for the 64B class + cross-class
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
    assert_eq!(mk_id(live), 10);
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
    assert_eq!(mk_id(next), 3, "reused slot must be fully rewritten");
    assert_eq!(mk_id(keep), 2);
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
    assert_eq!(mk_id(b), 25);
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
    assert_eq!(mk_id(b), 25);
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
