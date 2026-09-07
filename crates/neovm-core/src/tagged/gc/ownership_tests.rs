use super::*;

/// TASK 10 — the `dirty_owners` ABA regression.
///
/// The owner-tracking side tables (`dirty_owners` / `dirty_owner_bits` /
/// `dirty_writes`) are the remembered-set precursor. Their dedup is keyed on
/// the owner's address bits. If an entry recorded in one window survives into
/// a later cycle's sweep, that cycle can FREE the owner and the arena can hand
/// its slot (same size class ⇒ same address AND tag ⇒ identical bits) to a new
/// object — whose barriered write is then wrongly deduped ("suppressed") by
/// the stale entry. This test drives exactly that sequence with REAL frees and
/// REAL deterministic arena slot reuse, and asserts the tables track the new
/// occupant, not the freed ghosts.
///
/// It has teeth only under clear-at-BEGIN: revert `begin_collection`'s
/// `clear_dirty_owners()/clear_dirty_writes()` (restore the end-of-collection
/// clears) and BOTH the post-`begin_collection` `== 0` assertion and the final
/// `== 1` assertion fail (the stale O/Q entries linger, and O''s write is
/// deduped against O's ghost).
#[test]
fn dirty_owner_tracking_is_cleared_at_begin_so_freed_slot_reuse_is_not_deduped() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    set_tagged_heap(&mut heap);

    // --- Previous window: two GARBAGE owners O and Q, both mutated (barriered
    //     write) so both land in the owner tables. Neither is rooted, so the
    //     next collection will sweep them. ---
    let o = heap.alloc_vector(vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
    let q = heap.alloc_vector(vec![TaggedValue::fixnum(3), TaggedValue::fixnum(4)]);
    let o_addr = o.as_veclike_ptr().unwrap() as usize;
    assert!(crate::tagged::mutate::set_vector_slot(
        o,
        0,
        TaggedValue::fixnum(10)
    ));
    assert!(crate::tagged::mutate::set_vector_slot(
        q,
        0,
        TaggedValue::fixnum(20)
    ));
    assert_eq!(
        heap.dirty_owner_count(),
        2,
        "O and Q are both recorded dirty owners in this window"
    );

    // --- Start the collection that will free O and Q. Clear-at-begin empties
    //     the owner tables HERE, before the sweep can free-and-reuse a slot.
    //     (mark_all drains the internal runtime roots seeded by
    //     begin_collection so only the unrooted O and Q are swept.) ---
    heap.begin_collection();
    heap.mark_all();
    assert_eq!(
        heap.dirty_owner_count(),
        0,
        "begin_collection must clear owner tracking (clear-at-begin): a stale \
         pre-cycle entry that outlives this cycle's sweep is the ABA hazard",
    );

    // --- Sweep the vector arena: O and Q are unmarked ⇒ freed, their slots
    //     returned to the class free list. ---
    let vpages = heap.vector_arena.pages.len();
    let (_live, freed) = heap.sweep_arena_pages_ranges(
        (0, 0),
        (0, 0),
        (0, vpages),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
    );
    assert!(
        freed >= 2,
        "the sweep must reclaim the two unrooted garbage vectors (freed={freed})",
    );

    // --- Deterministic reuse: allocating the same class pops the just-freed
    //     slots off the free list, so O's exact address recurs. ---
    let mut o_prime = None;
    for _ in 0..64 {
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(0), TaggedValue::fixnum(0)]);
        if v.as_veclike_ptr().unwrap() as usize == o_addr {
            o_prime = Some(v);
            break;
        }
    }
    let o_prime = o_prime.expect("arena must hand O's freed slot back to a new same-class vector");
    assert_eq!(
        o_prime.as_veclike_ptr().unwrap() as usize,
        o_addr,
        "O' must occupy O's reclaimed slot (identical owner bits)",
    );

    // --- The barriered write to O' must be recorded as a FRESH owner. Under
    //     the ABA-prone clear-at-end lifecycle, O's ghost bits (== O''s bits)
    //     would dedup this write away, and Q's freed-but-uncleared entry would
    //     still inflate the count — so the table would read 2 ghosts, never
    //     the one true owner O'. ---
    assert!(crate::tagged::mutate::set_vector_slot(
        o_prime,
        1,
        TaggedValue::fixnum(99)
    ));
    assert!(
        heap.is_dirty_owner(o_prime),
        "O''s write must be recorded in the owner tables"
    );
    assert_eq!(
        heap.dirty_owner_count(),
        1,
        "exactly O' is dirty; a lingering ghost O (deduped) plus ghost Q \
         (freed, never cleared) would make this 2 under the stale-dedup ABA",
    );
}

#[test]
fn heap_identity_is_unique_across_heap_lifetimes() {
    crate::test_utils::init_test_tracing();

    let first_id = TaggedHeap::new().identity();
    let second_id = TaggedHeap::new().identity();

    assert_ne!(first_id, second_id);
}

/// Phase 5: drive a non-blocking concurrent mark with the GC thread marking
/// a large cons spine while THIS thread mutates (firing the SATB barrier) and
/// allocates (allocate-black). The graph is large on purpose so marking is
/// still in flight during the mutation, creating genuine overlap — run under
/// ThreadSanitizer (`-Zsanitizer=thread`) this is the race check. The liveness
/// asserts confirm the snapshot + SATB + allocate-black retain the right set.
#[test]
fn concurrent_mark_overlaps_mutation_and_retains_live_set() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A long reachable list: head -> ... -> tail (cdr-terminated with a
    // fixnum so traversal stops). Root = head.
    const N: i64 = 300_000;
    let mut list = TaggedValue::fixnum(0); // non-heap terminator
    for i in 0..N {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let head = list;
    // A second cons whose cdr we will rewire mid-mark (exercises SATB).
    let pivot = heap.alloc_cons(TaggedValue::fixnum(-1), head);
    // Unreachable garbage allocated before the mark begins.
    let _garbage = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
    let allocated_before = heap.cons_live_count;

    // Start the concurrent mark with `pivot` as the sole root (pivot -> head
    // -> whole list). begin_collection clears marks + seeds internal roots.
    heap.concurrent_begin();
    heap.seed_root(pivot);
    heap.launch_concurrent_mark();

    // While the GC thread marks: rewire pivot.cdr to a fresh cons D (the old
    // child `head` is logged to SATB and must stay live), and churn-allocate
    // (each new cons is born black). The list is long enough that the GC is
    // still traversing it during this.
    let d = heap.alloc_cons(TaggedValue::fixnum(7), head);
    assert!(crate::tagged::mutate::set_cons_cdr(pivot, d));
    for _ in 0..5_000 {
        let _ = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::fixnum(0));
    }

    // Wait for the GC thread to drain, then terminate stop-the-world.
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(pivot);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // The whole list (N) + pivot + D survive; `head` is retained as floating
    // garbage via SATB (it left pivot's cdr but was logged); the churn conses
    // are allocate-black so they survive this cycle too; only `_garbage` is
    // reclaimed. So exactly one cons (the pre-mark garbage) was swept.
    assert_eq!(
        heap.cons_live_count,
        allocated_before + 1 /* D */ + 5_000 /* churn */ - 1, /* garbage */
        "concurrent mark must retain the live + SATB + allocate-black set",
    );
    // The reachable spine is intact: walk pivot -> D -> head -> ... and check
    // a few cars (reading a swept cons would be caught by the sanitizer).
    let after_pivot = unsafe { (*pivot.xcons_ptr()).load_cdr() };
    assert!(after_pivot.is_cons());
    let head_again = unsafe { (*after_pivot.xcons_ptr()).load_cdr() };
    assert!(head_again.is_cons());
    assert_eq!(
        unsafe { (*head_again.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(N - 1).0,
    );
}

/// Gap 3 instrumentation: a deferred sweep must aggregate per-slice cost
/// (slice count, total µs, cons blocks, non-cons frees) into `sweep_stats`
/// and fold the cycle into the lifetime totals at completion.
#[test]
fn deferred_sweep_aggregates_slice_stats() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A small rooted list plus lots of garbage: dead conses spanning many
    // blocks and dead non-cons objects, so the sliced sweep has real work.
    let mut rooted = TaggedValue::fixnum(0);
    for i in 0..1_000 {
        rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
    }
    for i in 0..400_000 {
        let _ = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
    }
    for i in 0..4_000 {
        let _ = heap.alloc_float(i as f64);
    }

    // Mark to a fixpoint, arm the deferred sweep (the incremental
    // termination path), then drain it in bounded slices.
    heap.begin_collection();
    heap.seed_root(rooted);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    assert!(heap.sweep_in_progress());
    let mut slices = 1usize;
    while !heap.incremental_sweep_slice(8) {
        slices += 1;
    }
    assert!(!heap.sweep_in_progress());

    let stats = heap.sweep_stats();
    assert_eq!(stats.slice_count, slices);
    assert!(stats.slice_count > 1, "budget 8 must take several slices");
    assert!(stats.sweep_us > 0, "aggregated sweep cost must be non-zero");
    assert!(stats.cons_blocks_swept > 0);
    assert!(
        stats.noncons_freed >= 4_000,
        "the dead floats must be reclaimed by the deferred sweep \
         (freed={})",
        stats.noncons_freed,
    );
    assert_eq!(stats.lifetime_slices, stats.slice_count);
    assert_eq!(stats.lifetime_sweep_us, stats.sweep_us);
    assert_eq!(stats.lifetime_cons_blocks_swept, stats.cons_blocks_swept);
    assert_eq!(stats.lifetime_noncons_freed, stats.noncons_freed);
}

/// Gap 3 instrumentation: `join_concurrent_mark` must record how many
/// GC-thread-parked (deferred) values the STW termination drain was handed
/// — the number that sizes a records/closures/strings concurrent tier.
#[test]
fn concurrent_termination_records_deferred_drain_size() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A rooted cons spine carrying non-cons cars the claim dispatcher
    // REFUSES (records — floats/strings are claimed concurrently since
    // task 01 and never park): the GC thread marks the owned conses but
    // parks every record in `deferred`, so the termination drain size is
    // deterministically >= the car count.
    let mut list = TaggedValue::fixnum(0);
    for i in 0..1_000 {
        let car = heap.alloc_record(vec![TaggedValue::fixnum(i)]);
        list = heap.alloc_cons(car, list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_deferred >= 1_000,
        "every non-cons car must be parked for the termination drain \
         (deferred={})",
        stats.last_termination_deferred,
    );
    assert!(stats.max_termination_deferred >= stats.last_termination_deferred);
    assert_eq!(stats.last_termination_satb, 0, "no mutation ran mid-mark");

    // Finish the cycle cleanly: termination drain + deferred sweep.
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
    assert_eq!(heap.sweep_stats().noncons_freed, 0, "all records are live");
}

/// Handshake instrumentation (root-scan floor probe): a concurrent cycle
/// must populate the heap-side `HandshakeStats` phases — the start
/// handshake counter + cons/vector snapshot probes recorded by
/// `concurrent_begin`/`launch_concurrent_mark`, and the termination
/// counter + join cost recorded by `reseed_runtime_and_remembered_roots`/
/// `join_concurrent_mark`. Heap-level only: the per-group context-root
/// breakdown is evaluator-side and covered by
/// `eval_test::gc_concurrent_handshake_stats_populate_per_group`.
#[test]
fn concurrent_handshake_records_heap_side_phases() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A rooted spine with a vector so the Tier B vector snapshot has at
    // least one entry to count.
    let vec = heap.alloc_vector(vec![TaggedValue::fixnum(3); 4]);
    let mut list = heap.alloc_cons(vec, TaggedValue::fixnum(0));
    for i in 0..100 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    let hs = heap.handshake_stats();
    assert_eq!(hs.start_count, 1, "one concurrent start handshake ran");
    assert_eq!(hs.term_count, 1, "one termination reseed ran");
    assert!(
        hs.probe_cons_blocks >= 1,
        "the cons-base snapshot walked at least one owned block"
    );
    assert!(
        hs.probe_vector_snapshot_len >= 1,
        "the Tier B snapshot captured the allocated vector (len={})",
        hs.probe_vector_snapshot_len,
    );
    assert_eq!(
        hs.probe_mapped_remembered, 0,
        "no dump partition on a bare heap"
    );
    assert_eq!(
        hs.last_term_remembered_roots, 0,
        "termination reseed saw no remembered owners on a bare heap"
    );
    // µs fields can legitimately round to 0 on a tiny heap; the counters
    // above prove the recording points fired. The max tracks the last.
    assert!(hs.max_start_total_us >= hs.last_start_total_us);
    assert!(hs.max_term_roots_total_us >= hs.last_term_roots_total_us);
}

