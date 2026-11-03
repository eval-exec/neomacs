//! Same optimized libtest ELF, direct Rust allocation, collected loops only.
#![cfg(not(feature = "gc-memory-telemetry"))]
use crate::emacs_core::eval::Context;
use crate::tagged::gc::alloc_region::RegionStats;
use crate::tagged::gc::{RunSlice, SliceRoot, TaggedHeap};
use crate::tagged::header::ConsCell;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::mem::size_of;

const CAR: i64 = 3101;
const MAX_N: usize = 65_536;
const MAX_WARM: usize = 100_000;

#[derive(Clone, Copy)]
struct Counts {
    refills: u64,
    granted: u64,
    sources: [u64; 3],
}
impl Counts {
    fn of(stats: &RegionStats) -> Self {
        Self {
            refills: stats.cons_refills,
            granted: stats.cons_cells_granted,
            sources: [
                stats.cons_from_free_list,
                stats.cons_from_block_tail,
                stats.cons_from_fresh_block,
            ],
        }
    }
    fn since(self, before: Self) -> Self {
        Self {
            refills: self.refills - before.refills,
            granted: self.granted - before.granted,
            sources: std::array::from_fn(|i| self.sources[i] - before.sources[i]),
        }
    }
}

/// Match this stable identifier with --toggle-collect, never an inherited PC.
/// Setup, region close, verification and teardown are outside this function.
#[inline(never)]
fn r2_cons_micro_sim_loop(heap: &mut TaggedHeap, car: Value, mut head: Value, n: usize) -> Value {
    for _ in 0..n {
        head = heap.alloc_cons(car, head);
    }
    head
}

/// One alias is cloned before entry; no handle copies/swaps occur per cons.
/// The complete list remains observable through the final canonical slot.
/// No assertion/counter/black_box is added per cons beyond normal Result use.
#[inline(never)]
fn r2_cons_micro_local_loop<'brand>(
    slice: &mut RunSlice<'brand, '_>,
    car: &SliceRoot<'brand>,
    cdr: &SliceRoot<'brand>,
    head: &mut SliceRoot<'brand>,
    n: usize,
) {
    for _ in 0..n {
        slice.cons_into(car, cdr, head).unwrap();
    }
}

fn verify_list(heap: &TaggedHeap, mut head: Value, expected: usize) {
    for _ in 0..expected {
        assert!(heap.owns_heap_value_for_test(head));
        // SAFETY: either the sole serial Sim heap or exact AllStopped
        // authority retains this fully initialized list through inspection.
        unsafe {
            let cell = &*head.as_cons_ptr().expect("micro result is a list");
            assert_eq!(cell.car.as_fixnum(), Some(CAR));
            head = cell.cdr();
        }
    }
    assert!(head.is_nil(), "micro result has the exact bounded length");
}

fn receipt(
    arm: &str,
    shape: &str,
    n: usize,
    gen1: bool,
    warm: usize,
    counts: Counts,
    bank: (usize, usize),
) {
    // Unknown event counts stay null: an inlined mutex cannot be detected by
    // absence of lock_contended symbols. Profile/disassembly + the separate
    // held-root-table-lock gate establish the local non-registration path.
    println!(
        "R2_CONS_MICRO {{\"arm\":\"{arm}\",\"shape\":\"{shape}\",\"n\":{n},\"gen\":{},\"warm\":{warm},\"initial_heap_fixture\":\"full_collected_world\",\"verified\":true,\"refills\":{},\"cells_granted\":{},\"source_grants\":[{},{},{}],\"slot_creations\":0,\"slot_bank_len\":{},\"slot_bank_capacity\":{},\"root_registrations\":null,\"lock_attempts\":null,\"host_allocations\":null,\"event_counts_observed\":false}}",
        usize::from(gen1),
        counts.refills,
        counts.granted,
        counts.sources[0],
        counts.sources[1],
        counts.sources[2],
        bank.0,
        bank.1,
    );
}

fn sim(shape: &str, n: usize) {
    Context::with_test_collection_era(
        |context| {
            // Use the SAME collected World heap as the local arm, while this
            // setup still owns its serial, non-admitted allocator. The marker
            // must never run after the fixture freezes/adopts that heap.
            let heap = &mut *context.tagged_heap;
            heap.gc_threshold = usize::MAX;
            assert!(
                !heap.threads_admitted(),
                "Sim marker uses the sole allocator"
            );
            let initial = heap.allocated_count();
            let fresh_before = heap.region_stats.cons_from_fresh_block;
            let mut head = Value::NIL;
            let mut warm = 0;
            for _ in 0..MAX_WARM {
                head = heap.alloc_cons(Value::fixnum(CAR), head);
                warm += 1;
                if heap.region_stats.cons_from_fresh_block > fresh_before {
                    break;
                }
            }
            assert!(warm < MAX_WARM, "warmup reaches a new actual fresh region");
            let remaining =
                (heap.jit.cons_lim.get() - heap.jit.cons_cur.get()) / size_of::<ConsCell>();
            assert!(remaining >= 127, "fresh maximum region after one warm cell");
            let before = Counts::of(&heap.region_stats);
            let gen1 = heap.generational_enabled();
            head = r2_cons_micro_sim_loop(heap, Value::fixnum(CAR), head, n);
            let delta = Counts::of(&heap.region_stats).since(before);
            if shape == "resident" {
                assert_eq!(delta.refills, 0);
                assert_eq!(delta.granted, 0);
            }
            heap.close_alloc_regions();
            assert_eq!(heap.allocated_count() - initial, warm + n);
            verify_list(heap, head, warm + n);
            // SAFETY: setup remains the sole serial owner and the complete
            // list is live. One fixed root AFTER the measured marker retains
            // it across the fixture's subsequent drain/capture/admission.
            let root = unsafe { SharedRoot::new(heap, head) };
            (root, gen1, warm, delta)
        },
        |era, (root, gen1, warm, delta)| {
            // No synthetic owner and no detached catalog are needed for the
            // serial arm. The real major drains canonical legacy young work;
            // cleanup and verification are entirely outside the marker.
            let mut stopped = era.stop().unwrap();
            let report = era.collect_stopped(&mut stopped).unwrap();
            assert!(report.major);
            assert_eq!(report.frozen_owners, 0);
            era.with_stopped_heap_for_test(&stopped, |heap| {
                let head = root.materialize(heap).unwrap();
                verify_list(heap, head.value(), warm + n);
            })
            .unwrap();
            stopped.complete().unwrap();
            drop(root);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            receipt("sim", shape, n, gen1, warm, delta, (0, 0));
        },
    );
}

