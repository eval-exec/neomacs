use super::*;

fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    heap.extend_dump_span(4096, 16);
}

/// Drive one full concurrent cycle (copy of the bytecode_arena_tests
/// helper, local so this module stands alone).
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
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

/// A closure slot vector whose slot 0 is a fixnum IDENTITY and slot 1 is
/// `child` (an arbitrary value — a cons in the children-coverage tests),
/// padded to `n_slots` NILs. The slot `Vec` is the REAL `drop_in_place`
/// payload the page sweep must free.
fn lambda_slots(id: i64, child: TaggedValue, n_slots: usize) -> Vec<TaggedValue> {
    let mut v = vec![TaggedValue::NIL; n_slots.max(2)];
    v[0] = TaggedValue::fixnum(id);
    v[1] = child;
    v
}

fn lam_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}
fn lam_slot(v: TaggedValue, i: usize) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const LambdaObj) };
    obj.data.as_slice()[i]
}
fn mac_slot(v: TaggedValue, i: usize) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const MacroObj) };
    obj.data.as_slice()[i]
}

/// (a) PAGE-SPAN ORACLE EXACTNESS for the 128B stride: owned for a live
/// slot base ONLY — false for freed slots, interior/unaligned addresses,
/// and never-bumped slots. Cross-class registries never collide.
#[test]
fn lambda_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let keep = heap.alloc_lambda(lambda_slots(1, TaggedValue::NIL, 6));
    let dead = heap.alloc_lambda(lambda_slots(2, TaggedValue::NIL, 6));
    let keep2 = heap.alloc_lambda(lambda_slots(3, TaggedValue::NIL, 6));
    let f = heap.alloc_float(1.5);
    let m = heap.alloc_macro(lambda_slots(9, TaggedValue::NIL, 6));
    let dead_addr = lam_ptr(dead) as usize;

    // Page lambdas never touch the residual addr-set (TRAP A/B).
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.lambda_arena.owns(lam_ptr(dead)));

    heap.collect_exact([keep, keep2, f, m].into_iter());

    let b_addr = lam_ptr(keep) as usize;
    assert!(heap.lambda_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    // Freed slot answers NOT owned the instant its bit clears.
    assert!(!heap.lambda_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    // Interior (stride-misaligned) + arbitrary unaligned addresses.
    assert!(!heap.lambda_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.lambda_arena.owns((b_addr + 64) as *const u8));
    assert!(!heap.lambda_arena.owns((b_addr + 1) as *const u8));
    // Never-allocated slot beyond the bump cursor, inside the page.
    let page_base = ObjectPage::<LambdaObj>::page_base_for_ptr(b_addr as *const LambdaObj);
    let beyond_bump = page_base + 400 * <LambdaObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.lambda_arena.owns(beyond_bump as *const u8));
    // Cross-class registries: never merged, never colliding — including
    // the SIBLING 128B macro arena (same stride, distinct registry).
    let f_addr = f.as_float_ptr().unwrap() as usize;
    assert!(!heap.lambda_arena.owns(f_addr as *const u8));
    assert!(!heap.float_arena.owns(b_addr as *const u8));
    assert!(!heap.vector_arena.owns(b_addr as *const u8));
    assert!(!heap.macro_arena.owns(b_addr as *const u8));
    assert!(!heap.lambda_arena.owns(lam_ptr(m)));
    assert!(heap.macro_arena.owns(lam_ptr(m)));
    heap.assert_object_arenas_coherent();
}

/// (g) ownership-index-tracks-sweep: the sweep's alloc-bit clear IS the
/// ownership eviction; the residual addr-set stays empty throughout and
/// payloads stay intact.
#[test]
fn lambda_ownership_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let live = heap.alloc_lambda(lambda_slots(10, TaggedValue::NIL, 8));
    let dead = heap.alloc_lambda(lambda_slots(20, TaggedValue::NIL, 8));
    let live_ptr = lam_ptr(live);
    let dead_ptr = lam_ptr(dead);

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);

    heap.collect_exact(std::iter::once(live));

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(!heap.owns_non_cons_object(dead_ptr));
    assert!(heap.lambda_arena.owns(live_ptr));
    assert!(!heap.lambda_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert_eq!(lam_slot(live, 0).as_fixnum(), Some(10));
    heap.assert_object_arenas_coherent();
}