/// Termination-drain kind probe: a concurrent cycle over a rooted spine
/// carrying known counts of strings/records/closures/floats/hash-tables/
/// vectors must classify every parked entry into the right bucket. Each
/// value is reachable ONLY through the rooted cons spine, so the GC
/// thread's cons walk discovers it and parks it in `deferred` (vectors
/// included — Tier B traces their BACKINGS concurrently, but the vector
/// VALUE is still parked for its header mark). CONCURRENT STRING MARKING:
/// interval-FREE strings are now claimed on the GC thread instead of
/// parked, so the `str` bucket counts only the interval-BEARING ones and
/// the claim counter covers the rest.
#[test]
fn concurrent_termination_classifies_deferred_kinds() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};

    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    const N_STR: usize = 300;
    const N_STR_PROPS: usize = 40;
    const N_REC: usize = 200;
    const N_LAMBDA: usize = 150;
    const N_MACRO: usize = 30;
    const N_FLT: usize = 120;
    const N_HT: usize = 8;
    const N_VEC: usize = 50;

    let mut list = TaggedValue::fixnum(0);
    for _ in 0..N_STR {
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("drain-kind"));
        list = heap.alloc_cons(s, list);
    }
    // Interval-BEARING strings: still parked for the termination drain
    // (their interval children must be traced by `mark_value`).
    for _ in 0..N_STR_PROPS {
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("drain-props"));
        let payload = heap.alloc_cons(TaggedValue::fixnum(9), TaggedValue::fixnum(0));
        let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
        // Pre-mark direct install on a just-allocated string (unpublished
        // to any concurrent cycle yet).
        unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(payload) };
        list = heap.alloc_cons(s, list);
    }
    for i in 0..N_REC {
        let r = heap.alloc_record(vec![TaggedValue::fixnum(i as i64)]);
        list = heap.alloc_cons(r, list);
    }
    for _ in 0..N_LAMBDA {
        let c = heap.alloc_lambda(vec![TaggedValue::fixnum(1)]);
        list = heap.alloc_cons(c, list);
    }
    for _ in 0..N_MACRO {
        let m = heap.alloc_macro(vec![TaggedValue::fixnum(2)]);
        list = heap.alloc_cons(m, list);
    }
    for i in 0..N_FLT {
        let f = heap.alloc_float(i as f64);
        list = heap.alloc_cons(f, list);
    }
    for _ in 0..N_HT {
        let h = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));
        list = heap.alloc_cons(h, list);
    }
    for i in 0..N_VEC {
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(i as i64); 4]);
        list = heap.alloc_cons(v, list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    let kinds = stats.last_termination_kinds;
    assert!(
        stats.last_concurrent_str_claimed >= N_STR,
        "interval-free strings are claimed concurrently, not parked \
         (claimed={})",
        stats.last_concurrent_str_claimed,
    );
    assert!(
        kinds.string >= N_STR_PROPS,
        "interval-bearing strings stay parked (str={})",
        kinds.string,
    );
    assert!(
        kinds.string < N_STR,
        "the interval-free majority must have left the parked buffer \
         (str={})",
        kinds.string,
    );
    assert!(
        kinds.record >= N_REC,
        "records parked (rec={})",
        kinds.record
    );
    assert!(
        kinds.closure >= N_LAMBDA + N_MACRO,
        "lambdas + macros share the closure bucket (clo={})",
        kinds.closure,
    );
    // Task 01: owned young page floats are claimed on the GC thread
    // (zero children), so the float bucket collapses and the claim
    // counter carries the count instead.
    assert!(
        stats.last_concurrent_float_claimed >= N_FLT,
        "page floats are claimed concurrently, not parked (claimed={})",
        stats.last_concurrent_float_claimed,
    );
    assert_eq!(
        kinds.float, 0,
        "no float may remain parked on a bare page-only heap (f={})",
        kinds.float,
    );
    assert!(
        kinds.hash_table >= N_HT,
        "hash tables parked (ht={})",
        kinds.hash_table,
    );
    // Task 01: owned page vectors' headers are claimed on the GC thread
    // (their backings already traced concurrently via Tier B), so the
    // vector bucket collapses and the claim counter carries the count.
    assert!(
        stats.last_concurrent_vec_claimed >= N_VEC,
        "page vectors' headers are claimed concurrently, not parked \
         (claimed={})",
        stats.last_concurrent_vec_claimed,
    );
    assert_eq!(
        kinds.vector, 0,
        "no vector may remain parked on a bare page-only heap (vec={})",
        kinds.vector,
    );
    assert_eq!(
        kinds.total(),
        stats.last_termination_deferred,
        "every deferred entry lands in exactly one bucket",
    );
    assert_eq!(stats.termination_count, 1);
    assert!(stats.max_termination_kinds.string >= kinds.string);
    assert!(stats.max_termination_kinds.record >= kinds.record);
    assert!(stats.max_termination_kinds.closure >= kinds.closure);

    // Finish the cycle cleanly: termination drain + deferred sweep.
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
    assert_eq!(heap.sweep_stats().noncons_freed, 0, "everything is rooted");
}

/// Build an interval table whose sole plist value is `v` (chars [0, 1)).
/// `for_each_root` yields the plist (a heap cons chain carrying `v`), so
/// marking the table's roots transitively keeps `v` alive. Allocates the
/// plist conses on the thread-local tagged heap.
fn interval_table_carrying(v: TaggedValue) -> crate::buffer::text_props::TextPropertyTable {
    use crate::buffer::text_props::{PropertyInterval, TextPropertyTable};
    let key = TaggedValue::fixnum(1);
    let mut properties = std::collections::HashMap::new();
    properties.insert(key, v);
    TextPropertyTable::from_dump(vec![PropertyInterval {
        start: 0,
        end: 1,
        properties,
        key_order: vec![key],
    }])
}

/// Drive one full concurrent cycle to completion: wait for the GC thread,
/// terminate stop-the-world with `root` re-seeded, and drain the deferred
/// sweep. Mirrors the driver's state machine (and the other tests here).
fn finish_concurrent_cycle(heap: &mut TaggedHeap, root: TaggedValue) {
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(!heap.sweep_in_progress());
}

/// CONCURRENT STRING MARKING, load-bearing-barrier proof (production
/// path): a string S whose interval table is the ONLY reference to value V
/// has that table dropped MID-MARK through the `mutate.rs` wrapper. V must
/// survive the cycle purely via the SATB pre-image log — whichever side of
/// the clear the GC thread observed S on (non-null ⇒ deferred, then the
/// termination traces an already-empty table; null ⇒ claimed, never
/// re-traced).
#[test]
fn concurrent_string_claim_and_interval_clear_keep_children_alive() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // V (and its plist chain): reachable ONLY via S's interval table.
    let v = heap.alloc_cons(TaggedValue::fixnum(41), TaggedValue::fixnum(42));
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("props"));
    {
        let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
        // Pre-mark install on a fresh string: no barrier needed yet.
        unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(v) };
    }
    // S2: interval-free — exercises the claim fast path alongside.
    let s2 = heap.alloc_string(crate::heap_types::LispString::from_utf8("plain"));
    // Long spine so the GC thread is (almost certainly) still marking the
    // list when the mutator clears. Both correctness outcomes are asserted
    // identically, so the race direction cannot break the test.
    let mut list = heap.alloc_cons(s2, TaggedValue::fixnum(0));
    list = heap.alloc_cons(s, list);
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // Mid-mark, on the mutator thread: drop S's whole interval table via
    // the barrier wrapper (fires the StringData SATB pre-image push AND
    // the enforced in-mutator interval barrier).
    let cleared = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
    assert!(cleared.is_some());

    finish_concurrent_cycle(&mut heap, root);

    // V survived the cycle purely via SATB.
    assert_eq!(
        unsafe { (*v.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(41).0,
    );
    assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    assert!(heap.owns_non_cons_object(s2.as_string_ptr().unwrap() as *const u8));
}

/// MID-MARK-BORN STRING GAINS INTERVALS (the SATB argument at the claim
/// site, exercised end to end): a string S is allocated DURING a
/// concurrent mark (born-at-parity — the GC thread will never trace it
/// this cycle) and its freshly installed interval table becomes the ONLY
/// reference to young cons C, whose original home is overwritten
/// mid-mark. C must survive this cycle — not through S, but because the
/// overwrite of its snapshot-reachable home fired the SATB deletion
/// barrier (pre-image logged) — and the NEXT cycle must keep C alive
/// through S's interval trace (`mark_value` re-traces fresh marks).
#[test]
fn concurrent_mark_born_string_interval_child_survives() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // C: young cons, reachable at the snapshot ONLY via home H's car.
    let c = heap.alloc_cons(TaggedValue::fixnum(71), TaggedValue::fixnum(72));
    let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
    // Long spine so the GC thread is still marking during the mutation.
    let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // Mid-mark: allocate S (page string, absent from this cycle's claim
    // snapshot only if it opened a fresh page — either way born-at-parity
    // keeps it alive), install a table carrying C, then sever C's
    // original home. The home overwrite fires the SATB pre-image barrier
    // (`set_cons_car` -> record_heap_write), which is what keeps C alive
    // this cycle; S's table is never traced this cycle.
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("mid-mark"));
    let installed = crate::tagged::mutate::with_string_text_props_mut(s, |t| {
        *t = interval_table_carrying(c);
    });
    assert!(installed.is_some());
    assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

    // Terminate with S re-seeded alongside the spine (S is a live value
    // the mutator holds; the explicit-roots harness must name it).
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.seed_root(s);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // C survived its birth-cycle severing (SATB), S survived (born-at-
    // parity), and C's payload is intact.
    assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(71).0,
    );

    // NEXT full cycle: C is now reachable ONLY through S's intervals —
    // the termination's `mark_value` must trace them (S is white again
    // at the new parity, so it cannot be skipped as already-marked, and
    // its non-null interval word defers it to the STW trace).
    heap.concurrent_begin();
    heap.seed_root(root);
    heap.seed_root(s);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.seed_root(s);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(71).0,
    );
}

/// Same as above, but the mid-mark clear BYPASSES the `mutate.rs` wrappers
/// entirely (raw `clear_intervals` on the payload) — proving the SATB
/// barrier is enforced INSIDE the `LispString` mutators and cannot be
/// skipped by any call site.
#[test]
fn concurrent_string_raw_interval_clear_keeps_children_alive() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let v = heap.alloc_cons(TaggedValue::fixnum(51), TaggedValue::fixnum(52));
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("raw-clear"));
    let s_ptr = s.as_string_ptr().unwrap() as *mut StringObj;
    unsafe { *(*s_ptr).data.intervals_mut() = interval_table_carrying(v) };
    let mut list = heap.alloc_cons(s, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // Raw mutator call — no wrapper, no note_heap_write. The enforced
    // in-mutator barrier inside clear_intervals must log V's plist.
    unsafe { (*s_ptr).data.clear_intervals() };

    finish_concurrent_cycle(&mut heap, root);

    assert_eq!(
        unsafe { (*v.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(51).0,
    );
    assert!(heap.owns_non_cons_object(s_ptr as *const u8));
}

/// The claim + clear flow under the ARMED partition/tricolor verifiers
/// (`NEOVM_GC_VERIFY_PARTITION=1`): `verify_incremental_tricolor` is the
/// oracle that a concurrently-claimed (black) string presents no
/// black->white edge at termination. The fake dump span only activates the
/// partition; it maps no objects, so every string stays span-outside
/// (owned, claim-eligible).
#[test]
fn concurrent_string_claim_passes_partition_verifier() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16);

    // First partitioned cycle promotes + blackens; verifiers arm after it.
    heap.begin_collection();
    heap.complete_collection();
    assert!(heap.dump_blackened);

    let v = heap.alloc_cons(TaggedValue::fixnum(61), TaggedValue::fixnum(62));
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("verified"));
    let s_ptr = s.as_string_ptr().unwrap() as *mut StringObj;
    unsafe { *(*s_ptr).data.intervals_mut() = interval_table_carrying(v) };
    let s2 = heap.alloc_string(crate::heap_types::LispString::from_utf8("verified-free"));
    let mut list = heap.alloc_cons(s2, TaggedValue::fixnum(0));
    list = heap.alloc_cons(s, list);
    for i in 0..200_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    let _ = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
    // `incremental_finish` (inside) runs verify_dump_partition +
    // verify_incremental_tricolor and panics on any violation.
    finish_concurrent_cycle(&mut heap, root);

    assert_eq!(
        unsafe { (*v.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(61).0,
    );
    assert!(heap.owns_non_cons_object(s_ptr as *const u8));
    assert!(heap.owns_non_cons_object(s2.as_string_ptr().unwrap() as *const u8));
}

/// MAPPED-STRING CLASSIFICATION (regression guard for the mis-claim UAF):
/// with the partition span covering a registered mapped string, the GC
/// thread must DEFER it (its `GcHeader` bit untouched — mapped strings
/// mark via the `MappedStringObject` side bool) and the termination must
/// mark it on the mapped path and trace its interval child.
#[test]
fn concurrent_mark_defers_mapped_strings_and_marks_their_interval_children() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Fake-mapped string: a leaked StringObj registered exactly like the
    // pdump loader registers image objects (extends the dump span over it).
    let mapped = Box::into_raw(Box::new(StringObj {
        header: GcHeader::new(HeapObjectKind::String),
        data: crate::heap_types::LispString::from_utf8("mapped"),
    }));
    unsafe { heap.register_mapped_string_object(mapped, std::mem::size_of::<StringObj>()) };
    // C: heap value reachable ONLY via the mapped string's interval table.
    let c = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::fixnum(8));
    unsafe { *(*mapped).data.intervals_mut() = interval_table_carrying(c) };
    let mapped_val = unsafe { TaggedValue::from_string_ptr(mapped) };
    let root = heap.alloc_cons(mapped_val, TaggedValue::fixnum(0));

    // First cycle with a partition is a full trace (dump not blackened):
    // mapped marks were cleared, so the termination must re-mark the
    // mapped string and trace its intervals.
    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.string >= 1,
        "the mapped string must be parked, not claimed (str={})",
        stats.last_termination_kinds.string,
    );
    assert_eq!(
        stats.last_concurrent_str_claimed, 0,
        "nothing here is claim-eligible",
    );
    // Parity-aware form + raw pinning: a wrongful claim would swap in the
    // CURRENT parity (true here — exactly one begin_collection flip has
    // run), so assert both that the bit reads unmarked at this cycle's
    // parity and that the raw bit is still the untouched `false` it was
    // born with.
    assert!(
        unsafe { !(*mapped).header.is_marked_at(heap.mark_parity) },
        "a mapped string's GcHeader bit must never be claimed by the GC \
         thread (mapped marks live in the side table)",
    );
    assert!(
        unsafe { !(*mapped).header.is_marked() },
        "the mapped string's raw GcHeader bit must be untouched",
    );

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // Termination marked it on the mapped path and traced the child.
    let idx = heap.mapped_string_index_by_addr[&(mapped as usize)];
    assert!(heap.mapped_string_objects[idx].marked);
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(7).0,
    );

    // Free the fake image object after the heap is gone.
    drop(heap);
    let _ = unsafe { Box::from_raw(mapped) };
}

