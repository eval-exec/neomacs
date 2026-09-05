use super::*;

fn arm_verify(heap: &mut TaggedHeap) {
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    // Fake dump span: activates the dump partition so the first full
    // cycle promotes + blackens and later terminations run the verifiers.
    heap.extend_dump_span(4096, 16);
}

/// Drive one full concurrent cycle (start handshake → GC-thread drain →
/// termination → deferred sweep drained). Copy of the ownership_tests
/// helper, local so this module stands alone.
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

/// (a) Slot reuse WITHIN one cooperative sweep window: a page is swept in
/// an early slice, the mutator reallocates its freed slots between
/// slices (class free-list pop), and the rest of the sweep must neither
/// double-free nor prematurely free the reused slots. The arena stays
/// bitmap-coherent at every step.
fn reuse_within_one_cooperative_sweep_window_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_verify(&mut heap);
        // Bootstrap cycle: blackens the (fake) dump so later
        // terminations run the armed verifiers.
        heap.collect_exact(std::iter::empty());
    }

    // Exactly three full pages of floats.
    let n = 3 * FLOAT_PAGE_SLOTS;
    let mut floats = Vec::with_capacity(n);
    for i in 0..n {
        floats.push(heap.alloc_float(i as f64));
    }
    assert_eq!(
        heap.float_arena.pages.len(),
        3,
        "3 * PAGE_SLOTS floats = 3 pages"
    );
    heap.assert_object_arenas_coherent();

    // Keep the even-indexed half; the odd half is garbage.
    let keep: Vec<TaggedValue> = floats.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = floats
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_float_ptr().unwrap() as usize)
        .collect();
    let page0_base = heap.float_arena.pages[0].base_addr();

    // Mark to a fixpoint and ARM the deferred sweep (the incremental
    // termination path), then drain it slice by slice.
    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());

    // Slice 1 (budget 1): sweeps float page 0 only — the window is open.
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
    assert!(heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    // BETWEEN cooperative slices the mutator reallocates: the class free
    // list must hand back the slots the slice just freed, in page 0.
    let mut reused = Vec::new();
    for i in 0..64 {
        reused.push(heap.alloc_float(1_000.0 + i as f64));
    }
    for r in &reused {
        let ptr = r.as_float_ptr().unwrap();
        assert_eq!(
            ObjectPage::<FloatObj>::page_base_for_ptr(ptr),
            page0_base,
            "mid-sweep reuse must come from the just-swept page",
        );
        assert!(
            dead_addrs.contains(&(ptr as usize)),
            "reused slot must be one the sweep just freed",
        );
    }
    heap.assert_object_arenas_coherent();

    // Drain the rest. The reallocated slots were re-read from the LIVE
    // bitmap and born at the cycle parity, so the remaining slices must
    // not free them (no premature free) nor re-free their slots.
    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    for (i, r) in reused.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(r.as_float_ptr().unwrap() as *const u8),
            "mid-sweep reallocation was prematurely freed",
        );
        assert!((r.xfloat() - (1_000.0 + i as f64)).abs() < f64::EPSILON);
    }
    for (i, k) in keep.iter().enumerate() {
        assert!((k.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
    }
    // Every dead slot is now either evicted (freed) or reallocated —
    // owned iff reused. A violation in either direction is the
    // double-free / premature-free the window test exists to catch.
    let reused_addrs: std::collections::HashSet<usize> = reused
        .iter()
        .map(|r| r.as_float_ptr().unwrap() as usize)
        .collect();
    for &addr in &dead_addrs {
        assert_eq!(
            heap.owns_non_cons_object(addr as *const u8),
            reused_addrs.contains(&addr),
            "freed slot must be owned iff reallocated",
        );
    }
}

#[test]
fn reuse_within_one_cooperative_sweep_window() {
    reuse_within_one_cooperative_sweep_window_body(false);
}

#[test]
fn reuse_within_one_cooperative_sweep_window_verified() {
    reuse_within_one_cooperative_sweep_window_body(true);
}

/// (b) The parity two-cycle properties hold for page floats: an
/// allocate-black float survives the cycle it was born in (unrooted) and
/// the next one (rooted); idle-born garbage is reclaimed by the first
/// cycle after its birth; mark-born garbage floats through its birth
/// cycle and is reclaimed by the next.
fn parity_two_cycle_float_survival_and_reclaim_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_verify(&mut heap);
    }

    // STW bootstrap (flip #1) enables the concurrent collector (and
    // blackens the fake dump under verify).
    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());

    // Cycle 2: float born MID-MARK (allocate-black at this cycle's
    // parity), deliberately NOT seeded at the termination.
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    let f = heap.alloc_float(2.5);
    let f_ptr = f.as_float_ptr().unwrap() as *const u8;
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine); // f deliberately NOT seeded
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(f_ptr),
        "allocate-black float must survive the cycle it was born in",
    );
    heap.assert_object_arenas_coherent();

    // Cycle 3 (opposite parity): rooted now — must be traced as unmarked
    // via the seed and survive with its payload intact.
    run_concurrent_cycle(&mut heap, &[spine, f]);
    assert!(heap.owns_non_cons_object(f_ptr));
    assert!((f.xfloat() - 2.5).abs() < f64::EPSILON);
    heap.assert_object_arenas_coherent();

    // Reclaim: g1 idle-born (no allocate-black), g2 mark-born.
    let g1 = heap.alloc_float(9.0);
    let g1_ptr = g1.as_float_ptr().unwrap() as *const u8;
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(f);
    heap.launch_concurrent_mark();
    let g2 = heap.alloc_float(8.0);
    let g2_ptr = g2.as_float_ptr().unwrap() as *const u8;
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(f);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    // No allocations since the sweep: the ownership probes below cannot
    // be confused by slot reuse.
    assert!(
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage float must be reclaimed by the next cycle",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage float floats through its birth cycle",
    );
    heap.assert_object_arenas_coherent();

    run_concurrent_cycle(&mut heap, &[spine, f]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage float must be reclaimed by the SECOND cycle",
    );
    assert!((f.xfloat() - 2.5).abs() < f64::EPSILON);
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_float_survival_and_reclaim() {
    parity_two_cycle_float_survival_and_reclaim_body(false);
}