/// (b) Parity two-cycle properties: mark-born survives its birth cycle
/// unrooted then the next rooted; idle-born garbage reclaimed by the
/// first cycle after birth; mark-born garbage floats through its birth
/// cycle and is reclaimed by the next.
fn parity_two_cycle_lambda_survival_and_reclaim_body(verify: bool) {
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

    // Cycle 2: lambda born MID-MARK (allocate-black), NOT seeded.
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    let b = heap.alloc_lambda(lambda_slots(25, TaggedValue::NIL, 6));
    let b_ptr = lam_ptr(b);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "allocate-black lambda must survive the cycle it was born in",
    );
    heap.assert_object_arenas_coherent();

    // Cycle 3 (opposite parity): rooted — survives with payload intact.
    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_eq!(lam_slot(b, 0).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();

    // Reclaim: g1 idle-born, g2 mark-born.
    let g1 = heap.alloc_lambda(lambda_slots(-9, TaggedValue::NIL, 6));
    let g1_ptr = lam_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = heap.alloc_lambda(lambda_slots(-8, TaggedValue::NIL, 6));
    let g2_ptr = lam_ptr(g2);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(b);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage lambda must be reclaimed by the next cycle",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage lambda floats through its birth cycle",
    );
    heap.assert_object_arenas_coherent();

    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage lambda must be reclaimed by the SECOND cycle",
    );
    assert_eq!(lam_slot(b, 0).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_lambda_survival_and_reclaim() {
    parity_two_cycle_lambda_survival_and_reclaim_body(false);
}
#[test]
fn parity_two_cycle_lambda_survival_and_reclaim_verified() {
    parity_two_cycle_lambda_survival_and_reclaim_body(true);
}

/// (TRAP A) A lambda parked in `deferred` by the GC thread resolves at
/// the STW termination through `mark_value`'s OWNED veclike arm — routed
/// (since this commit) through the page-span oracle. A dropped route
/// reads as "mapped" and silently drops the mark (UAF). Slot children
/// (reachable only through the lambda) must be traced. Closures stay
/// DEFERRED for marking (`closure` drain bucket), unchanged by paging.
fn deferred_lambda_resolves_at_termination_body(verify: bool) {
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
    let mut lambdas = Vec::new();
    let mut children = Vec::new();
    for i in 0..300 {
        let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
        children.push(child);
        let b = heap.alloc_lambda(lambda_slots(i, child, 6));
        lambdas.push(b);
        list = heap.alloc_cons(b, list);
    }
    let garbage = heap.alloc_lambda(lambda_slots(-1, TaggedValue::NIL, 6));
    let garbage_ptr = lam_ptr(garbage);

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
        stats.last_termination_kinds.closure >= 300,
        "every rooted lambda must reach the termination via `deferred` \
         (got {}) — closures stay deferred in this commit",
        stats.last_termination_kinds.closure,
    );
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(list);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, b) in lambdas.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(lam_ptr(*b)),
            "deferred-then-resolved lambda {i} was swept while rooted",
        );
        assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(i as i64));
        assert_eq!(
            unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
            Some(10_000 + i as i64),
            "lambda {i}'s slot child was swept while live",
        );
    }
    assert!(
        !heap.owns_non_cons_object(garbage_ptr),
        "unrooted lambda must not be retained by the deferred machinery",
    );
    heap.assert_object_arenas_coherent();
}

