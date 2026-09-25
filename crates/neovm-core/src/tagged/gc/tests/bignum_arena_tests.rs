//! BIGNUM ARENA (lever P0.11): bignums live in 64-byte `ObjectArena` slots,
//! as GNU's `make_bignum_bits` takes a vector-block slot. These scenarios
//! mirror the record/marker arena suites; the class-specific points are the
//! payload (the slot's `Integer` owns its limb `Vec`, dropped in place by the
//! sweep) and the registry claim (no `non_cons_object_addrs` insert, no
//! intrusive-list node per bignum).

use super::*;
use malachite::base::num::arithmetic::traits::Pow;

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
    finish_concurrent_cycle(heap, roots);
}

fn finish_concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
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

/// `2^200 + id` (negated for a negative `id`): four heap limbs, so every
/// test bignum owns a limb `Vec` the sweep must free.
fn big_integer(id: i64) -> Integer {
    let magnitude = Integer::from(2).pow(200) + Integer::from(id.unsigned_abs());
    if id < 0 { -magnitude } else { magnitude }
}

/// The allocator under test: the production entry point.
fn alloc_big(heap: &mut TaggedHeap, value: Integer) -> TaggedValue {
    heap.alloc_bignum(value)
}

fn big(heap: &mut TaggedHeap, id: i64) -> TaggedValue {
    alloc_big(heap, big_integer(id))
}

fn big_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}

fn big_value(v: TaggedValue) -> &'static Integer {
    v.as_bignum().expect("a bignum value")
}

fn assert_big(v: TaggedValue, id: i64) {
    assert_eq!(*big_value(v), big_integer(id), "bignum {id} lost its value");
}

fn bignum_layout(heap: &TaggedHeap) -> ArenaLayoutStats {
    heap.layout_stats()
        .arenas
        .into_iter()
        .find(|arena| arena.class == "bignum")
        .expect("layout stats report the bignum class")
}

/// The slot fit (56 bytes of object, the free link in 56..64) and the
/// payload: unlike markers or symbols-with-pos, a bignum is NOT POD — its
/// `Integer` owns the limb vector the sweep's `drop_in_place` frees.
#[test]
fn bignum_obj_fits_64b_slot_and_owns_payload() {
    assert!(
        size_of::<BignumObj>() <= 56,
        "BignumObj outgrew its 64B slot"
    );
    assert_eq!(<BignumObj as PagedObject>::SLOT_BYTES, 64);
    assert_eq!(BIGNUM_PAGE_SLOTS, 1024);
    assert!(
        std::mem::needs_drop::<BignumObj>(),
        "BignumObj owns its limb Vec: the sweep must drop_in_place it",
    );
}

/// PAGE-SPAN ORACLE EXACTNESS: a live slot is owned; a freed slot, an
/// interior offset, a slot past the bump and a boxed object are not, and
/// the same-stride record arena never collides.
#[test]
fn bignum_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let keep = big(&mut heap, 1);
    let dead = big(&mut heap, 2);
    let keep2 = big(&mut heap, 3);
    let dead_addr = big_ptr(dead) as usize;
    assert!(heap.bignum_arena.owns(big_ptr(dead)));
    let record = heap.alloc_record(vec![TaggedValue::fixnum(1); 4]);
    let boxed = heap.alloc_timer(7);
    heap.collect_exact([keep, keep2, record, boxed].into_iter());

    let b_addr = big_ptr(keep) as usize;
    assert!(heap.bignum_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    assert!(!heap.bignum_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    assert!(!heap.bignum_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.bignum_arena.owns((b_addr + 1) as *const u8));
    let page_base = ObjectPage::<BignumObj>::page_base_for_ptr(b_addr as *const BignumObj);
    let beyond_bump = page_base + (BIGNUM_PAGE_SLOTS - 12) * <BignumObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.bignum_arena.owns(beyond_bump as *const u8));
    // Same-stride sibling arena and a boxed veclike: never answered by the
    // bignum registry, and the bignum is never answered by theirs.
    let r_addr = record.as_veclike_ptr().unwrap() as usize;
    assert!(!heap.bignum_arena.owns(r_addr as *const u8));
    assert!(!heap.record_arena.owns(b_addr as *const u8));
    let t_addr = boxed.as_veclike_ptr().unwrap() as usize;
    assert!(!heap.bignum_arena.owns(t_addr as *const u8));
    assert!(heap.owns_veclike_object(t_addr as *const u8));
    assert_big(keep, 1);
    assert_big(keep2, 3);
    heap.assert_object_arenas_coherent();
}