/// Build a leaked-static `SubrObj` exactly like the production
/// constructor (`allocate_static_subr_object` `Box::leak`s and never
/// registers with any heap list), returning the veclike value + raw ptr.
fn leaked_test_subr() -> (TaggedValue, *mut crate::tagged::header::SubrObj) {
    let obj = Box::new(crate::tagged::header::SubrObj {
        header: VecLikeHeader::new(VecLikeType::Subr),
        sym_id: crate::emacs_core::intern::SymId(1),
        name: crate::emacs_core::intern::NameId(1),
        min_args: 1,
        max_args: Some(2),
        dispatch_kind: crate::tagged::header::SubrDispatchKind::Builtin,
        interactivity: crate::tagged::header::SubrInteractivity::NonInteractive,
        function: None,
    });
    let ptr = Box::into_raw(obj);
    let val = unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) };
    (val, ptr)
}

/// Task 01 SUBR RECOGNIZE-AND-DROP: a leaked-static subr (the only
/// non-mapped subr population — `allocate_static_subr_object`
/// `Box::leak`s and never links) discovered by the GC thread is DROPPED
/// from the defer path: the subr bucket collapses to zero, the drop
/// counter records it, and its header — dead state nobody reads — is
/// never written. The subr stays permanently live with its payload
/// intact (`is_value_marked` unconditionally true for
/// not-owned/not-mapped).
#[test]
fn concurrent_leaked_subr_dropped_from_defer_path() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let (subr_val, subr_ptr) = leaked_test_subr();
    let root = heap.alloc_cons(subr_val, TaggedValue::fixnum(0));

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert_eq!(
        stats.last_termination_kinds.subr, 0,
        "a leaked subr must no longer park in `deferred` (sub={})",
        stats.last_termination_kinds.subr,
    );
    assert!(
        stats.last_concurrent_subr_dropped >= 1,
        "the drop must be counted (got {})",
        stats.last_concurrent_subr_dropped,
    );
    // Dead-state header: the raw bit is still the constructor's `false`
    // (a drop is NOT a claim).
    assert!(unsafe { !(*subr_ptr).header.gc.is_marked() });
    assert!(
        heap.is_value_marked(subr_val),
        "not-owned/not-mapped values answer unconditionally live",
    );

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // Subr payload intact after the full cycle (permanently live; the
    // sweep never visits it).
    unsafe {
        assert_eq!((*subr_ptr).min_args, 1);
        assert_eq!((*subr_ptr).max_args, Some(2));
        assert_eq!((*subr_ptr).header.type_tag, VecLikeType::Subr);
    }
    // Leaked on purpose, like production subrs (freeing it would U-A-F
    // the canonical registry pattern this mirrors).
}

/// Task 01 MAPPED-SUBR CLASSIFICATION (regression guard for the mis-drop
/// UAF): with the partition span covering a registered mapped subr, the
/// GC thread must DEFER it — the dump-span range check runs BEFORE the
/// leaked-static recognition, because a mapped subr's mark lives in the
/// `mapped_veclike_objects` side table that only the mutator's
/// termination may write. The termination must mark it there, and the
/// armed partition/tricolor verifiers must pass.
#[test]
fn concurrent_mapped_subr_still_deferred_and_side_table_marked() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Fake-mapped subr: registered exactly like the pdump loader
    // registers image veclikes (extends the dump span over it).
    let (subr_val, mapped) = leaked_test_subr();
    unsafe {
        heap.register_mapped_veclike_object(
            mapped as *mut VecLikeHeader,
            std::mem::size_of::<crate::tagged::header::SubrObj>(),
        )
    };
    let root = heap.alloc_cons(subr_val, TaggedValue::fixnum(0));

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.subr >= 1,
        "the mapped subr must be parked, not dropped (sub={})",
        stats.last_termination_kinds.subr,
    );
    assert_eq!(
        stats.last_concurrent_subr_dropped, 0,
        "nothing here is a leaked static",
    );
    assert!(
        unsafe { !(*mapped).header.gc.is_marked() },
        "a mapped subr's GcHeader bit must never be written by the GC \
         thread (mapped marks live in the side table)",
    );

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // The termination marked it via the mapped side table.
    let idx = heap.mapped_veclike_index_by_addr[&(mapped as usize)];
    assert!(heap.mapped_veclike_objects[idx].marked);

    // Free the fake image object after the heap is gone.
    drop(heap);
    let _ = unsafe { Box::from_raw(mapped) };
}

/// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (a): a page vector reachable
/// only via a rooted cons is claimed on the GC thread (header black at
/// parity, vec bucket empty, claim counter hot), its children survive
/// through the Tier-B backing scan, and a garbage vector plus its
/// otherwise-unreachable child are collected by the cycle's sweep.
#[test]
fn concurrent_vector_header_claimed_children_survive_garbage_freed() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Live: root cons -> V[c, s]; c and s are reachable ONLY through V.
    let c = heap.alloc_cons(TaggedValue::fixnum(11), TaggedValue::fixnum(12));
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-kid"));
    let v = heap.alloc_vector(vec![c, s]);
    let root = heap.alloc_cons(v, TaggedValue::fixnum(0));
    // Garbage: G[cg] with no inbound edge.
    let cg = heap.alloc_cons(TaggedValue::fixnum(13), TaggedValue::fixnum(14));
    let g = heap.alloc_vector(vec![cg]);
    let g_ptr = g.as_veclike_ptr().unwrap();

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_concurrent_vec_claimed >= 1,
        "the rooted page vector's header must be claimed on the GC \
         thread (claimed={})",
        stats.last_concurrent_vec_claimed,
    );
    assert_eq!(
        stats.last_termination_kinds.vector, 0,
        "no vector may park on a bare page-only heap (vec={})",
        stats.last_termination_kinds.vector,
    );
    // Claimed ≡ black at THIS cycle's parity.
    assert!(unsafe {
        (*v.as_veclike_ptr().unwrap())
            .gc
            .is_marked_at(heap.mark_parity)
    });

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // Children survived via the Tier-B backing scan (the claimed header
    // was never re-traced at termination).
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(11).0,
    );
    assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
    // The garbage vector (and with it its only reference to cg) is gone.
    assert!(
        !heap.owns_non_cons_object(g_ptr as *const u8),
        "the unrooted vector must be reclaimed",
    );
    heap.assert_object_arenas_coherent();
}

/// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (b), THE ADVERSARIAL ONE: a
/// vector allocated MID-CYCLE into a REUSED SLOT of an
/// already-snapshotted page (page-base HIT — it does NOT defer) holds
/// the only surviving reference to child C after C's snapshot home is
/// severed. C must survive: not through the vector (born-at-parity ⇒
/// the claim arm treats it as already-marked ⇒ never traced this cycle;
/// its backing is absent from the Tier-B snapshot) but through the SATB
/// deletion barrier on the home overwrite. Runs with the partition +
/// tricolor verifiers armed — `verify_incremental_tricolor` is the
/// oracle for the removed termination re-trace backstop.
#[test]
fn concurrent_mid_cycle_vector_in_reused_slot_keeps_child_alive() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16); // activates the partition

    // Page setup: keeper pins the page; v_dead's slot becomes the free
    // slot the mid-cycle allocation will reuse.
    let v_keep = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
    let v_dead = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
    let dead_ptr = v_dead.as_veclike_ptr().unwrap() as usize;
    // C: young cons, reachable at the snapshot ONLY via home H's car.
    let c = heap.alloc_cons(TaggedValue::fixnum(81), TaggedValue::fixnum(82));
    let home = heap.alloc_cons(c, TaggedValue::fixnum(0));
    // Long rooted spine (home at the bottom) so the GC thread is still
    // walking when the mutator severs; both race outcomes are asserted
    // identically (if the GC got to H first, C is simply already black).
    let mut list = heap.alloc_cons(home, TaggedValue::fixnum(0));
    list = heap.alloc_cons(v_keep, list);
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;
    // Bootstrap STW cycle: blackens the fake dump (arming the
    // verifiers), promotes survivors, and frees v_dead's slot.
    heap.collect_exact(std::iter::once(root));
    let pre_launch_bases: std::collections::HashSet<usize> = heap
        .vector_arena
        .pages
        .iter()
        .map(|p| p.base_addr())
        .collect();

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-CYCLE: allocate V_NEW carrying C — the arena's class free list
    // hands back v_dead's slot (page-base in this cycle's snapshot) —
    // then sever C's original home (fires the SATB pre-image barrier).
    let v_new = heap.alloc_vector(vec![c]);
    let new_ptr = v_new.as_veclike_ptr().unwrap() as usize;
    assert_eq!(
        new_ptr, dead_ptr,
        "the mid-cycle vector must land in the freed slot of a \
         snapshotted page (allocator changed? fix the test setup)",
    );
    assert!(
        pre_launch_bases.contains(&(new_ptr & !(OBJECT_PAGE_ALIGN - 1))),
        "the reused slot's page must be in this cycle's snapshot",
    );
    assert!(crate::tagged::mutate::set_cons_car(home, TaggedValue::NIL));

    // Terminate with v_new re-seeded alongside the spine (it is a live
    // value the mutator holds; the explicit-roots harness must name it).
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.seed_root(v_new);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    // Runs verify_dump_partition + verify_incremental_tricolor (armed
    // above): a black v_new with a white C would panic here.
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // C survived its birth-cycle severing (SATB), with payload intact.
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(81).0,
    );
    assert!(heap.owns_non_cons_object(v_new.as_veclike_ptr().unwrap() as *const u8));

    // NEXT full cycle: C is now reachable ONLY through V_NEW's backing —
    // the fresh Tier-B snapshot must carry it.
    heap.concurrent_begin();
    heap.seed_root(root);
    heap.seed_root(v_new);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    heap.seed_root(v_new);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(81).0,
    );
}

