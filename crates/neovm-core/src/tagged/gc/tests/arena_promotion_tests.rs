use super::*;

fn arm_partition(heap: &mut TaggedHeap, verify: bool) {
    if verify {
        unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    }
    // Fake dump span: activates the dump partition so the first full
    // cycle promotes + blackens.
    heap.extend_dump_span(4096, 16);
}

/// Build an interval table whose sole plist value is `v` (chars [0, 1)) —
/// local copy of the ownership_tests helper so this module stands alone.
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

/// Past the first partition cycle: every paged survivor (float, string,
/// vector) carries `header.tenured`; a FULL page retires (never swept,
/// never allocated into, STILL OWNED via the page oracle); partial pages
/// stay unretired; and the whole tenured population survives TWO further
/// cycles — one per parity — with payloads intact (the alternating
/// parity is what frees tenured slots if the sweep parity-reads them).
fn paged_survivors_tenure_and_full_pages_retire_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    // Exactly one FULL float page, all rooted through a cons spine.
    let mut root = TaggedValue::fixnum(0);
    let mut floats = Vec::with_capacity(FLOAT_PAGE_SLOTS);
    for i in 0..FLOAT_PAGE_SLOTS {
        let f = heap.alloc_float(i as f64);
        floats.push(f);
        root = heap.alloc_cons(f, root);
    }
    assert_eq!(heap.float_arena.pages.len(), 1);
    // A few strings and vectors: their pages stay PARTIAL (mixed).
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("tenure-me"));
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(9); 4]);
    root = heap.alloc_cons(s, root);
    root = heap.alloc_cons(v, root);

    // First partition cycle: full trace + sweep, then promotion.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);

    // Every paged survivor is tenured (the promotion page walk).
    for f in &floats {
        let ptr = f.as_float_ptr().unwrap();
        assert!(unsafe { (*ptr).header.tenured }, "page float not tenured");
    }
    let s_ptr = s.as_string_ptr().unwrap();
    assert!(
        unsafe { (*s_ptr).header.tenured },
        "page string not tenured",
    );
    let v_ptr = v.as_veclike_ptr().unwrap();
    assert!(unsafe { (*v_ptr).gc.tenured }, "page vector not tenured",);

    // The full float page RETIRED; the partial string/vector pages did
    // not. Retired ⇒ still registered + owned (C1), full, no free list.
    assert!(heap.float_arena.pages[0].retired, "full page must retire");
    assert!(!heap.string_arena.pages[0].retired, "partial page retired");
    assert!(!heap.vector_arena.pages[0].retired, "partial page retired");
    assert_eq!(
        heap.float_arena.pages[0].allocated, FLOAT_PAGE_SLOTS,
        "retired page must stay full",
    );
    heap.assert_object_arenas_coherent();

    // Two further cycles — parities false/true — the tenured slots are
    // never freed (retired page skipped whole; mixed pages tenured-skip)
    // and stay owned + intact.
    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        for (i, f) in floats.iter().enumerate() {
            let ptr = f.as_float_ptr().unwrap() as *const u8;
            assert!(
                heap.owns_non_cons_object(ptr),
                "tenured page float #{i} lost on cycle {cycle}",
            );
            assert!((f.xfloat() - i as f64).abs() < f64::EPSILON);
        }
        assert!(heap.owns_non_cons_object(s_ptr as *const u8));
        assert_eq!(
            unsafe { (*s_ptr).data.as_bytes() },
            b"tenure-me",
            "tenured string payload corrupted on cycle {cycle}",
        );
        assert!(heap.owns_non_cons_object(v_ptr as *const u8));
        assert_eq!(
            unsafe { &*(v_ptr as *const VectorObj) }.data.len(),
            4,
            "tenured vector payload lost on cycle {cycle}",
        );
        assert_eq!(heap.float_arena.pages[0].allocated, FLOAT_PAGE_SLOTS);
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn paged_survivors_tenure_and_full_pages_retire() {
    paged_survivors_tenure_and_full_pages_retire_body(false);
}

#[test]
fn paged_survivors_tenure_and_full_pages_retire_verified() {
    paged_survivors_tenure_and_full_pages_retire_body(true);
}

