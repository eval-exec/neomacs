use super::*;

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
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

/// slot 0 = fixnum IDENTITY, slot 1 = `child`, padded to `n` NILs.
fn record_items(id: i64, child: TaggedValue, n: usize) -> Vec<TaggedValue> {
    let mut v = vec![TaggedValue::NIL; n.max(2)];
    v[0] = TaggedValue::fixnum(id);
    v[1] = child;
    v
}
fn rec_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}
fn rec_slot(v: TaggedValue, i: usize) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const RecordObj) };
    obj.data.as_slice()[i]
}

/// (a) PAGE-SPAN ORACLE EXACTNESS for the 64B record class + cross-class
/// no-collision (incl. the same-stride string/vector arenas).
#[test]
fn record_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let keep = heap.alloc_record(record_items(1, TaggedValue::NIL, 4));
    let dead = heap.alloc_record(record_items(2, TaggedValue::NIL, 4));
    let keep2 = heap.alloc_record(record_items(3, TaggedValue::NIL, 4));
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(1); 4]);
    let dead_addr = rec_ptr(dead) as usize;

    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.record_arena.owns(rec_ptr(dead)));

    heap.collect_exact([keep, keep2, v].into_iter());

    let b_addr = rec_ptr(keep) as usize;
    assert!(heap.record_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    assert!(!heap.record_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    assert!(!heap.record_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.record_arena.owns((b_addr + 32) as *const u8));
    assert!(!heap.record_arena.owns((b_addr + 1) as *const u8));
    let page_base = ObjectPage::<RecordObj>::page_base_for_ptr(b_addr as *const RecordObj);
    let beyond_bump = page_base + 800 * <RecordObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.record_arena.owns(beyond_bump as *const u8));
    // Same-stride sibling arenas (vector/string 64B) never collide.
    let v_addr = v.as_veclike_ptr().unwrap() as usize;
    assert!(!heap.record_arena.owns(v_addr as *const u8));
    assert!(!heap.vector_arena.owns(b_addr as *const u8));
    assert!(!heap.string_arena.owns(b_addr as *const u8));
    assert!(!heap.float_arena.owns(b_addr as *const u8));
    heap.assert_object_arenas_coherent();
}

/// (g) ownership-index-tracks-sweep; addr-set stays empty; payload intact.
#[test]
fn record_ownership_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let live = heap.alloc_record(record_items(10, TaggedValue::NIL, 6));
    let dead = heap.alloc_record(record_items(20, TaggedValue::NIL, 6));
    let live_ptr = rec_ptr(live);
    let dead_ptr = rec_ptr(dead);

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);

    heap.collect_exact(std::iter::once(live));

    assert!(heap.record_arena.owns(live_ptr));
    assert!(!heap.record_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert_eq!(rec_slot(live, 0).as_fixnum(), Some(10));
    heap.assert_object_arenas_coherent();
}