#[test]
fn parity_two_cycle_float_survival_and_reclaim_verified() {
    parity_two_cycle_float_survival_and_reclaim_body(true);
}

/// (c) Task 01 CONCURRENT FLOAT CLAIMS: every rooted young page float
/// discovered during a concurrent mark is CLAIMED on the GC thread
/// (page-snapshot hit + `mark_claim_at`; zero children so the claim is
/// the whole trace), never parked — the float bucket collapses to zero
/// and the claim counter carries the count. Claimed floats survive the
/// sweep with their payloads intact; a garbage float is still collected
/// (claims only mark what the marker discovers — the garbage float has
/// no inbound edge, stays white, and the deferred sweep frees it within
/// this same cycle).
fn deferred_floats_resolve_at_termination_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_verify(&mut heap);
    }

    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());

    // A rooted cons list carrying float cars: the GC thread marks the
    // conses concurrently but parks every float in `deferred`.
    let mut list = TaggedValue::fixnum(0);
    let mut float_vals = Vec::new();
    for i in 0..500 {
        let f = heap.alloc_float(i as f64);
        float_vals.push(f);
        list = heap.alloc_cons(f, list);
    }
    let garbage = heap.alloc_float(-1.0);
    let garbage_ptr = garbage.as_float_ptr().unwrap() as *const u8;

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
        stats.last_concurrent_float_claimed >= 500,
        "every rooted page float must be claimed on the GC thread \
         (claimed={})",
        stats.last_concurrent_float_claimed,
    );
    assert_eq!(
        stats.last_termination_kinds.float, 0,
        "no float may be parked once the claim arm is live (f={})",
        stats.last_termination_kinds.float,
    );
    // Claimed ≡ black at THIS cycle's parity (spot-check one header).
    assert!(unsafe {
        (*(float_vals[0].as_float_ptr().unwrap()))
            .header
            .is_marked_at(heap.mark_parity)
    });
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(list);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, f) in float_vals.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(f.as_float_ptr().unwrap() as *const u8),
            "deferred-then-resolved float {i} was swept while rooted",
        );
        assert!((f.xfloat() - i as f64).abs() < f64::EPSILON);
    }
    assert!(
        !heap.owns_non_cons_object(garbage_ptr),
        "unrooted float must not be retained by the deferred machinery",
    );
    heap.assert_object_arenas_coherent();
}

