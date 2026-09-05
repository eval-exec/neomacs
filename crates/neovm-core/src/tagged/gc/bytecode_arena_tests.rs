use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::value::LambdaParams;

fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    // Fake dump span: activates the dump partition so the first full
    // cycle promotes + blackens.
    heap.extend_dump_span(4096, 16);
}

/// Drive one full concurrent cycle (start handshake → GC-thread drain →
/// termination → deferred sweep drained). Copy of the float_arena_tests
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

/// A `ByteCodeFunction` carrying `constants`, `n_ops` no-op instructions,
/// and `payload` raw GNU bytecode bytes — the REAL-`Drop` payloads the
/// page sweep must `drop_in_place`. Empty params keep the arglist NIL so
/// the object's only heap children are its constants (GC-exact tests).
fn bytecode_fn(constants: Vec<TaggedValue>, n_ops: usize, payload: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    f.constants = constants.into();
    f.ops = vec![Op::Nil; n_ops];
    if payload > 0 {
        f.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(vec![
            0xAA;
            payload
        ]));
    }
    f
}

fn bc_ptr(v: TaggedValue) -> *const u8 {
    v.as_veclike_ptr().unwrap() as *const u8
}

/// Read constant `i` of a live bytecode value (payload-intact probe).
fn bc_constant(v: TaggedValue, i: usize) -> TaggedValue {
    let obj = unsafe { &*(v.as_veclike_ptr().unwrap() as *const ByteCodeObj) };
    obj.data.constants[i]
}

/// (a) PAGE-SPAN ORACLE EXACTNESS for the 384B stride: owned for a live
/// slot base ONLY — false for freed slots, interior/unaligned addresses,
/// never-bumped slots, and (unique to the non-power-of-two stride) the
/// stride-aligned first byte of the 256B page TAIL. Cross-class
/// registries never collide.
#[test]
fn bytecode_page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let keep = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 4, 0));
    let dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 4, 0));
    let keep2 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(3)], 4, 0));
    let f = heap.alloc_float(1.5);
    let dead_addr = bc_ptr(dead) as usize;

    // Page bytecode never touches the residual addr-set (TRAP A/B: the
    // page oracle owns it from birth).
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!(heap.bytecode_arena.owns(bc_ptr(dead)));

    heap.collect_exact([keep, keep2, f].into_iter());

    let b_addr = bc_ptr(keep) as usize;
    // Live slot bases answer owned (arena + union + veclike routing).
    assert!(heap.bytecode_arena.owns(b_addr as *const u8));
    assert!(heap.owns_non_cons_object(b_addr as *const u8));
    assert!(heap.owns_veclike_object(b_addr as *const u8));
    // Freed slot answers NOT owned the instant its bit clears.
    assert!(!heap.bytecode_arena.owns(dead_addr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_addr as *const u8));
    // Interior (stride-misaligned) + arbitrary unaligned addresses.
    assert!(!heap.bytecode_arena.owns((b_addr + 8) as *const u8));
    assert!(!heap.bytecode_arena.owns((b_addr + 192) as *const u8));
    assert!(!heap.bytecode_arena.owns((b_addr + 1) as *const u8));
    // Never-allocated slot beyond the bump cursor, inside the page.
    let page_base = ObjectPage::<ByteCodeObj>::page_base_for_ptr(b_addr as *const ByteCodeObj);
    let beyond_bump = page_base + 100 * <ByteCodeObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.bytecode_arena.owns(beyond_bump as *const u8));
    // THE PAGE TAIL: slot index SLOTS (byte 65280) is stride-aligned but
    // past the last real slot — the explicit `< SLOTS` bound in `owns`
    // must answer NOT-owned (a power-of-two-stride oracle never sees
    // this case; the 384B class does).
    assert_eq!(ObjectPage::<ByteCodeObj>::SLOTS, BYTECODE_PAGE_SLOTS);
    let tail = page_base + BYTECODE_PAGE_SLOTS * <ByteCodeObj as PagedObject>::SLOT_BYTES;
    assert!(
        tail - page_base < OBJECT_PAGE_BYTES,
        "tail is inside the page"
    );
    assert!(!heap.bytecode_arena.owns(tail as *const u8));
    // Wrong-class registries: never merged, never colliding.
    let f_addr = f.as_float_ptr().unwrap() as usize;
    assert!(!heap.bytecode_arena.owns(f_addr as *const u8));
    assert!(!heap.float_arena.owns(b_addr as *const u8));
    assert!(!heap.vector_arena.owns(b_addr as *const u8));
    assert!(!heap.string_arena.owns(b_addr as *const u8));
    heap.assert_object_arenas_coherent();
}

/// (g) `ordinary_non_cons_ownership_index_tracks_sweep`, bytecode form:
/// the sweep's alloc-bit clear IS the ownership eviction; the residual
/// addr-set stays empty throughout and payloads stay intact.
#[test]
fn bytecode_ownership_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let live = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(10)], 8, 64));
    let dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(20)], 8, 64));
    let live_ptr = bc_ptr(live);
    let dead_ptr = bc_ptr(dead);

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert!(heap.bytecode_arena.owns(live_ptr));
    assert!(heap.bytecode_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);

    heap.collect_exact(std::iter::once(live));

    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(!heap.owns_non_cons_object(dead_ptr));
    assert!(heap.bytecode_arena.owns(live_ptr));
    assert!(!heap.bytecode_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert_eq!(bc_constant(live, 0).as_fixnum(), Some(10));
    heap.assert_object_arenas_coherent();
}

