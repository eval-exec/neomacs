//! Real STW collection through the canonical synthetic-owner protocol.
#![cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]

use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::gc::{ProofEra as ParallelHeapEra, TaggedHeap};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

// These helpers serve one-thread histories; an unexpected refusal is terminal.
fn retire(owner: Mutator<'_>) {
    owner.retire_rooted(&[]).unwrap();
}
fn complete(stopped: AllStopped<'_>) {
    stopped.complete().unwrap();
}

// Preserve the concurrent admission histories without imposing a serial start.
// Their unresolved admission retry policies are audited independently.
mod contended {
    use super::*;

    pub(super) fn retry<T>(mut f: impl FnMut() -> Result<T, RendezvousError>) -> T {
        loop {
            match f() {
                Ok(value) => return value,
                Err(RendezvousError::Busy) => thread::yield_now(),
                Err(error) => panic!("unexpected collection refusal: {error:?}"),
            }
        }
    }
    pub(super) fn activate<'era>(
        mut permit: MutatorPermit<'era>,
        heap: HeapIdentity,
        owner: u64,
    ) -> Mutator<'era> {
        loop {
            match permit.activate(heap, owner) {
                Ok(mutator) => return mutator,
                Err(failure) if failure.reason == RegistrationError::Busy => {
                    permit = failure.into_permit();
                    thread::yield_now();
                }
                Err(failure) => panic!("activation: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn apply(mut image: ResumeGcImage<'_, '_>) {
        loop {
            match image.apply() {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    image = failure.into_image();
                    thread::yield_now();
                }
                Err(failure) => panic!("restore: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn retire(mut owner: Mutator<'_>) {
        loop {
            match owner.retire_rooted(&[]) {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    owner = failure.into_owner();
                    thread::yield_now();
                }
                Err(failure) => panic!("retire: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn complete(mut stopped: AllStopped<'_>) {
        loop {
            match stopped.complete() {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    stopped = failure.into_stopped();
                    thread::yield_now();
                }
                Err(failure) => panic!("complete: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn assert_owned<K: EraKeeper>(
        era: &ParallelHeapEra<'_, K>,
        stop: &AllStopped<'_>,
        words: &[usize],
        expected: bool,
    ) {
        retry(|| {
            era.with_stopped_heap_for_test(stop, |heap| super::check_owned(heap, words, expected))
        });
    }
}
fn receive<T>(rx: &mpsc::Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(30))
        .expect("actor reaches collection boundary")
}
fn assert_owned<K: EraKeeper>(
    era: &ParallelHeapEra<'_, K>,
    stop: &AllStopped<'_>,
    words: &[usize],
    expected: bool,
) {
    era.with_stopped_heap_for_test(stop, |heap| check_owned(heap, words, expected))
        .unwrap();
}
fn check_owned(heap: &TaggedHeap, words: &[usize], expected: bool) {
    for &word in words {
        assert_eq!(
            heap.owns_heap_value_for_test(
                // SAFETY: `word` is the bit pattern `graph` stored from
                // a cons allocated in this heap (`head.bits()` /
                // `child.bits()`) before the stop. Rehydration only
                // feeds the ownership query.
                unsafe { Value::from_bits(word) }
            ),
            expected,
            "ownership of opaque observation {word:#x}"
        );
    }
}

pub(crate) type Graph = (SharedRoot, [usize; 2], Value);
pub(crate) type RequestersFixture = (
    Graph,
    Graph,
    [usize; 2],
    [usize; 2],
    [Graph; 3],
    usize,
    Graph,
    usize,
    bool,
);
pub(crate) type AfterWords = ([usize; 2], [usize; 2], [usize; 2]);
pub(crate) type ThenFixture = (Graph, [usize; 2], [usize; 2]);

/// One row per former value lib-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollectionCase {
    TwoRequestersCollectTransitiveWorldPrivateAndImageRoots,
    NativeImageLogsFoldOnceAcrossRepeatedFullMajors,
    ForeignStopGuardAndMissingRootAdmissionRefuseBeforeCollection,
    RetainedReaderRefusesCollectionAndAbortKeepsBacking,
    ExplicitlyFinishedReaderAllowsRealCollection,
    RepeatedWorldsHaveDistinctHeapsAndIndependentCollectionCycles,
    SerialHeapRestorationReclaimsFormerOwnerRootsAfterResume,
    ShouldCollectStoppedTracksPublishedDebtWithoutConsumingTrace,
    StoppedSummariesRejectsZeroEpochOutsidePreStopNative,
    CollectRequestedAfterTraceRefusesEarlyWithoutWorldWalk,
    ShouldCollectStoppedCountsKeeperHeapSerialDebt,
    PrepareFinishFoldsSerialFactsAndYoungShellWithoutCollection,
}

impl CollectionCase {
    pub const ALL: &'static [Self] = &[
        Self::TwoRequestersCollectTransitiveWorldPrivateAndImageRoots,
        Self::NativeImageLogsFoldOnceAcrossRepeatedFullMajors,
        Self::ForeignStopGuardAndMissingRootAdmissionRefuseBeforeCollection,
        Self::RetainedReaderRefusesCollectionAndAbortKeepsBacking,
        Self::ExplicitlyFinishedReaderAllowsRealCollection,
        Self::RepeatedWorldsHaveDistinctHeapsAndIndependentCollectionCycles,
        Self::SerialHeapRestorationReclaimsFormerOwnerRootsAfterResume,
        Self::ShouldCollectStoppedTracksPublishedDebtWithoutConsumingTrace,
        Self::StoppedSummariesRejectsZeroEpochOutsidePreStopNative,
        Self::CollectRequestedAfterTraceRefusesEarlyWithoutWorldWalk,
        Self::ShouldCollectStoppedCountsKeeperHeapSerialDebt,
        Self::PrepareFinishFoldsSerialFactsAndYoungShellWithoutCollection,
    ];
}

/// Dispatcher for the collection shells. The core shell performs the serial
/// setup and admits the era; the bodies and their assertions stay here.
pub(crate) fn run_collection_test<K, W>(
    case: CollectionCase,
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
    graph_pair_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, (Graph, Graph))),
    requesters_era: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RequestersFixture),
    ),
    empty_outer_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    empty_nested_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    mutator_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    graph_era_then: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            ThenFixture,
        ) -> (QuiescentCollector<'e>, K, AfterWords),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    match case {
        CollectionCase::TwoRequestersCollectTransitiveWorldPrivateAndImageRoots => {
            two_requesters_collect_transitive_world_private_and_image_roots(
                requesters_era,
                world_roots,
            );
        }
        CollectionCase::NativeImageLogsFoldOnceAcrossRepeatedFullMajors => {
            native_image_logs_fold_once_across_repeated_full_majors(graph_pair_era, world_roots);
        }
        CollectionCase::ForeignStopGuardAndMissingRootAdmissionRefuseBeforeCollection => {
            foreign_stop_guard_and_missing_root_admission_refuse_before_collection(
                empty_outer_era,
                empty_nested_era,
                mutator_era,
                world_roots,
            );
        }
        CollectionCase::RetainedReaderRefusesCollectionAndAbortKeepsBacking => {
            retained_reader_refuses_collection_and_abort_keeps_backing(graph_era, world_roots);
        }
        CollectionCase::ExplicitlyFinishedReaderAllowsRealCollection => {
            explicitly_finished_reader_allows_real_collection(graph_era, world_roots);
        }
        CollectionCase::RepeatedWorldsHaveDistinctHeapsAndIndependentCollectionCycles => {
            repeated_worlds_have_distinct_heaps_and_independent_collection_cycles(
                graph_era,
                world_roots,
            );
        }
        CollectionCase::SerialHeapRestorationReclaimsFormerOwnerRootsAfterResume => {
            serial_heap_restoration_reclaims_former_owner_roots_after_resume(
                graph_era_then,
                world_roots,
            );
        }
        CollectionCase::ShouldCollectStoppedTracksPublishedDebtWithoutConsumingTrace => {
            should_collect_stopped_tracks_published_debt_without_consuming_trace(
                graph_pair_era,
                world_roots,
            );
        }
        CollectionCase::StoppedSummariesRejectsZeroEpochOutsidePreStopNative => {
            stopped_summaries_rejects_zero_epoch_outside_pre_stop_native(graph_era);
        }
        CollectionCase::CollectRequestedAfterTraceRefusesEarlyWithoutWorldWalk => {
            collect_requested_after_trace_refuses_early_without_world_walk(graph_era);
        }
        CollectionCase::ShouldCollectStoppedCountsKeeperHeapSerialDebt => {
            should_collect_stopped_counts_keeper_heap_serial_debt(graph_pair_era, world_roots);
        }
        CollectionCase::PrepareFinishFoldsSerialFactsAndYoungShellWithoutCollection => {
            prepare_finish_folds_serial_facts_and_young_shell_without_collection(empty_outer_era);
        }
    }
}