#[test]
fn concurrent_floats_claimed_and_garbage_freed() {
    deferred_floats_resolve_at_termination_body(false);
}

#[test]
fn concurrent_floats_claimed_and_garbage_freed_verified() {
    deferred_floats_resolve_at_termination_body(true);
}

/// Task 01 H2 (snapshot-miss direction, deterministic unit test of the
/// dispatcher arm): a float living in a page created AFTER the
/// start-handshake snapshot must DEFER (miss ⇒ defer, never "miss ⇒
/// mapped" — the mid-cycle-float population), and a deferred float must
/// not bump the claim counter; a snapshot-page float claims at the job
/// parity. Drives `concurrent_try_mark_owned` directly with a hand-built
/// `ConcurrentClaimJob` so the page-boundary race is not left to timing.
#[test]
fn concurrent_claim_arm_defers_mid_cycle_float_pages() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // F_OLD lives in a page that exists at the "snapshot" instant.
    let f_old = heap.alloc_float(1.0);
    let snap: rustc_hash::FxHashSet<usize> = heap
        .float_arena
        .pages
        .iter()
        .map(|p| p.base_addr())
        .collect();
    // Allocate until the arena opens a NEW page; the last allocation is
    // the one that triggered it, so it lives in the post-snapshot page.
    let pages_before = heap.float_arena.pages.len();
    let mut f_new = f_old;
    while heap.float_arena.pages.len() == pages_before {
        f_new = heap.alloc_float(2.0);
    }
    let new_base = (f_new.as_float_ptr().unwrap() as usize) & !(OBJECT_PAGE_ALIGN - 1);
    assert!(
        !snap.contains(&new_base),
        "the defer probe must live in a post-snapshot page",
    );

    // Hand-built claim job. Both floats were born at the CURRENT heap
    // parity; a real cycle flips parity at `begin_collection` before
    // launching, so claim at the flipped value exactly like the job
    // a launch would carry.
    let job = ConcurrentClaimJob {
        parity: !heap.mark_parity,
        string_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        float_page_bases: std::sync::Arc::new(snap),
        vector_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        bytecode_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        dump_lo: usize::MAX,
        dump_hi: 0,
        drop_dump_children: false,
        str_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
        float_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
        subr_dropped: std::sync::Arc::new(AtomicUsize::new(0)),
        vec_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
        bc_claimed: std::sync::Arc::new(AtomicUsize::new(0)),
    };
    let mut gray = Vec::new();
    assert!(
        concurrent_try_mark_owned(f_old, &job, &mut gray),
        "snapshot-page float must be handled (claimed)",
    );
    assert_eq!(job.float_claimed.load(Ordering::Relaxed), 1);
    assert!(gray.is_empty(), "floats have no children to gray-push");
    assert!(unsafe {
        (*f_old.as_float_ptr().unwrap())
            .header
            .is_marked_at(!heap.mark_parity)
    });
    assert!(
        !concurrent_try_mark_owned(f_new, &job, &mut gray),
        "post-snapshot-page float must DEFER",
    );
    assert_eq!(
        job.float_claimed.load(Ordering::Relaxed),
        1,
        "a deferred float must not bump the claim counter",
    );
    // The deferred float's header was never touched: still born-at-the-
    // OLD-parity (i.e. unmarked at the job parity).
    assert!(unsafe {
        !(*f_new.as_float_ptr().unwrap())
            .header
            .is_marked_at(!heap.mark_parity)
    });
}