fn local(shape: &str, n: usize) {
    Context::with_test_collection_era(
        |context| {
            context.tagged_heap.gc_threshold = usize::MAX;
            let gen1 = context.tagged_heap.generational_enabled();
            // SAFETY: immediate values are live on this sole serial heap.
            let car = unsafe { SharedRoot::new(&context.tagged_heap, Value::fixnum(CAR)) };
            let nil = unsafe { SharedRoot::new(&context.tagged_heap, Value::NIL) };
            (car, nil, gen1)
        },
        |era, (car_root, nil_root, gen1)| {
            era.start_cons_storage().unwrap();
            let mut owner = era
                .register(3101, 2)
                .unwrap()
                .activate(era.heap_identity(), 3101)
                .unwrap();
            let (head_word, warm, delta, bank) = {
                let mut run = owner.run_access().unwrap();
                let car = run.root_slot(&car_root).unwrap();
                let head = run.root_slot(&nil_root).unwrap();
                assert_eq!(run.state().region_stats.cons_from_fresh_block, 0);
                let bank_len = run.state().root_scratch.len();
                let bank_capacity = run.state().root_scratch.capacity();
                // Entry validation, cold binding and one alias clone precede
                // both warmup and the measured marker. The wrapper retains
                // the actual guard and closes it after the marker returns.
                let (warm, delta) = run
                    .with_slice(|slice| {
                        let car = slice.bind(&car).unwrap();
                        let mut head = slice.bind(&head).unwrap();
                        let cdr = head.clone();
                        let mut warm = 0;
                        for _ in 0..MAX_WARM {
                            slice.cons_into(&car, &cdr, &mut head).unwrap();
                            warm += 1;
                            if slice.stats_for_test().cons_from_fresh_block != 0 {
                                break;
                            }
                        }
                        assert!(
                            warm < MAX_WARM,
                            "warmup reaches actual catalog fresh source"
                        );
                        assert!(slice.remaining_for_test() >= 127);
                        let before = Counts::of(&slice.stats_for_test());
                        r2_cons_micro_local_loop(slice, &car, &cdr, &mut head, n);
                        let delta = Counts::of(&slice.stats_for_test()).since(before);
                        if shape == "resident" {
                            assert_eq!(delta.refills, 0);
                            assert_eq!(delta.granted, 0);
                        }
                        (warm, delta)
                    })
                    .unwrap();
                assert_eq!(run.state().root_scratch.len(), bank_len);
                assert_eq!(run.state().root_scratch.capacity(), bank_capacity);
                let word = head.word_for_test(&run);
                assert!(
                    run.cons_region_for_test().is_none(),
                    "slice wrapper closed its region"
                );
                assert_eq!(run.state().hot.allocated_count, warm + n);
                (word, warm, delta, (bank_len, bank_capacity))
            };
            // No per-cons SharedRoot protected this graph. Its canonical
            // indexed census alone must survive real publication and marking.
            drop((car_root, nil_root));
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            era.with_stopped_heap_for_test(&stopped, |heap| {
                verify_list(heap, Value::from_bits(head_word), warm + n);
            })
            .unwrap();
            stopped.complete().unwrap();
            owner.leave_native().unwrap();
            {
                let mut run = owner.run_access().unwrap();
                run.clear_root_slots().unwrap();
                run.close_cons_region().unwrap();
            }
            owner.retire_rooted(&[]).unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            receipt("local", shape, n, gen1, warm, delta, bank);
        },
    );
}

/// Run directly from ONE preserved optimized libtest ELF, not through Cargo
/// during measurement. See tmp/peer-p1rv/r7/TEST-PLAN.md and run-micro.py.
#[test]
#[cfg(feature = "perf-probes")]
fn rooted_slot_cons_micro() {
    let arm = std::env::var("R2_CONS_MICRO_ARM").unwrap_or_else(|_| "local".into());
    let shape = std::env::var("R2_CONS_MICRO_SHAPE").unwrap_or_else(|_| "resident".into());
    let n: usize = std::env::var("R2_CONS_MICRO_N")
        .unwrap_or_else(|_| "64".into())
        .parse()
        .unwrap();
    assert!(n <= MAX_N);
    assert!(shape == "resident" || shape == "amortized");
    if shape == "resident" {
        assert!(n <= 127);
    } else {
        assert!(n == 0 || n >= 128);
    }
    match arm.as_str() {
        "sim" => sim(&shape, n),
        "local" => local(&shape, n),
        _ => panic!("micro arm must be sim or local"),
    }
}