/// Task 01 CONCURRENT VECTOR-HEADER CLAIMS (c): a vector whose backing
/// is BULK-MUTATED mid-mark (`with_vector_data_mut` clone-on-write)
/// while its header was claimed. Old-backing children survive via the
/// retire (the Tier-B snapshot keeps reading the retired original);
/// the new contents survive via SATB/born-black. Both race directions
/// (claim before/after the mutation) assert identically.
#[test]
fn concurrent_vector_bulk_cow_while_header_claimed() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // C_OLD is reachable ONLY through V's start-of-cycle backing.
    let c_old = heap.alloc_cons(TaggedValue::fixnum(91), TaggedValue::fixnum(92));
    let v = heap.alloc_vector(vec![c_old]);
    let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-MARK bulk mutation through the production wrapper: replaces
    // the whole backing (clone-on-write retires the original the GC's
    // snapshot points at) and grows it (realloc — the historical TOCTOU
    // shape). C_OLD's only reference is now the retired buffer.
    let c_new = heap.alloc_cons(TaggedValue::fixnum(93), TaggedValue::fixnum(94));
    let mutated = crate::tagged::mutate::with_vector_data_mut(v, |d| {
        d.clear();
        d.push(c_new);
        for i in 0..64 {
            d.push(TaggedValue::fixnum(i));
        }
    });
    assert!(mutated.is_some());

    finish_concurrent_cycle(&mut heap, root);

    let stats = heap.sweep_stats();
    assert!(
        stats.last_concurrent_vec_claimed >= 1,
        "V's header claim races the mutation but must land either way \
         (claimed={})",
        stats.last_concurrent_vec_claimed,
    );
    // Old-backing child survived via the retired buffer's Tier-B scan
    // (+ the VectorBulk SATB pre-image log).
    assert_eq!(
        unsafe { (*c_old.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(91).0,
    );
    // New content survived (allocate-black + live backing).
    assert_eq!(
        unsafe { (*c_new.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(93).0,
    );
    assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
}

/// Task 01 INSERTION-COVERAGE (the regression the vm_mapatoms SIGSEGV
/// exposed): a pre-existing value held only "in a register" (a Rust
/// local the explicit-roots harness does not seed — root→heap motion)
/// is stored mid-cycle into an already-CLAIMED vector's slot. The SATB
/// deletion barrier only logs pre-images and the claimed header
/// suppresses the termination re-trace, so ONLY the dirty-owner
/// insertion re-trace at `join_concurrent_mark` keeps the value alive.
#[test]
fn concurrent_vector_slot_insertion_of_inflight_value_survives() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // X: allocated BEFORE the cycle, never seeded as a root this cycle —
    // an in-flight register value.
    let x = heap.alloc_float(6.5);
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
    let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-CYCLE: root→heap motion into the (likely already claimed)
    // vector through the production barrier path.
    assert!(crate::tagged::mutate::set_vector_slot(v, 0, x));

    finish_concurrent_cycle(&mut heap, root);

    assert!(
        heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
        "the inserted in-flight value must survive via the dirty-owner \
         insertion re-trace",
    );
    assert!((x.xfloat() - 6.5).abs() < f64::EPSILON);
    // V's slot still reads X (no dangling slot).
    let slot0 = unsafe {
        (*(v.as_veclike_ptr().unwrap() as *const VectorObj))
            .data
            .load_atomic(0)
    };
    assert_eq!(slot0.0, x.0);
}

/// Same insertion-coverage regression through the BULK path: the value
/// is pushed into the claimed vector via `with_vector_data_mut`
/// (clone-on-write) — the post-mutation backing is only reachable
/// through the dirty-owner re-trace.
#[test]
fn concurrent_vector_bulk_insertion_of_inflight_value_survives() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let x = heap.alloc_float(7.5);
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
    let mut list = heap.alloc_cons(v, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    let mutated = crate::tagged::mutate::with_vector_data_mut(v, |d| {
        d.push(x);
    });
    assert!(mutated.is_some());

    finish_concurrent_cycle(&mut heap, root);

    assert!(
        heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
        "the bulk-inserted in-flight value must survive via the \
         dirty-owner insertion re-trace",
    );
    assert!((x.xfloat() - 7.5).abs() < f64::EPSILON);
}

/// #17 — CONS INTERIOR under concurrent marking. A value `x` reachable at
/// the snapshot ONLY through a pre-existing cons `p` is re-homed MID-MARK
/// into a FRESH (born-black) cons `c` and unlinked from `p`, both via the
/// production `mutate::set_cons_*` deletion barriers. `x` must survive: it
/// was snapshot-reachable (grayed when `p` is traced, or logged by the cons
/// deletion barrier on the unlink race), and the born-black `c` is merely
/// another reference to the already-protected value.
///
/// Deliberate asymmetry vs the vector insertion tests above: conses are
/// EXCLUDED from the dirty-owner re-gray (`satb_snapshotted_owners`, see
/// `record_heap_write`), so a fresh cons has NO fix-(2) insertion net. Cons
/// interiors are sound purely by SATB provenance — the value MUST be
/// snapshot-reachable (precise rooting). An UNSEEDED value laundered through
/// a fresh cons is CORRECTLY swept (a root-discipline violation the STW
/// collector mishandles at the same safe point too), so this test keeps `x`
/// snapshot-reachable. See CONCURRENT_GC.md, "Insertion coverage".
#[test]
fn concurrent_fresh_cons_interior_of_snapshot_value_survives() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // x reachable at the snapshot ONLY via p.car; p is buried in the seeded
    // list so it is traced late (the deletion barrier is the race net).
    let x = heap.alloc_float(6.5);
    let p = heap.alloc_cons(x, TaggedValue::fixnum(0));
    let mut list = heap.alloc_cons(p, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-CYCLE: re-home x into a FRESH born-black cons reachable from the
    // seeded root (p.cdr), then UNLINK x from p.car — both barriered.
    let c = heap.alloc_cons(x, TaggedValue::fixnum(0));
    assert!(crate::tagged::mutate::set_cons_cdr(p, c));
    assert!(crate::tagged::mutate::set_cons_car(
        p,
        TaggedValue::fixnum(99)
    ));

    finish_concurrent_cycle(&mut heap, root);

    assert!(
        heap.owns_non_cons_object(x.as_float_ptr().unwrap() as *const u8),
        "a snapshot-reachable value re-homed into a fresh born-black cons \
         must survive (SATB provenance; conses have no dirty-owner net)",
    );
    assert!((x.xfloat() - 6.5).abs() < f64::EPSILON);
}

/// #18 — MODULE-FUNCTION `interactive_form` barrier. A value V reachable
/// ONLY through a live `ModuleFunctionObj.interactive_form` slot is
/// overwritten MID-MARK (as `module_make_interactive` does), preceded by the
/// `note_heap_write(ModuleFunction)` SATB barrier the write site now fires.
/// V must survive purely via the barrier's pre-image log: the object is
/// Box-allocated ⇒ deferred ⇒ traced at STW on its CURRENT (overwritten)
/// form, so the barrier is V's ONLY net. Guards the barrier + `ModuleFunction`
/// coverage in `collect_veclike_children` (drop either ⇒ V is swept).
#[test]
fn concurrent_module_function_interactive_form_overwrite_keeps_child_alive() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // V reachable ONLY via mf.interactive_form.
    let v = heap.alloc_float(9.5);
    let mf = heap.alloc_module_function(
        0,
        0,
        std::ptr::null(),
        std::ptr::null_mut(),
        TaggedValue::fixnum(0),
        v,
    );
    // Bury mf in a seeded list so the mark takes real time.
    let mut list = heap.alloc_cons(mf, TaggedValue::fixnum(0));
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // MID-CYCLE: overwrite interactive_form (unlinking V) exactly as
    // module_make_interactive does — SATB barrier BEFORE the raw store.
    note_heap_write(mf, HeapWriteKind::ModuleFunction);
    unsafe {
        let mf_ptr = mf.as_veclike_ptr().unwrap() as *mut ModuleFunctionObj;
        (*mf_ptr).interactive_form = TaggedValue::fixnum(99);
    }

    finish_concurrent_cycle(&mut heap, root);

    assert!(
        heap.owns_non_cons_object(v.as_float_ptr().unwrap() as *const u8),
        "the overwritten interactive_form value must survive via the SATB \
         pre-image barrier (module-function objects are Box-deferred and \
         traced at STW on their CURRENT form only)",
    );
    assert!((v.xfloat() - 9.5).abs() < f64::EPSILON);
}

/// Task 01 MAPPED-VECTOR CLASSIFICATION (d): a registered mapped vector
/// page-MISSES the claim arm and keeps the STW defer path — the
/// termination marks it via the mapped side table AND re-traces its
/// CURRENT backing (`trace_veclike`), keeping its child alive; its
/// `GcHeader` bit is never written by the GC thread. (Box-residual
/// vectors are NOT constructible today — `alloc_vector` is the single
/// Vector chokepoint and the pdump restore writes into mapped storage —
/// so the Box population has no test; a miss would merely defer.)
#[test]
fn concurrent_mapped_vector_still_deferred_and_traced() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // C: heap value reachable ONLY via the mapped vector's slot.
    let c = heap.alloc_cons(TaggedValue::fixnum(21), TaggedValue::fixnum(22));
    let mapped = Box::into_raw(Box::new(VectorObj {
        header: VecLikeHeader::new(VecLikeType::Vector),
        data: vec![c].into(),
    }));
    unsafe {
        heap.register_mapped_veclike_object(
            mapped as *mut VecLikeHeader,
            std::mem::size_of::<VectorObj>(),
        )
    };
    let mapped_val = unsafe { TaggedValue::from_veclike_ptr(mapped as *const VecLikeHeader) };
    let root = heap.alloc_cons(mapped_val, TaggedValue::fixnum(0));

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_termination_kinds.vector >= 1,
        "the mapped vector must be parked, not claimed (vec={})",
        stats.last_termination_kinds.vector,
    );
    assert_eq!(
        stats.last_concurrent_vec_claimed, 0,
        "nothing here is a page vector",
    );
    assert!(
        unsafe { !(*mapped).header.gc.is_marked() },
        "a mapped vector's GcHeader bit must never be written by the GC \
         thread (mapped marks live in the side table)",
    );

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // Termination marked it on the mapped path and traced its child.
    let idx = heap.mapped_veclike_index_by_addr[&(mapped as usize)];
    assert!(heap.mapped_veclike_objects[idx].marked);
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(21).0,
    );

    // Free the fake image object after the heap is gone.
    drop(heap);
    let _ = unsafe { Box::from_raw(mapped) };
}

/// RACE TEST: the mutator flips strings' interval tables None<->Some in a
/// loop (through the production wrappers) while the GC thread marks a
/// large spine. Liveness: every flipped-in value and every string must
/// survive; run under a data-race detector this is the strings race check
/// (the seqlock test is the precedent).
#[test]
fn concurrent_mark_races_interval_flips_and_retains_live_set() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    const N_STR: usize = 512;
    let mut strings = Vec::with_capacity(N_STR);
    let mut values = Vec::with_capacity(N_STR);
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N_STR {
        let v = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::fixnum(-1));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("flip"));
        let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
        unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(v) };
        strings.push(s);
        values.push(v);
        list = heap.alloc_cons(s, list);
    }
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // Mutator: clear + reinstall every string's table, twice, while the
    // GC thread walks the spine and claims/defers the strings.
    for round in 0..2 {
        for (i, s) in strings.iter().enumerate() {
            let _ = crate::tagged::mutate::with_lisp_string_mut(*s, |ls| ls.clear_intervals());
            if round == 0 || i % 2 == 0 {
                let table = interval_table_carrying(values[i]);
                let _ = crate::tagged::mutate::with_string_text_props_mut(*s, |t| *t = table);
            }
        }
    }

    finish_concurrent_cycle(&mut heap, root);

    for (i, v) in values.iter().enumerate() {
        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(i as i64).0,
            "flipped-in interval value #{i} must survive",
        );
    }
    for s in &strings {
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    }
}