#[test]
fn deferred_lambda_resolves_at_termination() {
    deferred_lambda_resolves_at_termination_body(false);
}
#[test]
fn deferred_lambda_resolves_at_termination_verified() {
    deferred_lambda_resolves_at_termination_body(true);
}

/// ALLOCATED-BIT-FIRST under adversarial staleness, payload-class form:
/// garbage scribbled into freed slots' object bytes (a junk kind would
/// Drop-dispatch garbage `Vec`/`OnceLock` pointers if trusted) is never
/// read by the sweep, verifiers, or teardown; reallocation
/// FULL-HEADER-WRITEs every stale byte away.
fn lambda_freed_slot_garbage_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
        heap.collect_exact(std::iter::empty());
    }

    let mut lambdas = Vec::new();
    for i in 0..100 {
        lambdas.push(heap.alloc_lambda(lambda_slots(i, TaggedValue::NIL, 6)));
    }
    let keep: Vec<TaggedValue> = lambdas.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut LambdaObj> = lambdas
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut LambdaObj)
        .collect();

    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }

    // ADVERSARY: scribble every freed slot's object bytes with 0xFF
    // (everything but the trailing link word at 120..128).
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<LambdaObj>()) };
    }
    heap.assert_object_arenas_coherent();

    // A full cycle re-sweeps: scribbled slots' bits are clear, so no
    // header is Drop-dispatched, size-read, or parity-read.
    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(lam_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
    }
    heap.assert_object_arenas_coherent();

    // Reallocate exactly the freed population: the FULL-HEADER WRITE must
    // rebuild every byte — a stale 0xFF kind/type would misroute the next
    // sweep's `drop_in_place` (type-confused Drop of garbage pointers).
    let mut reused = Vec::new();
    for i in 0..dead_ptrs.len() {
        reused.push(heap.alloc_lambda(lambda_slots(500 + i as i64, TaggedValue::NIL, 8)));
    }
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_veclike_ptr().unwrap() as *const LambdaObj;
        assert!(
            dead_addrs.contains(&(ptr as usize)),
            "reallocation must reuse the freed (scribbled) slots",
        );
        unsafe {
            assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
            assert_eq!((*ptr).header.type_tag, VecLikeType::Lambda);
            assert!(
                !(*ptr).header.gc.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.gc.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert_eq!(lam_slot(*r, 0).as_fixnum(), Some(500 + i as i64));
    }
    heap.assert_object_arenas_coherent();

    // Rebuilt headers + payloads survive a rooted cycle, and a final
    // unrooted cycle reclaims them cleanly (REAL Drop on rewritten — valid
    // — pointers, not the scribble).
    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(lam_ptr(*r)));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn lambda_freed_slot_garbage_never_read() {
    lambda_freed_slot_garbage_never_read_body(false);
}
#[test]
fn lambda_freed_slot_garbage_never_read_verified() {
    lambda_freed_slot_garbage_never_read_body(true);
}

/// Mid-sweep slot reuse within one cooperative sweep window (the class
/// free list hands freed slots to a mutator running BETWEEN slices) for
/// the payload class: no double-free, no premature free, `drop_in_place`
/// only on dead slots.
#[test]
fn lambda_reuse_within_one_cooperative_sweep_window() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Exactly three full pages of lambdas.
    let n = 3 * LAMBDA_PAGE_SLOTS;
    let mut lambdas = Vec::with_capacity(n);
    for i in 0..n {
        lambdas.push(heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 2)));
    }
    assert_eq!(heap.lambda_arena.pages.len(), 3);
    heap.assert_object_arenas_coherent();

    let keep: Vec<TaggedValue> = lambdas.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = lambdas
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| lam_ptr(*v) as usize)
        .collect();
    let page0_base = heap.lambda_arena.pages[0].base_addr();

    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    assert!(heap.sweep_in_progress());

    // Slice 1 (budget 1): sweeps lambda page 0 only.
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
    assert!(heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    // BETWEEN slices the mutator reallocates from the just-swept page.
    let mut reused = Vec::new();
    for i in 0..32 {
        reused.push(heap.alloc_lambda(lambda_slots(1_000 + i, TaggedValue::NIL, 2)));
    }
    for r in &reused {
        let ptr = r.as_veclike_ptr().unwrap() as *const LambdaObj;
        assert_eq!(
            ObjectPage::<LambdaObj>::page_base_for_ptr(ptr),
            page0_base,
            "mid-sweep reuse must come from the just-swept page",
        );
        assert!(dead_addrs.contains(&(ptr as usize)));
    }
    heap.assert_object_arenas_coherent();

    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    for (i, r) in reused.iter().enumerate() {
        assert!(heap.owns_non_cons_object(lam_ptr(*r)));
        assert_eq!(lam_slot(*r, 0).as_fixnum(), Some(1_000 + i as i64));
    }
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(lam_slot(*k, 0).as_fixnum(), Some(2 * i as i64));
    }
}

