//! Minor admission preserves an unfinished major until its explicit drain.

use super::*;
use strum::IntoEnumIterator;

/// This fixture reads only stopped owner-confined state; no field is shared
/// with or read through a live worker. Sorted sets make equality deterministic.
#[derive(Debug, PartialEq, Eq)]
struct SatbOwnerSnapshot {
    words: Vec<mark_word::MarkWord>,
    owners: Vec<usize>,
    strings: Vec<usize>,
}

impl SatbOwnerSnapshot {
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
struct CollectionSnapshot {
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
    satb_mutators: Vec<SatbOwnerSnapshot>,
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
}

impl CollectionSnapshot {
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
                .map(SatbOwnerSnapshot::capture)
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
        }
    }
}

fn heap(enabled: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational = GenState::new(enabled);
    heap.set_generational_enabled(enabled);
    heap
}

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum SyntheticBusyState {
    Marking,
    Major,
    Sweeping,
    Gray,
    StagedMappedCons,
    StagedMappedVeclikes,
}

/// A stopped fixture's isolated busy state. Drop restores the injected field
/// even when an admission assertion panics; no synthetic work reaches heap Drop.
struct SyntheticBusyGuard<'heap> {
    heap: &'heap mut TaggedHeap,
    state: SyntheticBusyState,
}

impl<'heap> SyntheticBusyGuard<'heap> {
    fn new(heap: &'heap mut TaggedHeap, state: SyntheticBusyState, root: TaggedValue) -> Self {
        match state {
            SyntheticBusyState::Marking => heap.mark_in_progress = true,
            SyntheticBusyState::Major => heap.generational.major_in_progress = true,
            SyntheticBusyState::Sweeping => heap.sweep_in_progress = true,
            SyntheticBusyState::Gray => heap.gray_queue.push(root),
            SyntheticBusyState::StagedMappedCons => heap.staged_mapped_cons_scan = Some(Vec::new()),
            SyntheticBusyState::StagedMappedVeclikes => {
                // SAFETY: the fixture owns the sole heap writer, with no
                // callbacks or safepoints during this empty capture.
                let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(heap) };
                // SAFETY: an empty header iterator lends no object storage.
                let snapshot =
                    unsafe { MappedVeclikeScanSnapshot::capture(&world, std::iter::empty()) };
                heap.staged_mapped_veclikes = Some(snapshot);
            }
        }
        Self { heap, state }
    }
}

impl Drop for SyntheticBusyGuard<'_> {
    fn drop(&mut self) {
        match self.state {
            SyntheticBusyState::Marking => self.heap.mark_in_progress = false,
            SyntheticBusyState::Major => self.heap.generational.major_in_progress = false,
            SyntheticBusyState::Sweeping => self.heap.sweep_in_progress = false,
            SyntheticBusyState::Gray => self.heap.gray_queue.clear(),
            SyntheticBusyState::StagedMappedCons => self.heap.staged_mapped_cons_scan = None,
            SyntheticBusyState::StagedMappedVeclikes => self.heap.staged_mapped_veclikes = None,
        }
    }
}

#[test]
fn minor_and_major_admissions_reject_each_independent_busy_state() {
    for enabled in [false, true] {
        for state in SyntheticBusyState::iter() {
            let mut heap = heap(enabled);
            let root = heap.alloc_float(11.5);
            let idle = CollectionSnapshot::capture(&heap, &[root]);
            {
                let busy = SyntheticBusyGuard::new(&mut heap, state, root);
                let before = CollectionSnapshot::capture(busy.heap, &[root]);
                // SAFETY: the stopped fixture is the sole heap owner; no real
                // worker or callback exists in these synthetic busy states.
                assert!(
                    matches!(
                        unsafe { busy.heap.permit_concurrent_mark() },
                        Err(ConcurrentMarkAdmissionError::CollectorBusy)
                    ),
                    "GEN1={enabled}, {state:?}"
                );
                assert_eq!(CollectionSnapshot::capture(busy.heap, &[root]), before);
                assert_eq!(
                    busy.heap.begin_minor_collection(),
                    Err(MinorCollectionAdmissionError::CollectorBusy),
                    "GEN1={enabled}, {state:?}"
                );
                assert_eq!(CollectionSnapshot::capture(busy.heap, &[root]), before);
            }
            assert_eq!(CollectionSnapshot::capture(&heap, &[root]), idle);
        }
    }
}