/// (b) Parity two-cycle properties for page bytecode: mark-born
/// (allocate-black) survives its birth cycle unrooted and the next one
/// rooted; idle-born garbage is reclaimed by the first cycle after its
/// birth; mark-born garbage floats through its birth cycle and is
/// reclaimed by the next.
fn parity_two_cycle_bytecode_survival_and_reclaim_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
    }

    // STW bootstrap (flip #1) enables the concurrent collector (and
    // blackens the fake dump under verify).
    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());

    // Cycle 2: bytecode born MID-MARK (allocate-black at this cycle's
    // parity), deliberately NOT seeded at the termination.
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(25)], 4, 32));
    let b_ptr = bc_ptr(b);
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine); // b deliberately NOT seeded
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(b_ptr),
        "allocate-black bytecode must survive the cycle it was born in",
    );
    heap.assert_object_arenas_coherent();

    // Cycle 3 (opposite parity): rooted now — traced as unmarked via the
    // seed and survives with its payload intact.
    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(heap.owns_non_cons_object(b_ptr));
    assert_eq!(bc_constant(b, 0).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();

    // Reclaim: g1 idle-born (no allocate-black), g2 mark-born.
    let g1 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-9)], 4, 32));
    let g1_ptr = bc_ptr(g1);
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.seed_root(b);
    heap.launch_concurrent_mark();
    let g2 = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-8)], 4, 32));
    let g2_ptr = bc_ptr(g2);
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
    // No allocations since the sweep: the ownership probes below cannot
    // be confused by slot reuse.
    assert!(
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage bytecode must be reclaimed by the next cycle",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage bytecode floats through its birth cycle",
    );
    heap.assert_object_arenas_coherent();

    run_concurrent_cycle(&mut heap, &[spine, b]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage bytecode must be reclaimed by the SECOND cycle",
    );
    assert_eq!(bc_constant(b, 0).as_fixnum(), Some(25));
    heap.assert_object_arenas_coherent();
}

#[test]
fn parity_two_cycle_bytecode_survival_and_reclaim() {
    parity_two_cycle_bytecode_survival_and_reclaim_body(false);
}

#[test]
fn parity_two_cycle_bytecode_survival_and_reclaim_verified() {
    parity_two_cycle_bytecode_survival_and_reclaim_body(true);
}

/// (TRAP A, updated for the task 01 bytecode arm) Rooted page bytecode
/// discovered during a concurrent mark is CLAIMED on the GC thread
/// (page-snapshot hit + `mark_claim_at` + children gray-push) — the
/// deferred bytecode bucket collapses to zero and the claim counter
/// carries the count. Every field `trace_veclike`'s ByteCode arm traces
/// (arglist, constants, env, doc_form, interactive, extra_slots) holds a
/// child reachable ONLY through the bytecode; all must survive via the
/// GC-thread gray-push (the claimed header suppresses the termination
/// re-trace, so nothing else covers them). Garbage bytecode + its
/// otherwise-unreachable children must be collected within two cycles.
fn deferred_bytecode_resolves_at_termination_body(verify: bool) {
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

    // A rooted cons list carrying bytecode cars whose constants carry a
    // cons child reachable ONLY through the bytecode (children coverage).
    let mut list = TaggedValue::fixnum(0);
    let mut bytecodes = Vec::new();
    let mut children = Vec::new();
    for i in 0..300 {
        let child = heap.alloc_cons(TaggedValue::fixnum(10_000 + i), TaggedValue::fixnum(0));
        children.push(child);
        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i), child], 4, 16));
        bytecodes.push(b);
        list = heap.alloc_cons(b, list);
    }
    // One bytecode exercising EVERY traced field: each child cons is
    // reachable only through that field.
    let c_arg = heap.alloc_cons(TaggedValue::fixnum(1_001), TaggedValue::fixnum(0));
    let c_env = heap.alloc_cons(TaggedValue::fixnum(1_002), TaggedValue::fixnum(0));
    let c_doc = heap.alloc_cons(TaggedValue::fixnum(1_003), TaggedValue::fixnum(0));
    let c_int = heap.alloc_cons(TaggedValue::fixnum(1_004), TaggedValue::fixnum(0));
    let c_extra = heap.alloc_cons(TaggedValue::fixnum(1_005), TaggedValue::fixnum(0));
    let full = {
        let mut f = bytecode_fn(vec![TaggedValue::fixnum(0)], 4, 16);
        f.arglist = c_arg;
        f.env = Some(c_env);
        f.doc_form = Some(c_doc);
        f.interactive = Some(c_int);
        f.extra_slots = vec![c_extra];
        heap.alloc_bytecode(f)
    };
    list = heap.alloc_cons(full, list);
    // Garbage bytecode whose constants hold an otherwise-unreachable
    // string child (ownership-probe-able, unlike a cons): both must go.
    let g_child = heap.alloc_string(crate::heap_types::LispString::from_utf8("bc-garbage-kid"));
    let g_child_ptr = g_child.as_string_ptr().unwrap() as *const u8;
    let garbage = heap.alloc_bytecode(bytecode_fn(vec![g_child], 4, 16));
    let garbage_ptr = bc_ptr(garbage);

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
        stats.last_concurrent_bc_claimed >= 301,
        "every rooted page bytecode must be claimed on the GC thread \
         (claimed={})",
        stats.last_concurrent_bc_claimed,
    );
    assert_eq!(
        stats.last_termination_kinds.bytecode, 0,
        "no bytecode may park on a bare page-only heap (bc={})",
        stats.last_termination_kinds.bytecode,
    );
    // Claimed ≡ black at THIS cycle's parity (spot-check one header).
    assert!(unsafe {
        (*bytecodes[0].as_veclike_ptr().unwrap())
            .gc
            .is_marked_at(heap.mark_parity)
    });
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine);
    heap.seed_root(list);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    for (i, b) in bytecodes.iter().enumerate() {
        assert!(
            heap.owns_non_cons_object(bc_ptr(*b)),
            "claimed bytecode {i} was swept while rooted",
        );
        assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(i as i64));
        // The constants child (reachable only through the bytecode) was
        // traced via the claim arm's gray-push.
        assert_eq!(
            unsafe { (*children[i].xcons_ptr()).load_car() }.as_fixnum(),
            Some(10_000 + i as i64),
            "bytecode {i}'s constants child was swept while live",
        );
    }
    for (child, expect, field) in [
        (c_arg, 1_001, "arglist"),
        (c_env, 1_002, "env"),
        (c_doc, 1_003, "doc_form"),
        (c_int, 1_004, "interactive"),
        (c_extra, 1_005, "extra_slots"),
    ] {
        assert_eq!(
            unsafe { (*child.xcons_ptr()).load_car() }.as_fixnum(),
            Some(expect),
            "claimed bytecode's {field} child was swept while live \
             (the claim arm must gray-push every trace_veclike field)",
        );
    }
    assert!(
        !heap.owns_non_cons_object(garbage_ptr),
        "unrooted bytecode must not be retained by the claim machinery",
    );
    // Second cycle: the garbage child must be gone too (the garbage
    // bytecode was never discovered, so nothing pushed its children).
    run_concurrent_cycle(&mut heap, &[spine, list]);
    assert!(
        !heap.owns_non_cons_object(g_child_ptr),
        "the garbage bytecode's only child must be collected by cycle 2",
    );
    heap.assert_object_arenas_coherent();
}