/// MIXED page: tenured slots and post-promotion YOUNG slots share a
/// page. Across TWO alternating-parity cycles the tenured slots survive
/// with intact payloads (the parity-blind sweep of the float-v1 template
/// would free them on the flipped cycle) while young garbage in the SAME
/// page is reclaimed, and freed slots are reused for young objects
/// without disturbing their tenured neighbors.
fn mixed_page_tenured_slots_survive_alternating_parities_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    // Interleave keepers and garbage in the same (single) page per class.
    let mut keep_floats = Vec::new();
    let mut keep_strings = Vec::new();
    let mut keep_vectors = Vec::new();
    let mut root = TaggedValue::fixnum(0);
    for i in 0..10 {
        let f = heap.alloc_float(i as f64);
        let s = heap.alloc_string(crate::heap_types::LispString::from_utf8(&format!(
            "mixed-{i}"
        )));
        let v = heap.alloc_vector(vec![TaggedValue::fixnum(i as i64); 3]);
        if i % 2 == 0 {
            keep_floats.push(f);
            keep_strings.push(s);
            keep_vectors.push(v);
            root = heap.alloc_cons(f, root);
            root = heap.alloc_cons(s, root);
            root = heap.alloc_cons(v, root);
        }
    }

    // Promotion cycle: odd-indexed garbage is swept FIRST (its slots are
    // free at promotion), then the survivors tenure ⇒ MIXED pages.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(!heap.float_arena.pages[0].retired);
    assert!(!heap.string_arena.pages[0].retired);
    assert!(!heap.vector_arena.pages[0].retired);

    // Refill the freed slots with YOUNG garbage (free-list reuse puts it
    // in the same mixed pages), then run one cycle per parity.
    for cycle in 0..2 {
        for i in 0..5 {
            let _ = heap.alloc_float(1000.0 + i as f64);
            let _ = heap.alloc_string(crate::heap_types::LispString::from_utf8("young-garbage"));
            let _ = heap.alloc_vector(vec![TaggedValue::fixnum(-1); 2]);
        }
        heap.collect_exact(std::iter::once(root));
        for (i, f) in keep_floats.iter().enumerate() {
            assert!(
                heap.owns_non_cons_object(f.as_float_ptr().unwrap() as *const u8),
                "tenured float #{i} freed on parity cycle {cycle}",
            );
            assert!((f.xfloat() - (2 * i) as f64).abs() < f64::EPSILON);
        }
        for (i, s) in keep_strings.iter().enumerate() {
            let ptr = s.as_string_ptr().unwrap();
            assert!(
                heap.owns_non_cons_object(ptr as *const u8),
                "tenured string #{i} freed on parity cycle {cycle}",
            );
            assert_eq!(
                unsafe { (*ptr).data.as_bytes() },
                format!("mixed-{}", 2 * i).as_bytes(),
            );
        }
        for (i, v) in keep_vectors.iter().enumerate() {
            let ptr = v.as_veclike_ptr().unwrap();
            assert!(
                heap.owns_non_cons_object(ptr as *const u8),
                "tenured vector #{i} freed on parity cycle {cycle}",
            );
            let obj = unsafe { &*(ptr as *const VectorObj) };
            assert_eq!(obj.data.as_slice()[0].as_fixnum(), Some(2 * i as i64));
        }
        heap.assert_object_arenas_coherent();
    }
}

#[test]
fn mixed_page_tenured_slots_survive_alternating_parities() {
    mixed_page_tenured_slots_survive_alternating_parities_body(false);
}

#[test]
fn mixed_page_tenured_slots_survive_alternating_parities_verified() {
    mixed_page_tenured_slots_survive_alternating_parities_body(true);
}

