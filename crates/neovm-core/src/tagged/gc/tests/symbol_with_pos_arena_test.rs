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
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

/// A symbol-with-pos whose `pos` fixnum is the IDENTITY and `sym` is
/// `sym_val` (`T` in the basic tests, a young cons in the child tests).
fn swp(heap: &mut TaggedHeap, id: i64, sym_val: TaggedValue) -> TaggedValue {
    heap.alloc_symbol_with_pos(sym_val, TaggedValue::fixnum(id))
}
fn swp_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}
fn swp_pos(v: TaggedValue) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const SymbolWithPosObj) };
    obj.pos
}
fn swp_sym(v: TaggedValue) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const SymbolWithPosObj) };
    obj.sym
}

/// SymbolWithPos is POD (no Drop) — the class behaves like FloatObj, so
/// the generic sweep/teardown `drop_in_place` walk compiles out.
#[test]
fn symbol_with_pos_is_pod() {
    assert!(
        !std::mem::needs_drop::<SymbolWithPosObj>(),
        "SymbolWithPosObj must stay POD (no Drop) — if this fails a \
         Drop-worthy field was added and the sweep must drop_in_place it",
    );
}

/// (a) PAGE-SPAN ORACLE EXACTNESS for the 64B class + cross-class
/// no-collision (incl. the same-stride record/vector/string arenas).
#[test]
fn symbol_with_pos_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let keep = swp(&mut heap, 1, TaggedValue::T);
    let dead = swp(&mut heap, 2, TaggedValue::T);
    let keep2 = swp(&mut heap, 3, TaggedValue::T);
    let r = heap.alloc_record(vec![TaggedValue::fixnum(1); 4]);
    let dead_addr = swp_ptr(dead) as usize;

    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.symbol_with_pos_arena.owns(swp_ptr(dead)));

    heap.collect_exact([keep, keep2, r].into_iter());

    let b_addr = swp_ptr(keep) as usize;
    assert!(heap.symbol_with_pos_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    assert!(!heap.symbol_with_pos_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    assert!(!heap.symbol_with_pos_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.symbol_with_pos_arena.owns((b_addr + 32) as *const u8));
    assert!(!heap.symbol_with_pos_arena.owns((b_addr + 1) as *const u8));
    let page_base =
        ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(b_addr as *const SymbolWithPosObj);
    let beyond_bump = page_base + 900 * <SymbolWithPosObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.symbol_with_pos_arena.owns(beyond_bump as *const u8));
    // Same-stride sibling arenas (record/vector/string 64B) never collide.
    let r_addr = r.as_veclike_ptr().unwrap() as usize;
    assert!(!heap.symbol_with_pos_arena.owns(r_addr as *const u8));
    assert!(!heap.record_arena.owns(b_addr as *const u8));
    assert!(!heap.vector_arena.owns(b_addr as *const u8));
    assert!(!heap.string_arena.owns(b_addr as *const u8));
    heap.assert_object_arenas_coherent();
}

/// (g) ownership-index-tracks-sweep; addr-set empty; sym/pos intact.
#[test]
fn symbol_with_pos_ownership_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let live = swp(&mut heap, 10, TaggedValue::T);
    let dead = swp(&mut heap, 20, TaggedValue::T);
    let live_ptr = swp_ptr(live);
    let dead_ptr = swp_ptr(dead);

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);

    heap.collect_exact(std::iter::once(live));

    assert!(heap.symbol_with_pos_arena.owns(live_ptr));
    assert!(!heap.symbol_with_pos_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert_eq!(swp_pos(live).as_fixnum(), Some(10));
    assert_eq!(swp_sym(live).0, TaggedValue::T.0);
    heap.assert_object_arenas_coherent();
}