/// THE CLAIM OF THIS LEVER: allocating bignums touches neither the
/// residual `Box` registry (`non_cons_object_addrs`, an FxHashSet insert
/// per object before) nor the intrusive young list (a node per object, a
/// list walk and a hash remove per object at sweep).
#[test]
fn bignum_alloc_leaves_registry_untouched() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let addrs_before = heap.non_cons_object_addrs.len();
    let list_before = heap.all_objects;
    let mut keep = Vec::new();
    for i in 0..10_000 {
        let v = big(&mut heap, i);
        if i % 1000 == 0 {
            keep.push(v);
        }
    }
    assert_eq!(heap.non_cons_object_addrs.len(), addrs_before);
    assert_eq!(
        heap.all_objects, list_before,
        "no bignum joined all_objects"
    );
    assert_eq!(heap.bignum_arena.pages.len(), 10);
    heap.collect_exact(keep.iter().copied());
    for (n, v) in keep.iter().enumerate() {
        assert_big(*v, n as i64 * 1000);
    }
    assert_eq!(heap.non_cons_object_addrs.len(), addrs_before);
    heap.assert_object_arenas_coherent();
}

/// Parity two-cycle survival/reclaim under the concurrent collector: an
/// allocate-black bignum survives its birth cycle, idle-born garbage dies
/// in the next cycle, and mark-born garbage floats one cycle, then dies.
fn parity_two_cycle_bignum_survival_and_reclaim_body(verify: bool) {
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
    let b = big(&mut heap, 25);
    let b_ptr = big_ptr(b);
    finish_concurrent_cycle(&mut heap, &[spine]);
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "allocate-black bignum must survive its birth cycle",
    );
    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_big(b, 25);

    let g1 = big(&mut heap, 91);
    let g1_ptr = big_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = big(&mut heap, 92);
    let g2_ptr = big_ptr(g2);
    finish_concurrent_cycle(&mut heap, &[spine, b]);
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
    assert_big(b, 25);
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_bignum_survival_and_reclaim() {
    parity_two_cycle_bignum_survival_and_reclaim_body(false);
}
#[test]
fn parity_two_cycle_bignum_survival_and_reclaim_verified() {
    parity_two_cycle_bignum_survival_and_reclaim_body(true);
}

/// Bignums reachable only through a page VECTOR (whose backing the GC
/// thread scans concurrently) are deferred to the termination — the GC
/// thread does not claim bignum pages (design §3.6) — and resolved there
/// by `mark_value` through `owns_veclike_object`'s bignum arm.
fn deferred_bignum_resolves_at_termination_body(verify: bool) {
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

    let bigs: Vec<TaggedValue> = (0..300).map(|i| big(&mut heap, i)).collect();
    let holder = heap.alloc_vector(bigs.clone());
    let garbage = big(&mut heap, -1);
    let garbage_ptr = big_ptr(garbage);

    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(holder);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.other >= 300,
        "every rooted bignum must reach the termination via `deferred` \
         (other bucket, got {})",
        stats.last_termination_kinds.other,
    );
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(holder);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, b) in bigs.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(big_ptr(*b)),
            "deferred-then-resolved bignum {i} was swept while rooted",
        );
        assert_big(*b, i as i64);
    }
    assert!(!heap.owns_non_cons_object(garbage_ptr));
    heap.assert_object_arenas_coherent();
}

#[test]
fn deferred_bignum_resolves_at_termination() {
    deferred_bignum_resolves_at_termination_body(false);
}
#[test]
fn deferred_bignum_resolves_at_termination_verified() {
    deferred_bignum_resolves_at_termination_body(true);
}

/// SATB: a bignum whose only home is a vector slot overwritten MID-MARK
/// survives that cycle through the deletion barrier's pre-image, and the
/// next cycle (no home left) reclaims it and frees its limbs.
#[test]
fn satb_overwritten_bignum_survives_cycle_then_reclaims() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let b = big(&mut heap, 4242);
    let b_ptr = big_ptr(b);
    let v = heap.alloc_vector(vec![b]);
    // `v` at the far end of a long list, so the GC thread is unlikely to
    // have traced it before the overwrite (either order must be safe).
    let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;
    heap.collect_exact(std::iter::once(root));
    assert!(heap.should_run_concurrent());

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    // MID-CYCLE: drop the only reference through the production barrier.
    assert!(crate::tagged::mutate::set_vector_slot(
        v,
        0,
        TaggedValue::fixnum(0)
    ));
    finish_concurrent_cycle(&mut heap, &[root]);
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "the snapshot-reachable bignum must survive the cycle that overwrote it",
    );
    assert_big(b, 4242);

    run_concurrent_cycle(&mut heap, &[root]);
    assert!(
        !heap.owns_non_cons_object(b_ptr),
        "with no home left the next cycle must reclaim it",
    );
    heap.assert_object_arenas_coherent();
}