#[test]
fn deferred_bytecode_resolves_at_termination() {
    deferred_bytecode_resolves_at_termination_body(false);
}

#[test]
fn deferred_bytecode_resolves_at_termination_verified() {
    deferred_bytecode_resolves_at_termination_body(true);
}

/// Task 01 H2 (snapshot-miss direction, deterministic unit test of the
/// bytecode arm): bytecode living in a page created AFTER the
/// start-handshake snapshot must DEFER (miss ⇒ defer, never "miss ⇒
/// mapped"), without a counter bump or a header write; a snapshot-page
/// bytecode claims at the job parity AND gray-pushes exactly its heap
/// children; a re-discovered (already-marked) one is handled WITHOUT a
/// second push. Drives `concurrent_try_mark_owned` directly with a
/// hand-built `ConcurrentClaimJob` so the page-boundary race is not
/// left to timing.
#[test]
fn concurrent_claim_arm_defers_mid_cycle_bytecode_pages() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // B_OLD lives in a page that exists at the "snapshot" instant; its
    // constants carry one heap child (plus a fixnum that must NOT be
    // pushed).
    let child = heap.alloc_cons(TaggedValue::fixnum(51), TaggedValue::fixnum(52));
    let b_old = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(7), child], 2, 0));
    let snap: rustc_hash::FxHashSet<usize> = heap
        .bytecode_arena
        .pages
        .iter()
        .map(|p| p.base_addr())
        .collect();
    // Allocate until the arena opens a NEW page; the last allocation
    // lives in the post-snapshot page.
    let pages_before = heap.bytecode_arena.pages.len();
    let mut b_new = b_old;
    while heap.bytecode_arena.pages.len() == pages_before {
        b_new = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 2, 0));
    }
    let new_base = (bc_ptr(b_new) as usize) & !(OBJECT_PAGE_ALIGN - 1);
    assert!(
        !snap.contains(&new_base),
        "the defer probe must live in a post-snapshot page",
    );

    // Hand-built claim job (both bytecodes were born at the CURRENT
    // heap parity; a real cycle flips parity at `begin_collection`
    // before launching, so claim at the flipped value).
    let job = ConcurrentClaimJob {
        parity: !heap.mark_parity,
        string_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        float_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        vector_page_bases: std::sync::Arc::new(rustc_hash::FxHashSet::default()),
        bytecode_page_bases: std::sync::Arc::new(snap),
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
        concurrent_try_mark_owned(b_old, &job, &mut gray),
        "snapshot-page bytecode must be handled (claimed)",
    );
    assert_eq!(job.bc_claimed.load(Ordering::Relaxed), 1);
    assert!(unsafe {
        (*b_old.as_veclike_ptr().unwrap())
            .gc
            .is_marked_at(!heap.mark_parity)
    });
    // The fresh claim gray-pushed exactly the HEAP children: the one
    // constants cons (the fixnum constant and the NIL arglist are not
    // heap objects).
    assert_eq!(
        gray.iter().map(|v| v.0).collect::<Vec<_>>(),
        vec![child.0],
        "a fresh bytecode claim must gray-push exactly its heap children",
    );
    // Re-discovery through another edge: already marked ⇒ handled, no
    // counter bump, no duplicate children push.
    gray.clear();
    assert!(
        concurrent_try_mark_owned(b_old, &job, &mut gray),
        "an already-claimed bytecode is handled (nothing further owed)",
    );
    assert_eq!(job.bc_claimed.load(Ordering::Relaxed), 1);
    assert!(
        gray.is_empty(),
        "an already-marked bytecode must not re-push its children",
    );
    // Post-snapshot-page bytecode DEFERS: no claim, no counter, no push,
    // header untouched (still unmarked at the job parity).
    assert!(
        !concurrent_try_mark_owned(b_new, &job, &mut gray),
        "post-snapshot-page bytecode must DEFER",
    );
    assert_eq!(
        job.bc_claimed.load(Ordering::Relaxed),
        1,
        "a deferred bytecode must not bump the claim counter",
    );
    assert!(
        gray.is_empty(),
        "a deferred bytecode must not push children"
    );
    assert!(unsafe {
        !(*b_new.as_veclike_ptr().unwrap())
            .gc
            .is_marked_at(!heap.mark_parity)
    });
}