/// (b) Parity two-cycle survival/reclaim.
fn parity_two_cycle_record_survival_and_reclaim_body(verify: bool) {
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
    let b = heap.alloc_record(record_items(25, TaggedValue::NIL, 4));
    let b_ptr = rec_ptr(b);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "allocate-black record must survive the cycle it was born in",
    );
    heap.assert_object_arenas_coherent();

    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_eq!(rec_slot(b, 0).as_fixnum(), Some(25));

    let g1 = heap.alloc_record(record_items(-9, TaggedValue::NIL, 4));
    let g1_ptr = rec_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = heap.alloc_record(record_items(-8, TaggedValue::NIL, 4));
    let g2_ptr = rec_ptr(g2);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(b);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage record must be reclaimed by the next cycle",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage record floats through its birth cycle",
    );

    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage record must be reclaimed by the SECOND cycle",
    );
    assert_eq!(rec_slot(b, 0).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_record_survival_and_reclaim() {
    parity_two_cycle_record_survival_and_reclaim_body(false);
}
#[test]
fn parity_two_cycle_record_survival_and_reclaim_verified() {
    parity_two_cycle_record_survival_and_reclaim_body(true);
}

/// (TRAP A) Records parked in `deferred` resolve at termination through
/// the page-oracle-routed veclike arm; slot children traced. Records stay
/// DEFERRED for marking (`record` drain bucket).
fn deferred_record_resolves_at_termination_body(verify: bool) {
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

    let mut list = TaggedValue::fixnum(0);
    let mut records = Vec::new();
    let mut children = Vec::new();
    for i in 0..300 {
        let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
        children.push(child);
        let b = heap.alloc_record(record_items(i, child, 4));
        records.push(b);
        list = heap.alloc_cons(b, list);
    }
    let garbage = heap.alloc_record(record_items(-1, TaggedValue::NIL, 4));
    let garbage_ptr = rec_ptr(garbage);

    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(list);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.record >= 300,
        "every rooted record must reach the termination via `deferred` \
         (got {})",
        stats.last_termination_kinds.record,
    );
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(list);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, b) in records.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(rec_ptr(*b)),
            "deferred-then-resolved record {i} was swept while rooted",
        );
        assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(i as i64));
        assert_eq!(
            unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
            Some(10_000 + i as i64),
            "record {i}'s slot child was swept while live",
        );
    }
    assert!(!heap.owns_non_cons_object(garbage_ptr));
    heap.assert_object_arenas_coherent();
}

#[test]
fn deferred_record_resolves_at_termination() {
    deferred_record_resolves_at_termination_body(false);
}
#[test]
fn deferred_record_resolves_at_termination_verified() {
    deferred_record_resolves_at_termination_body(true);
}

/// ALLOCATED-BIT-FIRST under adversarial staleness (payload class).
fn record_freed_slot_garbage_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
        heap.collect_exact(std::iter::empty());
    }

    let mut records = Vec::new();
    for i in 0..100 {
        records.push(heap.alloc_record(record_items(i, TaggedValue::NIL, 4)));
    }
    let keep: Vec<TaggedValue> = records.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut RecordObj> = records
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut RecordObj)
        .collect();

    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<RecordObj>()) };
    }
    heap.assert_object_arenas_coherent();

    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(rec_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
    }

    let mut reused = Vec::new();
    for i in 0..dead_ptrs.len() {
        reused.push(heap.alloc_record(record_items(500 + i as i64, TaggedValue::NIL, 6)));
    }
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_veclike_ptr().unwrap() as *const RecordObj;
        assert!(dead_addrs.contains(&(ptr as usize)));
        unsafe {
            assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
            assert_eq!((*ptr).header.type_tag, VecLikeType::Record);
            assert!(
                !(*ptr).header.gc.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.gc.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert_eq!(rec_slot(*r, 0).as_fixnum(), Some(500 + i as i64));
    }
    heap.assert_object_arenas_coherent();

    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(rec_ptr(*r)));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn record_freed_slot_garbage_never_read() {
    record_freed_slot_garbage_never_read_body(false);
}
#[test]
fn record_freed_slot_garbage_never_read_verified() {
    record_freed_slot_garbage_never_read_body(true);
}

/// Mid-sweep cooperative-window slot reuse (payload class).
#[test]
fn record_reuse_within_one_cooperative_sweep_window() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let n = 3 * RECORD_PAGE_SLOTS;
    let mut records = Vec::with_capacity(n);
    for i in 0..n {
        records.push(heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 2)));
    }
    assert_eq!(heap.record_arena.pages.len(), 3);

    let keep: Vec<TaggedValue> = records.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = records
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| rec_ptr(*v) as usize)
        .collect();
    let page0_base = heap.record_arena.pages[0].base_addr();

    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    assert!(heap.sweep_in_progress());
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");

    let mut reused = Vec::new();
    for i in 0..32 {
        reused.push(heap.alloc_record(record_items(1_000 + i, TaggedValue::NIL, 2)));
    }
    for r in &reused {
        let ptr = r.as_veclike_ptr().unwrap() as *const RecordObj;
        assert_eq!(ObjectPage::<RecordObj>::page_base_for_ptr(ptr), page0_base);
        assert!(dead_addrs.contains(&(ptr as usize)));
    }
    heap.assert_object_arenas_coherent();

    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    for (i, r) in reused.iter().enumerate() {
        assert!(heap.owns_non_cons_object(rec_ptr(*r)));
        assert_eq!(rec_slot(*r, 0).as_fixnum(), Some(1_000 + i as i64));
    }
}