/// (c) VARIABLE-size live-bytes accounting on BOTH recompute sites: big
/// slot vectors counted for survivors (fixed struct + owned slot storage),
/// garbage not.
#[test]
fn lambda_sweep_live_bytes_track_variable_payload_sizes() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let l_big = heap.alloc_lambda(lambda_slots(7, TaggedValue::NIL, 2_000));
    let l_small = heap.alloc_lambda(lambda_slots(1, TaggedValue::NIL, 2));
    let _dead = heap.alloc_lambda(lambda_slots(0, TaggedValue::NIL, 4_000));
    let mut root = TaggedValue::fixnum(0);
    let mut cons_count = 0usize;
    for val in [l_big, l_small] {
        root = heap.alloc_cons(val, root);
        cons_count += 1;
    }

    let expected_objects: usize = [l_big, l_small]
        .iter()
        .map(|b| {
            TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
        })
        .sum::<usize>();
    let expected = expected_objects + cons_count * size_of::<ConsCell>();
    assert!(expected_objects > 2 * size_of::<LambdaObj>() + 2_000 * size_of::<TaggedValue>());

    // Eager (finalize_collection) recompute site.
    heap.collect_exact(std::iter::once(root));
    assert_eq!(
        heap.live_bytes(),
        expected,
        "eager sweep live_bytes != summed survivor bytes",
    );

    // Incremental (sweep slices -> finish_incremental_sweep) site.
    heap.begin_collection();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, crate::host::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(
        heap.live_bytes(),
        expected,
        "incremental sweep live_bytes != summed survivor bytes",
    );
}

/// (d) LOADUP-SHAPED tenure + retirement: a full page of rooted lambdas
/// retires at the one-time promotion (still owned — C1), a partial page
/// does not; the tenured population survives one cycle per parity with
/// payloads intact; post-retirement allocation never lands in the retired
/// page.
fn lambda_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut root = TaggedValue::fixnum(0);
    let mut lambdas = Vec::with_capacity(LAMBDA_PAGE_SLOTS + 2);
    for i in 0..(LAMBDA_PAGE_SLOTS + 2) {
        let b = heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 4));
        lambdas.push(b);
        root = heap.alloc_cons(b, root);
    }
    assert_eq!(heap.lambda_arena.pages.len(), 2);
    assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);

    for b in &lambdas {
        let ptr = b.as_veclike_ptr().unwrap();
        assert!(unsafe { (*ptr).gc.tenured }, "page lambda not tenured");
    }
    assert!(heap.lambda_arena.pages[0].retired, "full page must retire");
    assert!(!heap.lambda_arena.pages[1].retired, "partial page retired");
    assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);
    // C1: retired-page slots STAY owned.
    assert!(heap.owns_non_cons_object(lam_ptr(lambdas[0])));
    assert!(heap.lambda_arena.owns(lam_ptr(lambdas[0])));
    heap.assert_object_arenas_coherent();

    // Post-retirement allocation must never land in the retired page.
    let retired_base = heap.lambda_arena.pages[0].base_addr();
    let fresh = heap.alloc_lambda(lambda_slots(-5, TaggedValue::NIL, 2));
    assert_ne!(
        ObjectPage::<LambdaObj>::page_base_for_ptr(
            fresh.as_veclike_ptr().unwrap() as *const LambdaObj
        ),
        retired_base,
        "allocation reused a retired page",
    );

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, b) in lambdas.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(lam_ptr(*b)),
                "tenured page lambda #{i} lost on cycle {cycle}",
            );
            assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(i as i64));
        }
        assert_eq!(heap.lambda_arena.pages[0].allocated, LAMBDA_PAGE_SLOTS);
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn lambda_survivors_tenure_and_full_pages_retire() {
    lambda_survivors_tenure_and_full_pages_retire_body(false);
}
#[test]
fn lambda_survivors_tenure_and_full_pages_retire_verified() {
    lambda_survivors_tenure_and_full_pages_retire_body(true);
}