/// Task 01 H5 (tenured short-circuit): a TENURED page bytecode
/// discovered by the GC thread is recognize-and-DROPPED — handled
/// without a parity claim (counter stays zero), never parked (bytecode
/// bucket zero), its FROZEN mark bit is not scribbled, and its young
/// constants child is not orphaned (the promotion-time page-tenured
/// remembered-set scan keeps covering it, exactly as on the old defer
/// path). Partition + tricolor verifiers armed.
#[test]
fn concurrent_tenured_bytecode_dropped_not_claimed() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, true);

    // B survives the FIRST partitioned cycle, so the promotion page
    // walk tenures it; its young cons child stays young (conses never
    // tenure) and is reachable ONLY through B's constants.
    let young = heap.alloc_cons(TaggedValue::fixnum(4_321), TaggedValue::fixnum(0));
    let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(6), young], 4, 16));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));
    heap.collect_exact(std::iter::once(root));
    let b_hdr = b.as_veclike_ptr().unwrap();
    assert!(
        unsafe { (*b_hdr).gc.tenured },
        "the first partitioned cycle must promote the surviving bytecode",
    );
    let frozen_bit = unsafe { (*b_hdr).gc.is_marked() };

    // One full concurrent cycle with B reachable via the rooted cons:
    // the GC thread discovers B, page-hits, sees `tenured`, and drops.
    run_concurrent_cycle(&mut heap, &[root]);
    let stats = heap.sweep_stats();
    assert_eq!(
        stats.last_concurrent_bc_claimed, 0,
        "tenured bytecode is dropped, not claimed",
    );
    assert_eq!(
        stats.last_termination_kinds.bytecode, 0,
        "tenured bytecode is dropped, not parked",
    );
    assert_eq!(
        unsafe { (*b_hdr).gc.is_marked() },
        frozen_bit,
        "the frozen tenured mark bit must not be scribbled",
    );
    assert!(unsafe { (*b_hdr).gc.tenured });
    assert_eq!(bc_constant(b, 0).as_fixnum(), Some(6));
    assert_eq!(
        unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
        Some(4_321),
        "the tenured bytecode's young constants child must survive the \
         drop (page-tenured remembered-set coverage)",
    );
    heap.assert_object_arenas_coherent();
}

/// Task 01 bytecode-claim coverage leg (c), THE ADVERSARIAL ONE:
/// bytecode constructed MID-CYCLE into a REUSED SLOT of an
/// already-snapshotted page (page-base HIT — it does NOT defer) holds,
/// in its constants, the only surviving reference to child C after C's
/// snapshot home is severed. C must survive its birth cycle: not
/// through the bytecode (born-at-parity ⇒ the claim arm treats it as
/// already-marked ⇒ handled WITHOUT a children push) but through the
/// SATB deletion barrier on the home overwrite. The NEXT cycle then
/// re-traces: C is reachable ONLY through the bytecode's constants, so
/// the fresh claim's GC-thread gray-push is the ONLY thing carrying it
/// (bytecode has no Tier-B backing snapshot — this is where the arm's
/// children push is load-bearing). Runs with the partition + tricolor
/// verifiers armed (`verify_incremental_tricolor` is the oracle for
/// the removed termination re-trace backstop).
#[test]
fn concurrent_mid_cycle_bytecode_in_reused_slot_keeps_child_alive() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16); // activates the partition

    // Page setup: keeper pins the page; b_dead's slot becomes the free
    // slot the mid-cycle allocation will reuse.
    let b_keep = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 2, 0));
    let b_dead = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(2)], 2, 0));
    let dead_ptr = bc_ptr(b_dead) as usize;
    // C: young cons, reachable at the snapshot ONLY via home H's car.
    let c = heap.alloc_cons(TaggedValue::fixnum(81), TaggedValue::fixnum(82));
    let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
    // Long rooted spine (home at the bottom) so the GC thread is still
    // walking when the mutator severs; both race outcomes are asserted
    // identically (if the GC got to H first, C is simply already black).
    let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
    list = heap.alloc_cons(b_keep, list);
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;
    // Bootstrap STW cycle: blackens the fake dump (arming the
    // verifiers), promotes survivors, and frees b_dead's slot.
    heap.collect_exact(std::iter::once(root));
    let pre_launch_bases: std::collections::HashSet<usize> = heap
        .bytecode_arena
        .pages
        .iter()
        .map(|p| p.base_addr())
        .collect();

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-CYCLE: construct B_NEW carrying C in its constants — the
    // arena's class free list hands back b_dead's slot (page-base in
    // this cycle's snapshot) — then sever C's original home (fires the
    // SATB pre-image barrier).
    let b_new = heap.alloc_bytecode(bytecode_fn(vec![c], 2, 0));
    let new_ptr = bc_ptr(b_new) as usize;
    assert_eq!(
        new_ptr, dead_ptr,
        "the mid-cycle bytecode must land in the freed slot of a \
         snapshotted page (allocator changed? fix the test setup)",
    );
    assert!(
        pre_launch_bases.contains(&(new_ptr & !(OBJECT_PAGE_ALIGN - 1))),
        "the reused slot's page must be in this cycle's snapshot",
    );
    assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

    // Terminate with b_new re-seeded alongside the spine (it is a live
    // value the mutator holds; the explicit-roots harness must name it).
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.seed_root(b_new);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    // Runs verify_dump_partition + verify_incremental_tricolor (armed
    // above): a black b_new with a white C would panic here.
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // C survived its birth-cycle severing (SATB), with payload intact.
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(81).0,
    );
    assert!(heap.owns_non_cons_object(bc_ptr(b_new)));
    assert_eq!(bc_constant(b_new, 0).0, c.0);

    // NEXT full cycle: C is now reachable ONLY through B_NEW's
    // constants — the fresh claim's children gray-push must carry it.
    run_concurrent_cycle(&mut heap, &[root, b_new]);
    assert!(
        heap.sweep_stats().last_concurrent_bc_claimed >= 1,
        "the second cycle must claim the (now pre-existing) bytecode",
    );
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(81).0,
    );
    assert_eq!(bc_constant(b_new, 0).0, c.0);
    heap.assert_object_arenas_coherent();
}