/// (b) Parity two-cycle survival/reclaim.
fn parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(verify: bool) {
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
    let b = swp(&mut heap, 25, TaggedValue::T);
    let b_ptr = swp_ptr(b);
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
        "allocate-black symbol-with-pos must survive its birth cycle",
    );

    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_eq!(swp_pos(b).as_fixnum(), Some(25));

    let g1 = swp(&mut heap, -9, TaggedValue::T);
    let g1_ptr = swp_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = swp(&mut heap, -8, TaggedValue::T);
    let g2_ptr = swp_ptr(g2);
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
    assert_eq!(swp_pos(b).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_symbol_with_pos_survival_and_reclaim() {
    parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(false);
}
#[test]
fn parity_two_cycle_symbol_with_pos_survival_and_reclaim_verified() {
    parity_two_cycle_symbol_with_pos_survival_and_reclaim_body(true);
}

/// (TRAP A) SymbolWithPos parked in `deferred` resolves at termination
/// through the page-oracle-routed veclike arm; its `sym` child (a young
/// cons reachable only through it) is traced. SymbolWithPos parks in the
/// `other` drain bucket (marking unchanged by paging).
fn deferred_symbol_with_pos_resolves_at_termination_body(verify: bool) {
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
    let mut swps = Vec::new();
    let mut children = Vec::new();
    for i in 0..300 {
        let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
        children.push(child);
        // `sym` = the young cons child (traced by collect_veclike_children).
        let b = heap.alloc_symbol_with_pos(child, TaggedValue::fixnum(i));
        swps.push(b);
        list = heap.alloc_cons(b, list);
    }
    let garbage = swp(&mut heap, -1, TaggedValue::T);
    let garbage_ptr = swp_ptr(garbage);

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
        stats.last_termination_kinds.other >= 300,
        "every rooted symbol-with-pos must reach the termination via \
         `deferred` (other bucket, got {})",
        stats.last_termination_kinds.other,
    );
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(list);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, b) in swps.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(swp_ptr(*b)),
            "deferred-then-resolved symbol-with-pos {i} was swept while rooted",
        );
        assert_eq!(swp_pos(*b).as_fixnum(), Some(i as i64));
        assert_eq!(
            unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
            Some(10_000 + i as i64),
            "symbol-with-pos {i}'s sym child was swept while live",
        );
    }
    assert!(!heap.owns_non_cons_object(garbage_ptr));
    heap.assert_object_arenas_coherent();
}

#[test]
fn deferred_symbol_with_pos_resolves_at_termination() {
    deferred_symbol_with_pos_resolves_at_termination_body(false);
}
#[test]
fn deferred_symbol_with_pos_resolves_at_termination_verified() {
    deferred_symbol_with_pos_resolves_at_termination_body(true);
}

/// ALLOCATED-BIT-FIRST under adversarial staleness. POD: no Drop to
/// type-confuse, but a stale header still misreads parity/tenured/size;
/// the full-header rewrite + allocated-bit-first keep the sweep exact.
fn symbol_with_pos_freed_slot_garbage_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
        heap.collect_exact(std::iter::empty());
    }

    let mut objs = Vec::new();
    for i in 0..100 {
        objs.push(swp(&mut heap, i, TaggedValue::T));
    }
    let keep: Vec<TaggedValue> = objs.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut SymbolWithPosObj> = objs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut SymbolWithPosObj)
        .collect();

    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<SymbolWithPosObj>()) };
    }
    heap.assert_object_arenas_coherent();

    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(swp_pos(*k).as_fixnum(), Some(2 * i as i64));
    }

    let mut reused = Vec::new();
    for i in 0..dead_ptrs.len() {
        reused.push(swp(&mut heap, 500 + i as i64, TaggedValue::T));
    }
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_veclike_ptr().unwrap() as *const SymbolWithPosObj;
        assert!(dead_addrs.contains(&(ptr as usize)));
        unsafe {
            assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
            assert_eq!((*ptr).header.type_tag, VecLikeType::SymbolWithPos);
            assert!(
                !(*ptr).header.gc.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.gc.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert_eq!(swp_pos(*r).as_fixnum(), Some(500 + i as i64));
    }
    heap.assert_object_arenas_coherent();

    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(swp_ptr(*r)));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn symbol_with_pos_freed_slot_garbage_never_read() {
    symbol_with_pos_freed_slot_garbage_never_read_body(false);
}
#[test]
fn symbol_with_pos_freed_slot_garbage_never_read_verified() {
    symbol_with_pos_freed_slot_garbage_never_read_body(true);
}