/// (c) VARIABLE-size live-bytes on BOTH recompute sites.
#[test]
fn record_sweep_live_bytes_track_variable_payload_sizes() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let r_big = heap.alloc_record(record_items(7, TaggedValue::NIL, 2_000));
    let r_small = heap.alloc_record(record_items(1, TaggedValue::NIL, 2));
    let _dead = heap.alloc_record(record_items(0, TaggedValue::NIL, 4_000));
    let mut root = TaggedValue::fixnum(0);
    let mut cons_count = 0usize;
    for val in [r_big, r_small] {
        root = heap.alloc_cons(val, root);
        cons_count += 1;
    }

    let expected_objects: usize = [r_big, r_small]
        .iter()
        .map(|b| {
            TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
        })
        .sum::<usize>();
    let expected = expected_objects + cons_count * size_of::<ConsCell>();
    assert!(expected_objects > 2 * size_of::<RecordObj>() + 2_000 * size_of::<TaggedValue>());

    heap.collect_exact(std::iter::once(root));
    assert_eq!(heap.live_bytes(), expected, "eager site");

    heap.begin_collection();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(heap.live_bytes(), expected, "incremental site");
}

/// (d) LOADUP-SHAPED tenure + FULL-page retirement (C1).
fn record_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut root = TaggedValue::fixnum(0);
    let mut records = Vec::with_capacity(RECORD_PAGE_SLOTS + 2);
    for i in 0..(RECORD_PAGE_SLOTS + 2) {
        let b = heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 4));
        records.push(b);
        root = heap.alloc_cons(b, root);
    }
    assert_eq!(heap.record_arena.pages.len(), 2);
    assert_eq!(heap.record_arena.pages[0].allocated, RECORD_PAGE_SLOTS);

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    for b in &records {
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });
    }
    assert!(heap.record_arena.pages[0].retired, "full page must retire");
    assert!(!heap.record_arena.pages[1].retired);
    assert!(heap.owns_non_cons_object(rec_ptr(records[0])));
    heap.assert_object_arenas_coherent();

    let retired_base = heap.record_arena.pages[0].base_addr();
    let fresh = heap.alloc_record(record_items(-5, TaggedValue::NIL, 2));
    assert_ne!(
        ObjectPage::<RecordObj>::page_base_for_ptr(
            fresh.as_veclike_ptr().unwrap() as *const RecordObj
        ),
        retired_base,
    );

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, b) in records.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(rec_ptr(*b)),
                "tenured page record #{i} lost on cycle {cycle}",
            );
            assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(i as i64));
        }
        assert_eq!(heap.record_arena.pages[0].allocated, RECORD_PAGE_SLOTS);
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn record_survivors_tenure_and_full_pages_retire() {
    record_survivors_tenure_and_full_pages_retire_body(false);
}
#[test]
fn record_survivors_tenure_and_full_pages_retire_verified() {
    record_survivors_tenure_and_full_pages_retire_body(true);
}