/// ALLOCATED-BIT-FIRST under adversarial staleness, payload-class form:
/// garbage scribbled into freed slots' object bytes (a junk kind would
/// Drop-dispatch garbage `Vec` pointers if any reader trusted it) is
/// never read by the sweep, verifiers, or teardown; reallocation
/// FULL-HEADER-WRITEs every stale byte away. The trailing link word
/// (bytes 376..384) is arena metadata the adversary leaves alone.
fn bytecode_freed_slot_garbage_never_read_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    if verify {
        arm_partition(&mut heap, true);
        heap.collect_exact(std::iter::empty());
    }

    let mut bytecodes = Vec::new();
    for i in 0..100 {
        bytecodes.push(heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i)], 4, 16)));
    }
    let keep: Vec<TaggedValue> = bytecodes.iter().copied().step_by(2).collect();
    let dead_ptrs: Vec<*mut ByteCodeObj> = bytecodes
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| v.as_veclike_ptr().unwrap() as *mut ByteCodeObj)
        .collect();

    heap.collect_exact(keep.iter().copied());
    for &p in &dead_ptrs {
        assert!(!heap.owns_non_cons_object(p as *const u8));
    }

    // ADVERSARY: scribble every freed slot's object bytes with 0xFF
    // (kind, type tag, vec pointers — everything but the link word).
    for &p in &dead_ptrs {
        unsafe { std::ptr::write_bytes(p as *mut u8, 0xFF, size_of::<ByteCodeObj>()) };
    }
    // The free list (trailing link words) survived the scribble.
    heap.assert_object_arenas_coherent();

    // A full cycle re-sweeps the page: the scribbled slots' bits are
    // clear, so no header is Drop-dispatched, size-read, or parity-read.
    heap.collect_exact(keep.iter().copied());
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(bc_constant(*k, 0).as_fixnum(), Some(2 * i as i64));
    }
    heap.assert_object_arenas_coherent();

    // Reallocate exactly the freed population: the class free list hands
    // the scribbled slots back; the FULL-HEADER WRITE must rebuild every
    // byte — a stale 0xFF kind/type would misroute the next sweep's
    // `drop_in_place` (type-confused Drop of garbage pointers).
    let mut reused = Vec::new();
    for i in 0..dead_ptrs.len() {
        reused.push(heap.alloc_bytecode(bytecode_fn(
            vec![TaggedValue::fixnum(500 + i as i64)],
            8,
            32,
        )));
    }
    let dead_addrs: std::collections::HashSet<usize> =
        dead_ptrs.iter().map(|&p| p as usize).collect();
    for (i, r) in reused.iter().enumerate() {
        let ptr = r.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        assert!(
            dead_addrs.contains(&(ptr as usize)),
            "reallocation must reuse the freed (scribbled) slots",
        );
        unsafe {
            assert_eq!((*ptr).header.gc.kind, HeapObjectKind::VecLike);
            assert_eq!((*ptr).header.type_tag, VecLikeType::ByteCode);
            assert!(
                !(*ptr).header.gc.tenured,
                "stale tenured byte must be rewritten"
            );
            assert!(
                (*ptr).header.gc.next.is_null(),
                "stale next ptr must be rewritten"
            );
        }
        assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(500 + i as i64));
    }
    heap.assert_object_arenas_coherent();

    // The rebuilt headers + payloads survive a rooted cycle, and a final
    // unrooted cycle reclaims them cleanly (their REAL Drop runs on the
    // rewritten — valid — vec pointers, not the scribble).
    let mut roots: Vec<TaggedValue> = keep.clone();
    roots.extend(reused.iter().copied());
    heap.collect_exact(roots.iter().copied());
    for (i, r) in reused.iter().enumerate() {
        assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(500 + i as i64));
    }
    heap.collect_exact(keep.iter().copied());
    for r in &reused {
        assert!(!heap.owns_non_cons_object(bc_ptr(*r)));
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn bytecode_freed_slot_garbage_never_read() {
    bytecode_freed_slot_garbage_never_read_body(false);
}

#[test]
fn bytecode_freed_slot_garbage_never_read_verified() {
    bytecode_freed_slot_garbage_never_read_body(true);
}

/// Mid-sweep slot reuse within one cooperative sweep window (the class
/// free list hands freed slots to a mutator running BETWEEN slices) for
/// the payload class: no double-free, no premature free, `drop_in_place`
/// only on dead slots.
#[test]
fn bytecode_reuse_within_one_cooperative_sweep_window() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Exactly three full pages of bytecode.
    let n = 3 * BYTECODE_PAGE_SLOTS;
    let mut bytecodes = Vec::with_capacity(n);
    for i in 0..n {
        bytecodes.push(heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i as i64)], 2, 8)));
    }
    assert_eq!(heap.bytecode_arena.pages.len(), 3);
    heap.assert_object_arenas_coherent();

    let keep: Vec<TaggedValue> = bytecodes.iter().copied().step_by(2).collect();
    let dead_addrs: std::collections::HashSet<usize> = bytecodes
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| bc_ptr(*v) as usize)
        .collect();
    let page0_base = heap.bytecode_arena.pages[0].base_addr();

    heap.begin_collection();
    for &k in &keep {
        heap.seed_root(k);
    }
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());

    // Slice 1 (budget 1): sweeps bytecode page 0 only.
    assert!(!heap.incremental_sweep_slice(1), "3 pages need >1 slice");
    assert!(heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    // BETWEEN slices the mutator reallocates from the just-swept page.
    let mut reused = Vec::new();
    for i in 0..32 {
        reused.push(heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1_000 + i)], 2, 8)));
    }
    for r in &reused {
        let ptr = r.as_veclike_ptr().unwrap() as *const ByteCodeObj;
        assert_eq!(
            ObjectPage::<ByteCodeObj>::page_base_for_ptr(ptr),
            page0_base,
            "mid-sweep reuse must come from the just-swept page",
        );
        assert!(dead_addrs.contains(&(ptr as usize)));
    }
    heap.assert_object_arenas_coherent();

    // Drain the rest; reallocated slots are born-at-parity survivors.
    while !heap.incremental_sweep_slice(1) {}
    assert!(!heap.sweep_in_progress());
    heap.assert_object_arenas_coherent();

    for (i, r) in reused.iter().enumerate() {
        assert!(heap.owns_non_cons_object(bc_ptr(*r)));
        assert_eq!(bc_constant(*r, 0).as_fixnum(), Some(1_000 + i as i64));
    }
    for (i, k) in keep.iter().enumerate() {
        assert_eq!(bc_constant(*k, 0).as_fixnum(), Some(2 * i as i64));
    }
    let reused_addrs: std::collections::HashSet<usize> =
        reused.iter().map(|r| bc_ptr(*r) as usize).collect();
    for &addr in &dead_addrs {
        assert_eq!(
            heap.owns_non_cons_object(addr as *const u8),
            reused_addrs.contains(&addr),
            "freed slot must be owned iff reallocated",
        );
    }
}