fn reject_and_recover(
    heap: &mut TaggedHeap,
    root: TaggedValue,
    child: TaggedValue,
    garbage: TaggedValue,
) {
    assert!(heap.mark_in_progress());
    assert!(!heap.concurrent_mark_running());
    assert_eq!(
        heap.generational.major_in_progress,
        heap.generational.enabled
    );
    assert_eq!(heap.generational.cycle, GenerationCycle::Major);
    // Draining gray work is one step of termination, not collection completion.
    // Leave populated marks and promotion records, then reseed residual work so
    // a rejected admission must preserve all three, independent of worker timing.
    heap.incremental_drain_all();
    heap.seed_root(root);
    assert!(!heap.gray_queue.is_empty());
    assert!(heap.is_value_marked(root));
    assert!(heap.is_value_marked(child));
    if heap.generational.enabled {
        assert!(!heap.generational.promo.is_empty());
    }
    let before = CollectionSnapshot::capture(heap, &[child, garbage]);
    // SAFETY: the stopped fixture remains the sole owner through admission;
    // rejection must not change any residual collector work or marks.
    assert!(matches!(
        unsafe { heap.permit_concurrent_mark() },
        Err(ConcurrentMarkAdmissionError::CollectorBusy)
    ));
    assert_eq!(CollectionSnapshot::capture(heap, &[child, garbage]), before);
    assert_eq!(
        heap.begin_minor_collection(),
        Err(MinorCollectionAdmissionError::CollectorBusy)
    );
    assert_eq!(CollectionSnapshot::capture(heap, &[child, garbage]), before);

    // SAFETY: this fixture is the sole heap owner, the worker has joined (or
    // never launched), and ROOT is its complete external root graph. The
    // callback only seeds roots; it cannot allocate or invoke Lisp callbacks.
    drop(unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(root)) }.unwrap());
    assert!(!heap.mark_in_progress());
    assert!(!heap.sweep_in_progress());
    assert!(!heap.generational.major_in_progress);
    assert!(heap.gray_queue.is_empty());
    assert_eq!(heap.gc_collections, before.collections + 1);
    assert!(heap.owns_heap_value_for_test(root));
    assert!(heap.owns_heap_value_for_test(child));
    assert!(!heap.owns_heap_value_for_test(garbage));
    assert_eq!(root.cons_car(), child);
    assert_eq!(child.xfloat(), 9.5);
    if heap.generational.enabled {
        assert!(heap.value_is_old_for_test(root));
        assert!(heap.value_is_old_for_test(child));
    }
}

#[test]
fn minor_rejects_joined_major_without_changing_collection_state() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        let child = heap.alloc_float(9.5);
        let root = heap.alloc_cons(child, TaggedValue::NIL);
        let garbage = heap.alloc_float(10.5);
        let orphan = heap.alloc_cons(TaggedValue::fixnum(317), TaggedValue::NIL);
        // SAFETY: this direct Rust fixture owns its heap exclusively, including
        // root seeding and publication; no callbacks or mutator aliases exist.
        let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
        capture.heap_mut().seed_root(root);
        capture.launch().unwrap();
        // The published worker retains the running flag through explicit
        // finish, including when it has tentatively drained. Inspect only
        // owner-written phase state while worker mark words may still change.
        let phase = (
            heap.generational.cycle,
            heap.generational.major_in_progress,
            heap.mark_in_progress,
            heap.concurrent_mark_running,
            heap.mark_parity,
        );
        assert!(heap.concurrent_mark_running());
        // SAFETY: this serialized fixture owns the mutator side; admission
        // must refuse the published worker without touching its live marks.
        assert!(matches!(
            unsafe { heap.permit_concurrent_mark() },
            Err(ConcurrentMarkAdmissionError::CollectorBusy)
        ));
        assert_eq!(
            heap.begin_minor_collection(),
            Err(MinorCollectionAdmissionError::CollectorBusy)
        );
        assert_eq!(
            (
                heap.generational.cycle,
                heap.generational.major_in_progress,
                heap.mark_in_progress,
                heap.concurrent_mark_running,
                heap.mark_parity,
            ),
            phase
        );
        // This owner-resident partial is not visible to the live worker.
        // No mark-word/transport snapshot is read until finish joins it.
        heap.note_root_overwrite_value(orphan);
        heap.finish_concurrent_mark().unwrap();
        reject_and_recover(&mut heap, root, child, garbage);
        assert!(heap.owns_heap_value_for_test(orphan));
        assert_eq!(orphan.cons_car().bits(), TaggedValue::fixnum(317).bits());
    }
}