/// ALLOCATED-BIT-FIRST under adversarial staleness: freed slots are
/// scribbled; nothing may read them, and reuse fully rewrites the header.
fn bignum_freed_slot_garbage_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
        heap.collect_exact(std::iter::empty());
    }

    let bigs: Vec<TaggedValue> = (0..100).map(|i| big(&mut heap, i)).collect();
    let keep: Vec<TaggedValue> = bigs.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut BignumObj> = bigs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut BignumObj)
        .collect();

    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }
    // The object bytes only: the free-list link word (56..64) stays intact.
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<BignumObj>()) };
    }
    heap.assert_object_arenas_coherent();

    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert_big(*k, 2 * i as i64);
    }

    let reused: Vec<TaggedValue> = (0..dead_ptrs.len())
        .map(|i| big(&mut heap, 500 + i as i64))
        .collect();
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_veclike_ptr().unwrap() as *const BignumObj;
        assert!(dead_addrs.contains(&(ptr as usize)));
        unsafe {
            assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
            assert_eq!((*ptr).header.type_tag, VecLikeType::Bignum);
            assert!(
                !(*ptr).header.gc.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.gc.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert_big(*r, 500 + i as i64);
    }
    heap.assert_object_arenas_coherent();

    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(big_ptr(*r)));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn bignum_freed_slot_garbage_never_read() {
    bignum_freed_slot_garbage_never_read_body(false);
}
#[test]
fn bignum_freed_slot_garbage_never_read_verified() {
    bignum_freed_slot_garbage_never_read_body(true);
}

/// Mid-sweep cooperative-window slot reuse: slots freed by the first slice
/// are handed out before the sweep finishes, and the later slices keep them
/// (born-at-parity ⇒ read as marked).
#[test]
fn bignum_reuse_within_one_cooperative_sweep_window() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let n = 3 * BIGNUM_PAGE_SLOTS;
    let bigs: Vec<TaggedValue> = (0..n).map(|i| big(&mut heap, i as i64)).collect();
    assert_eq!(heap.bignum_arena.pages.len(), 3);

    let keep: Vec<TaggedValue> = bigs.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = bigs
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| big_ptr(*v) as usize)
        .collect();
    let page0_base = heap.bignum_arena.pages[0].base_addr();

    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");

    let reused: Vec<TaggedValue> = (0..32).map(|i| big(&mut heap, 1_000 + i)).collect();
    for r in &reused {
        let ptr = r.as_veclike_ptr().unwrap() as *const BignumObj;
        assert_eq!(ObjectPage::<BignumObj>::page_base_for_ptr(ptr), page0_base);
        assert!(dead_addrs.contains(&(ptr as usize)));
    }
    heap.assert_object_arenas_coherent();

    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    for (i, r) in reused.iter().enumerate() {
        assert!(heap.owns_non_cons_object(big_ptr(*r)));
        assert_big(*r, 1_000 + i as i64);
    }
    for (i, k) in keep.iter().enumerate() {
        assert_big(*k, 2 * i as i64);
    }
}