/// (c) VARIABLE-size live-bytes accounting on BOTH recompute sites for
/// bytecode: big ops/constants/raw-bytes payloads counted for survivors
/// (fixed struct + every separately-allocated payload), garbage not.
#[test]
fn bytecode_sweep_live_bytes_track_variable_payload_sizes() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let b_big = heap.alloc_bytecode(bytecode_fn(
        vec![TaggedValue::fixnum(7); 500],
        1_000,
        10_000,
    ));
    let b_small = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(1)], 1, 0));
    // Garbage that must NOT be counted after the sweep.
    let _dead = heap.alloc_bytecode(bytecode_fn(
        vec![TaggedValue::fixnum(0); 2_000],
        4_000,
        50_000,
    ));
    let mut root = TaggedValue::fixnum(0);
    let mut cons_count = 0usize;
    for val in [b_big, b_small] {
        root = heap.alloc_cons(val, root);
        cons_count += 1;
    }

    let expected_objects: usize = [b_big, b_small]
        .iter()
        .map(|b| {
            TaggedHeap::object_bytes_from_header(b.as_veclike_ptr().unwrap() as *const GcHeader)
        })
        .sum::<usize>();
    let expected = expected_objects + cons_count * size_of::<ConsCell>();
    // The payload really is variable-size (ops + constants + raw bytes
    // dominate the 384B slot).
    assert!(expected_objects > 2 * size_of::<ByteCodeObj>() + 1_000 * size_of::<Op>() + 10_000);

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
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(
        heap.live_bytes(),
        expected,
        "incremental sweep live_bytes != summed survivor bytes",
    );
}