/// PAGE-SPAN ORACLE EXACTNESS: `owns` answers true for a live slot's
/// base address ONLY — false for a freed slot (alloc bit), for an
/// interior address of a live object (stride misalignment), for a
/// non-slot-aligned address, and for a never-allocated slot beyond the
/// bump cursor. Per class.
#[test]
fn page_span_oracle_freed_slot_exactness() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    // float: keep, dead, keep2 occupy consecutive slots.
    let keep_f = heap.alloc_float(1.0);
    let dead_f = heap.alloc_float(2.0);
    let keep_f2 = heap.alloc_float(3.0);
    let keep_s = heap.alloc_string(crate::heap_types::LispString::from_utf8("live"));
    let dead_s = heap.alloc_string(crate::heap_types::LispString::from_utf8("dead"));
    let keep_v = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
    let dead_v = heap.alloc_vector(vec![TaggedValue::fixnum(2)]);
    let dead_f_ptr = dead_f.as_float_ptr().unwrap() as usize;
    let dead_s_ptr = dead_s.as_string_ptr().unwrap() as usize;
    let dead_v_ptr = dead_v.as_veclike_ptr().unwrap() as usize;

    heap.collect_exact([keep_f, keep_f2, keep_s, keep_v].into_iter());

    let f_addr = keep_f.as_float_ptr().unwrap() as usize;
    let s_addr = keep_s.as_string_ptr().unwrap() as usize;
    let v_addr = keep_v.as_veclike_ptr().unwrap() as usize;
    // Live slot bases answer owned.
    assert!(heap.float_arena.owns(f_addr as *const u8));
    assert!(heap.string_arena.owns(s_addr as *const u8));
    assert!(heap.vector_arena.owns(v_addr as *const u8));
    // Freed-slot addresses answer NOT owned (alloc bit cleared).
    assert!(!heap.float_arena.owns(dead_f_ptr as *const u8));
    assert!(!heap.string_arena.owns(dead_s_ptr as *const u8));
    assert!(!heap.vector_arena.owns(dead_v_ptr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_f_ptr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_s_ptr as *const u8));
    assert!(!heap.owns_non_cons_object(dead_v_ptr as *const u8));
    // Mid-object interior addresses (stride-misaligned) answer NOT owned.
    assert!(!heap.float_arena.owns((f_addr + 8) as *const u8));
    assert!(!heap.string_arena.owns((s_addr + 16) as *const u8));
    assert!(!heap.vector_arena.owns((v_addr + 24) as *const u8));
    // Arbitrary non-slot-aligned addresses answer NOT owned.
    assert!(!heap.float_arena.owns((f_addr + 1) as *const u8));
    assert!(!heap.string_arena.owns((s_addr + 63) as *const u8));
    // Never-allocated slots beyond the bump cursor answer NOT owned even
    // though they are inside a registered page.
    let f_page_base = ObjectPage::<FloatObj>::page_base_for_ptr(f_addr as *const FloatObj);
    let beyond_bump = f_page_base + 100 * <FloatObj as PagedObject>::SLOT_BYTES;
    assert!(!heap.float_arena.owns(beyond_bump as *const u8));
    // Wrong-class registry: a float slot address is not owned by the
    // string/vector arenas (tag-first dispatch to distinct registries).
    assert!(!heap.string_arena.owns(f_addr as *const u8));
    assert!(!heap.vector_arena.owns(f_addr as *const u8));
    heap.assert_object_arenas_coherent();
}

/// Teardown with PAYLOAD-BEARING strings + vectors: every string page
/// and vector page is freed exactly once at heap drop — including
/// RETIRED pages — with the per-slot `drop_in_place` releasing byte
/// storage, interval tables, and element Vecs (a leak or double-free
/// here is what ASAN/MIRI lanes would catch; the counters prove the
/// page-level accounting either way).
fn payload_pages_freed_at_heap_drop_body(mid_mark: bool) {
    crate::test_utils::init_test_tracing();
    let strings_before = LIVE_STRING_PAGES.load(Ordering::Relaxed);
    let vectors_before = LIVE_VECTOR_PAGES.load(Ordering::Relaxed);
    {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap.extend_dump_span(4096, 16);

        let mut root = TaggedValue::fixnum(0);
        for i in 0..200 {
            let s = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
                b'p';
                1024
            ]));
            // Half the strings carry interval tables (dropped at Drop).
            if i % 2 == 0 {
                let carried = heap.alloc_cons(TaggedValue::fixnum(i), TaggedValue::NIL);
                let ptr = s.as_string_ptr().unwrap() as *mut StringObj;
                unsafe { *(*ptr).data.intervals_mut() = interval_table_carrying(carried) };
            }
            let v = heap.alloc_vector(vec![s; 8]);
            root = heap.alloc_cons(v, root);
        }
        assert!(LIVE_STRING_PAGES.load(Ordering::Relaxed) > strings_before);
        assert!(LIVE_VECTOR_PAGES.load(Ordering::Relaxed) > vectors_before);

        // Promotion + retirement happen before the drop (retired pages
        // must be freed by teardown too).
        heap.collect_exact(std::iter::once(root));
        assert!(heap.dump_blackened);
        heap.assert_object_arenas_coherent();

        if mid_mark {
            // Drop while the GC thread is concurrently marking: the heap
            // Drop must join FIRST, then free pages (under TSAN/ASAN an
            // early page free is a UAF on the GC thread).
            heap.concurrent_begin();
            heap.seed_root(root);
            heap.launch_concurrent_mark();
            assert!(heap.concurrent_mark_running());
        }
        drop(heap);
    }
    assert_eq!(
        LIVE_STRING_PAGES.load(Ordering::Relaxed),
        strings_before,
        "string pages leaked or double-freed at teardown",
    );
    assert_eq!(
        LIVE_VECTOR_PAGES.load(Ordering::Relaxed),
        vectors_before,
        "vector pages leaked or double-freed at teardown",
    );
}

#[test]
fn payload_pages_freed_at_heap_drop() {
    payload_pages_freed_at_heap_drop_body(false);
}

