//! The generation census (`census.rs`): survivor classes across cycles on
//! both termination paths, the remembered-set probe, and that it changes
//! nothing it measures.

use super::knobs::{CensusMode, set_census_mode_for_test};
use super::*;

/// A heap created with the census in `mode`, installed on this thread. The
/// override stays set: the barrier's outlined part reads the process knob
/// (nextest runs each test in its own process).
fn census_heap(mode: CensusMode) -> Box<TaggedHeap> {
    set_census_mode_for_test(Some(mode));
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    heap
}

/// A proper list of `n` fresh conses holding fixnums.
fn list(heap: &mut TaggedHeap, n: usize) -> TaggedValue {
    let mut tail = TaggedValue::NIL;
    for i in 0..n {
        tail = heap.alloc_cons(TaggedValue::fixnum(i as i64), tail);
    }
    tail
}

fn last(heap: &TaggedHeap) -> CensusRecord {
    heap.last_census_for_test()
        .expect("the census records every cycle")
}

fn object_bytes(value: TaggedValue) -> usize {
    let addr = TaggedHeap::value_heap_addr(value).expect("a heap object");
    TaggedHeap::object_bytes_from_header(addr as *const GcHeader)
}

const CELL: usize = size_of::<ConsCell>();

#[test]
fn census_is_off_by_default() {
    let mut heap = census_heap(CensusMode::Off);
    let root = list(&mut heap, 10);
    heap.collect_exact(std::iter::once(root));
    assert!(heap.census.is_none());
    assert_eq!(heap.last_census_for_test(), None);
}

/// Conses across three stop-the-world cycles: everything is young at the
/// first census; what survived it is old at the second, where a new list is
/// young; that list dropped before the third is promoted-then-dead.
#[test]
fn census_classifies_conses_across_stw_cycles() {
    let mut heap = census_heap(CensusMode::Survivors);
    let a = list(&mut heap, 100);
    heap.collect_exact(std::iter::once(a));
    let first = last(&heap);
    assert_eq!(first.kind, CensusCycleKind::StopTheWorld);
    assert_eq!(first.cycle, heap.gc_collections());
    assert_eq!(first.old_survivors.conses, 0);
    assert_eq!(first.young_survivors.conses, 100);
    assert_eq!(first.promoted_dead.conses, 0);

    let b = list(&mut heap, 50);
    let root = heap.alloc_cons(a, b);
    heap.collect_exact(std::iter::once(root));
    let second = last(&heap);
    assert_eq!(second.cycle, first.cycle + 1);
    assert_eq!(
        second.old_survivors.conses, 100,
        "A survived the first census"
    );
    assert_eq!(second.young_survivors.conses, 51, "B and the new root");
    assert_eq!(second.promoted_dead.conses, 0);
    assert_eq!(second.old_survivors.bytes, 100 * CELL);

    // Drop B and the root: 51 young survivors of the second cycle die.
    heap.collect_exact(std::iter::once(a));
    let third = last(&heap);
    assert_eq!(third.old_survivors.conses, 100);
    assert_eq!(third.young_survivors.conses, 0);
    assert_eq!(third.promoted_dead.conses, 51);
    assert_eq!(third.promoted_dead.bytes, 51 * CELL);

    // An old object that dies is not promoted-then-dead twice, and a dead
    // old survivor is simply gone from the next census.
    heap.collect_exact(std::iter::empty());
    let fourth = last(&heap);
    assert_eq!(fourth.old_survivors.conses, 0);
    assert_eq!(fourth.young_survivors.conses, 0);
    assert_eq!(
        fourth.promoted_dead.conses, 0,
        "A was old, not young, last cycle"
    );
}

/// Non-cons objects (a page vector, a page string, a Box hash table) follow
/// the same classes, with their `object_bytes_from_header` sizes.
#[test]
fn census_classifies_objects_with_their_bytes() {
    let mut heap = census_heap(CensusMode::Survivors);
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(1); 4]);
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("census"));
    let root = heap.alloc_cons(v, s);
    heap.collect_exact(std::iter::once(root));
    let first = last(&heap);
    assert_eq!(first.young_survivors.objects, 2);
    assert_eq!(
        first.young_survivors.bytes,
        CELL + object_bytes(v) + object_bytes(s)
    );

    let w = heap.alloc_vector(vec![TaggedValue::NIL; 9]);
    let root2 = heap.alloc_cons(w, root);
    heap.collect_exact(std::iter::once(root2));
    let second = last(&heap);
    assert_eq!(second.old_survivors.objects, 2);
    assert_eq!(
        second.old_survivors.bytes,
        CELL + object_bytes(v) + object_bytes(s)
    );
    assert_eq!(second.young_survivors.objects, 1);
    assert_eq!(second.young_survivors.bytes, CELL + object_bytes(w));

    let w_bytes = object_bytes(w);
    heap.collect_exact(std::iter::once(root));
    let third = last(&heap);
    assert_eq!(third.promoted_dead.objects, 1);
    assert_eq!(third.promoted_dead.conses, 1);
    assert_eq!(third.promoted_dead.bytes, CELL + w_bytes);
    assert_eq!(third.old_survivors.objects, 2);
}