#[test]
fn minor_rejects_dropped_major_capture_without_changing_collection_state() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        let child = heap.alloc_float(9.5);
        let root = heap.alloc_cons(child, TaggedValue::NIL);
        let garbage = heap.alloc_float(10.5);
        let orphan = heap.alloc_cons(TaggedValue::fixnum(317), TaggedValue::NIL);
        // SAFETY: this direct Rust fixture is the serialized heap owner, and
        // no Lisp callback or allocation safepoint runs during capture.
        let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
        capture.heap_mut().seed_root(root);
        capture.heap_mut().note_root_overwrite_value(orphan);
        drop(capture);
        assert_eq!(
            heap.current_mutator_satb().satb.words_for_test(),
            &[MarkWord::of(orphan)]
        );
        reject_and_recover(&mut heap, root, child, garbage);
        assert!(heap.owns_heap_value_for_test(orphan));
        assert_eq!(orphan.cons_car().bits(), TaggedValue::fixnum(317).bits());
    }
}

#[test]
fn minor_rejects_quiescent_gen0_without_changing_collection_state() {
    let mut heap = heap(false);
    let value = heap.alloc_float(11.5);
    let before = CollectionSnapshot::capture(&heap, &[value]);
    assert_eq!(
        heap.begin_minor_collection(),
        Err(MinorCollectionAdmissionError::GenerationalDisabled)
    );
    assert_eq!(CollectionSnapshot::capture(&heap, &[value]), before);
}

#[test]
fn minor_accepts_quiescent_gen1_and_completes() {
    let mut heap = heap(true);
    let child = heap.alloc_float(9.5);
    let root = heap.alloc_cons(child, TaggedValue::NIL);
    let garbage = heap.alloc_float(10.5);
    heap.begin_minor_collection().unwrap();
    heap.seed_root(root);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    assert!(!heap.mark_in_progress());
    assert!(!heap.sweep_in_progress());
    assert!(heap.value_is_old_for_test(root));
    assert!(heap.value_is_old_for_test(child));
    assert!(!heap.owns_heap_value_for_test(garbage));
    assert_eq!(root.cons_car(), child);
}

/// Every case represents a distinct owner/handoff state; none is an invented
/// old shared queue, and no marker runs while transport snapshots are read.
#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum SatbResidualSource {
    OwnerPartial,
    PublishedChunk,
    RegisteredOwnerPartial,
    RetiredOwnerPublished,
}

fn inject_satb_preimage(heap: &mut TaggedHeap, source: SatbResidualSource, value: TaggedValue) {
    let handoff = heap.satb_handoff.clone();
    match source {
        SatbResidualSource::OwnerPartial => {
            heap.current_mutator_satb_mut()
                .satb
                .push(MarkWord::of(value), &handoff);
        }
        SatbResidualSource::PublishedChunk => {
            let owner = heap.current_mutator_satb_mut();
            owner.satb.push(MarkWord::of(value), &handoff);
            owner.satb.publish_partial(&handoff).unwrap();
        }
        SatbResidualSource::RegisteredOwnerPartial | SatbResidualSource::RetiredOwnerPublished => {
            // SAFETY: these deterministic fixtures own all registrations,
            // have no launched worker, TLS aliases, callbacks or safepoints.
            let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(heap) };
            let id = world.register_mutator_gc_for_test();
            world
                .mutator_gc_for_test(id)
                .satb
                .satb
                .push(MarkWord::of(value), &handoff);
            if matches!(source, SatbResidualSource::RetiredOwnerPublished) {
                world.retire_mutator_gc_for_test(id).unwrap().acknowledge();
            }
        }
    }
}

fn assert_both_admissions_preserve(heap: &mut TaggedHeap, headers: &[TaggedValue]) {
    let before = CollectionSnapshot::capture(heap, headers);
    // SAFETY: this fixture owns the stopped heap and every registered owner;
    // no worker can mutate mark words or consume the handoff during snapshots.
    assert!(matches!(
        unsafe { heap.permit_concurrent_mark() },
        Err(ConcurrentMarkAdmissionError::CollectorBusy)
    ));
    assert_eq!(CollectionSnapshot::capture(heap, headers), before);
    assert_eq!(
        heap.begin_minor_collection(),
        Err(MinorCollectionAdmissionError::CollectorBusy)
    );
    assert_eq!(CollectionSnapshot::capture(heap, headers), before);
}