#[test]
fn payload_pages_freed_at_heap_drop_mid_concurrent_mark() {
    payload_pages_freed_at_heap_drop_body(true);
}

/// VARIABLE-size live-bytes accounting on BOTH recompute sites: after a
/// sweep, `live_bytes` equals the independently summed per-survivor
/// sizes (fixed struct + payload storage) — big string payloads and
/// vector backings included. An undercount here (e.g. summing fixed
/// sizes only) skews the adaptive pacer into overtriggering.
#[test]
fn sweep_live_bytes_track_variable_payload_sizes() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);

    let s_big = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
        b'q';
        10_000
    ]));
    let s_small = heap.alloc_string(crate::heap_types::LispString::from_utf8("s"));
    let v_big = heap.alloc_vector(vec![TaggedValue::fixnum(5); 1000]);
    let f = heap.alloc_float(2.5);
    // Garbage that must NOT be counted after the sweep.
    let _dead = heap.alloc_string(crate::heap_types::LispString::from_unibyte(vec![
        b'd';
        50_000
    ]));
    let mut root = TaggedValue::fixnum(0);
    let mut cons_count = 0usize;
    for val in [s_big, s_small, v_big, f] {
        root = heap.alloc_cons(val, root);
        cons_count += 1;
    }

    let expected_objects: usize = [s_big, s_small]
        .iter()
        .map(
            |s| TaggedHeap::object_bytes_from_header(s.as_string_ptr().unwrap() as *const GcHeader),
        )
        .sum::<usize>()
        + TaggedHeap::object_bytes_from_header(v_big.as_veclike_ptr().unwrap() as *const GcHeader)
        + TaggedHeap::object_bytes_from_header(f.as_float_ptr().unwrap() as *const GcHeader);
    let expected = expected_objects + cons_count * size_of::<ConsCell>();

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
    heap.incremental_finish(bytes_before, neomacs_host_runtime::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(
        heap.live_bytes(),
        expected,
        "incremental sweep live_bytes != summed survivor bytes",
    );
}

/// THE PROMOTION-SCAN UAF REGRESSION (the reason
/// `scan_permanents_for_young_children` walks page-tenured slots): a
/// page vector/string tenured at promotion holds a young CONS child
/// (conses never tenure) and is never mutated again. Without the
/// page-tenured remembered-set scan, the next cycle sweeps the cons
/// while its permanently-black owner still points at it. Two cycles
/// (both parities) must keep the children readable; under
/// `NEOVM_GC_VERIFY_PARTITION=1` the extended dump-partition verifier
/// independently checks every tenured-page child is marked.
fn tenured_page_owner_keeps_young_cons_child_alive_body(verify: bool) {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    arm_partition(&mut heap, verify);

    // Young cons children reachable ONLY through paged owners.
    let y_vec = heap.alloc_cons(TaggedValue::fixnum(777), TaggedValue::fixnum(0));
    let v = heap.alloc_vector(vec![y_vec]);
    let y_str = heap.alloc_cons(TaggedValue::fixnum(888), TaggedValue::fixnum(0));
    let s = heap.alloc_string(crate::heap_types::LispString::from_utf8("carrier"));
    unsafe {
        *(*(s.as_string_ptr().unwrap() as *mut StringObj))
            .data
            .intervals_mut() = interval_table_carrying(y_str)
    };
    let tail = heap.alloc_cons(s, TaggedValue::fixnum(0));
    let root = heap.alloc_cons(v, tail);

    // Promotion: v and s tenure via the page walk; y_* stay young.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.dump_blackened);
    assert!(unsafe { (*(v.as_veclike_ptr().unwrap())).gc.tenured });
    assert!(unsafe { (*(s.as_string_ptr().unwrap())).header.tenured });

    // Two partitioned cycles (one per parity): the owners are black and
    // never re-traced; the children survive ONLY via the promotion-time
    // page-tenured remembered-set scan.
    for cycle in 0..2 {
        heap.collect_exact(std::iter::once(root));
        assert_eq!(
            unsafe { (*y_vec.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(777).0,
            "tenured page vector's young cons child lost on cycle {cycle}",
        );
        assert_eq!(
            unsafe { (*y_str.xcons_ptr()).load_car() }.0,
            TaggedValue::fixnum(888).0,
            "tenured page string's young interval child lost on cycle {cycle}",
        );
    }
    heap.assert_object_arenas_coherent();
}

#[test]
fn tenured_page_owner_keeps_young_cons_child_alive() {
    tenured_page_owner_keeps_young_cons_child_alive_body(false);
}

#[test]
fn tenured_page_owner_keeps_young_cons_child_alive_verified() {
    tenured_page_owner_keeps_young_cons_child_alive_body(true);
}