/// Task 01 H5 (tenured short-circuit): a TENURED page float discovered
/// by the GC thread is recognize-and-DROPPED — handled without a parity
/// claim (counter stays zero), never parked (float bucket zero), and its
/// FROZEN mark bit is not scribbled. Runs with the partition + verifiers
/// armed; the first STW cycle performs the one-shot promotion.
#[test]
fn concurrent_tenured_float_dropped_not_claimed() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_verify(&mut heap);

    // F is alive across the FIRST partitioned cycle, so the promotion
    // page walk tenures it (page slots of survivors freeze).
    let f = heap.alloc_float(3.25);
    let root = heap.alloc_cons(f, TaggedValue::fixnum(0));
    heap.collect_exact(std::iter::once(root));
    let f_ptr = f.as_float_ptr().unwrap();
    assert!(
        unsafe { (*f_ptr).header.tenured },
        "the first partitioned cycle must promote the surviving float",
    );
    let frozen_bit = unsafe { (*f_ptr).header.is_marked() };

    // One full concurrent cycle with F reachable via the rooted cons:
    // the GC thread discovers F, page-hits (retired/tenured pages stay
    // in the snapshot), sees `tenured`, and drops it.
    run_concurrent_cycle(&mut heap, &[root]);
    let stats = heap.sweep_stats();
    assert_eq!(
        stats.last_concurrent_float_claimed, 0,
        "tenured floats are dropped, not claimed",
    );
    assert_eq!(
        stats.last_termination_kinds.float, 0,
        "tenured floats are dropped, not parked",
    );
    assert_eq!(
        unsafe { (*f_ptr).header.is_marked() },
        frozen_bit,
        "the frozen tenured mark bit must not be scribbled",
    );
    assert!(unsafe { (*f_ptr).header.tenured });
    assert!((f.xfloat() - 3.25).abs() < f64::EPSILON);
    heap.assert_object_arenas_coherent();
}

/// (d) Teardown: dropping the heap frees every float page exactly once
/// (page floats are on none of the intrusive lists, so this is the
/// explicit `Vec<ObjectPage<FloatObj>>` drop path). Counter deltas are deterministic
/// under nextest's process-per-test execution.
fn pages_freed_at_heap_drop_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        heap.extend_dump_span(4096, 16);
        heap.collect_exact(std::iter::empty());
    }
    for i in 0..(2 * FLOAT_PAGE_SLOTS + 5) {
        let _ = heap.alloc_float(i as f64);
    }
    assert_eq!(
        LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
        before + 3,
        "2 full pages + 5 slots must occupy exactly 3 pages",
    );
    // A GC in between releases pages that have no surviving slots.
    heap.collect_exact(std::iter::empty());
    assert_eq!(
        LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
        before,
        "a completed sweep must release completely empty arena pages",
    );
    assert!(heap.float_arena.pages.is_empty());
    heap.assert_object_arenas_coherent();
    drop(heap);
    assert_eq!(
        LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
        before,
        "heap teardown must free every float page exactly once",
    );
}

#[test]
fn pages_freed_at_heap_drop() {
    pages_freed_at_heap_drop_body(false);
}

#[test]
fn pages_freed_at_heap_drop_verified() {
    pages_freed_at_heap_drop_body(true);
}

/// (d, mid-mark variant) Dropping the heap while the GC thread is still
/// concurrently marking must join the thread FIRST and then free the
/// pages — the join runs in `TaggedHeap::drop`'s body, before the
/// `Vec<ObjectPage<FloatObj>>` field drop. Under TSAN/ASAN a page freed early would
/// be a use-after-free on the GC thread; the counter catches leaks and
/// double-frees.
fn pages_freed_at_heap_drop_mid_concurrent_mark_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    // A long spine (the GC thread is genuinely marking at drop) whose
    // cars are floats spanning multiple pages.
    const N: usize = 3 * FLOAT_PAGE_SLOTS;
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N {
        let f = heap.alloc_float(i as f64);
        list = heap.alloc_cons(f, list);
    }
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 3);
    heap.concurrent_begin();
    heap.seed_root(list);
    heap.launch_concurrent_mark();
    assert!(heap.concurrent_mark_running());
    drop(heap); // must join, then free 3 pages exactly once
    assert_eq!(
        LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
        before,
        "mid-mark teardown must join the GC thread and free every page",
    );
}