/// (d, mixed) Tenured + post-promotion YOUNG slots share a lambda page
/// across TWO alternating-parity cycles: tenured survive intact (a
/// parity-blind sweep would free them on the flipped cycle), young
/// garbage in the SAME page is reclaimed.
fn lambda_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut keep = Vec::new();
    let mut root = TaggedValue::fixnum(0);
    for i in 0..10 {
        let b = heap.alloc_lambda(lambda_slots(i as i64, TaggedValue::NIL, 4));
        if i % 2 == 0 {
            keep.push(b);
            root = heap.alloc_cons(b, root);
        }
    }
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(!heap.lambda_arena.pages[0].retired);

    for cycle in 0..2 {
        for i in 0..5 {
            let _ = heap.alloc_lambda(lambda_slots(-(i as i64), TaggedValue::NIL, 4));
        }
        heap.collect_exact(std::iter::once(root));
        for (i, b) in keep.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(lam_ptr(*b)),
                "tenured lambda #{i} freed on parity cycle {cycle}",
            );
            assert_eq!(lam_slot(*b, 0).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn lambda_mixed_page_tenured_survive_alternating_parities() {
    lambda_mixed_page_tenured_survive_alternating_parities_body(false);
}
#[test]
fn lambda_mixed_page_tenured_survive_alternating_parities_verified() {
    lambda_mixed_page_tenured_survive_alternating_parities_body(true);
}

/// (e) Teardown with payload-bearing lambdas: every lambda page is freed
/// exactly once at heap drop — retired pages included — with the per-slot
/// `drop_in_place` releasing the closure slot `Vec` (ASAN/MIRI catch a
/// leak/double-free; the counters prove page accounting either way). The
/// sweep-time `drop_in_place` path is exercised too (half die first).
fn lambda_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_LAMBDA_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        let mut root = TaggedValue::fixnum(0);
        for i in 0..1_500 {
            let b = heap.alloc_lambda(lambda_slots(i, TaggedValue::NIL, 32));
            if i % 2 == 0 {
                root = heap.alloc_cons(b, root);
            }
        }
        assert!(LIVE_LAMBDA_PAGES.load(Ordering::Relaxed) > before);

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
        LIVE_LAMBDA_PAGES.load(Ordering::Relaxed),
        before,
        "lambda pages leaked or double-freed at teardown",
    );
}

#[test]
fn lambda_payload_pages_freed_at_heap_drop() {
    lambda_payload_pages_freed_at_heap_drop_body(false);
}
#[test]
fn lambda_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
    lambda_payload_pages_freed_at_heap_drop_body(true);
}