/// Mid-sweep cooperative-window slot reuse.
#[test]
fn symbol_with_pos_reuse_within_one_cooperative_sweep_window() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let n = 3 * SYMBOL_WITH_POS_PAGE_SLOTS;
    let mut objs = Vec::with_capacity(n);
    for i in 0..n {
        objs.push(swp(&mut heap, i as i64, TaggedValue::T));
    }
    assert_eq!(heap.symbol_with_pos_arena.pages.len(), 3);

    let keep: Vec<TaggedValue> = objs.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = objs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| swp_ptr(*v) as usize)
        .collect();
    let page0_base = heap.symbol_with_pos_arena.pages[0].base_addr();

    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");

    let mut reused = Vec::new();
    for i in 0..32 {
        reused.push(swp(&mut heap, 1_000 + i, TaggedValue::T));
    }
    for r in &reused {
        let ptr = r.as_veclike_ptr().unwrap() as *const SymbolWithPosObj;
        assert_eq!(
            ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(ptr),
            page0_base
        );
        assert!(dead_addrs.contains(&(ptr as usize)));
    }
    heap.assert_object_arenas_coherent();

    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    for (i, r) in reused.iter().enumerate() {
        assert!(heap.owns_non_cons_object(swp_ptr(*r)));
        assert_eq!(swp_pos(*r).as_fixnum(), Some(1_000 + i as i64));
    }
}

/// (c) FIXED-size live-bytes on BOTH recompute sites (SymbolWithPos has no
/// variable payload, so survivors count exactly size_of::<SymbolWithPosObj>()
/// each — but the accounting must still be exact on both paths).
#[test]
fn symbol_with_pos_sweep_live_bytes_fixed_size_both_sites() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let a = swp(&mut heap, 1, TaggedValue::T);
    let b = swp(&mut heap, 2, TaggedValue::T);
    let _dead = swp(&mut heap, 3, TaggedValue::T);
    let mut root = TaggedValue::fixnum(0);
    let mut cons_count = 0usize;
    for val in [a, b] {
        root = heap.alloc_cons(val, root);
        cons_count += 1;
    }
    let expected = 2 * size_of::<SymbolWithPosObj>() + cons_count * size_of::<ConsCell>();

    heap.collect_exact(std::iter::once(root));
    assert_eq!(heap.live_bytes(), expected, "eager site");

    heap.begin_collection();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(heap.live_bytes(), expected, "incremental site");
}

/// (d) LOADUP-SHAPED tenure + FULL-page retirement (C1).
fn symbol_with_pos_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut root = TaggedValue::fixnum(0);
    let mut objs = Vec::with_capacity(SYMBOL_WITH_POS_PAGE_SLOTS + 2);
    for i in 0..(SYMBOL_WITH_POS_PAGE_SLOTS + 2) {
        let b = swp(&mut heap, i as i64, TaggedValue::T);
        objs.push(b);
        root = heap.alloc_cons(b, root);
    }
    assert_eq!(heap.symbol_with_pos_arena.pages.len(), 2);
    assert_eq!(
        heap.symbol_with_pos_arena.pages[0].allocated,
        SYMBOL_WITH_POS_PAGE_SLOTS
    );

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    for b in &objs {
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });
    }
    assert!(
        heap.symbol_with_pos_arena.pages[0].retired,
        "full page must retire"
    );
    assert!(!heap.symbol_with_pos_arena.pages[1].retired);
    assert!(heap.owns_non_cons_object(swp_ptr(objs[0])));
    heap.assert_object_arenas_coherent();

    let retired_base = heap.symbol_with_pos_arena.pages[0].base_addr();
    let fresh = swp(&mut heap, -5, TaggedValue::T);
    assert_ne!(
        ObjectPage::<SymbolWithPosObj>::page_base_for_ptr(
            fresh.as_veclike_ptr().unwrap() as *const SymbolWithPosObj
        ),
        retired_base,
    );

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, b) in objs.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(swp_ptr(*b)),
                "tenured page symbol-with-pos #{i} lost on cycle {cycle}",
            );
            assert_eq!(swp_pos(*b).as_fixnum(), Some(i as i64));
        }
        assert_eq!(
            heap.symbol_with_pos_arena.pages[0].allocated,
            SYMBOL_WITH_POS_PAGE_SLOTS
        );
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn symbol_with_pos_survivors_tenure_and_full_pages_retire() {
    symbol_with_pos_survivors_tenure_and_full_pages_retire_body(false);
}
#[test]
fn symbol_with_pos_survivors_tenure_and_full_pages_retire_verified() {
    symbol_with_pos_survivors_tenure_and_full_pages_retire_body(true);
}