/// (d) LOADUP-SHAPED tenure + retirement: bytecode is the first class
/// where FULL-page retirement meaningfully fires. A full page of rooted
/// bytecode retires at the one-time promotion (still owned — C1), a
/// partial page does not; the tenured population survives one cycle per
/// parity with payloads intact; post-retirement allocation never lands
/// in the retired page; and the C1 write-barrier edge holds: a RETIRED-
/// page bytecode given a young cons child (through the test-only seam)
/// keeps that child alive across both parities.
fn bytecode_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    // Exactly one FULL bytecode page, all rooted through a cons spine,
    // plus two overflow objects on a second (partial) page.
    let mut root = TaggedValue::fixnum(0);
    let mut bytecodes = Vec::with_capacity(BYTECODE_PAGE_SLOTS + 2);
    for i in 0..(BYTECODE_PAGE_SLOTS + 2) {
        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i as i64)], 4, 16));
        bytecodes.push(b);
        root = heap.alloc_cons(b, root);
    }
    assert_eq!(heap.bytecode_arena.pages.len(), 2);
    assert_eq!(heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS);

    // First partition cycle: full trace + sweep, then promotion.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);

    // Every paged survivor is tenured (the promotion page walk covers
    // the bytecode arena).
    for b in &bytecodes {
        let ptr = b.as_veclike_ptr().unwrap();
        assert!(unsafe { (*ptr).gc.tenured }, "page bytecode not tenured");
    }
    // The FULL page retired; the partial overflow page did not.
    assert!(
        heap.bytecode_arena.pages[0].retired,
        "full page must retire"
    );
    assert!(
        !heap.bytecode_arena.pages[1].retired,
        "partial page retired"
    );
    assert_eq!(
        heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS,
        "retired page must stay full",
    );
    // C1: retired-page slots STAY owned via the page oracle.
    assert!(heap.owns_non_cons_object(bc_ptr(bytecodes[0])));
    assert!(heap.bytecode_arena.owns(bc_ptr(bytecodes[0])));
    heap.assert_object_arenas_coherent();

    // Post-retirement allocation must never land in the retired page.
    let retired_base = heap.bytecode_arena.pages[0].base_addr();
    let fresh = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-5)], 2, 0));
    assert_ne!(
        ObjectPage::<ByteCodeObj>::page_base_for_ptr(
            fresh.as_veclike_ptr().unwrap() as *const ByteCodeObj
        ),
        retired_base,
        "allocation reused a retired page",
    );

    // C1 write-barrier edge on a RETIRED page: hand a retired-page
    // tenured bytecode a YOUNG cons child through the (test-only,
    // barrier-firing) seam. `value_is_tenured` must answer through the
    // page oracle (retired pages included) so `record_heap_write`
    // remembers the owner and the child survives both parities.
    let young = heap.alloc_cons(TaggedValue::fixnum(777_777), TaggedValue::fixnum(0));
    let carrier = bytecodes[3];
    assert!(
        crate::tagged::mutate::with_bytecode_data_mut_for_test(carrier, |data| {
            data.constants[0] = young;
        })
        .is_some()
    );

    // Two further cycles — parities false/true — retired page skipped
    // whole, partial page tenured-skipped, payloads intact, and the
    // young child of the retired-page owner survives.
    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, b) in bytecodes.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(bc_ptr(*b)),
                "tenured page bytecode #{i} lost on cycle {cycle}",
            );
            if i != 3 {
                assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(i as i64));
            }
        }
        assert_eq!(
            unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
            Some(777_777),
            "retired-page owner's young cons child lost on cycle {cycle} (C1)",
        );
        assert_eq!(heap.bytecode_arena.pages[0].allocated, BYTECODE_PAGE_SLOTS);
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn bytecode_survivors_tenure_and_full_pages_retire() {
    bytecode_survivors_tenure_and_full_pages_retire_body(false);
}

#[test]
fn bytecode_survivors_tenure_and_full_pages_retire_verified() {
    bytecode_survivors_tenure_and_full_pages_retire_body(true);
}