/// Promotion-scan coverage: a page lambda tenured at promotion whose slot
/// holds a young CONS child (conses never tenure) and is never mutated —
/// the promotion-time page-tenured remembered-set scan must walk lambda
/// pages or the child is swept while its permanently-black owner points
/// at it.
fn tenured_page_lambda_keeps_young_cons_child_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
    let b = heap.alloc_lambda(lambda_slots(1, y, 4));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
            Some(999),
            "tenured page lambda's young cons child lost on cycle {cycle}",
        );
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_page_lambda_keeps_young_cons_child_alive() {
    tenured_page_lambda_keeps_young_cons_child_alive_body(false);
}
#[test]
fn tenured_page_lambda_keeps_young_cons_child_alive_verified() {
    tenured_page_lambda_keeps_young_cons_child_alive_body(true);
}

// ---- MACRO arena: an independent battery proving its own 128B arena ----

/// Macro page-span oracle exactness + ownership-tracks-sweep + payload
/// intact + no cross-class collision with the sibling lambda arena.
#[test]
fn macro_oracle_and_sweep_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let live = heap.alloc_macro(lambda_slots(10, TaggedValue::NIL, 8));
    let dead = heap.alloc_macro(lambda_slots(20, TaggedValue::NIL, 8));
    let sibling = heap.alloc_lambda(lambda_slots(30, TaggedValue::NIL, 8));
    let live_ptr = lam_ptr(live);
    let dead_ptr = lam_ptr(dead);

    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.macro_arena.owns(live_ptr));
    assert!(heap.macro_arena.owns(dead_ptr));
    assert!(!heap.macro_arena.owns(lam_ptr(sibling)));
    assert!(!heap.lambda_arena.owns(live_ptr));

    heap.collect_exact(std::iter::once(live));

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(!heap.owns_non_cons_object(dead_ptr));
    assert!(heap.macro_arena.owns(live_ptr));
    assert!(!heap.macro_arena.owns(dead_ptr));
    // Interior + unaligned answer NOT-owned.
    assert!(!heap.macro_arena.owns((live_ptr as usize + 8) as *const u8));
    assert!(!heap.macro_arena.owns((live_ptr as usize + 1) as *const u8));
    assert_eq!(mac_slot(live, 0).as_fixnum(), Some(10));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    heap.assert_object_arenas_coherent();
}

/// Macro loadup-shaped tenure + FULL-page retirement (C1) + teardown
/// counters with payload `drop_in_place`.
fn macro_tenure_retire_and_teardown_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_MACRO_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        arm_partition(&mut heap, verify);

        let mut root = TaggedValue::fixnum(0);
        let mut macros = Vec::with_capacity(LAMBDA_PAGE_SLOTS + 2);
        for i in 0..(LAMBDA_PAGE_SLOTS + 2) {
            let m = heap.alloc_macro(lambda_slots(i as i64, TaggedValue::NIL, 8));
            macros.push(m);
            root = heap.alloc_cons(m, root);
        }
        assert_eq!(heap.macro_arena.pages.len(), 2);
        assert!(LIVE_MACRO_PAGES.load(Ordering::Relaxed) > before);

        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        assert!(
            heap.macro_arena.pages[0].retired,
            "full macro page must retire"
        );
        assert!(!heap.macro_arena.pages[1].retired);
        // C1: retired-page macro slots stay owned across both parities.
        for cycle in 0..2 {
            heap.collect_exact(std::iter::once(root));
            for (i, m) in macros.iter().enumerate() {
                assert!(
                    heap.owns_non_cons_object(lam_ptr(*m)),
                    "tenured macro #{i} lost on cycle {cycle}",
                );
                assert_eq!(mac_slot(*m, 0).as_fixnum(), Some(i as i64));
            }
            heap.assert_object_arenas_coherent();
        }
        drop(heap);
    }
    assert_eq!(
        LIVE_MACRO_PAGES.load(Ordering::Relaxed),
        before,
        "macro pages leaked or double-freed at teardown",
    );
}

#[test]
fn macro_tenure_retire_and_teardown() {
    macro_tenure_retire_and_teardown_body(false);
}
#[test]
fn macro_tenure_retire_and_teardown_verified() {
    macro_tenure_retire_and_teardown_body(true);
}
