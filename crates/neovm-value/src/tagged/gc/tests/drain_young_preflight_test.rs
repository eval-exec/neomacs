use super::facade_mark::CollectorQuiescenceError;
use super::generational::GenerationCycle;
use super::scan_contract::SingleMutatorWorld;
use super::young_registry::{YoungLogLoan, YoungRegistryError};
use super::*;
use std::cell::Cell;
use std::sync::atomic::Ordering;

/// This fixture reads only stopped owner-confined state; no field is shared
/// with or read through a live worker. Sorted sets make equality deterministic.
#[derive(Debug, PartialEq, Eq)]
struct DrainSatbOwnerSnapshot {
    words: Vec<mark_word::MarkWord>,
    owners: Vec<usize>,
    strings: Vec<usize>,
}

impl DrainSatbOwnerSnapshot {
    fn capture(owner: &mutator_gc::SatbMutatorState) -> Self {
        let mut owners: Vec<_> = owner.satb_snapshotted_owners.iter().copied().collect();
        let mut strings: Vec<_> = owner.satb_string_preimage_addrs.iter().copied().collect();
        owners.sort_unstable();
        strings.sort_unstable();
        Self {
            words: owner.satb.words_for_test().to_vec(),
            owners,
            strings,
        }
    }
}

/// Read only after the worker has joined, or before it has been launched.
/// No snapshot is taken while a concurrent marker can change mark words.
#[derive(Debug, PartialEq, Eq)]
struct DrainCollectionSnapshot {
    cycle: GenerationCycle,
    major: bool,
    marking: bool,
    concurrent: bool,
    sweeping: bool,
    parity: MarkParity,
    promo: Vec<usize>,
    gray: Vec<usize>,
    remembered: Vec<usize>,
    cons_marks: Vec<Vec<usize>>,
    cons_old: Vec<Vec<u64>>,
    header_marks: Vec<u8>,
    marked_symbols: usize,
    weak_tables: Vec<usize>,
    satb_published: Option<Vec<mark_word::MarkWord>>,
    satb_mutators: Vec<DrainSatbOwnerSnapshot>,
    satb_poisoned: bool,
    termination_partial_words: usize,
    pending_obarray: bool,
    staged_cons: bool,
    staged_veclikes: bool,
    sweep_cursor: usize,
    sweep_end: usize,
    old_pending: usize,
    noncons_pending: usize,
    incremental_mark_us: u64,
    pace_start: Option<std::time::Instant>,
    pace_start_bytes: usize,
    collections: usize,
    receiver_present: bool,
    stop_requested: bool,
    termination_satb: usize,
    termination_deferred: usize,
    cons_live_count: usize,
    allocated_count: usize,
    cons_cursor: usize,
    cons_limit: usize,
}