#[test]
fn minor_and_major_admissions_reject_satb_residuals_before_collection_mutations() {
    for enabled in [false, true] {
        for source in SatbResidualSource::iter() {
            let mut heap = heap(enabled);
            let orphan = heap.alloc_float(317.5);
            inject_satb_preimage(&mut heap, source, orphan);
            assert!(!heap.mark_in_progress && !heap.generational.major_in_progress);
            assert!(!heap.concurrent_mark_running && !heap.sweep_in_progress);
            assert!(heap.gray_queue.is_empty());
            let before = CollectionSnapshot::capture(&heap, &[orphan]);
            let words = before.satb_published.as_ref().unwrap().len()
                + before
                    .satb_mutators
                    .iter()
                    .map(|owner| owner.words.len())
                    .sum::<usize>();
            assert_eq!(words, 1, "GEN1={enabled}, {source:?}");
            assert_both_admissions_preserve(&mut heap, &[orphan]);
            // Cleanup the intentionally unattached preimage through the same
            // stopped capability; no collector epoch was started by this test.
            let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
            assert_eq!(world.fold_satb().unwrap(), 1);
            world.ensure_satb_empty().unwrap();
            drop(world);
            heap.gray_queue.clear();
        }
    }
}

#[test]
fn minor_and_major_admissions_reject_poisoned_satb_without_collection_mutations() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        let orphan = heap.alloc_float(319.5);
        inject_satb_preimage(&mut heap, SatbResidualSource::OwnerPartial, orphan);
        heap.satb_handoff.poison_for_test();
        assert_both_admissions_preserve(&mut heap, &[orphan]);
        assert_eq!(
            heap.current_mutator_satb().satb.words_for_test(),
            &[MarkWord::of(orphan)]
        );
        // Poisoned state is terminal: this fixture does not fabricate a drain
        // or clear poison. Heap Drop abandons its owner buffers explicitly.
    }
}

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum MajorHandoffRoute {
    JoinedWorker,
    DroppedCapture,
}

#[test]
fn minor_rejection_then_major_drain_preserves_each_satb_orphan_source() {
    for enabled in [false, true] {
        for route in MajorHandoffRoute::iter() {
            for source in SatbResidualSource::iter() {
                let mut heap = heap(enabled);
                let child = heap.alloc_float(9.5);
                let root = heap.alloc_cons(child, TaggedValue::NIL);
                let garbage = heap.alloc_float(10.5);
                let orphan = heap.alloc_cons(TaggedValue::fixnum(331), TaggedValue::NIL);
                // SAFETY: the fixture owns every writer and registration; no
                // callbacks or allocation safepoints run through this capture.
                let mut capture = unsafe { heap.permit_concurrent_mark() }.unwrap().begin();
                capture.heap_mut().seed_root(root);
                match route {
                    MajorHandoffRoute::JoinedWorker => {
                        // Launch requires every SATB transport to be empty.
                        // Join before synthetic residual transport injection,
                        // so no worker can consume or race its published chunk.
                        capture.launch().unwrap();
                        heap.finish_concurrent_mark().unwrap();
                    }
                    MajorHandoffRoute::DroppedCapture => drop(capture),
                }
                assert!(heap.mark_in_progress());
                assert!(!heap.concurrent_mark_running());
                assert!(!heap.sweep_in_progress());
                assert_eq!(heap.generational.major_in_progress, enabled);
                assert_eq!(heap.generational.cycle, GenerationCycle::Major);
                assert!(
                    !heap.is_value_marked(orphan),
                    "orphan must be absent from the explicit root graph"
                );
                // Both routes now retain an unfinished major with no worker.
                // Keep this orphan ONLY in the selected SATB source; root and
                // child were seeded above, but orphan was never seeded/marked.
                inject_satb_preimage(&mut heap, source, orphan);
                assert!(!heap.is_value_marked(orphan));
                let transport = CollectionSnapshot::capture(&heap, &[child, garbage]);
                let residual_words = transport.satb_published.as_ref().unwrap().len()
                    + transport
                        .satb_mutators
                        .iter()
                        .map(|owner| owner.words.len())
                        .sum::<usize>();
                assert_eq!(residual_words, 1, "GEN1={enabled}, {route:?}, {source:?}");
                // This helper snapshots only after join or before any launch,
                // rejects both admissions without mutation, then drains the
                // one existing major using its complete external root graph.
                reject_and_recover(&mut heap, root, child, garbage);
                assert!(
                    heap.owns_heap_value_for_test(orphan),
                    "GEN1={enabled}, {route:?}, {source:?}"
                );
                assert_eq!(orphan.cons_car().bits(), TaggedValue::fixnum(331).bits());
                if enabled {
                    assert!(heap.value_is_old_for_test(orphan));
                }
                assert!(heap.satb_handoff.is_empty().unwrap());
                assert!(
                    heap.mutator_satb_states()
                        .all(|owner| owner.satb.is_empty())
                );
            }
        }
    }
}
