//! U3.5 leaf claims, ownership refusals, parity/promotion and symbol handoff.

use super::*;
use crate::emacs_core::intern::intern_uninterned;
use crate::heap_types::LispMarker;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaf {
    Marker,
    Bignum,
    SymbolWithPos,
}

impl Leaf {
    const ALL: [Self; 3] = [Self::Marker, Self::Bignum, Self::SymbolWithPos];

    fn class(self) -> ChunkClass {
        match self {
            Self::Marker => ChunkClass::Marker,
            Self::Bignum => ChunkClass::Bignum,
            Self::SymbolWithPos => ChunkClass::SymbolWithPos,
        }
    }

    fn allocate(self, heap: &mut TaggedHeap) -> TaggedValue {
        match self {
            Self::Marker => heap.alloc_marker(marker()),
            Self::Bignum => heap.alloc_bignum(Integer::from(1u64 << 62)),
            Self::SymbolWithPos => {
                heap.alloc_symbol_with_pos(TaggedValue::T, TaggedValue::fixnum(3))
            }
        }
    }

    fn page_count(self, heap: &TaggedHeap) -> usize {
        match self {
            Self::Marker => heap.marker_arena.pages.len(),
            Self::Bignum => heap.bignum_arena.pages.len(),
            Self::SymbolWithPos => heap.symbol_with_pos_arena.pages.len(),
        }
    }
}

fn marker() -> LispMarker {
    LispMarker {
        buffer: None,
        insertion_type: false,
        marker_id: Some(35),
        bytepos: 0,
        charpos: 0,
        last_position_valid: true,
        next_marker: std::ptr::null_mut(),
        chained: false,
    }
}

fn heap(enabled: bool, chunk_map: bool, generational: bool) -> Box<TaggedHeap> {
    knobs::set_concurrent_claims_for_test(Some(enabled));
    knobs::set_chunk_map_for_test(Some(chunk_map));
    let mut heap = Box::new(TaggedHeap::new());
    knobs::set_concurrent_claims_for_test(None);
    knobs::set_chunk_map_for_test(None);
    heap.generational.enabled = generational;
    set_tagged_heap(&mut heap);
    heap
}

fn header(value: TaggedValue) -> *mut GcHeader {
    value.as_veclike_ptr().unwrap().cast_mut().cast()
}

fn job(heap: &mut TaggedHeap, major: bool) -> ConcurrentClaimJob {
    heap.close_alloc_regions();
    ConcurrentClaimJob {
        parity: heap.mark_parity.flip(),
        major,
        concurrent_claims: heap.concurrent_claims,
        pages: heap.page_snapshot_for_mark(),
        dump_lo: heap.dump_addr_lo,
        dump_hi: heap.dump_addr_hi,
        drop_dump_children: false,
        str_claimed: Arc::new(AtomicUsize::new(0)),
        float_claimed: Arc::new(AtomicUsize::new(0)),
        vec_claimed: Arc::new(AtomicUsize::new(0)),
        bc_claimed: Arc::new(AtomicUsize::new(0)),
        subr_dropped: Arc::new(AtomicUsize::new(0)),
        leaf_claimed: heap
            .concurrent_claims
            .then(|| Arc::new(AtomicUsize::new(0))),
    }
}

fn claim(
    value: TaggedValue,
    job: &ConcurrentClaimJob,
    gray: &mut Vec<TaggedValue>,
    logs: &mut WorkerMarkLogs,
) -> bool {
    if job.major {
        concurrent_try_mark_owned_logged::<true>(value, job, gray, logs)
    } else {
        concurrent_try_mark_owned_logged::<false>(value, job, gray, logs)
    }
}

#[test]
fn concurrent_leaf_claims_off_keeps_the_original_defer_path_and_allocates_no_counter() {
    for chunk_map in [false, true] {
        let mut heap = heap(false, chunk_map, false);
        assert!(!heap.concurrent_claims);
        assert!(heap.concurrent_leaf_claimed.is_none());
        let values: Vec<_> = Leaf::ALL.map(|kind| kind.allocate(&mut heap)).into();
        let job = job(&mut heap, false);
        for (kind, value) in Leaf::ALL.into_iter().zip(values) {
            let before = unsafe { (*header(value)).raw_mark() };
            assert!(!job.pages.contains(kind.class(), header(value) as usize));
            assert_eq!(job.pages.leaf_class(header(value) as usize), None);
            assert!(!claim(
                value,
                &job,
                &mut Vec::new(),
                &mut WorkerMarkLogs::default()
            ));
            assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
        }
    }
}