/// TSan ADVERSARIAL (task 11): the widest write/claim overlap in one test.
/// The mutator (this thread) hammers, across many strings and vectors:
///   * remove-text-properties — `clear_intervals` swaps the `intervals`
///     `AtomicPtr` to null and frees the old table;
///   * put-text-property — `with_string_text_props_mut` -> `ensure_intervals`
///     Release-stores a freshly-allocated table into the same `AtomicPtr`;
///   * vector `aset` — `set_vector_slot` does an atomic slot store + notes
///     the remembered set,
/// while the GC thread concurrently marks a large cons spine and CLAIMS
/// floats/strings/vectors through the 2026-07 concurrent claim dispatcher
/// (parity mark bits + `mark_claim_at`). This is the exact overlap the new
/// machinery must survive with zero data races: the `intervals` AtomicPtr
/// store/swap vs. the GC's `intervals_ptr` word read, the SATB pre-image
/// Mutex log vs. the GC drain, and the atomic vector-slot store vs. the GC
/// Tier B backing scan. Under `-Zsanitizer=thread` this is a race check; the
/// liveness asserts confirm the last-installed children survive uncorrupted.
#[test]
fn concurrent_mark_races_textprop_churn_and_aset_with_claiming() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    const N: usize = 256;
    const ROUNDS: i64 = 4;
    let mut strings = Vec::with_capacity(N);
    let mut vectors = Vec::with_capacity(N);
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N {
        // A value reachable ONLY through the string's initial interval table.
        let born = heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::fixnum(-1));
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("adv"));
        unsafe {
            *(*(s.as_string_ptr().unwrap() as *mut StringObj))
                .data
                .intervals_mut() = interval_table_carrying(born);
        }
        // A vector with a placeholder slot we will `aset` in-flight values into.
        let vec = heap.alloc_vector(vec![TaggedValue::fixnum(0)]);
        // Root both via the spine so they MUST survive the cycle.
        list = heap.alloc_cons(s, list);
        list = heap.alloc_cons(vec, list);
        strings.push(s);
        vectors.push(vec);
    }
    // Large filler spine so the GC thread is still marking during the churn.
    for i in 0..300_000 {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();

    // Mutator: hammer put/remove text-property + ensure_intervals churn +
    // vector aset while the GC thread claims/defers. Track the LAST value
    // installed into each sink so the liveness asserts are exact.
    let mut last_prop = vec![TaggedValue::fixnum(0); N];
    let mut last_slot = vec![TaggedValue::fixnum(0); N];
    for round in 0..ROUNDS {
        for i in 0..N {
            let s = strings[i];
            let vec = vectors[i];
            // remove-text-properties: drop the whole table (AtomicPtr swap).
            let _ = crate::tagged::mutate::with_lisp_string_mut(s, |ls| ls.clear_intervals());
            // put-text-property: reinstall a fresh table (ensure_intervals
            // AtomicPtr store) carrying a fresh in-flight child value.
            let prop_v = heap.alloc_cons(
                TaggedValue::fixnum(round * N as i64 + i as i64),
                TaggedValue::fixnum(-2),
            );
            let table = interval_table_carrying(prop_v);
            let _ = crate::tagged::mutate::with_string_text_props_mut(s, |t| *t = table);
            last_prop[i] = prop_v;
            // vector aset of a fresh in-flight value (atomic slot store).
            let slot_v = heap.alloc_cons(
                TaggedValue::fixnum(1_000_000 + round * N as i64 + i as i64),
                TaggedValue::fixnum(-3),
            );
            crate::tagged::mutate::set_vector_slot(vec, 0, slot_v);
            last_slot[i] = slot_v;
        }
    }

    finish_concurrent_cycle(&mut heap, root);

    // Every rooted string + vector survived the concurrent cycle.
    for s in &strings {
        assert!(heap.owns_non_cons_object(s.as_string_ptr().unwrap() as *const u8));
    }
    for v in &vectors {
        assert!(heap.owns_non_cons_object(v.as_veclike_ptr().unwrap() as *const u8));
    }
    // The last-installed interval child + last-`aset` slot child of each sink
    // are reachable from the re-seeded root at termination, so they survive
    // uncorrupted (a swept or torn child reads back the wrong car here).
    for (i, v) in last_prop.iter().enumerate() {
        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum((ROUNDS - 1) * N as i64 + i as i64).0,
            "final interval child of string #{i} must survive uncorrupted",
        );
    }
    for (i, v) in last_slot.iter().enumerate() {
        assert_eq!(
            unsafe { (*v.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(1_000_000 + (ROUNDS - 1) * N as i64 + i as i64).0,
            "final aset slot child of vector #{i} must survive uncorrupted",
        );
    }
}

/// CLAIM-AT-ALL-SINKS (vector sink): strings reachable ONLY through a
/// vector's slots are discovered by the Tier B backing scan on the GC
/// thread; the interval-free one must be claimed there (claim counter),
/// the interval-bearing one parked (str bucket) and its child traced.
#[test]
fn concurrent_claim_reaches_vector_slot_strings() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let s_free = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-free"));
    let c = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(4));
    let s_props = heap.alloc_string(crate::heap_types::LispString::from_utf8("vec-props"));
    unsafe {
        *(*(s_props.as_string_ptr().unwrap() as *mut StringObj))
            .data
            .intervals_mut() = interval_table_carrying(c)
    };
    let vec = heap.alloc_vector(vec![s_free, s_props]);
    let root = heap.alloc_cons(vec, TaggedValue::fixnum(0));

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();

    let stats = heap.sweep_stats();
    assert!(
        stats.last_concurrent_str_claimed >= 1,
        "the interval-free vector-slot string must be claimed on the GC \
         thread (claimed={})",
        stats.last_concurrent_str_claimed,
    );
    assert!(
        stats.last_termination_kinds.string >= 1,
        "the interval-bearing vector-slot string must be parked (str={})",
        stats.last_termination_kinds.string,
    );

    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    assert!(heap.owns_non_cons_object(s_free.as_string_ptr().unwrap() as *const u8));
    assert!(heap.owns_non_cons_object(s_props.as_string_ptr().unwrap() as *const u8));
    assert_eq!(
        unsafe { (*c.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(3).0,
    );
}

/// CLAIM-AT-ALL-SINKS (obarray sink): a string reachable ONLY through an
/// obarray symbol's value cell is discovered by the Stage 1b symbol-cell
/// scan on the GC thread and must be claimed there.
#[test]
fn concurrent_claim_reaches_obarray_symbol_value_strings() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    set_tagged_heap(&mut ev.tagged_heap);

    // Interval-free string reachable ONLY via the symbol value cell.
    let s = ev
        .tagged_heap
        .alloc_string(crate::heap_types::LispString::from_utf8("obarray-only"));
    ev.obarray.set_symbol_value("neovm--str-claim-probe", s);

    // Stage the obarray snapshot exactly like the start handshake does.
    let snap = ev.obarray.scan_snapshot();
    ev.tagged_heap.set_pending_obarray_scan(snap);
    ev.tagged_heap.concurrent_begin();
    ev.tagged_heap.launch_concurrent_mark();
    while !ev.tagged_heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    ev.tagged_heap.join_concurrent_mark();

    let stats = ev.tagged_heap.sweep_stats();
    assert!(
        stats.last_concurrent_str_claimed >= 1,
        "the obarray-value string must be claimed via the symbol-cell scan \
         (claimed={})",
        stats.last_concurrent_str_claimed,
    );
    // The claimed string is black — at THIS cycle's parity (the raw bit
    // value alone is meaningless under parity marks).
    assert!(unsafe {
        (*(s.as_string_ptr().unwrap()))
            .header
            .is_marked_at(ev.tagged_heap.mark_parity)
    });
    // No sweep here: this bare-heap driver does not re-seed the Context
    // roots at termination, so sweeping would free live Context objects.
    // Claim + mark are the assertions under test (survival-under-sweep is
    // covered by the vector-sink test); the heap frees everything at drop.
}

/// Gap 3: a dump-less heap enables the concurrent collector after its
/// first completed STW collection (the bootstrap), and a full concurrent
/// cycle on such a heap retains the rooted live set and reclaims garbage
/// (mirrors `collect_exact_retains_rooted_and_frees_unrooted`). The dump
/// span is empty (`dump_addr_lo/hi` = MAX/0), so the GC thread's dump
/// check must never match and the remembered-set seeding must no-op.
#[test]
fn dumpless_heap_enables_concurrent_after_bootstrap_and_collects() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Fresh dump-less heap: the first collection must be the STW bootstrap.
    assert!(!heap.should_run_concurrent());

    const N: i64 = 10_000;
    // Rooted list: rooted_head -> cons(N-1) -> ... -> cons(0) -> fixnum(0).
    let mut rooted = TaggedValue::fixnum(0);
    for i in 0..N {
        rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
    }
    let rooted_head = rooted;
    heap.collect_exact(std::iter::once(rooted_head));
    assert!(
        heap.should_run_concurrent(),
        "the completed STW bootstrap must enable concurrent marking"
    );

    // Allocation churn after the bootstrap: garbage for the concurrent
    // cycle to reclaim.
    let mut unrooted = TaggedValue::fixnum(0);
    for i in 0..N {
        unrooted = heap.alloc_cons(TaggedValue::fixnum(1_000_000 + i), unrooted);
    }
    let _unrooted_head = unrooted;
    let before = heap.cons_live_count;

    // One full concurrent cycle, mirroring the driver's state machine:
    // start handshake -> GC thread marks -> STW termination -> deferred
    // sweep drained.
    heap.concurrent_begin();
    heap.seed_root(rooted_head);
    heap.launch_concurrent_mark();
    assert!(heap.concurrent_mark_running());
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(rooted_head);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // The unrooted churn was reclaimed...
    let after = heap.cons_live_count;
    assert!(
        after < before,
        "the concurrent cycle must reclaim garbage (before={before}, after={after})",
    );
    // ...and the rooted spine survives, fully readable.
    let mut node = rooted_head;
    let mut count = 0i64;
    while node.is_cons() {
        let car = unsafe { (*node.xcons_ptr()).load_car() };
        assert_eq!(
            car.0,
            TaggedValue::fixnum(N - 1 - count).0,
            "rooted car intact at index {count}",
        );
        node = unsafe { (*node.xcons_ptr()).load_cdr() };
        count += 1;
    }
    assert_eq!(
        count, N,
        "the whole rooted list survived the concurrent cycle"
    );
}

/// Drive one full concurrent cycle re-seeding SEVERAL roots at the
/// termination (the single-root `finish_concurrent_cycle` generalized).
/// Parity tests use this because single-cycle tests are structurally
/// blind: cycle 1 behaves like the pre-parity collector by construction,
/// so every parity property is asserted across at least TWO cycles.
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

/// PARITY MARK BITS (a): two-cycle survival, allocate-black variant. A
/// non-cons object allocated DURING a concurrent mark is born at the
/// cycle parity (allocate-black) and must survive THAT cycle's sweep
/// unrooted; re-seeded the next cycle (opposite parity) it must be traced
/// as unmarked and survive again. The cycle-2 (parity=false) allocation
/// is the regression for the literal `set_marked(true)` allocate-black,
/// which would read as WHITE on a false-parity cycle and be swept while
/// live (UAF).
#[test]
fn parity_allocate_black_object_survives_two_cycles() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // STW bootstrap (flip #1: parity false -> true) enables concurrent.
    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());
    assert!(heap.mark_parity, "bootstrap flip must yield parity=true");

    // Cycle 2 (flip #2: parity true -> false): allocate non-cons objects
    // MID-MARK. They are reachable only from Rust locals (not seeded), so
    // surviving this cycle's sweep proves allocate-black at parity=false.
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    assert!(!heap.mark_parity, "second flip must yield parity=false");
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(77)]);
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("mid-mark"));
    let v_ptr = v.as_veclike_ptr().unwrap() as *const u8;
    let s_ptr = s.as_string_ptr().unwrap() as *const u8;
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(spine); // v/s deliberately NOT seeded this cycle
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert!(
        heap.owns_non_cons_object(v_ptr),
        "allocate-black vector must survive the cycle it was born in",
    );
    assert!(
        heap.owns_non_cons_object(s_ptr),
        "allocate-black string must survive the cycle it was born in",
    );

    // Cycle 3 (flip #3: parity false -> true): the survivors' bits hold
    // the OLD parity, so they must read unmarked, be traced via their
    // seeds, and survive this cycle's sweep too.
    run_concurrent_cycle(&mut heap, &[spine, v, s]);
    assert!(heap.owns_non_cons_object(v_ptr));
    assert!(heap.owns_non_cons_object(s_ptr));
    let slot = unsafe { (*(v_ptr as *const VectorObj)).data.load_atomic(0) };
    assert_eq!(slot.0, TaggedValue::fixnum(77).0, "vector payload intact");
    assert_eq!(
        unsafe { (*(s_ptr as *const StringObj)).data.as_bytes() },
        b"mid-mark",
        "string payload intact",
    );
}

/// PARITY MARK BITS (b): two-cycle reclaim. Garbage born between cycles
/// is freed by the very next cycle; garbage born DURING a mark
/// (allocate-black) floats through that cycle and is freed by the one
/// after — "freed by cycle 2 at the latest", with the deferred sweep
/// completing between cycles (a parity flip mid-sweep is forbidden).
#[test]
fn parity_reclaims_garbage_within_two_cycles() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let mut spine = TaggedValue::fixnum(0);
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine));
    assert!(heap.should_run_concurrent());

    // G1: born BETWEEN cycles (idle) — never seeded, no allocate-black.
    let g1 = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
    let g1_ptr = g1.as_veclike_ptr().unwrap() as *const u8;

    // Cycle 2: G2 born MID-MARK (allocate-black at this cycle's parity).
    heap.concurrent_begin();
    heap.seed_root(spine);
    heap.launch_concurrent_mark();
    let g2 = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
    let g2_ptr = g2.as_veclike_ptr().unwrap() as *const u8;
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
        !heap.owns_non_cons_object(g1_ptr),
        "idle-born garbage must be reclaimed by the first cycle after its birth",
    );
    assert!(
        heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage floats through its birth cycle (allocate-black)",
    );

    // Cycle 3: G2's bit now holds the old parity — unmarked, unseeded,
    // reclaimed. (No allocations happened since the cycle-2 sweep, so the
    // ownership-set probes cannot be confused by address reuse.)
    run_concurrent_cycle(&mut heap, &[spine]);
    assert!(
        !heap.owns_non_cons_object(g2_ptr),
        "mark-born garbage must be reclaimed by the NEXT cycle",
    );
}