/// (d, mixed) Tenured + young slots share a page across parities.
fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut keep = Vec::new();
    let mut root = TaggedValue::fixnum(0);
    for i in 0..10 {
        let b = swp(&mut heap, i as i64, TaggedValue::T);
        if i % 2 == 0 {
            keep.push(b);
            root = heap.alloc_cons(b, root);
        }
    }
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(!heap.symbol_with_pos_arena.pages[0].retired);

    for cycle in 0..2 {
        for i in 0..5 {
            let _ = swp(&mut heap, -(i as i64), TaggedValue::T);
        }
        heap.collect_exact(std::iter::once(root));
        for (i, b) in keep.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(swp_ptr(*b)),
                "tenured symbol-with-pos #{i} freed on parity cycle {cycle}",
            );
            assert_eq!(swp_pos(*b).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities() {
    symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(false);
}
#[test]
fn symbol_with_pos_mixed_page_tenured_survive_alternating_parities_verified() {
    symbol_with_pos_mixed_page_tenured_survive_alternating_parities_body(true);
}

/// (e) Teardown page counters (POD — the drop_in_place walk compiles out,
/// but every page — retired included — is still dealloc'd exactly once).
fn symbol_with_pos_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        let mut root = TaggedValue::fixnum(0);
        for i in 0..3_000 {
            let b = swp(&mut heap, i, TaggedValue::T);
            if i % 2 == 0 {
                root = heap.alloc_cons(b, root);
            }
        }
        assert!(LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed) > before);

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
        LIVE_SYMBOL_WITH_POS_PAGES.load(Ordering::Relaxed),
        before,
        "symbol-with-pos pages leaked or double-freed at teardown",
    );
}

#[test]
fn symbol_with_pos_pages_freed_at_heap_drop() {
    symbol_with_pos_pages_freed_at_heap_drop_body(false);
}
#[test]
fn symbol_with_pos_pages_freed_at_heap_drop_mid_concurrent_mark() {
    symbol_with_pos_pages_freed_at_heap_drop_body(true);
}

/// Promotion-scan coverage: a tenured page symbol-with-pos whose `sym`
/// holds a young cons keeps it alive across both parities (the promotion
/// page walk covers the symbol-with-pos arena; `sym` is a traced child).
fn tenured_page_symbol_with_pos_keeps_young_child_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
    let b = heap.alloc_symbol_with_pos(y, TaggedValue::fixnum(1));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
            Some(999),
            "tenured page symbol-with-pos's young sym child lost on cycle {cycle}",
        );
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_page_symbol_with_pos_keeps_young_child_alive() {
    tenured_page_symbol_with_pos_keeps_young_child_alive_body(false);
}
#[test]
fn tenured_page_symbol_with_pos_keeps_young_child_alive_verified() {
    tenured_page_symbol_with_pos_keeps_young_child_alive_body(true);
}