impl DrainCollectionSnapshot {
    fn capture(heap: &TaggedHeap, headers: &[TaggedValue]) -> Self {
        assert!(!heap.concurrent_mark_running());
        Self {
            cycle: heap.generational.cycle,
            major: heap.generational.major_in_progress,
            marking: heap.mark_in_progress,
            concurrent: heap.concurrent_mark_running,
            sweeping: heap.sweep_in_progress,
            parity: heap.mark_parity,
            promo: heap
                .generational
                .promo
                .iter()
                .map(|ptr| *ptr as usize)
                .collect(),
            gray: heap.gray_queue.iter().map(|value| value.bits()).collect(),
            remembered: heap
                .generational
                .r_seed
                .iter()
                .map(|value| value.bits())
                .collect(),
            cons_marks: heap
                .cons_blocks
                .iter()
                .map(|block| {
                    (0..cons_mark_words(block.next_index as usize))
                        .map(|word| block.mark_word(word).load(Ordering::Relaxed))
                        .collect()
                })
                .collect(),
            cons_old: heap
                .cons_blocks
                .iter()
                .map(|block| {
                    (0..cons_mark_words(block.next_index as usize))
                        .map(|word| block.trailer().old_word(word))
                        .collect()
                })
                .collect(),
            header_marks: headers
                .iter()
                .map(|value| {
                    assert!(heap.owns_heap_value_for_test(*value));
                    let address = TaggedHeap::value_heap_addr(*value).unwrap();
                    // SAFETY: the fixture supplies live non-cons objects owned by
                    // this stopped heap, all of which start with GcHeader.
                    unsafe { (*(address as *const GcHeader)).raw_mark() }
                })
                .collect(),
            marked_symbols: heap.marked_symbols.count(),
            weak_tables: heap
                .weak_hash_tables
                .iter()
                .map(|ptr| *ptr as usize)
                .collect(),
            satb_published: heap.satb_handoff.snapshot_words_for_test().ok(),
            satb_mutators: heap
                .mutator_satb_states()
                .map(DrainSatbOwnerSnapshot::capture)
                .collect(),
            satb_poisoned: heap.satb_handoff.is_poisoned(),
            termination_partial_words: heap.last_termination_partial_words(),
            pending_obarray: heap.pending_obarray_scan.is_some(),
            staged_cons: heap.staged_mapped_cons_scan.is_some(),
            staged_veclikes: heap.staged_mapped_veclikes.is_some(),
            sweep_cursor: heap.sweep_cons_cursor,
            sweep_end: heap.sweep_cons_end,
            old_pending: heap.generational.old_sweep_pending as usize,
            noncons_pending: heap.sweep_noncons_pending as usize,
            incremental_mark_us: heap.incremental_mark_us,
            pace_start: heap.pace_mark_start,
            pace_start_bytes: heap.pace_mark_start_bytes,
            collections: heap.gc_collections,
            receiver_present: heap.gc_exited.is_some(),
            stop_requested: heap.gc_stop.load(Ordering::Relaxed),
            termination_satb: heap.last_termination_satb,
            termination_deferred: heap.last_termination_deferred,
            cons_live_count: heap.cons_live_count,
            allocated_count: heap.current_mutator_gc().allocated_count,
            cons_cursor: heap.jit.cons_cur.get(),
            cons_limit: heap.jit.cons_lim.get(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DrainYoungOwner {
    Current,
    Registered,
}

#[derive(Clone, Copy, Debug)]
enum DrainYoungGeneration {
    Gen0,
    Gen1,
}

#[derive(Clone, Copy, Debug)]
enum DrainYoungPhase {
    Idle,
    DroppedMajorCapture,
}

fn drain_young_heap(generation: DrainYoungGeneration) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.set_generational_enabled(match generation {
        DrainYoungGeneration::Gen0 => false,
        DrainYoungGeneration::Gen1 => true,
    });
    heap.generational.verify = true;
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.set_gc_threshold(usize::MAX);
    heap
}

fn drain_young_take_loan(heap: &mut TaggedHeap, owner: DrainYoungOwner) -> YoungLogLoan {
    // SAFETY: fixture is the sole stopped owner; no marker is running and
    // no TLS aliases, callbacks, safepoints or mutator resumption can occur.
    let mut world = unsafe { SingleMutatorWorld::from_heap(heap) };
    let id = match owner {
        DrainYoungOwner::Current => world.current_young_registration_id(),
        DrainYoungOwner::Registered => {
            let physical = world.register_mutator_gc_for_test();
            world.registered_young_id_for_test(physical).unwrap()
        }
    };
    world.take_young_log_loan(id).unwrap()
}

fn drain_young_return_loan(heap: &mut TaggedHeap, loan: YoungLogLoan) {
    // SAFETY: same confined owner, with no running marker or callback.
    let mut world = unsafe { SingleMutatorWorld::from_heap(heap) };
    assert_eq!(
        world.ensure_young_logs_stopped(),
        Err(YoungRegistryError::OutstandingLoans(1))
    );
    world.return_young_log_loan(loan).unwrap();
    world.ensure_young_logs_stopped().unwrap();
}

#[test]
fn drain_rejects_current_and_registered_young_loans_before_roots_or_buffers_change() {
    for generation in [DrainYoungGeneration::Gen0, DrainYoungGeneration::Gen1] {
        for owner in [DrainYoungOwner::Current, DrainYoungOwner::Registered] {
            for phase in [DrainYoungPhase::Idle, DrainYoungPhase::DroppedMajorCapture] {
                let mut heap = drain_young_heap(generation);
                let child = heap.alloc_float(11.5);
                let root = heap.alloc_cons(child, TaggedValue::NIL);
                let orphan = heap.alloc_cons(TaggedValue::fixnum(701), TaggedValue::NIL);
                heap.close_alloc_regions();
                match phase {
                    DrainYoungPhase::Idle => {}
                    DrainYoungPhase::DroppedMajorCapture => {
                        // SAFETY: same serialized stopped owner, no callbacks.
                        let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
                        capture.heap_mut().seed_root(root);
                        capture.heap_mut().note_root_overwrite_value(orphan);
                        drop(capture);
                        assert!(heap.mark_in_progress());
                        assert!(!heap.gray_queue.is_empty());
                        assert!(!heap.current_mutator_satb().satb.words_for_test().is_empty());
                    }
                }
                let mut loan = drain_young_take_loan(&mut heap, owner);
                let young_before = {
                    let log = loan.log_mut();
                    (log.id(), log.blocks().to_vec(), log.pending_index())
                };
                let before = DrainCollectionSnapshot::capture(&heap, &[child]);
                let identity = heap.heap_identity();
                let callbacks = Cell::new(0);
                // SAFETY: no worker is running and this is the exclusive
                // stopped owner. A missing young loan must cause typed refusal
                // before the callback or any phase/transport mutation.
                let error = match unsafe {
                    heap.drain_to_quiescent(|heap| {
                        callbacks.set(callbacks.get() + 1);
                        heap.seed_root(root);
                    })
                } {
                    Err(error) => error,
                    Ok(quiescent) => {
                        drop(quiescent);
                        panic!("a young loan must exclude quiescence");
                    }
                };
                assert!(
                    matches!(
                        error,
                        CollectorQuiescenceError::Marker(
                            MarkFinishError::MutatorsNotStopped(actual)
                        ) if actual == identity
                    ),
                    "{generation:?}/{owner:?}/{phase:?}"
                );
                assert_eq!(callbacks.get(), 0);
                assert_eq!(DrainCollectionSnapshot::capture(&heap, &[child]), before);
                let young_after = {
                    let log = loan.log_mut();
                    (log.id(), log.blocks().to_vec(), log.pending_index())
                };
                assert_eq!(young_after, young_before);
                drain_young_return_loan(&mut heap, loan);

                // Returned-loan control uses the real drain, not manually
                // clearing the residual cycle or transport snapshot.
                drop(
                    unsafe {
                        heap.drain_to_quiescent(|heap| {
                            callbacks.set(callbacks.get() + 1);
                            heap.seed_root(root);
                        })
                    }
                    .unwrap(),
                );
                let completed = match phase {
                    DrainYoungPhase::Idle => 0,
                    DrainYoungPhase::DroppedMajorCapture => 1,
                };
                assert_eq!(callbacks.get(), completed);
                assert_eq!(heap.gc_collections, before.collections + completed);
                assert!(!heap.mark_in_progress());
                assert!(!heap.sweep_in_progress());
                assert!(heap.owns_heap_value_for_test(root));
                assert!(heap.owns_heap_value_for_test(child));
                assert!(heap.owns_heap_value_for_test(orphan));
            }
        }
    }
}

#[test]
fn drain_young_loan_refusal_precedes_marker_completion_take_or_stop_request() {
    for generation in [DrainYoungGeneration::Gen0, DrainYoungGeneration::Gen1] {
        for owner in [DrainYoungOwner::Current, DrainYoungOwner::Registered] {
            let mut heap = drain_young_heap(generation);
            let root = heap.alloc_float(13.5);
            heap.close_alloc_regions();
            let loan = drain_young_take_loan(&mut heap, owner);
            let before = DrainCollectionSnapshot::capture(&heap, &[root]);
            // Deterministic disconnected-completion sentinel, not a running
            // worker. If finish is reached, it sets gc_stop and takes receiver
            // before producing Disconnected. No channel wait can hang here.
            let (sender, receiver) = std::sync::mpsc::channel();
            drop(sender);
            heap.gc_exited = Some(receiver);
            heap.concurrent_mark_running = true;
            let identity = heap.heap_identity();
            let callbacks = Cell::new(0);
            // SAFETY: this is the sole stopped owner and there is NO actual
            // worker; only the deterministic receiver sentinel is installed.
            let error =
                match unsafe { heap.drain_to_quiescent(|_| callbacks.set(callbacks.get() + 1)) } {
                    Err(error) => error,
                    Ok(quiescent) => {
                        drop(quiescent);
                        panic!("a young loan must exclude marker finish");
                    }
                };
            assert!(matches!(
                error,
                CollectorQuiescenceError::Marker(
                    MarkFinishError::MutatorsNotStopped(actual)
                ) if actual == identity
            ));
            assert_eq!(callbacks.get(), 0);
            assert!(heap.concurrent_mark_running);
            assert!(heap.gc_exited.is_some());
            assert!(!heap.gc_stop.load(Ordering::Relaxed));
            // Remove ONLY the fixture's sentinel, then compare full stopped
            // state. No marker exists or reads any object/transport snapshot.
            heap.concurrent_mark_running = false;
            heap.gc_exited = None;
            assert_eq!(DrainCollectionSnapshot::capture(&heap, &[root]), before);
            drain_young_return_loan(&mut heap, loan);
            drop(unsafe { heap.drain_to_quiescent(|_| {}) }.unwrap());
        }
    }
}