/// PARITY MARK BITS (c): tenured stability. After `promote_and_blacken`,
/// a tenured object's frozen mark bit must never be re-interpreted or
/// re-written: across two subsequent concurrent cycles (one at each
/// parity) the raw bit stays exactly as frozen (a re-trace would have
/// stored the flipped cycle's parity into it), the object stays owned,
/// its young child stays live via the remembered set, and the armed
/// partition + tricolor verifiers stay green (without the tenured
/// short-circuit, `is_value_marked` would read the frozen bit as WHITE on
/// the flipped cycle and panic the tricolor verifier on the black root ->
/// tenured edge).
#[test]
fn parity_tenured_objects_stay_frozen_across_cycles_under_verifier() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16); // fake span: activates the partition

    // T: a Box RECORD that will be tenured by the first partition
    // cycle's list promotion (page vectors tenure via the stage-3
    // promotion page walk and carry their own coverage). Y: a young cons
    // reachable ONLY through T (conses never tenure), so its survival on
    // later cycles proves the promotion-time remembered set, not
    // accidental re-tracing of T.
    let y = heap.alloc_cons(TaggedValue::fixnum(424_242), TaggedValue::fixnum(0));
    let t = heap.alloc_record(vec![y]);
    let root = heap.alloc_cons(t, TaggedValue::fixnum(0));

    // First partition cycle: STW full trace + sweep, then promotion.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    let t_header = t.as_veclike_ptr().unwrap();
    assert!(
        unsafe { (*t_header).gc.tenured },
        "the surviving record must have been promoted to the old generation",
    );
    let frozen_bit = unsafe { (*t_header).gc.is_marked() };

    // Two concurrent cycles — parities false then true — with the
    // verifiers armed at each termination.
    for cycle in 0..2 {
        run_concurrent_cycle(&mut heap, &[root]);
        assert!(
            heap.owns_non_cons_object(t_header as *const u8),
            "tenured record swept on post-promotion cycle {cycle}",
        );
        assert_eq!(
            unsafe { (*t_header).gc.is_marked() },
            frozen_bit,
            "tenured mark bit re-written on post-promotion cycle {cycle} \
             (a parity-blind re-trace stored into the frozen bit)",
        );
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(424_242).0,
            "young child of the tenured record lost on cycle {cycle}",
        );
    }
}

/// PARITY MARK BITS (d): the concurrent string claim works at BOTH
/// parities. The same rooted interval-free string is claimed by the GC
/// thread on two consecutive cycles: on the second one its bit holds the
/// previous cycle's parity, which a parity-blind `swap(true)` claim would
/// misread as "already marked" — the string would never be marked that
/// cycle and the sweep would free it while rooted.
#[test]
fn parity_string_claim_works_across_two_cycles() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("still-here"));
    let s_ptr = s.as_string_ptr().unwrap() as *const u8;
    let mut spine = heap.alloc_cons(s, TaggedValue::fixnum(0));
    for i in 0..100_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine)); // bootstrap

    for cycle in 0..2 {
        run_concurrent_cycle(&mut heap, &[spine]);
        assert!(
            heap.sweep_stats().last_concurrent_str_claimed >= 1,
            "cycle {cycle}: the interval-free string must be claimed on \
             the GC thread at this cycle's parity",
        );
        assert!(
            heap.owns_non_cons_object(s_ptr),
            "cycle {cycle}: claimed string swept while rooted",
        );
        assert!(
            unsafe {
                (*(s_ptr as *const StringObj))
                    .header
                    .is_marked_at(heap.mark_parity)
            },
            "cycle {cycle}: claimed string must be black at the cycle parity",
        );
    }
    assert_eq!(
        unsafe { (*(s_ptr as *const StringObj)).data.as_bytes() },
        b"still-here",
    );
}

/// PARITY MARK BITS (born-at-parity, idle window): an object allocated
/// BETWEEN cycles is born with bit == current parity, so the next flip
/// reads it as white and traces it. Born at `!parity` instead (the naive
/// "born white NOW" store), the next flip would read it as BLACK: never
/// traced, its sole-reference child swept while referenced — this test's
/// X->Y chain is exactly that UAF, asserted across two full cycles.
#[test]
fn parity_idle_born_object_is_traced_on_the_next_cycle() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let mut spine = TaggedValue::fixnum(0);
    for i in 0..10_000 {
        spine = heap.alloc_cons(TaggedValue::fixnum(i), spine);
    }
    heap.collect_exact(std::iter::once(spine)); // bootstrap (parity -> true)

    // Idle window: no mark, no sweep. Y is reachable ONLY through X.
    let y = heap.alloc_cons(TaggedValue::fixnum(31_337), TaggedValue::fixnum(0));
    let x = heap.alloc_vector(vec![y]);
    let x_ptr = x.as_veclike_ptr().unwrap() as *const u8;

    for cycle in 0..2 {
        run_concurrent_cycle(&mut heap, &[spine, x]);
        assert!(
            heap.owns_non_cons_object(x_ptr),
            "cycle {cycle}: idle-born rooted vector swept",
        );
        assert_eq!(
            unsafe { (*y.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(31_337).0,
            "cycle {cycle}: child reachable only through the idle-born \
             vector was swept (the vector was falsely black and never traced)",
        );
    }
}

/// Gap 3 drop safety: dropping a heap while the GC thread is still
/// concurrently marking it must stop + join the GC thread before any
/// storage it can read is freed (dump-less heaps now reach this state at
/// every safe-point collection after bootstrap, e.g. a test Context
/// dropped mid-mark).
#[test]
fn dropping_heap_mid_concurrent_mark_joins_gc_thread() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A long spine so the GC thread is genuinely still marking at drop.
    const N: i64 = 300_000;
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    heap.concurrent_begin();
    heap.seed_root(list);
    heap.launch_concurrent_mark();
    assert!(heap.concurrent_mark_running());
    // Drop with the mark in flight; under TSAN/ASAN a missing join is a
    // use-after-free the sanitizer catches, and the join panics if the GC
    // thread is gone.
    drop(heap);
}

/// GNU `sweep_conses` (src/alloc.c:6856-6858) threads the free list through
/// the dead cells and then writes `dead_object ()` into the car, so a cell
/// on the free list is "recognizable in O(1)" (`deadp`, src/alloc.c:425-429).
///
/// That poison is what makes a use-after-free diagnosable HERE, because the
/// free-list link lives in the cdr union and a raw `*mut ConsCell` has
/// `TAG_SYMBOL` (0b000) in its low three bits — so an unpoisoned reclaimed
/// cons reads back as the perfectly ordinary `(nil . SOME-SYMBOL)` and the
/// garbage only faults much later, in the symbol resolver
/// (DIVERGENCES.md 161).
#[test]
fn a_reclaimed_cons_is_recognizable_as_dead() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let rooted = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::fixnum(2));
    let doomed = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(4));
    assert!(!unsafe { (*doomed.xcons_ptr()).load_car() }.is_dead());

    heap.collect_exact(std::iter::once(rooted));

    assert!(
        !unsafe { (*rooted.xcons_ptr()).load_car() }.is_dead(),
        "a rooted cons must not be poisoned",
    );
    assert!(
        unsafe { (*doomed.xcons_ptr()).load_car() }.is_dead(),
        "a reclaimed cons must carry GNU's dead_object in its car, not nil: \
         without it a use-after-free is indistinguishable from live data",
    );
    // And the free-list link the cdr now holds is exactly the shape that
    // decodes as a bogus symbol: an aligned raw pointer under TAG_SYMBOL.
    let link = unsafe { (*doomed.xcons_ptr()).load_cdr() };
    assert!(
        link.is_symbol(),
        "the free-list link decodes through TAG_SYMBOL (bits 0x{:x})",
        link.bits(),
    );
}

/// The string-side twin of `a_reclaimed_cons_is_recognizable_as_dead`.
///
/// GNU `sweep_strings` (src/alloc.c:1878-1882) ends a dead string with
///
/// ```c
///   /* Reset the strings's `data' member so that we
///      know it's free.  */
///   s->u.s.data = NULL;
/// ```
///
/// and reads the marker back at :1851 and :1892.  `LispString::drop` only
/// nulled `data` for a string that OWNED its bytes
/// (`release_owned_storage` returns early when `storage_capacity == 0`),
/// so a swept BORROWED payload — every pdump-mapped and static-rodata
/// string — stayed byte-identical to a live one and a stale borrow of it
/// read on silently (DIVERGENCES.md 163).
#[test]
fn a_reclaimed_string_is_recognizable_as_dead() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // The rooted string keeps the arena page populated, so the doomed
    // slot's storage is still mapped and readable after the sweep — the
    // same arrangement the cons pin above relies on.
    let rooted = heap.alloc_string(crate::heap_types::LispString::from_utf8("rooted"));
    let doomed = heap.alloc_string(crate::heap_types::LispString::from_utf8("doomed"));
    let doomed_ptr = doomed.as_string_ptr().unwrap();
    assert!(
        !unsafe { (*doomed_ptr).data.is_reclaimed() },
        "a live string must not look reclaimed",
    );

    heap.collect_exact(std::iter::once(rooted));

    assert!(
        !unsafe { (*rooted.as_string_ptr().unwrap()).data.is_reclaimed() },
        "a rooted string must not be marked free",
    );
    assert!(
        unsafe { (*doomed_ptr).data.is_reclaimed() },
        "a reclaimed string must carry GNU's free marker (data == NULL, \
         src/alloc.c:1878-1882): without it a `&LispString` that outlived \
         its object is indistinguishable from a live borrow",
    );
}

/// The borrowed-payload half of the parity, which is the half that was
/// actually missing: `release_owned_storage` returns early for a string
/// whose bytes it does not own, so before DIVERGENCES.md 163 a swept
/// mapped/rodata string kept a perfectly valid-looking `data` pointer.
#[test]
fn a_reclaimed_string_with_borrowed_bytes_is_also_marked_free() {
    crate::test_utils::init_test_tracing();
    // Static, NUL-terminated: exactly the shape a pdump-mapped or
    // static-rodata payload has (`storage_capacity == 0`).
    static BYTES: &[u8] = b"borrowed\0";
    let borrowed =
        unsafe { crate::heap_types::LispString::from_mapped_bytes(BYTES.as_ptr(), 8, 8, -1) };
    assert!(!borrowed.is_reclaimed());
    let mut owner = std::mem::ManuallyDrop::new(borrowed);
    unsafe { std::ptr::drop_in_place(&mut *owner as *mut crate::heap_types::LispString) };
    assert!(
        owner.is_reclaimed(),
        "GNU nulls `data` for EVERY dead string, not only for one whose \
         bytes it owned (src/alloc.c:1878-1882)",
    );
}

/// `verify_marked_objects_owned` was written for the missing-root class
/// and had zero callers — 161 listed it as "dead code written for exactly
/// this failure". It is wired into `complete_collection` now, behind a
/// gate. This is the pin that it RUNS and that a healthy heap reports zero
/// problems: without the wiring the gate function does not exist and this
/// does not compile (DIVERGENCES.md 162). The helpers are
/// debug-only, so the pin compiles in debug builds only — a release
/// test build must not see this function at all.
#[test]
#[cfg(debug_assertions)]
fn post_mark_ownership_verification_runs_and_finds_nothing() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    super::set_verify_marked_objects_for_test(true);
    assert!(super::verify_marked_objects_enabled());

    let kept = heap.alloc_string(crate::heap_types::LispString::new("kept".into(), false));
    let vector = heap.alloc_vector(vec![TaggedValue::fixnum(7)]);
    let rooted = heap.alloc_cons(kept, vector);
    let _doomed = heap.alloc_string(crate::heap_types::LispString::new("dropped".into(), false));

    // Asserts internally (problems == 0) at the one moment where "marked"
    // and "owned" must agree.
    heap.collect_exact(std::iter::once(rooted));
    assert_eq!(heap.verify_marked_objects_owned(), 0);

    super::set_verify_marked_objects_for_test(false);
}