/// (d, mixed) Tenured + young slots share a record page across parities.
fn record_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut keep = Vec::new();
    let mut root = TaggedValue::fixnum(0);
    for i in 0..10 {
        let b = heap.alloc_record(record_items(i as i64, TaggedValue::NIL, 4));
        if i % 2 == 0 {
            keep.push(b);
            root = heap.alloc_cons(b, root);
        }
    }
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(!heap.record_arena.pages[0].retired);

    for cycle in 0..2 {
        for i in 0..5 {
            let _ = heap.alloc_record(record_items(-(i as i64), TaggedValue::NIL, 4));
        }
        heap.collect_exact(std::iter::once(root));
        for (i, b) in keep.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(rec_ptr(*b)),
                "tenured record #{i} freed on parity cycle {cycle}",
            );
            assert_eq!(rec_slot(*b, 0).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn record_mixed_page_tenured_survive_alternating_parities() {
    record_mixed_page_tenured_survive_alternating_parities_body(false);
}
#[test]
fn record_mixed_page_tenured_survive_alternating_parities_verified() {
    record_mixed_page_tenured_survive_alternating_parities_body(true);
}

/// (e) Payload-bearing teardown counters + sweep-time drop_in_place.
fn record_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_RECORD_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        let mut root = TaggedValue::fixnum(0);
        for i in 0..3_000 {
            let b = heap.alloc_record(record_items(i, TaggedValue::NIL, 16));
            if i % 2 == 0 {
                root = heap.alloc_cons(b, root);
            }
        }
        assert!(LIVE_RECORD_PAGES.load(Ordering::Relaxed) > before);

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        heap.assert_object_arenas_coherent();

        if mid_mark {
            heap.concurrent_begin();
            heap.seed_root(root);
            heap.launch_concurrent_mark();
            assert!(heap.concurrent_mark_running());
        }
        drop(heap);
    }
    assert_eq!(
        LIVE_RECORD_PAGES.load(Ordering::Relaxed),
        before,
        "record pages leaked or double-freed at teardown",
    );
}

#[test]
fn record_payload_pages_freed_at_heap_drop() {
    record_payload_pages_freed_at_heap_drop_body(false);
}
#[test]
fn record_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
    record_payload_pages_freed_at_heap_drop_body(true);
}

/// Promotion-scan coverage: a tenured page record whose slot holds a
/// young cons child keeps it alive across both parities.
fn tenured_page_record_keeps_young_cons_child_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
    let b = heap.alloc_record(record_items(1, y, 4));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
            Some(999),
            "tenured page record's young cons child lost on cycle {cycle}",
        );
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_page_record_keeps_young_cons_child_alive() {
    tenured_page_record_keeps_young_cons_child_alive_body(false);
}
#[test]
fn tenured_page_record_keeps_young_cons_child_alive_verified() {
    tenured_page_record_keeps_young_cons_child_alive_body(true);
}

/// WindowConfiguration shares the record arena (same `RecordObj`, distinct
/// tag): page-owned, coherent (the type-tag check accepts the tag), and
/// survives a GC alongside plain records.
#[test]
fn window_configuration_shares_record_arena() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let wc = heap.alloc_window_configuration(record_items(42, TaggedValue::NIL, 4));
    let rec = heap.alloc_record(record_items(7, TaggedValue::NIL, 4));
    assert_eq!(
        wc.veclike_type(),
        Some(VecLikeType::WindowConfiguration),
        "tag must be WindowConfiguration",
    );
    assert!(
        heap.record_arena.owns(rec_ptr(wc)),
        "window-configuration must live on the record arena pages",
    );
    assert!(heap.owns_veclike_object(rec_ptr(wc)));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    heap.assert_object_arenas_coherent();

    let tail = heap.alloc_cons(rec, TaggedValue::fixnum(0));
    let root = heap.alloc_cons(wc, tail);
    heap.collect_exact(std::iter::once(root));
    assert!(heap.owns_non_cons_object(rec_ptr(wc)));
    assert!(heap.owns_non_cons_object(rec_ptr(rec)));
    assert_eq!(rec_slot(wc, 0).as_fixnum(), Some(42));
    assert_eq!(
        wc.veclike_type(),
        Some(VecLikeType::WindowConfiguration),
        "tag survives the arena round-trip",
    );

    // A dead window-configuration is reclaimed via the record page sweep.
    let dead_wc = heap.alloc_window_configuration(record_items(-1, TaggedValue::NIL, 4));
    let dead_ptr = rec_ptr(dead_wc);
    heap.collect_exact(std::iter::once(root));
    assert!(!heap.owns_non_cons_object(dead_ptr));
    heap.assert_object_arenas_coherent();
}