/// LOADUP-SHAPED tenure + FULL-page retirement (pdump-restored bignums are
/// loadup survivors): tenured values stay exact across both parities, and
/// a retired page is never allocated into again.
fn bignum_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut root = TaggedValue::fixnum(0);
    let mut bigs = Vec::with_capacity(BIGNUM_PAGE_SLOTS + 2);
    for i in 0..(BIGNUM_PAGE_SLOTS + 2) {
        let b = big(&mut heap, i as i64);
        bigs.push(b);
        root = heap.alloc_cons(b, root);
    }
    assert_eq!(heap.bignum_arena.pages.len(), 2);
    assert_eq!(heap.bignum_arena.pages[0].allocated, BIGNUM_PAGE_SLOTS);

    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    for b in &bigs {
        assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });
    }
    assert!(heap.bignum_arena.pages[0].retired, "full page must retire");
    assert!(!heap.bignum_arena.pages[1].retired);
    assert!(heap.owns_non_cons_object(big_ptr(bigs[0])));
    heap.assert_object_arenas_coherent();

    let retired_base = heap.bignum_arena.pages[0].base_addr();
    let fresh = big(&mut heap, -5);
    assert_ne!(
        ObjectPage::<BignumObj>::page_base_for_ptr(
            fresh.as_veclike_ptr().unwrap() as *const BignumObj
        ),
        retired_base,
    );

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, b) in bigs.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(big_ptr(*b)),
                "tenured page bignum #{i} lost on cycle {cycle}",
            );
            assert_big(*b, i as i64);
        }
        assert_eq!(heap.bignum_arena.pages[0].allocated, BIGNUM_PAGE_SLOTS);
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn bignum_survivors_tenure_and_full_pages_retire() {
    bignum_survivors_tenure_and_full_pages_retire_body(false);
}
#[test]
fn bignum_survivors_tenure_and_full_pages_retire_verified() {
    bignum_survivors_tenure_and_full_pages_retire_body(true);
}

/// Teardown owns the payloads: every bignum page goes back at heap drop
/// (also mid-concurrent-mark), and the limb bytes the layout stats report
/// fall to zero once the owning slots are reclaimed.
fn bignum_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_BIGNUM_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);

        let mut garbage = Vec::new();
        for i in 0..3_000 {
            garbage.push(big(&mut heap, i));
        }
        assert!(LIVE_BIGNUM_PAGES.load(Ordering::Relaxed) > before);
        let stats = bignum_layout(&heap);
        assert_eq!(stats.allocated_slots, 3_000);
        assert_eq!(
            stats.payload_logical_bytes,
            3_000 * 4 * size_of::<u64>(),
            "each 2^200 + i owns four limbs",
        );
        assert_eq!(stats.owned_payloads, 3_000);
        heap.collect_exact(std::iter::empty());
        let stats = bignum_layout(&heap);
        assert_eq!(stats.allocated_slots, 0);
        assert_eq!(
            stats.payload_logical_bytes, 0,
            "reclaimed limbs still counted"
        );

        heap.extend_dump_span(4096, 16);
        let mut root = TaggedValue::fixnum(0);
        for i in 0..3_000 {
            let b = big(&mut heap, i);
            if i % 2 == 0 {
                root = heap.alloc_cons(b, root);
            }
        }
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
        LIVE_BIGNUM_PAGES.load(Ordering::Relaxed),
        before,
        "bignum pages leaked or double-freed at teardown",
    );
}

#[test]
fn bignum_payload_pages_freed_at_heap_drop() {
    bignum_payload_pages_freed_at_heap_drop_body(false);
}
#[test]
fn bignum_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
    bignum_payload_pages_freed_at_heap_drop_body(true);
}

/// A dies, B takes A's slot: every value reader sees exactly B — printing,
/// `eql`, `equal` and `sxhash-equal` read through `as_bignum` and must not
/// see A's limbs (a stale limb pointer would be a use-after-free).
#[test]
fn bignum_value_exact_after_slot_reuse() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let a_value = Integer::from(3).pow(300);
    let b_value = -(Integer::from(7).pow(150));
    let a = alloc_big(&mut heap, a_value.clone());
    let a_ptr = big_ptr(a);
    let keep = big(&mut heap, 9);
    heap.collect_exact(std::iter::once(keep));
    assert!(!heap.owns_non_cons_object(a_ptr));

    let b = alloc_big(&mut heap, b_value.clone());
    assert_eq!(big_ptr(b), a_ptr, "B must take A's freed slot");
    assert_eq!(*big_value(b), b_value);
    assert_eq!(big_value(b).to_string(), b_value.to_string());

    let b_twin = alloc_big(&mut heap, b_value.clone());
    let a_twin = alloc_big(&mut heap, a_value);
    assert!(crate::emacs_core::value::eql_value(&b, &b_twin));
    assert!(crate::emacs_core::value::equal_value(&b, &b_twin, 0));
    assert!(!crate::emacs_core::value::eql_value(&b, &a_twin));
    let sxhash = |v: TaggedValue| {
        crate::emacs_core::hashtab::builtin_sxhash_equal(vec![v])
            .expect("sxhash-equal of a bignum")
            .as_fixnum()
    };
    assert_eq!(sxhash(b), sxhash(b_twin));
    assert_big(keep, 9);
    heap.assert_object_arenas_coherent();
}