/// Workstream A path-collapse safety net (characterization): a forced
/// `collect_exact` retains a rooted live cons graph and reclaims an unrooted
/// one, INDEPENDENT of which internal path (concurrent / incremental /
/// STW-full) runs it. This must keep passing as the incremental slicer + the
/// `NEOVM_GC_CONCURRENT`/`NEOVM_GC_SATB` env flags are deleted in the collapse.
#[test]
fn collect_exact_retains_rooted_and_frees_unrooted() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    const N: i64 = 1_000;
    // Rooted list: rooted_head -> cons(N-1) -> ... -> cons(0) -> fixnum(0).
    let mut rooted = TaggedValue::fixnum(0);
    for i in 0..N {
        rooted = heap.alloc_cons(TaggedValue::fixnum(i), rooted);
    }
    let rooted_head = rooted;
    // Unrooted list (never named in the explicit root set): must be reclaimed.
    // A precise collector roots only the iterator passed to collect_exact, not
    // the Rust stack, so holding this local does NOT keep it alive.
    let mut unrooted = TaggedValue::fixnum(0);
    for i in 0..N {
        unrooted = heap.alloc_cons(TaggedValue::fixnum(1_000_000 + i), unrooted);
    }
    let _unrooted_head = unrooted;
    let before = heap.cons_live_count;

    // Force a full collection with only the rooted list reachable.
    heap.collect_exact(std::iter::once(rooted_head));
    let after = heap.cons_live_count;

    // The unrooted list was reclaimed...
    assert!(
        after < before,
        "unrooted conses must be reclaimed (before={before}, after={after})",
    );
    // ...and the entire rooted spine survives + is readable (a swept cons here
    // would be a use-after-free the asserts / sanitizer catch).
    let mut node = rooted_head;
    let mut count = 0i64;
    while node.is_cons() {
        let car = unsafe { (*node.xcons_ptr()).load_car() };
        assert_eq!(
            car.0,
            TaggedValue::fixnum(N - 1 - count).0,
            "rooted car intact at index {count}",
        );
        node = unsafe { (*node.xcons_ptr()).load_cdr() };
        count += 1;
    }
    assert_eq!(count, N, "the whole rooted list survived collection");
}

#[test]
fn ordinary_non_cons_ownership_index_tracks_sweep() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    let live = heap.alloc_float(1.0);
    let dead = heap.alloc_float(2.0);
    let live_ptr = live.as_float_ptr().unwrap() as *const u8;
    let dead_ptr = dead.as_float_ptr().unwrap() as *const u8;

    // Stage-3 fold-in: page floats are owned via the PAGE-SPAN oracle
    // and never touch the residual `non_cons_object_addrs` set.
    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(heap.owns_non_cons_object(dead_ptr));
    assert!(heap.float_arena.owns(live_ptr));
    assert!(heap.float_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);

    heap.collect_exact(std::iter::once(live));

    // The sweep's alloc-bit clear IS the ownership eviction: the freed
    // slot answers NOT-owned with no addr-set bookkeeping involved.
    assert!(heap.owns_non_cons_object(live_ptr));
    assert!(!heap.owns_non_cons_object(dead_ptr));
    assert!(heap.float_arena.owns(live_ptr));
    assert!(!heap.float_arena.owns(dead_ptr));
    assert_eq!(heap.non_cons_object_addrs.len(), 0);
    assert!((live.xfloat() - 1.0).abs() < f64::EPSILON);
}

/// Task #7 stage 2a (Fix A): the incremental vector registry must yield
/// exactly the Tier-B snapshot the old full-set filter produced, across
/// alloc/free cycles and both sweep paths. Computes BOTH methods — the
/// registry walk and the old `non_cons_object_addrs` filter — and compares
/// snapshot contents (backing base/len/kind), not just counts.
#[test]
fn vector_registry_matches_full_filter_across_cycles() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    fn entry_key(addr: usize) -> (usize, usize, bool) {
        // Safety: `addr` is a live owned Vector's `GcHeader` address (both
        // callers iterate live-object sets under a stopped world).
        let obj = unsafe { &*(addr as *const VectorObj) };
        let entry = obj.data.scan_entry();
        (entry.base as usize, entry.len, entry.is_mapped)
    }
    // Ground-truth snapshot contents, stage-3 form: allocated VECTOR
    // ARENA PAGE SLOTS (walked allocated-bit-first) ∪ any residual
    // Box Vector in the non-cons set (none are allocated anymore, but
    // the union keeps the test honest about the invariant's shape).
    fn full_filter_entries(heap: &TaggedHeap) -> Vec<(usize, usize, bool)> {
        let mut entries: Vec<(usize, usize, bool)> = heap
            .non_cons_object_addrs
            .iter()
            .filter(|&&addr| unsafe {
                (*(addr as *const GcHeader)).kind == HeapObjectKind::VecLike
                    && (*(addr as *const VecLikeHeader)).type_tag == VecLikeType::Vector
            })
            .map(|&addr| entry_key(addr))
            .collect();
        entries.extend(
            heap.vector_arena
                .collect_allocated_slots()
                .into_iter()
                .map(|slot| entry_key(slot as usize)),
        );
        entries.sort_unstable();
        entries
    }
    // New-method snapshot contents: iterate the incremental registry.
    fn registry_entries(heap: &TaggedHeap) -> Vec<(usize, usize, bool)> {
        let mut entries: Vec<(usize, usize, bool)> = heap
            .vector_object_addrs
            .iter()
            .map(|&addr| entry_key(addr))
            .collect();
        entries.sort_unstable();
        entries
    }
    fn assert_snapshots_match(heap: &TaggedHeap) {
        assert_eq!(
            registry_entries(heap),
            full_filter_entries(heap),
            "registry snapshot != full-filter snapshot",
        );
    }

    // Mixed population: vectors + non-vector decoys, both non-veclike
    // (float) and veclike-non-Vector (record) — the registry must exclude
    // every decoy kind.
    let keep_vec = heap.alloc_vector(vec![TaggedValue::fixnum(1); 8]);
    let dead_vec = heap.alloc_vector(vec![TaggedValue::fixnum(2); 4]);
    let keep_float = heap.alloc_float(1.5);
    let _dead_record = heap.alloc_record(vec![TaggedValue::fixnum(3); 5]);
    assert_snapshots_match(&heap);
    assert_eq!(registry_entries(&heap).len(), 2);

    // Cycle 1 (synchronous sweep_objects path): the unrooted vector and
    // record are reclaimed; the registry follows.
    let _ = dead_vec;
    heap.collect_exact([keep_vec, keep_float].into_iter());
    assert_snapshots_match(&heap);
    assert_eq!(registry_entries(&heap).len(), 1);

    // Cycle 2: fresh vectors on the reused address space, then free one.
    let dead_vec2 = heap.alloc_vector(vec![keep_float; 3]);
    let keep_vec2 = heap.alloc_vector(vec![TaggedValue::fixnum(4); 2]);
    let _ = dead_vec2;
    assert_snapshots_match(&heap);
    assert_eq!(registry_entries(&heap).len(), 3);
    heap.collect_exact([keep_vec, keep_vec2].into_iter());
    assert_snapshots_match(&heap);
    assert_eq!(registry_entries(&heap).len(), 2);

    // A full CONCURRENT cycle exercises the launch-time invariant
    // cross-check (`cfg(test)`) plus the deferred-sweep removal path
    // (`incremental_sweep_slice`) end to end. Only `keep_vec` is rooted,
    // so `keep_vec2` is reclaimed by the deferred sweep.
    heap.concurrent_begin();
    heap.seed_root(keep_vec);
    heap.launch_concurrent_mark();
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(keep_vec);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_snapshots_match(&heap);
    assert_eq!(registry_entries(&heap).len(), 1);
}

/// Task #7 stage 2a (Fix B): a stop request that lands MID-DRAIN (joining
/// without waiting for `concurrent_mark_done`) makes the GC thread break
/// at its stop-check quantum and hand ALL residual gray work to the
/// termination fold via `deferred` — the STW drain then finishes it, so
/// the live set is retained bit-for-bit and only real garbage is swept.
/// Exercises every interleaving outcome (job not yet started / mid-drain
/// quantum break / already drained) with the same outcome-based asserts.
#[test]
fn immediate_join_mid_drain_hands_residual_work_to_termination() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A long rooted list so the GC thread is (almost surely) still
    // draining when the join lands, plus one unrooted garbage cons.
    const N: i64 = 300_000;
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let root = list;
    let _garbage = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
    let live_before = heap.cons_live_count;

    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    // JOIN IMMEDIATELY — no `concurrent_mark_done` wait.
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    assert_eq!(
        heap.cons_live_count,
        live_before - 1,
        "exactly the one garbage cons is swept; the whole rooted list survives",
    );
    // The rooted spine is intact and readable (a swept live cons here
    // would be a use-after-free the asserts / sanitizer catch).
    let mut node = root;
    let mut count = 0i64;
    while node.is_cons() {
        let car = unsafe { (*node.xcons_ptr()).load_car() };
        assert_eq!(
            car.0,
            TaggedValue::fixnum(N - 1 - count).0,
            "rooted car intact at index {count}",
        );
        node = unsafe { (*node.xcons_ptr()).load_cdr() };
        count += 1;
    }
    assert_eq!(count, N, "the whole rooted list survived the early join");
}

/// Characterization safety net for the path-collapse refactor: a forced full
/// collection must retain a rooted cons graph and reclaim an unrooted one,
/// regardless of which internal mark path runs. Pins the observable contract
/// (`collect_exact` keeps the live set, frees garbage, leaves the spine
/// readable) so collapsing the three GC paths into one cannot silently change
/// it.
#[test]
fn collect_exact_retains_rooted_graph_and_frees_garbage() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Rooted spine: a -> b -> c (cdr-terminated by a fixnum).
    let c = heap.alloc_cons(TaggedValue::fixnum(3), TaggedValue::fixnum(0));
    let b = heap.alloc_cons(TaggedValue::fixnum(2), c);
    let a = heap.alloc_cons(TaggedValue::fixnum(1), b);
    // Unrooted garbage: reachable from neither the root nor the spine.
    let _g1 = heap.alloc_cons(TaggedValue::fixnum(-1), TaggedValue::fixnum(0));
    let _g2 = heap.alloc_cons(TaggedValue::fixnum(-2), TaggedValue::fixnum(0));
    let live_before = heap.cons_live_count;
    assert!(live_before >= 5);

    // Force a full collection rooted only at `a`.
    heap.collect_exact(std::iter::once(a));

    // The 3-cons rooted spine survives; the 2 garbage conses are reclaimed.
    assert_eq!(
        heap.cons_live_count,
        live_before - 2,
        "rooted graph retained, unrooted garbage reclaimed",
    );
    // The spine is intact and readable (reading a swept cons would corrupt).
    let a_cdr = unsafe { (*a.xcons_ptr()).load_cdr() };
    assert!(a_cdr.is_cons());
    let b_cdr = unsafe { (*a_cdr.xcons_ptr()).load_cdr() };
    assert!(b_cdr.is_cons());
    assert_eq!(
        unsafe { (*b_cdr.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(3).0,
    );
}

#[test]
fn native_font_object_traces_properties_and_capability() {
    use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};

    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let property = heap.alloc_string(crate::heap_types::LispString::from_utf8("font-name"));
    let capability = heap.alloc_string(crate::heap_types::LispString::from_utf8("font-capability"));
    let property_ptr = property.as_string_ptr().unwrap() as *const u8;
    let capability_ptr = capability.as_string_ptr().unwrap() as *const u8;
    let font = heap.alloc_font(FontObjectData {
        fields: vec![property].into(),
        metrics: FontObjectMetrics {
            pixel_size: 16,
            height: 19,
            max_width: 9,
            ascent: 14,
            descent: 5,
            space_width: 8,
            average_width: 8,
        },
        capability,
        identity: ResolvedFontIdentity::from_memory(
            FontBackendKind::Fontconfig,
            "test:native-font".to_string(),
            0,
            None,
        ),
    });

    heap.collect_exact(std::iter::once(font));
    assert!(heap.owns_non_cons_object(property_ptr));
    assert!(heap.owns_non_cons_object(capability_ptr));

    heap.collect_exact(std::iter::empty());
    assert!(!heap.owns_non_cons_object(property_ptr));
    assert!(!heap.owns_non_cons_object(capability_ptr));
}