/// (d, mixed) Tenured and post-promotion YOUNG slots share a bytecode
/// page across TWO alternating-parity cycles: tenured slots survive with
/// intact payloads (a parity-blind sweep would free them on the flipped
/// cycle), young garbage in the SAME page is reclaimed, and freed slots
/// are reused without disturbing tenured neighbors.
fn bytecode_mixed_page_tenured_survive_alternating_parities_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let mut keep = Vec::new();
    let mut root = TaggedValue::fixnum(0);
    for i in 0..10 {
        let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(i as i64)], 4, 16));
        if i % 2 == 0 {
            keep.push(b);
            root = heap.alloc_cons(b, root);
        }
    }

    // Promotion cycle: odd-indexed garbage swept first, survivors tenure
    // ⇒ a MIXED page.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(!heap.bytecode_arena.pages[0].retired);

    // Refill freed slots with YOUNG garbage, one cycle per parity.
    for cycle in 0..2 {
        for i in 0..5 {
            let _ = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(-(i as i64))], 4, 16));
        }
        heap.collect_exact(std::iter::once(root));
        for (i, b) in keep.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(bc_ptr(*b)),
                "tenured bytecode #{i} freed on parity cycle {cycle}",
            );
            assert_eq!(bc_constant(*b, 0).as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn bytecode_mixed_page_tenured_survive_alternating_parities() {
    bytecode_mixed_page_tenured_survive_alternating_parities_body(false);
}

#[test]
fn bytecode_mixed_page_tenured_survive_alternating_parities_verified() {
    bytecode_mixed_page_tenured_survive_alternating_parities_body(true);
}

/// (e) Teardown with payload-bearing bytecode: every bytecode page is
/// freed exactly once at heap drop — retired pages included — with the
/// per-slot `drop_in_place` releasing ops/constants vectors, raw GNU
/// bytes, and docstrings (ASAN/MIRI lanes catch a leak or double-free;
/// the counters prove page-level accounting either way). The sweep-time
/// `drop_in_place` path is exercised too (half the population dies
/// before the drop).
fn bytecode_payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let before = LIVE_BYTECODE_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        let mut root = TaggedValue::fixnum(0);
        for i in 0..300 {
            let mut f = bytecode_fn(vec![TaggedValue::fixnum(i); 16], 128, 1024);
            f.docstring = Some(crate::heap_types::LispString::from_utf8(
                "payload-bearing bytecode docstring",
            ));
            let b = heap.alloc_bytecode(f);
            // Root every other one; the rest dies at the collection
            // below (sweep-time drop_in_place on this page class).
            if i % 2 == 0 {
                root = heap.alloc_cons(b, root);
            }
        }
        assert!(LIVE_BYTECODE_PAGES.load(Ordering::Relaxed) > before);

        // Promotion + (partial-page) tenure happen before the drop;
        // retired/mixed pages must be freed by teardown too.
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        heap.assert_object_arenas_coherent();

        if mid_mark {
            // Drop while the GC thread is concurrently marking: the heap
            // Drop must join FIRST, then free pages.
            heap.concurrent_begin();
            heap.seed_root(root);
            heap.launch_concurrent_mark();
            assert!(heap.concurrent_mark_running());
        }
        drop(heap);
    }
    assert_eq!(
        LIVE_BYTECODE_PAGES.load(Ordering::Relaxed),
        before,
        "bytecode pages leaked or double-freed at teardown",
    );
}

#[test]
fn bytecode_payload_pages_freed_at_heap_drop() {
    bytecode_payload_pages_freed_at_heap_drop_body(false);
}

#[test]
fn bytecode_payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
    bytecode_payload_pages_freed_at_heap_drop_body(true);
}

/// (f) The constants-immutability seam: production bytecode is immutable
/// post-publish (the mutation helper is `#[cfg(test)]` — enforced at
/// compile time; this is the invariant task 01's concurrent claim
/// consumes). The blessed TEST seam still fires the write barrier, so a
/// tenured owner mutated mid-test keeps its new young child alive —
/// verified under the partition verifier, which would flag a missed
/// barrier as a tenured→young violation.
fn bytecode_constants_test_seam_fires_write_barrier_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    let b = heap.alloc_bytecode(bytecode_fn(vec![TaggedValue::fixnum(0)], 2, 0));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));
    heap.collect_exact(std::iter::once(root));
    assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

    // The seam refuses non-bytecode values.
    assert!(crate::tagged::mutate::with_bytecode_data_mut_for_test(root, |_| ()).is_none());

    // Mutate the tenured owner's constants to a YOUNG cons through the
    // seam; the pre-write barrier must remember the owner.
    let young = heap.alloc_cons(TaggedValue::fixnum(4_242), TaggedValue::fixnum(0));
    assert!(
        crate::tagged::mutate::with_bytecode_data_mut_for_test(b, |data| {
            data.constants[0] = young;
        })
        .is_some()
    );

    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*young.xcons_ptr()).load_car() }.as_fixnum(),
            Some(4_242),
            "seam-written young child lost on cycle {cycle} — the \
             test-only mutation seam must fire the write barrier",
        );
        assert_eq!(bc_constant(b, 0).0, young.0);
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn bytecode_constants_test_seam_fires_write_barrier() {
    bytecode_constants_test_seam_fires_write_barrier_body(false);
}

#[test]
fn bytecode_constants_test_seam_fires_write_barrier_verified() {
    bytecode_constants_test_seam_fires_write_barrier_body(true);
}

/// Promotion-scan coverage for the bytecode arena: a page bytecode
/// tenured at promotion whose constants hold a young CONS child (conses
/// never tenure) and is never mutated again — the promotion-time
/// page-tenured remembered-set scan must walk bytecode pages or the
/// child is swept while its permanently-black owner still points at it.
fn tenured_page_bytecode_keeps_young_cons_child_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    // A young cons child reachable ONLY through the bytecode's constants.
    let y = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::fixnum(0));
    let b = heap.alloc_bytecode(bytecode_fn(vec![y], 4, 0));
    let root = heap.alloc_cons(b, TaggedValue::fixnum(0));

    // Promotion: b tenures via the page walk; y stays young.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(unsafe { (*b.as_veclike_ptr().unwrap()).gc.tenured });

    // Two partitioned cycles (one per parity): the owner is black and
    // never re-traced; the child survives ONLY via the promotion-time
    // page-tenured remembered-set scan (which now walks bytecode pages).
    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.as_fixnum(),
            Some(999),
            "tenured page bytecode's young cons child lost on cycle {cycle}",
        );
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_page_bytecode_keeps_young_cons_child_alive() {
    tenured_page_bytecode_keeps_young_cons_child_alive_body(false);
}

#[test]
fn tenured_page_bytecode_keeps_young_cons_child_alive_verified() {
    tenured_page_bytecode_keeps_young_cons_child_alive_body(true);
}