fn two_requesters_collect_transitive_world_private_and_image_roots<K, W>(
    requesters_era: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RequestersFixture),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use contended::{activate, apply, assert_owned, complete, retire, retry};

    requesters_era(&mut |era,
                         (
        a,
        b,
        world,
        legacy,
        logs,
        dead,
        shared_only,
        internal,
        generational,
    )| {
        let heap = era.heap_identity();
        let rv = era.rendezvous();
        let permits = [era.register(801, 1).unwrap(), era.register(802, 1).unwrap()];
        let barrier = Arc::new(Barrier::new(3));
        let (tx, rx) = mpsc::channel();
        let observations = [a.1, b.1];
        thread::scope(|scope| {
            for (index, (permit, root)) in permits.into_iter().zip([a.0, b.0]).enumerate() {
                let barrier = Arc::clone(&barrier);
                let tx = tx.clone();
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, 801 + index as u64);
                    {
                        let mut run = owner.run_access().unwrap();
                        let hot = &mut run.state().hot;
                        hot.bytes_since_gc = 64 + index;
                        if generational {
                            hot.pacing.promoted_since_major = 32;
                            hot.pacing.minors_since_major = 4;
                            hot.pacing.stress_cycles_since_major = 3;
                        }
                    }
                    barrier.wait();
                    let request = retry(|| rv.request());
                    let Request::Active(epoch) = request else {
                        panic!("unexpected debt")
                    };
                    retry(|| owner.acknowledge(epoch, std::slice::from_ref(&root)));
                    // Only the batch publication roots this graph during mark.
                    drop(root);
                    tx.send((owner.key(), epoch)).unwrap();
                    let response = owner.begin_restore_retry(false).unwrap();
                    apply(response);
                    {
                        let mut run = owner.run_access_after_resume_drain().unwrap();
                        assert_eq!(run.state().hot.bytes_since_gc, 0);
                        assert_eq!(run.state().hot.pacing.promoted_since_major, 0);
                        assert_eq!(run.state().hot.pacing.minors_since_major, 0);
                        assert_eq!(run.state().hot.pacing.stress_cycles_since_major, 0);
                        assert_eq!(run.state().hot.bytes_banked_at_resets, (64 + index) as u64);
                        run.state().root_scratch.clear();
                    }
                    retire(owner);
                });
            }
            barrier.wait();
            let first = receive(&rx);
            let second = receive(&rx);
            assert_eq!(first.1, second.1, "requesters coalesce one epoch");
            let mut stopped = retry(|| era.stop());
            // S3 exactly-once-composition pin: each owner's ACK batch is its
            // only root source. The published image's root words come from
            // `OwnerGcState::root_scratch`, which must stay empty here; a
            // duplicate of the batch in `CollectionWork.roots` would make the
            // union of pre-mark sources visit the same logical roots twice.
            for key in [first.0, second.0] {
                let (roots, other) = retry(|| stopped.owner_work_census_for_test(key));
                assert!(
                    roots.is_empty(),
                    "the ACK batch must be the owner's only root source, \
                     but the image carries root words {roots:?} (other work {other:?})"
                );
            }
            let [root, remembered, cons_write] = logs;
            retry(|| {
                stopped.with_owner_image(first.0, |image| {
                    image.seed_collection_words_for_test(
                        MarkWord::of(root.2),
                        MarkWord::of(remembered.2),
                        MarkWord::of(cons_write.2),
                    );
                })
            });
            let log_words = [root.1, remembered.1, cons_write.1].concat();
            drop((root.0, remembered.0, cons_write.0));
            let report = retry(|| era.collect_stopped(&mut stopped, world_roots));
            assert!(report.major);
            assert_eq!(report.epoch, first.1.get());
            assert_eq!(report.frozen_owners, 2);
            for words in observations.iter().chain([&world, &legacy]) {
                assert_owned(&era, &stopped, words, true);
            }
            assert_owned(&era, &stopped, &log_words, true);
            assert_owned(&era, &stopped, &shared_only.1, true);
            assert_owned(&era, &stopped, &[internal], true);
            assert_owned(&era, &stopped, &[dead], false);
            // S2 two-owner join: each owner ACKs one root word. This counter is
            // taken at the SEED site, so it counts the frozen owner words the
            // collector actually seeded, not merely the words it read. It is
            // asserted after the survival rows on purpose: survival alone passes
            // a seed-loop `take(1)` because a dropped SharedRoot's transport
            // cell can keep the object alive for one collection.
            assert_eq!(
                report.frozen_root_words, 2,
                "the keeper must seed both owners' published batches"
            );
            complete(stopped);
        });
        drop(shared_only.0);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn native_image_logs_fold_once_across_repeated_full_majors<K, W>(
    graph_pair_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, (Graph, Graph))),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_pair_era(&mut |era, (graph, remembered)| {
        let mut owner = era
            .register(803, 1)
            .unwrap()
            .activate(era.heap_identity(), 803)
            .unwrap();
        let key = owner.key();
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
        drop(graph.0);
        let mut previous_cycle = None;
        let remembered_observations = remembered.1;
        let mut remembered = Some(remembered);
        for round in 0..2 {
            let mut stopped = era.stop().unwrap();
            let work_words = if let Some((root, words, word)) = remembered.take() {
                stopped
                    .with_owner_image(key, |image| {
                        image.seed_collection_words_for_test(
                            MarkWord::of(Value::NIL),
                            MarkWord::of(word),
                            MarkWord::of(Value::NIL),
                        );
                    })
                    .unwrap();
                drop(root);
                words
            } else {
                [0; 2]
            };
            let report = era.collect_stopped(&mut stopped, world_roots).unwrap();
            assert!(report.major);
            if let Some(previous) = previous_cycle {
                assert_eq!(report.cycle, previous + 1);
            }
            previous_cycle = Some(report.cycle);
            assert_owned(&era, &stopped, &graph.1, true);
            if round == 0 {
                assert_owned(&era, &stopped, &work_words, true);
            } else {
                assert_owned(&era, &stopped, &remembered_observations, false);
            }
            stopped
                .with_owner_image(key, |image| {
                    let facts = image.summary();
                    assert_eq!(facts.remembered_words, 0);
                    assert_eq!(facts.cons_writes, 0);
                    assert_eq!(facts.bytes_since_gc, 0);
                    assert_eq!(facts.promoted_since_major, 0);
                    assert_eq!(facts.minors_since_major, 0);
                    assert_eq!(facts.stress_cycles_since_major, 0);
                    assert!(image.young_empty());
                })
                .unwrap();
            complete(stopped);
        }
        owner.leave_native().unwrap();
        {
            let mut run = owner.run_access().unwrap();
            run.state().root_scratch.clear();
        }
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn foreign_stop_guard_and_missing_root_admission_refuse_before_collection<K, W>(
    empty_outer_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    empty_nested_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    mutator_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    empty_outer_era(0, &mut |outer| {
        empty_nested_era(0, &mut |foreign| {
            let mut stop = foreign.stop().unwrap();
            assert_eq!(
                outer.collect_stopped(&mut stop, world_roots).unwrap_err(),
                RendezvousError::Identity
            );
            assert_eq!(stop.epoch().get(), 1);
            stop.trace(|_, _| Ok(())).unwrap();
            complete(stop);
            let (collector, keeper) = foreign.finish().unwrap();
            drop(collector);
            drop(keeper);
        });
        let (collector, keeper) = outer.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
    mutator_era(0, &mut |era| {
        let mut stopped = era.stop().unwrap();
        assert_eq!(
            era.collect_stopped(&mut stopped, world_roots).unwrap_err(),
            RendezvousError::Publication
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn retained_reader_refuses_collection_and_abort_keeps_backing<K, W>(
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_era(301, &mut |era, graph| {
        let mut owner = era
            .register(804, 1)
            .unwrap()
            .activate(era.heap_identity(), 804)
            .unwrap();
        let key = owner.key();
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
        drop(graph.0);
        let mut stopped = era.stop().unwrap();
        stopped
            .with_root_reader(key, |reader| std::mem::forget(reader))
            .unwrap();
        assert!(matches!(
            era.shutdown_admission_stopped_for_test(&stopped).unwrap(),
            Err(crate::tagged::gc::MarkFinishError::FacadeRetained(_))
        ));
        assert_eq!(
            era.collect_stopped(&mut stopped, world_roots).unwrap_err(),
            RendezvousError::Readers
        );
        assert_owned(&era, &stopped, &graph.1, true);
        drop(stopped);
        drop(owner);
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

fn explicitly_finished_reader_allows_real_collection<K, W>(
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_era(302, &mut |era, graph| {
        let mut owner = era
            .register(805, 1)
            .unwrap()
            .activate(era.heap_identity(), 805)
            .unwrap();
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
        drop(graph.0);
        let mut stopped = era.stop().unwrap();
        stopped
            .with_root_reader(owner.key(), |reader| reader.finish().unwrap())
            .unwrap();
        assert!(
            era.collect_stopped(&mut stopped, world_roots)
                .unwrap()
                .major
        );
        assert_owned(&era, &stopped, &graph.1, true);
        complete(stopped);
        owner.leave_native().unwrap();
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn repeated_worlds_have_distinct_heaps_and_independent_collection_cycles<K, W>(
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    let mut identities = std::collections::HashSet::new();
    for round in 0..8 {
        graph_era(400 + round, &mut |era, graph| {
            assert!(identities.insert(era.heap_identity()));
            let mut owner = era
                .register(806, 1)
                .unwrap()
                .activate(era.heap_identity(), 806)
                .unwrap();
            owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
            drop(graph.0);
            let mut stopped = era.stop().unwrap();
            let report = era.collect_stopped(&mut stopped, world_roots).unwrap();
            assert_eq!(report.epoch, 1);
            assert!(report.major);
            assert_owned(&era, &stopped, &graph.1, true);
            complete(stopped);
            owner.leave_native().unwrap();
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        });
    }
}

fn serial_heap_restoration_reclaims_former_owner_roots_after_resume<K, W>(
    graph_era_then: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            ThenFixture,
        ) -> (QuiescentCollector<'e>, K, AfterWords),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_era_then(&mut |era, (participant, world, legacy)| {
        let mut owner = era
            .register(807, 1)
            .unwrap()
            .activate(era.heap_identity(), 807)
            .unwrap();
        owner
            .enter_native(std::slice::from_ref(&participant.0))
            .unwrap();
        drop(participant.0);
        let mut stopped = era.stop().unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        assert_owned(&era, &stopped, &participant.1, true);
        complete(stopped);
        owner.leave_native().unwrap();
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        (collector, keeper, (participant.1, world, legacy))
    });
}

/// K5 a4.1: `should_collect_stopped` aggregates `bytes_since_gc` through
/// `AllStopped::summaries` (F1 `lock_for_keeper`, zero census `Busy`), compares
/// against `TaggedHeap::gc_threshold()`, and does not consume `stopped.trace`
/// so `collect_requested` can run immediately on the same `AllStopped`.
fn should_collect_stopped_tracks_published_debt_without_consuming_trace<K, W>(
    graph_pair_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, (Graph, Graph))),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_pair_era(&mut |era, (keep, garbage)| {
        let garbage_words = garbage.1;
        drop(garbage.0);

        let mut owner = era
            .register(808, 1)
            .unwrap()
            .activate(era.heap_identity(), 808)
            .unwrap();
        owner.enter_native(std::slice::from_ref(&keep.0)).unwrap();

        // Stop 1: read current entry-0 debt and threshold; below threshold -> false,
        // and `stopped.trace` remains unconsumed.
        let mut stopped = era.stop().unwrap();
        let threshold = era
            .with_stopped_heap_for_test(&stopped, |heap| heap.gc_threshold())
            .unwrap();
        let baseline = stopped
            .summaries()
            .unwrap()
            .census()
            .unwrap()
            .bytes_since_gc;
        assert!(
            baseline < threshold,
            "fresh fixture debt ({baseline}) must start below threshold ({threshold})"
        );
        assert_owned(&era, &stopped, &garbage_words, true);
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(false),
            "aggregate debt below threshold must not trigger collection"
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);

        // Charge the peer so entry-0 + peer reaches `threshold` exactly, then
        // drop `keep.0` after native publication so `keep` stays rooted only
        // through the owner's native batch.
        owner.leave_native().unwrap();
        {
            let mut run = owner.run_access().unwrap();
            run.state().hot.bytes_since_gc = threshold - baseline;
        }
        owner.enter_native(std::slice::from_ref(&keep.0)).unwrap();
        drop(keep.0);

        // Stop 2: aggregate debt meets threshold -> true, and `collect_requested`
        // runs on the SAME `AllStopped` (proving `stopped.trace` was not consumed),
        // preserves native-rooted survivors, reclaims unrooted garbage, consumes
        // the stop's single trace, and resets `bytes_since_gc` below threshold.
        let mut stopped = era.stop().unwrap();
        assert_owned(&era, &stopped, &garbage_words, true);
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(true),
            "published peer debt reaching threshold must report collection due"
        );
        // SAFETY: `world_roots` is the core shell's canonical Baton World-root
        // walk for this stopped era.
        let report = unsafe { era.collect_requested(&mut stopped, world_roots) }.unwrap();
        assert!(report.major);
        assert_eq!(report.epoch, stopped.epoch().get());
        assert_eq!(report.frozen_owners, 1);
        assert_eq!(report.frozen_root_words, 1);
        assert_owned(&era, &stopped, &keep.1, true);
        assert_owned(&era, &stopped, &garbage_words, false);
        assert_eq!(
            stopped.trace(|_, _| Ok(())),
            Err(RendezvousError::Publication),
            "collect_requested must consume the stop's single trace"
        );
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(false),
            "collection resets bytes_since_gc below threshold"
        );
        complete(stopped);

        owner.leave_native().unwrap();
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// K5 a4.1 negative row: `StoppedSummaries::from_gate` accepts `state.epoch == 0`
/// only for a genuine pre-first-stop `Mode::NativeStopped` entry whose root batch
/// also carries `publication_epoch == 0`. Both a non-native (`Mode::Stopped`)
/// zero-epoch entry and a `Mode::NativeStopped` entry with `state.epoch == 0`
/// against a nonzero root batch `publication_epoch` must refuse with
/// `RegistrationError::Publication`.
fn stopped_summaries_rejects_zero_epoch_outside_pre_stop_native<K>(
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    graph_era(809, &mut |era, graph| {
        let permit = era.register(809, 1).unwrap();
        let key = permit.key();
        let entry = {
            let gate = era.registry().lock().unwrap();
            Arc::clone(&gate.entries[1])
        };
        assert_eq!(entry.key, key);
        let mut owner = permit.activate(era.heap_identity(), 809).unwrap();

        // Publish before Stop #1 completes: `state.epoch == 0`,
        // `mode == Mode::NativeStopped`, root batch `publication_epoch == 0`.
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();

        let mut stopped = era.stop().unwrap();
        assert!(
            stopped.summaries().is_ok(),
            "genuine pre-stop NativeStopped entry must pass summaries"
        );
        // Case A: non-native (`Published`, `Mode::Stopped`) with `epoch == 0`
        // must refuse with `Publication`.
        {
            let mut state = entry.state.lock().unwrap();
            assert_eq!(state.phase, EntryPhase::Published);
            assert_eq!(state.epoch, 0);
            state.rendezvous.mode = rendezvous::Mode::Stopped;
        }
        assert_eq!(
            stopped.summaries().unwrap_err(),
            RegistrationError::Publication,
            "non-native Mode::Stopped entry with epoch == 0 must refuse Publication"
        );
        entry.state.lock().unwrap().rendezvous.mode = rendezvous::Mode::NativeStopped;
        // Case B (N4): `publication_generation == 0` must still refuse with
        // `Publication` even on a pre-stop `Mode::NativeStopped` entry.
        let saved_generation = {
            let mut state = entry.state.lock().unwrap();
            std::mem::replace(&mut state.publication_generation, 0)
        };
        assert_eq!(
            stopped.summaries().unwrap_err(),
            RegistrationError::Publication,
            "publication_generation == 0 must refuse Publication even for pre-stop NativeStopped"
        );
        entry.state.lock().unwrap().publication_generation = saved_generation;
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);

        // Re-publish after Stop #1 completed (`completed == 1`): root batch now
        // carries `publication_epoch == 1`, matching `state.epoch == 1`.
        owner.leave_native().unwrap();
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();

        let mut stopped = era.stop().unwrap();
        assert!(
            stopped.summaries().is_ok(),
            "post-stop NativeStopped entry with matching nonzero epoch must pass"
        );
        // Case B: `Mode::NativeStopped` with `state.epoch == 0` while root batch
        // has `publication_epoch != 0` must refuse with `Publication`.
        {
            let mut state = entry.state.lock().unwrap();
            assert_eq!(state.rendezvous.mode, rendezvous::Mode::NativeStopped);
            assert_eq!(state.epoch, 1);
            assert_eq!(
                state.roots.as_ref().unwrap().identity().publication_epoch(),
                1
            );
            state.epoch = 0;
        }
        assert_eq!(
            stopped.summaries().unwrap_err(),
            RegistrationError::Publication,
            "NativeStopped entry with epoch == 0 and nonzero root-batch publication_epoch must refuse Publication"
        );
        entry.state.lock().unwrap().epoch = 1;
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);

        owner.leave_native().unwrap();
        retire(owner);
        drop(graph.0);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Lead #13986 (a4.2 condition N3): calling `collect_requested` on an already
/// traced `AllStopped` must refuse `RendezvousError::Publication` at the top of
/// `collect_stopped_kind`, before returning owner-cons leases, finishing root
/// readers, or invoking the World-root walk closure.
fn collect_requested_after_trace_refuses_early_without_world_walk<K>(
    graph_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Graph)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    graph_era(810, &mut |era, graph| {
        let mut owner = era
            .register(810, 1)
            .unwrap()
            .activate(era.heap_identity(), 810)
            .unwrap();
        owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();

        let mut stopped = era.stop().unwrap();
        stopped.trace_serial().unwrap();

        let mut world_walk_ran = false;
        // SAFETY: closure never executes on an already-traced stop.
        let result = unsafe {
            era.collect_requested(&mut stopped, |_, _, _| {
                world_walk_ran = true;
                Ok(Vec::new())
            })
        };
        assert_eq!(
            result,
            Err(RendezvousError::Publication),
            "collect_requested after trace_serial must refuse with Publication"
        );
        assert!(
            !world_walk_ran,
            "collect_requested must refuse an already-traced stop before running the World walk"
        );
        complete(stopped);

        owner.leave_native().unwrap();
        retire(owner);
        drop(graph.0);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Lead #13986 (a4.2 pacer condition): `should_collect_stopped` must include
/// the keeper heap's own `bytes_since_gc()` (serial-turn allocation debt) in
/// addition to published images, and `collect_requested` must collect cleanly
/// and reset both debt sources so `should_collect_stopped` returns `false` and
/// `era.finish()` succeeds.
fn should_collect_stopped_counts_keeper_heap_serial_debt<K, W>(
    graph_pair_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, (Graph, Graph))),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    graph_pair_era(&mut |era, (keep, garbage)| {
        let garbage_words = garbage.1;
        drop(garbage.0);

        let mut owner = era
            .register(811, 1)
            .unwrap()
            .activate(era.heap_identity(), 811)
            .unwrap();
        owner.enter_native(std::slice::from_ref(&keep.0)).unwrap();
        drop(keep.0);

        let mut stopped = era.stop().unwrap();
        let threshold = era.gc_threshold_for_test(&stopped).unwrap();
        let baseline = stopped
            .summaries()
            .unwrap()
            .census()
            .unwrap()
            .bytes_since_gc;
        assert!(baseline < threshold);
        assert_eq!(era.should_collect_stopped(&mut stopped), Ok(false));

        // Charge ONLY the keeper heap's own `bytes_since_gc` (zero peer debt).
        era.charge_serial_bytes_for_test(&stopped, threshold - baseline)
            .unwrap();
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(true),
            "keeper heap serial-turn debt reaching threshold must trigger collection"
        );
        // SAFETY: `world_roots` is the core shell's canonical Baton World-root walk.
        let report = unsafe { era.collect_requested(&mut stopped, world_roots) }.unwrap();
        assert!(report.major);
        assert_eq!(report.frozen_owners, 1);
        assert_eq!(report.frozen_root_words, 1);
        assert_owned(&era, &stopped, &keep.1, true);
        assert_owned(&era, &stopped, &garbage_words, false);
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(false),
            "collection resets both keeper heap and published bytes_since_gc"
        );
        complete(stopped);

        owner.leave_native().unwrap();
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// Lead #14210 nit 1: `EraCore::prepare_finish` folds live non-young serial
/// facts (e.g. `bytes_since_gc`) and resident young-log shell blocks accrued
/// directly on the keeper heap with no intervening collection, reversing
/// 878f675854's strict `restore_legacy` refusal (`DestinationOccupied` /
/// `LogNotLoaned`) at era finish.
fn prepare_finish_folds_serial_facts_and_young_shell_without_collection<K>(
    empty_outer_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    // Subcase A: non-young live serial fact (`bytes_since_gc`) with no collection.
    empty_outer_era(901, &mut |era| {
        let mut stopped = era.stop().unwrap();
        era.charge_serial_bytes_for_test(&stopped, 128).unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (mut collector, keeper) = era
            .finish()
            .expect("prepare_finish must fold non-young serial facts without DestinationOccupied");
        assert!(
            collector.registration_heap_mut().bytes_since_gc() >= 128,
            "folded serial-turn debt must survive era finish"
        );
        drop(collector);
        drop(keeper);
    });

    // Subcase B: resident young-log shell cons allocation with no collection.
    empty_outer_era(902, &mut |era| {
        let mut stopped = era.stop().unwrap();
        let cons_word = era.alloc_serial_cons_for_test(&stopped).unwrap();
        assert!(
            era.owns_heap_word_for_test(&stopped, cons_word).unwrap(),
            "positive control: allocated cons is owned by keeper heap"
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (mut collector, keeper) = era
            .finish()
            .expect("prepare_finish must fold resident shell young cons without LogNotLoaned");
        let heap = collector.registration_heap_mut();
        assert!(
            // SAFETY: bit pattern from a live cons just allocated on this heap.
            heap.owns_heap_value_for_test(unsafe { Value::from_bits(cons_word.bits()) }),
            "cons allocated onto resident shell must remain owned after era finish"
        );
        drop(collector);
        drop(keeper);
    });
}

/// E3.2: a heap that holds deferred-native payloads is admissible. The stopped
/// sweep no longer refuses these kinds; it detaches them for the post-stop
/// drain.
#[test]
fn stopped_admission_accepts_deferred_native_payloads() {
    let mut heap = TaggedHeap::new();
    unsafe { heap.alloc_user_ptr(std::ptr::null_mut(), None) };
    unsafe {
        heap.alloc_module_function(
            0,
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            Value::NIL,
            Value::NIL,
        )
    };
    assert!(
        super::collection::supported_for_test(&heap).is_ok(),
        "user-ptr and module-function payloads must be admissible once the \
         stopped sweep defers their reclamation"
    );
}