/// A concurrent cycle takes its census at the termination, with the bytes
/// its mark window allocated.
#[test]
fn census_runs_at_the_concurrent_termination() {
    let mut heap = census_heap(CensusMode::Survivors);
    let a = list(&mut heap, 64);
    // The stop-the-world bootstrap enables concurrent cycles on a dump-less
    // heap.
    heap.collect_exact(std::iter::once(a));
    assert!(heap.should_run_concurrent());

    heap.concurrent_begin();
    heap.seed_root(a);
    heap.launch_concurrent_mark();
    let mid = list(&mut heap, 10);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(a);
    heap.seed_root(mid);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    let record = last(&heap);
    assert_eq!(record.kind, CensusCycleKind::Concurrent);
    assert_eq!(record.cycle, heap.gc_collections());
    assert_eq!(record.old_survivors.conses, 64);
    assert_eq!(
        record.young_survivors.conses, 10,
        "born black in the window"
    );
    assert!(
        record.mark_window_alloc_bytes >= 10 * CELL,
        "the window allocated the 10 conses: {record:?}"
    );
}

/// A released cons block's history goes with it: a block later allocated at
/// the same address starts young.
#[test]
fn census_forgets_a_released_cons_block() {
    let mut heap = census_heap(CensusMode::Survivors);
    let a = list(&mut heap, 100);
    heap.collect_exact(std::iter::once(a));
    let base = ConsBlock::block_base_for_ptr(a.as_cons_ptr().unwrap());
    assert!(heap.census.as_deref().unwrap().has_cons_block_history(base));
    // Nothing survives: the only block empties and is released.
    heap.collect_exact(std::iter::empty());
    assert!(heap.cons_blocks.is_empty(), "the empty block is released");
    assert!(!heap.census.as_deref().unwrap().has_cons_block_history(base));
}

/// The remembered-set probe: the window covers every owner, and each old
/// owner written with a heap value counts once per cycle, with its heap
/// children; immediates and young owners do not count.
#[test]
fn census_remset_probe_counts_old_owners_once() {
    let mut heap = census_heap(CensusMode::SurvivorsAndRemset);
    assert_eq!(published_barrier_window(), BarrierWindow::ALL);
    assert_eq!(heap.jit_barrier_window_for_test(), BarrierWindow::ALL);
    let old_a = heap.alloc_vector(vec![TaggedValue::NIL; 3]);
    let old_b = heap.alloc_vector(vec![TaggedValue::NIL; 3]);
    let root = heap.alloc_cons(old_a, old_b);
    heap.collect_exact(std::iter::once(root));

    let young = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let child1 = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let child2 = heap.alloc_cons(TaggedValue::fixnum(2), TaggedValue::NIL);
    assert!(crate::tagged::mutate::set_vector_slot(old_a, 0, child1));
    assert!(crate::tagged::mutate::set_vector_slot(old_a, 1, child2));
    // An immediate creates no edge; a young owner is traced by a minor anyway.
    assert!(crate::tagged::mutate::set_vector_slot(
        old_b,
        0,
        TaggedValue::fixnum(5)
    ));
    assert!(crate::tagged::mutate::set_vector_slot(young, 0, child1));
    // The old root cons, written with a heap value, counts too.
    assert!(crate::tagged::mutate::set_cons_cdr(root, young));

    heap.collect_exact(std::iter::once(root));
    let record = last(&heap);
    assert_eq!(
        record.remset_owners, 2,
        "old_a and the root cons: {record:?}"
    );
    // old_a holds two conses; the root cons holds old_a and `young`.
    assert_eq!(record.remset_children, 4);

    // The next cycle starts an empty set.
    heap.collect_exact(std::iter::once(root));
    assert_eq!(last(&heap).remset_owners, 0);
}

/// The census measures and changes nothing: the same allocation script
/// leaves the same live bytes and counters with it on and off.
#[test]
fn census_changes_nothing_it_measures() {
    fn run(mode: CensusMode) -> (usize, [u64; MEMORY_USE_COUNT_LEN], usize) {
        let mut heap = census_heap(mode);
        let mut keep = TaggedValue::NIL;
        for round in 0..4 {
            let v = heap.alloc_vector(vec![TaggedValue::fixnum(round); 5]);
            let l = list(&mut heap, 30);
            let _garbage = list(&mut heap, 20);
            let pair = heap.alloc_cons(v, l);
            keep = heap.alloc_cons(pair, keep);
            heap.collect_exact(std::iter::once(keep));
        }
        let out = (
            heap.live_bytes(),
            heap.memory_use_counts_snapshot(),
            heap.allocated_count(),
        );
        clear_tagged_heap_if_installed(&heap);
        out
    }
    assert_eq!(run(CensusMode::Off), run(CensusMode::Survivors));
    assert_eq!(run(CensusMode::Off), run(CensusMode::SurvivorsAndRemset));
}

/// `NEOVM_GC_CENSUS_FILE`: every record is also appended to the file, one
/// line per cycle, in the format `tmp/census/parse_census.py` reads.
#[test]
fn census_records_go_to_the_census_file() {
    let dir = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(format!("census-file-test-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // SAFETY: nextest runs each test in its own process; the census reads
    // the variable once, at its first record below.
    unsafe { std::env::set_var("NEOVM_GC_CENSUS_FILE", &path) };
    let mut heap = census_heap(CensusMode::Survivors);
    let a = list(&mut heap, 5);
    heap.collect_exact(std::iter::once(a));
    heap.collect_exact(std::iter::once(a));
    let text = std::fs::read_to_string(&path).expect("the census file");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("NEOVM_GC census gc#"), "{text}");
    assert!(lines[1].contains("old_surv=(5,0,"), "{text}");
    let _ = std::fs::remove_file(&path);
}