/// Regression test for the O(n²) SATB blow-up: building a large container
/// (here a hash table) in a loop WHILE a concurrent mark is running must log
/// each container's pre-image to the SATB buffer at most ONCE per cycle, not
/// re-enumerate ALL of the container's children on every single mutation.
///
/// Before the per-cycle dedup fix, every `puthash` ran
/// `push_value_children_to_satb_shared` -> `collect_veclike_children`, which
/// enumerates `ht.data.values()` + `ht.key_snapshots.values()` — the WHOLE
/// table. N inserts each snapshot ~k*N values => Θ(N²) entries pushed into
/// `satb_shared` (and the equivalent memory), which OOMs on a 200K-entry
/// build like `(ucs-names)`. The fix snapshots the table's full pre-image
/// once, so the cumulative SATB volume is O(N).
///
/// We drive the SATB barrier directly (set `concurrent_mark_running` without
/// launching the background GC thread) so nothing drains `satb_shared`
/// concurrently and the cumulative push count is deterministic.
#[test]
fn satb_barrier_on_growing_hash_table_is_linear_not_quadratic() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};

    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // An `equal` hash table whose VALUES are heap objects (conses), so the
    // SATB enumeration actually pushes them to the shared buffer.
    let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));

    // Arm the SATB barrier exactly as `launch_concurrent_mark` does, but
    // WITHOUT the GC thread, so `satb_shared` is never drained and its length
    // measures the cumulative SATB push volume deterministically.
    heap.concurrent_mark_running = true;
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(true));

    const N: i64 = 50_000;
    for i in 0..N {
        // Each value is a fresh heap cons (a brand-new key => an INSERT, no
        // prior value at that key for SATB to log).
        let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
        let key = crate::emacs_core::value::HashKey::Int(i);
        let key_snapshot = TaggedValue::fixnum(i);
        crate::tagged::mutate::with_hash_table_mut(table, |ht| {
            ht.insert(key, key_snapshot, value);
        });
    }

    let satb_len = heap.satb_shared.lock().unwrap().len();

    // Disarm before dropping the heap so no later mutation hits the barrier.
    heap.concurrent_mark_running = false;
    TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(false));

    // O(n) bound. The full pre-image is snapshotted at most a small constant
    // number of times across the whole cycle (ideally once), so the
    // cumulative pushes are within a small multiple of N. The buggy
    // (re-enumerate-on-every-write) barrier produces ~N²/2 ≈ 1.25e9 pushes
    // for N=50_000, blowing far past this bound.
    let bound = (N as usize) * 4;
    assert!(
        satb_len <= bound,
        "SATB barrier is super-linear: pushed {satb_len} values for {N} inserts \
         (O(n) bound is {bound}); the per-write full-container enumeration was \
         not deduplicated per cycle",
    );
}

/// End-to-end correctness for the per-cycle SATB dedup under a REAL concurrent
/// mark + sweep: a hash table is mutated MANY times during marking (so the
/// dedup suppresses all but the first per-owner snapshot), values are
/// OVERWRITTEN (update) and the table is GROWN (insert+resize/rehash), and
/// churn garbage is allocated and dropped. After termination + sweep:
///   * every value reachable through the live table survives and is readable;
///   * a value that was OVERWRITTEN before the snapshot-time first mutation is
///     retained by the SATB pre-image (Yuasa: it was live at snapshot time);
///   * unrooted pre-mark garbage is reclaimed.
/// If the dedup ever dropped a still-reachable value's pre-image, the sweep
/// would free a live cons and the readback would observe corruption (and TSan
/// /ASan would fault). Mirrors `concurrent_mark_overlaps_mutation_and_retains_live_set`
/// but exercises the deduped multi-child (hash-table) owner path specifically.
#[test]
fn concurrent_mark_dedup_retains_hash_table_live_set() {
    use crate::emacs_core::value::{HashKey, HashTableTest, LispHashTable};

    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // Build the table BEFORE the mark so its initial values are part of the
    // start-of-cycle snapshot. Each value is a heap cons we can read back.
    let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Equal));
    const PRE: i64 = 2_000;
    for i in 0..PRE {
        let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
        let key = HashKey::Int(i);
        crate::tagged::mutate::with_hash_table_mut(table, |ht| {
            ht.insert(key, TaggedValue::fixnum(i), value);
        });
    }
    // Pre-mark garbage: reachable from nothing.
    let _garbage = heap.alloc_cons(TaggedValue::fixnum(-99), TaggedValue::fixnum(0));

    // Start a real concurrent mark with the table as the sole root.
    heap.concurrent_begin();
    heap.seed_root(table);
    heap.launch_concurrent_mark();

    // While the GC thread marks: (a) OVERWRITE an existing key's value — the
    // OLD cons leaves the table and must be retained via the SATB pre-image;
    // (b) GROW the table with many new keys (insert + resize/rehash), whose
    // values are born-black; (c) churn-allocate dropped garbage.
    let key0 = HashKey::Int(0);
    let old_value0 =
        crate::tagged::mutate::with_hash_table_mut(table, |ht| ht.data[&key0]).unwrap();
    let new_value0 = heap.alloc_cons(TaggedValue::fixnum(123_456), TaggedValue::fixnum(0));
    crate::tagged::mutate::with_hash_table_mut(table, |ht| {
        *ht.data.get_mut(&key0).unwrap() = new_value0;
    });
    for i in PRE..(PRE + 3_000) {
        let value = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::fixnum(0));
        let key = HashKey::Int(i);
        crate::tagged::mutate::with_hash_table_mut(table, |ht| {
            maybe_resize_for_test(ht);
            ht.insert(key, TaggedValue::fixnum(i), value);
        });
    }
    for _ in 0..5_000 {
        let _ = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::fixnum(0));
    }

    // Terminate stop-the-world + sweep.
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(table);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    // (1) The overwritten OLD value (live at snapshot time, then unlinked) must
    //     still be a readable, non-swept cons (SATB pre-image retained it).
    assert!(old_value0.is_cons());
    assert_eq!(
        unsafe { (*old_value0.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(0).0,
        "overwritten pre-snapshot value was swept — dedup dropped a live pre-image",
    );
    // (2) Every value currently in the table is readable (none swept).
    let snapshot = table.with_hash_table_mut(|ht| {
        ht.data
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>()
    });
    let entries = snapshot.expect("hash table");
    assert_eq!(entries.len() as i64, PRE + 3_000);
    for (key, value) in entries {
        assert!(
            value.is_cons(),
            "table value {key:?} is not a cons (swept?)"
        );
        let car = unsafe { (*value.xcons_ptr()).load_car() }.0;
        let expected = match key {
            HashKey::Int(0) => TaggedValue::fixnum(123_456).0, // the updated value
            HashKey::Int(n) => TaggedValue::fixnum(n).0,
            other => panic!("unexpected key {other:?}"),
        };
        assert_eq!(car, expected, "table value {key:?} corrupted/swept");
    }
}

/// GNU-parity finalizers, STW path: a finalizer a full collection finds
/// unreachable leaves the registry, its function is queued + re-marked
/// (transitively) so the sweep keeps it, and the finalizer object itself
/// is swept. A queued-but-not-taken function survives later cycles via
/// the runtime-root seeding.
#[test]
fn finalizer_doomed_on_stw_collection_queues_and_keeps_function() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let payload = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::fixnum(8));
    let function = heap.alloc_cons(TaggedValue::fixnum(42), payload);
    let finalizer = heap.alloc_finalizer(function);
    let fin_ptr = finalizer.as_veclike_ptr().unwrap();
    // The verifier enumeration must cover the function slot
    // (`collect_veclike_children` stays a superset of `trace_veclike`).
    let children = heap.collect_veclike_children(fin_ptr as *mut VecLikeHeader);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].0, function.0);

    // Cycle 1: the finalizer is rooted — still registered, nothing queued,
    // and the traced function survives.
    heap.begin_collection();
    heap.seed_root(finalizer);
    heap.complete_collection();
    assert!(heap.doomed_finalizer_functions.is_empty());
    assert_eq!(heap.finalizer_registry.len(), 1);
    assert_eq!(
        unsafe { (*function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(42).0,
    );

    // Cycle 2: nothing roots the finalizer — doomed. The function (and
    // what it reaches) survives the sweep; the finalizer object does not.
    heap.begin_collection();
    heap.complete_collection();
    assert!(heap.finalizer_registry.is_empty());
    assert!(
        !heap.owns_non_cons_object(fin_ptr as *const u8),
        "doomed finalizer object must be swept",
    );
    assert_eq!(
        unsafe { (*function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(42).0,
    );
    assert_eq!(
        unsafe { (*payload.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(7).0,
        "everything the queued function reaches must survive",
    );

    // Cycle 3, queue still undrained: the queued function is a runtime
    // root and must survive again.
    heap.begin_collection();
    heap.complete_collection();
    assert_eq!(
        unsafe { (*function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(42).0,
    );

    let doomed = heap.take_doomed_finalizer_functions();
    assert_eq!(doomed.len(), 1);
    assert_eq!(doomed[0].0, function.0);
    assert!(heap.take_doomed_finalizer_functions().is_empty());
}

/// GNU-parity finalizers, concurrent path: the doomed-finalizer scan must
/// run at `incremental_finish` too — a miss there means finalizers never
/// run under the concurrent collector. Also checks allocate-black: a
/// finalizer born during the mark survives that cycle and is doomable on
/// the next one.
#[test]
fn finalizer_doomed_on_concurrent_termination_queues_and_keeps_function() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // A long spine keeps the GC thread marking while the mutator runs.
    const N: i64 = 100_000;
    let mut list = TaggedValue::fixnum(0);
    for i in 0..N {
        list = heap.alloc_cons(TaggedValue::fixnum(i), list);
    }
    let function = heap.alloc_cons(TaggedValue::fixnum(43), TaggedValue::fixnum(0));
    let doomed_fin = heap.alloc_finalizer(function);
    let doomed_ptr = doomed_fin.as_veclike_ptr().unwrap();
    let live_fin = heap.alloc_finalizer(function);

    heap.concurrent_begin();
    heap.seed_root(list);
    heap.seed_root(live_fin); // doomed_fin is unreachable this cycle
    heap.launch_concurrent_mark();

    // Born during the mark: allocate-black, so it survives this cycle
    // even though nothing references it.
    let churn_function = heap.alloc_cons(TaggedValue::fixnum(44), TaggedValue::fixnum(0));
    let churn_fin = heap.alloc_finalizer(churn_function);
    let churn_ptr = churn_fin.as_veclike_ptr().unwrap();

    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(list);
    heap.seed_root(live_fin);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();

    assert!(
        !heap.owns_non_cons_object(doomed_ptr as *const u8),
        "doomed finalizer object must be swept",
    );
    assert!(heap.owns_non_cons_object(live_fin.as_veclike_ptr().unwrap() as *const u8));
    assert!(
        heap.owns_non_cons_object(churn_ptr as *const u8),
        "a finalizer born during the mark must survive that cycle",
    );
    assert_eq!(heap.finalizer_registry.len(), 2);
    let doomed = heap.take_doomed_finalizer_functions();
    assert_eq!(doomed.len(), 1);
    assert_eq!(doomed[0].0, function.0);
    assert_eq!(
        unsafe { (*function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(43).0,
    );

    // Next cycle: the born-black churn finalizer (still unreferenced) is
    // doomed now; the rooted one stays registered.
    heap.begin_collection();
    heap.seed_root(live_fin);
    heap.complete_collection();
    assert!(!heap.owns_non_cons_object(churn_ptr as *const u8));
    assert_eq!(heap.finalizer_registry.len(), 1);
    let doomed = heap.take_doomed_finalizer_functions();
    assert_eq!(doomed.len(), 1);
    assert_eq!(doomed[0].0, churn_function.0);
    assert_eq!(
        unsafe { (*churn_function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(44).0,
    );
}

/// The dump-partition + tricolor verifiers must accept the finalizer
/// arms: a LIVE finalizer is enumerated through
/// `collect_veclike_children`, and a doomed one's re-marked function must
/// not present a black->white edge. The fake dump span only activates the
/// partition; it maps no objects.
#[test]
fn finalizer_cycle_passes_partition_verifier() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16);

    // First partitioned cycle promotes survivors + blackens the (empty)
    // dump; verification gates arm on the cycles after it.
    heap.begin_collection();
    heap.complete_collection();
    assert!(heap.dump_blackened);

    let payload = heap.alloc_cons(TaggedValue::fixnum(5), TaggedValue::fixnum(6));
    let doomed_function = heap.alloc_cons(TaggedValue::fixnum(45), payload);
    let _doomed_fin = heap.alloc_finalizer(doomed_function);
    let live_function = heap.alloc_cons(TaggedValue::fixnum(46), TaggedValue::fixnum(0));
    let live_fin = heap.alloc_finalizer(live_function);

    // Verified cycle: `complete_collection` panics if the finalizer arms
    // break the partition/tricolor invariants.
    heap.begin_collection();
    heap.seed_root(live_fin);
    heap.complete_collection();

    let doomed = heap.take_doomed_finalizer_functions();
    assert_eq!(doomed.len(), 1);
    assert_eq!(doomed[0].0, doomed_function.0);
    assert_eq!(
        unsafe { (*payload.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(5).0,
    );
    assert_eq!(heap.finalizer_registry.len(), 1);
    assert_eq!(
        unsafe { (*live_function.xcons_ptr()).load_car() }.0,
        TaggedValue::fixnum(46).0,
    );
}
