//! `NEOVM_GC_VEC_SCAN=defer` (falsifier F-G (c), design P3.2 F1b): with no
//! Tier-B snapshot, page vectors defer to the termination and are traced by
//! reachability — a dead vector no longer marks its children, and a live
//! one, stored into mid-cycle or not, keeps them.

use super::knobs::{VecScanMode, set_chunk_map_for_test, set_vec_scan_mode_for_test};
use super::*;

fn heap_with(mode: VecScanMode, chunk_map: bool) -> Box<TaggedHeap> {
    set_vec_scan_mode_for_test(Some(mode));
    set_chunk_map_for_test(Some(chunk_map));
    let mut heap = Box::new(TaggedHeap::new());
    set_vec_scan_mode_for_test(None);
    set_chunk_map_for_test(None);
    set_tagged_heap(&mut heap);
    heap
}

fn list(heap: &mut TaggedHeap, n: i64) -> TaggedValue {
    let mut tail = TaggedValue::NIL;
    for i in 0..n {
        tail = heap.alloc_cons(TaggedValue::fixnum(i), tail);
    }
    tail
}

/// Start a concurrent mark over `roots`, run `during` while it marks, then
/// terminate with `roots` and hand back before the sweep drains (marks are
/// still this cycle's).
fn concurrent_cycle_until_termination(
    heap: &mut TaggedHeap,
    roots: &[TaggedValue],
    during: impl FnOnce(&mut TaggedHeap),
) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    during(heap);
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
}

/// A dead vector marks its children through the Tier-B snapshot (floating
/// garbage for one cycle); deferred, it does not. Both chunk-map settings.
#[test]
fn a_dead_vector_marks_its_children_only_through_the_snapshot() {
    for chunk_map in [false, true] {
        for (mode, marked) in [(VecScanMode::Snapshot, true), (VecScanMode::Defer, false)] {
            let mut heap = heap_with(mode, chunk_map);
            let keep = list(&mut heap, 10);
            heap.collect_exact(std::iter::once(keep));
            assert!(heap.should_run_concurrent());
            // Dead before the mark starts: nothing roots it.
            let child = list(&mut heap, 100);
            let _dead = heap.alloc_vector(vec![child]);
            concurrent_cycle_until_termination(&mut heap, &[keep], |_| {});
            assert_eq!(
                heap.is_value_marked(child),
                marked,
                "{mode:?}, chunk map {chunk_map}: the dead vector's child"
            );
            let snapshot_len = heap.handshake_stats().probe_vector_snapshot_len;
            match mode {
                VecScanMode::Snapshot => assert!(snapshot_len > 0),
                VecScanMode::Defer => {
                    assert_eq!(snapshot_len, 0);
                    assert_eq!(
                        heap.sweep_stats().last_concurrent_vec_claimed,
                        0,
                        "no vector claims"
                    );
                }
            }
            heap.finish_incremental_sweep_now();
            assert!(heap.is_value_marked(keep));
            clear_tagged_heap_if_installed(&heap);
        }
    }
}

/// Deferred live vectors keep their children, including one stored and one
/// bulk-written mid-cycle; the bulk write clones nothing (there is no
/// snapshot to protect).
#[test]
fn deferred_live_vectors_keep_their_children_across_mid_cycle_writes() {
    let mut heap = heap_with(VecScanMode::Defer, true);
    let a_child = list(&mut heap, 20);
    let a = heap.alloc_vector(vec![a_child, TaggedValue::NIL]);
    let b = heap.alloc_vector(vec![TaggedValue::NIL; 3]);
    let root = heap.alloc_cons(a, b);
    heap.collect_exact(std::iter::once(root));
    let mut stored = TaggedValue::NIL;
    let mut bulk = TaggedValue::NIL;
    concurrent_cycle_until_termination(&mut heap, &[root], |heap| {
        stored = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
        assert!(crate::tagged::mutate::set_vector_slot(a, 1, stored));
        bulk = heap.alloc_string(crate::heap_types::LispString::from_utf8("bulk"));
        let wrote = crate::tagged::mutate::with_vector_data_mut(b, |items| {
            items[2] = bulk;
        });
        assert!(wrote.is_some());
        assert!(
            heap.retired_vector_buffers.is_empty(),
            "no clone-on-write without a snapshot"
        );
    });
    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.vector >= 2,
        "the page vectors deferred to the termination: {:?}",
        stats.last_termination_kinds
    );
    heap.finish_incremental_sweep_now();
    for value in [a, b, a_child, stored] {
        assert!(heap.is_value_marked(value), "{value:?}");
    }
    assert!(heap.owns_string_object(bulk.as_string_ptr().unwrap() as *const u8));
    // And the next cycle still traces them.
    concurrent_cycle_until_termination(&mut heap, &[root], |_| {});
    heap.finish_incremental_sweep_now();
    assert!(heap.is_value_marked(stored));
}