#[test]
fn pages_freed_at_heap_drop_mid_concurrent_mark() {
    pages_freed_at_heap_drop_mid_concurrent_mark_body(false);
}

#[test]
fn pages_freed_at_heap_drop_mid_concurrent_mark_verified() {
    pages_freed_at_heap_drop_mid_concurrent_mark_body(true);
}

/// (e) Mapped-float coexistence: `register_mapped_float_range` floats are
/// a third storage class — side-table marks, never in the addr set,
/// never freed — and must route correctly alongside heap page floats
/// within one object graph, across the first (promote/blacken) cycle and
/// a partitioned cycle.
fn mapped_and_page_floats_coexist_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Stand-in for a pdump image: a leaked, heap-external FloatObj array
    // (must stay mapped for the heap's lifetime — leaking satisfies it).
    let mapped: &'static mut [FloatObj] = Box::leak(
        (0..4)
            .map(|i| FloatObj {
                header: GcHeader::new(HeapObjectKind::Float),
                value: 10.0 + i as f64,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let mapped_ptr = mapped.as_mut_ptr();
    // Registers the range AND activates the dump partition.
    unsafe { heap.register_mapped_float_range(mapped_ptr, 4) };

    let h = heap.alloc_float(1.25);
    let h_ptr = h.as_float_ptr().unwrap() as *const u8;
    let g = heap.alloc_float(2.5);
    let g_ptr = g.as_float_ptr().unwrap() as *const u8;
    let m_ptr = unsafe { mapped_ptr.add(2) };
    let m = unsafe { TaggedValue::from_float_ptr(m_ptr) };
    // One root reaching both storage classes.
    let root = heap.alloc_cons(h, m);

    // First partition cycle: full STW trace + sweep, then promote/blacken.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);

    // Routing: the page float is owned (addr set); the mapped float is
    // NOT in the set (side tables are its mark state) yet was marked and
    // is fully readable; the garbage page float was swept.
    assert!(heap.owns_non_cons_object(h_ptr));
    assert!(!heap.owns_non_cons_object(m_ptr as *const u8));
    assert!(!heap.owns_non_cons_object(g_ptr));
    assert!((h.xfloat() - 1.25).abs() < f64::EPSILON);
    assert!((m.xfloat() - 12.0).abs() < f64::EPSILON);
    // The mapped float's masked page base can never be a live page's
    // base (a page owns its whole 64KB span; allocations are disjoint) —
    // so the page registry cannot misroute mapped floats.
    assert!(
        !heap
            .float_arena
            .page_index_by_base
            .contains_key(&ObjectPage::<FloatObj>::page_base_for_ptr(m_ptr)),
    );
    heap.assert_object_arenas_coherent();

    // Partitioned cycle: the mapped float is permanent-black (skipped),
    // the page float re-marks via the root, fresh garbage is swept.
    let g2 = heap.alloc_float(3.5);
    let g2_ptr = g2.as_float_ptr().unwrap() as *const u8;
    heap.collect_exact(std::iter::once(root));
    assert!(heap.owns_non_cons_object(h_ptr));
    assert!(!heap.owns_non_cons_object(g2_ptr));
    assert!((h.xfloat() - 1.25).abs() < f64::EPSILON);
    assert!((m.xfloat() - 12.0).abs() < f64::EPSILON);
    heap.assert_object_arenas_coherent();
}

#[test]
fn mapped_and_page_floats_coexist() {
    mapped_and_page_floats_coexist_body(false);
}

#[test]
fn mapped_and_page_floats_coexist_verified() {
    mapped_and_page_floats_coexist_body(true);
}

/// (f) ALLOCATED-BIT-FIRST under adversarial staleness: garbage written
/// into freed slots' OBJECT bytes (header + value — an invalid kind, a
/// tenured-looking flag, a junk next pointer) must never be read by the
/// sweep, the verifiers, or teardown; reallocation must FULL-HEADER-WRITE
/// every stale byte away. The trailing free-list link word (bytes 24..32)
/// is arena metadata, not object bytes, and is untouched by the
/// adversary.
fn freed_slot_garbage_headers_are_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_verify(&mut heap);
    }

    let mut floats = Vec::new();
    for i in 0..100 {
        floats.push(heap.alloc_float(i as f64));
    }
    let keep: Vec<TaggedValue> = floats.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut FloatObj> = floats
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_float_ptr().unwrap() as *mut FloatObj)
        .collect();

    // Free the odd half (this is the first/promote cycle under verify).
    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }

    // ADVERSARY: scribble every freed slot's first 24 bytes with 0xFF.
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<FloatObj>()) };
    }
    // The free list (trailing link words) survived the scribble.
    heap.assert_object_arenas_coherent();

    // A full cycle re-sweeps the page: the scribbled slots' bits are
    // clear, so no header is Drop-dispatched, size-read, or parity-read
    // (reading one would trip the kind/tenured debug asserts — or UB).
    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert!((k.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
    }
    heap.assert_object_arenas_coherent();

    // Reallocate exactly the freed population: the class free list hands
    // the scribbled slots back; the FULL-HEADER WRITE must rebuild every
    // header byte (kind, mark bit, tenured, next) from scratch.
    let mut reused = Vec::new();
    for i in 0..dead_ptrs.len() {
        reused.push(heap.alloc_float(500.0 + i as f64));
    }
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_float_ptr().unwrap();
        assert!(
            dead_addrs.contains(&(ptr as usize)),
            "reallocation must reuse the freed (scribbled) slots",
        );
        unsafe {
            assert_eq!((*ptr).header.kind, HeapObjectKind::Float);
            assert!(
                !(*ptr).header.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert!((r.xfloat() - (500.0 + i as f64)).abs() < f64::EPSILON);
    }
    heap.assert_object_arenas_coherent();

    // The rebuilt headers survive a rooted cycle (the sweep now reads
    // them — the debug asserts prove they are coherent again), and a
    // final unrooted cycle reclaims them cleanly.
    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    for (i, r) in reused.iter().enumerate() {
        assert!((r.xfloat() - (500.0 + i as f64)).abs() < f64::EPSILON);
    }
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(r.as_float_ptr().unwrap() as *const u8));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn freed_slot_garbage_headers_are_never_read() {
    freed_slot_garbage_headers_are_never_read_body(false);
}

#[test]
fn freed_slot_garbage_headers_are_never_read_verified() {
    freed_slot_garbage_headers_are_never_read_body(true);
}

/// Remembered-set safety across the young/tenured boundary: when a page
/// float's only owner is TENURED at promotion (a Box RECORD here — the
/// list-promotion path; page vectors get their own tenure coverage with
/// the stage-3 promotion page walk), the promotion-time permanents scan
/// must record the owner so the float is re-seeded and survives every
/// later partitioned cycle.
fn tenured_owner_keeps_young_page_float_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16); // activates the partition

    // F reachable ONLY through record T (T is a Box veclike, so the
    // list promotion tenures it).
    let f = heap.alloc_float(42.5);
    let f_ptr = f.as_float_ptr().unwrap() as *const u8;
    let t = heap.alloc_record(vec![f]);
    let root = heap.alloc_cons(t, TaggedValue::fixnum(0));

    // First partition cycle: T promotes to tenured.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    let t_header = t.as_veclike_ptr().unwrap();
    assert!(
        unsafe { (*t_header).gc.tenured },
        "record must have tenured"
    );
    assert!(heap.owns_non_cons_object(f_ptr));

    // Two partitioned cycles (one per parity): T is permanent-black and
    // never re-traced; F survives ONLY via the promotion-time remembered
    // set — the permanent owner's young-float edge.
    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert!(
            heap.owns_non_cons_object(f_ptr),
            "young page float lost on partitioned cycle {cycle} \
             (remembered set must retain the tenured owner's float edge)",
        );
        assert!((f.xfloat() - 42.5).abs() < f64::EPSILON);
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_owner_keeps_young_page_float_alive() {
    tenured_owner_keeps_young_page_float_alive_body(false);
}

#[test]
fn tenured_owner_keeps_young_page_float_alive_verified() {
    tenured_owner_keeps_young_page_float_alive_body(true);
}