#[test]
fn concurrent_leaf_claims_obey_generation_parity_and_once_only_promotion() {
    for chunk_map in [false, true] {
        for major in [false, true] {
            for kind in Leaf::ALL {
                for age in 0..3 {
                    let mut heap = heap(true, chunk_map, major);
                    let value = kind.allocate(&mut heap);
                    unsafe {
                        if age == 1 {
                            (*header(value)).tenured = true;
                        } else if age == 2 {
                            (*header(value)).make_permanent();
                        }
                    }
                    for parity in [heap.mark_parity, heap.mark_parity.flip()] {
                        let mut job = job(&mut heap, major);
                        job.parity = parity;
                        unsafe { (*header(value)).set_marked(parity.flip()) };
                        let before = unsafe { (*header(value)).raw_mark() };
                        let expected = usize::from(age == 0 || (major && age == 1));
                        let mut gray = Vec::new();
                        let mut logs = WorkerMarkLogs::default();
                        assert!(claim(value, &job, &mut gray, &mut logs), "{kind:?}");
                        assert!(claim(value, &job, &mut gray, &mut logs), "{kind:?}");
                        assert_eq!(
                            job.leaf_claimed.as_ref().unwrap().load(Ordering::Relaxed),
                            expected
                        );
                        assert!(gray.is_empty());
                        assert!(logs.result.symbols.is_empty());
                        if major && age == 0 {
                            assert_eq!(logs.result.promo, [header(value) as usize]);
                        } else {
                            assert!(logs.result.promo.is_empty());
                        }
                        if expected == 1 {
                            assert!(unsafe { (*header(value)).is_marked_at(parity) });
                        } else {
                            assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn concurrent_leaf_claims_refuse_post_start_pages_without_touching_the_header() {
    for chunk_map in [false, true] {
        for kind in Leaf::ALL {
            let mut heap = heap(true, chunk_map, false);
            let old = kind.allocate(&mut heap);
            let job = job(&mut heap, false);
            let pages = kind.page_count(&heap);
            let mut fresh = old;
            while kind.page_count(&heap) == pages {
                fresh = kind.allocate(&mut heap);
            }
            assert!(job.pages.contains(kind.class(), header(old) as usize));
            assert!(!job.pages.contains(kind.class(), header(fresh) as usize));
            assert_eq!(
                job.pages.leaf_class(header(old) as usize),
                Some(kind.class())
            );
            assert_eq!(job.pages.leaf_class(header(fresh) as usize), None);
            let before = unsafe { (*header(fresh)).raw_mark() };
            let mut logs = WorkerMarkLogs::default();
            assert!(!claim(fresh, &job, &mut Vec::new(), &mut logs));
            assert_eq!(unsafe { (*header(fresh)).raw_mark() }, before);
            assert!(logs.result.promo.is_empty());
            assert_eq!(
                job.leaf_claimed.as_ref().unwrap().load(Ordering::Relaxed),
                0
            );
        }
    }
}

#[test]
fn concurrent_leaf_claims_keep_mapped_and_residual_box_symbol_positions_deferred() {
    for mapped in [false, true] {
        let mut heap = heap(true, true, false);
        let object = Box::into_raw(Box::new(SymbolWithPosObj {
            header: VecLikeHeader::new(VecLikeType::SymbolWithPos),
            sym: TaggedValue::T,
            pos: TaggedValue::fixnum(7),
        }));
        if mapped {
            // Image storage must outlive the heap; this test leaks one small
            // object just like FakeImage's process-lifetime mapped fixtures.
            unsafe {
                heap.register_mapped_veclike_object(object.cast(), size_of::<SymbolWithPosObj>())
            };
        } else {
            heap.link_veclike(object.cast());
        }
        let value = unsafe { TaggedValue::from_veclike_ptr(object.cast()) };
        let job = job(&mut heap, false);
        let before = unsafe { (*header(value)).raw_mark() };
        let mut logs = WorkerMarkLogs::default();
        assert!(!claim(value, &job, &mut Vec::new(), &mut logs));
        assert_eq!(unsafe { (*header(value)).raw_mark() }, before);
        assert!(logs.result.symbols.is_empty());
        assert_eq!(
            job.leaf_claimed.as_ref().unwrap().load(Ordering::Relaxed),
            0
        );
    }
}

#[test]
fn concurrent_leaf_symbol_positions_route_both_fields_and_skip_born_black_payloads() {
    for major in [false, true] {
        let mut heap = heap(true, true, major);
        let symbol = TaggedValue::from_sym_id(intern_uninterned("u35-leaf-symbol"));
        let child = heap.alloc_cons(TaggedValue::fixnum(31), TaggedValue::NIL);
        // The ordinary constructor accepts two Values. Use a heap value in
        // pos here to prove both trace slots follow the mutator tracer.
        let value = heap.alloc_symbol_with_pos(symbol, child);
        let job = job(&mut heap, major);
        let mut gray = Vec::new();
        let mut logs = WorkerMarkLogs::default();
        unsafe { (*header(value)).set_marked(job.parity) };
        assert!(claim(value, &job, &mut gray, &mut logs));
        assert!(gray.is_empty());
        assert!(logs.result.symbols.is_empty());
        assert_eq!(
            job.leaf_claimed.as_ref().unwrap().load(Ordering::Relaxed),
            0
        );
        unsafe { (*header(value)).set_marked(job.parity.flip()) };
        assert!(claim(value, &job, &mut gray, &mut logs));
        assert!(claim(value, &job, &mut gray, &mut logs));
        assert_eq!(gray, [child]);
        let crate::tagged::value::ValueKind::Symbol(id) = symbol.kind() else {
            unreachable!()
        };
        assert_eq!(logs.result.symbols, [id]);
    }
}

fn concurrent_cycle(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.launch_concurrent_mark();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !heap.concurrent_mark_done() {
        assert!(
            std::time::Instant::now() < deadline,
            "leaf worker did not finish"
        );
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    heap.reseed_runtime_and_remembered_roots();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.incremental_drain_all();
    heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
    heap.finish_incremental_sweep_now();
}

#[test]
fn concurrent_leaf_claims_preserve_weak_symbol_liveness_and_reclaim_dead_objects() {
    use crate::emacs_core::value::{HashTableTest, LispHashTable};
    for generational in [false, true] {
        for chunk_map in [false, true] {
            let mut heap = heap(true, chunk_map, generational);
            let key = TaggedValue::from_sym_id(intern_uninterned("u35-leaf-weak-key"));
            let position = heap.alloc_symbol_with_pos(key, TaggedValue::fixnum(3));
            let marker = Leaf::Marker.allocate(&mut heap);
            let bignum = Leaf::Bignum.allocate(&mut heap);
            let dead: Vec<_> = Leaf::ALL.map(|kind| kind.allocate(&mut heap)).into();
            let mut table = LispHashTable::new(HashTableTest::Eq);
            table.weakness = Some(crate::emacs_core::value::HashTableWeakness::Key);
            table.insert(
                key.to_hash_key(&HashTableTest::Eq),
                key,
                TaggedValue::fixnum(53),
            );
            let weak = heap.alloc_hash_table(table);
            let roots = [position, marker, bignum, weak];
            for _ in 0..2 {
                concurrent_cycle(&mut heap, &roots);
                assert_eq!(heap.sweep_stats().last_concurrent_leaf_claimed, 3);
                assert!(heap.is_value_marked(key));
                assert_eq!(weak.as_hash_table().unwrap().data.len(), 1);
                for value in [position, marker, bignum] {
                    assert!(heap.owns_heap_value_for_test(value));
                    if generational {
                        assert!(heap.value_is_old_for_test(value));
                    }
                }
            }
            for value in dead {
                assert!(!heap.owns_heap_value_for_test(value));
            }
            concurrent_cycle(&mut heap, &[weak]);
            assert_eq!(weak.as_hash_table().unwrap().data.len(), 0);
            for value in [position, marker, bignum] {
                assert!(!heap.owns_heap_value_for_test(value));
            }
        }
    }
}

#[test]
fn concurrent_leaf_claims_do_not_read_marker_chain_payloads() {
    for generational in [false, true] {
        let mut heap = heap(true, true, generational);
        let text = crate::buffer::buffer_text::BufferText::new();
        let live = Leaf::Marker.allocate(&mut heap);
        let dead = Leaf::Marker.allocate(&mut heap);
        let live_ptr = live
            .as_veclike_ptr()
            .unwrap()
            .cast_mut()
            .cast::<MarkerObj>();
        let dead_ptr = dead
            .as_veclike_ptr()
            .unwrap()
            .cast_mut()
            .cast::<MarkerObj>();
        text.chain_splice_at_head(live_ptr);
        text.chain_splice_at_head(dead_ptr);
        unsafe { heap.set_marker_chain_head_slots(vec![text.markers_head_slot_raw()]) };
        concurrent_cycle(&mut heap, &[live]);
        assert_eq!(heap.sweep_stats().last_concurrent_leaf_claimed, 1);
        assert_eq!(text.chain_walk_collect(), [live_ptr]);
        assert!(!heap.marker_arena.owns(dead_ptr.cast()));
    }
}

#[test]
fn concurrent_leaf_claim_rmw_has_one_winner_across_threads() {
    let mut heap = heap(true, true, true);
    let value = Leaf::Marker.allocate(&mut heap);
    let job = job(&mut heap, true);
    let promotions = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let job = &job;
                scope.spawn(move || {
                    let mut logs = WorkerMarkLogs::default();
                    assert!(claim(value, job, &mut Vec::new(), &mut logs));
                    logs.result.promo.len()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum::<usize>()
    });
    assert_eq!(promotions, 1);
    assert_eq!(
        job.leaf_claimed.as_ref().unwrap().load(Ordering::Relaxed),
        1
    );
}

#[test]
fn concurrent_leaf_claims_lost_worker_result_fails_closed_with_generations_off() {
    let mut heap = heap(true, true, false);
    let (sender, receiver) = std::sync::mpsc::channel::<ConcurrentMarkResult>();
    drop(sender);
    // Model an exited worker that lost its child/symbol result. No real
    // worker is using this heap, and no marking or sweep may follow the loss.
    heap.concurrent_mark_running = true;
    heap.gc_exited = Some(receiver);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        heap.join_concurrent_mark();
    }));
    heap.concurrent_mark_running = false;
    set_tagged_heap(&mut heap);
    assert!(result.is_err());
    assert!(!heap.sweep_in_progress);
}
