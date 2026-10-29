//! Transport-only admission tests. Test witnesses exercise metadata refusal;
//! actual stop/owner authority is exercised by rendezvous's integration tests.
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::transport::SharedRoot;
use crate::tagged::transport::root_table::with_root_table_lock_for_test;

fn identity(
    era: &ParallelHeapEra<'_>,
    owner: u64,
    generation: u64,
    epoch: u64,
) -> ParallelRootIdentity {
    let permit = era.register(owner, 0).unwrap();
    let key = permit.key();
    drop(permit);
    ParallelRootIdentity::new(key, NonZeroU64::new(generation).unwrap(), epoch)
}
fn reserve(
    era: &ParallelHeapEra<'_>,
    id: ParallelRootIdentity,
    capacity: usize,
) -> PreparedParallelRootBatch {
    PreparedParallelRootBatch::reserve(id, era.test_root_epoch_retention(), capacity).unwrap()
}
fn publish(
    era: &ParallelHeapEra<'_>,
    id: ParallelRootIdentity,
    roots: &[SharedRoot],
) -> ParallelRootBatchLease {
    reserve(era, id, roots.len())
        .publish(&OwnerRootAdmission::for_test(id, roots))
        .unwrap()
}
fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}
fn outstanding(error: &ParallelRootBatchError, readers: usize, abandoned: usize) {
    assert!(
        matches!(error, ParallelRootBatchError::Transport(RootBatchError::ReadersOutstanding {
        readers: got_readers, abandoned: got_abandoned,
    }) if *got_readers == readers && *got_abandoned == abandoned)
    );
}

#[test]
fn real_root_payload_crosses_thread_without_heap_view_and_retires() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 51, 1, 3);
        let expected: Vec<_> = roots.iter().map(SharedRoot::parallel_word).collect();
        let lease = publish(&era, id, &roots);
        assert_eq!(lease.root_count(), 4);
        assert_eq!(lease.word_count(), 4);
        let lease = std::thread::spawn(move || {
            let admission = StoppedRootAdmission::for_test(id, 3);
            let first = lease.reader(&admission).unwrap();
            let second = lease.reader(&admission).unwrap();
            let mut copied = Vec::new();
            first.append_words(&mut copied);
            assert_eq!(copied, expected);
            outstanding(&lease.validate_no_readers().unwrap_err(), 2, 0);
            first.finish().unwrap();
            outstanding(&lease.validate_no_readers().unwrap_err(), 1, 0);
            second.finish().unwrap();
            lease.validate_no_readers().unwrap();
            lease
        })
        .join()
        .unwrap();
        assert_eq!(lease.try_finish().unwrap().root_count(), 4);
        drop(roots);
        finish(era);
    });
}

#[test]
fn publish_checks_owner_generation_epoch_before_filling_and_allows_retry() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 52, 2, 4);
        let foreign_key = identity(&era, 53, 2, 4).key();
        let alternatives = [
            ParallelRootIdentity::new(foreign_key, id.publication_generation(), 4),
            ParallelRootIdentity::new(id.key(), NonZeroU64::new(3).unwrap(), 4),
            ParallelRootIdentity::new(id.key(), id.publication_generation(), 5),
        ];
        let mut prepared = reserve(&era, id, roots.len());
        let opaque_address = prepared.payload.words.as_ptr();
        for wrong in alternatives {
            let failure = prepared
                .publish(&OwnerRootAdmission::for_test(wrong, &roots))
                .unwrap_err();
            assert!(matches!(failure.error(), ParallelRootBatchError::Identity));
            (prepared, _) = failure.into_parts();
            assert_eq!(prepared.payload.used, 0);
            assert_eq!(prepared.prepared.payload.used, 0);
            assert_eq!(prepared.payload.words.as_ptr(), opaque_address);
        }
        let lease = prepared
            .publish(&OwnerRootAdmission::for_test(id, &roots))
            .unwrap();
        lease.try_finish().unwrap();
        drop(roots);
        finish(era);
    });
}

#[test]
fn capacity_failure_keeps_reserved_payload_and_can_publish_smaller_snapshot() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 54, 1, 1);
        let prepared = reserve(&era, id, 2);
        let failure = prepared
            .publish(&OwnerRootAdmission::for_test(id, &roots))
            .unwrap_err();
        assert!(matches!(
            failure.error(),
            ParallelRootBatchError::Transport(RootBatchError::Capacity {
                required: 4,
                capacity: 2
            })
        ));
        let (prepared, _) = failure.into_parts();
        assert_eq!(prepared.payload.used, 0);
        let lease = prepared
            .publish(&OwnerRootAdmission::for_test(id, &roots[..2]))
            .unwrap();
        assert_eq!(lease.root_count(), 2);
        assert_eq!(lease.word_count(), 2);
        lease.try_finish().unwrap();
        drop(roots);
        finish(era);
    });
}

#[test]
fn foreign_root_failure_keeps_reservation_for_correct_heap_retry() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        Context::with_test_rooted_mutator_era(|other_era, other_roots| {
            let id = identity(&era, 55, 1, 1);
            let failure = reserve(&era, id, 4)
                .publish(&OwnerRootAdmission::for_test(id, &other_roots))
                .unwrap_err();
            assert!(matches!(
                failure.error(),
                ParallelRootBatchError::Transport(RootBatchError::ForeignHeap { .. })
            ));
            let (prepared, _) = failure.into_parts();
            assert_eq!(prepared.payload.used, 0);
            prepared
                .publish(&OwnerRootAdmission::for_test(id, &roots))
                .unwrap()
                .try_finish()
                .unwrap();
            drop(other_roots);
            finish(other_era);
        });
        drop(roots);
        finish(era);
    });
}

#[test]
fn stopped_admission_requires_exact_identity_and_covering_nonzero_epoch() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 56, 7, 8);
        let lease = publish(&era, id, &roots);
        let other_key = identity(&era, 57, 7, 8).key();
        for wrong in [
            ParallelRootIdentity::new(other_key, id.publication_generation(), 8),
            ParallelRootIdentity::new(id.key(), NonZeroU64::new(8).unwrap(), 8),
            ParallelRootIdentity::new(id.key(), id.publication_generation(), 7),
        ] {
            let admission = StoppedRootAdmission::for_test(wrong, 9);
            assert!(matches!(
                lease.reader(&admission),
                Err(ParallelRootBatchError::Identity)
            ));
        }
        for current in [0, 7] {
            let admission = StoppedRootAdmission::for_test(id, current);
            assert!(
                matches!(lease.reader(&admission), Err(ParallelRootBatchError::StopEpoch { publication: 8, current: actual }) if actual == current)
            );
        }
        let admission = StoppedRootAdmission::for_test(id, 9);
        lease.reader(&admission).unwrap().finish().unwrap();
        lease.try_finish().unwrap();
        drop(roots);
        finish(era);
    });
}

#[test]
fn native_epoch_zero_snapshot_covers_consecutive_stops() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 58, 1, 0);
        let lease = publish(&era, id, &roots);
        for epoch in [1, 2] {
            let admission = StoppedRootAdmission::for_test(id, epoch);
            let reader = lease.reader(&admission).unwrap();
            let mut words = Vec::new();
            reader.append_words(&mut words);
            assert_eq!(words.len(), 4);
            assert_eq!(
                words.iter().copied().fold(0, usize::wrapping_add),
                roots
                    .iter()
                    .map(SharedRoot::parallel_word)
                    .fold(0, usize::wrapping_add)
            );
            reader.finish().unwrap();
        }
        lease.try_finish().unwrap();
        drop(roots);
        finish(era);
    });
}

#[test]
fn busy_retirement_returns_same_payload_without_closing_or_losing_roots() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 59, 1, 1);
        let mut lease = Some(publish(&era, id, &roots));
        let payload_address = Arc::as_ptr(&lease.as_ref().unwrap().payload);
        with_root_table_lock_for_test(era.heap_identity(), || {
            let failure = lease.take().unwrap().try_finish().unwrap_err();
            assert!(matches!(
                failure.error(),
                ParallelRootBatchError::Transport(RootBatchError::Busy)
            ));
            lease = Some(failure.into_parts().0);
        });
        let lease = lease.unwrap();
        assert_eq!(lease.identity(), id);
        assert_eq!(Arc::as_ptr(&lease.payload), payload_address);
        assert_eq!(lease.root_count(), 4);
        lease.validate_no_readers().unwrap();
        lease.try_finish().unwrap();
        drop(roots);
        finish(era);
    });
}

#[test]
fn abandoned_reader_blocks_retirement_and_preserves_closing_scan() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 60, 1, 1);
        let lease = publish(&era, id, &roots);
        let admission = StoppedRootAdmission::for_test(id, 1);
        drop(lease.reader(&admission).unwrap());
        outstanding(&lease.validate_no_readers().unwrap_err(), 0, 1);
        let failure = lease.try_finish().unwrap_err();
        outstanding(failure.error(), 0, 1);
        let (lease, _) = failure.into_parts();
        assert_eq!(lease.identity(), id);
        let reader = lease.reader(&admission).unwrap();
        let mut copied = Vec::new();
        reader.append_words(&mut copied);
        assert_eq!(
            copied,
            roots
                .iter()
                .map(SharedRoot::parallel_word)
                .collect::<Vec<_>>()
        );
        reader.finish().unwrap();
        outstanding(&lease.validate_no_readers().unwrap_err(), 0, 1);
        // Deliberate unfinished obligation: both lease and era fail closed.
        drop(lease);
        drop(roots);
        drop(era);
    });
}

#[test]
fn forgotten_reader_cannot_disappear_from_retirement_census() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let id = identity(&era, 61, 1, 1);
        let lease = publish(&era, id, &roots);
        let admission = StoppedRootAdmission::for_test(id, 1);
        std::mem::forget(lease.reader(&admission).unwrap());
        outstanding(&lease.validate_no_readers().unwrap_err(), 1, 0);
        let failure = lease.try_finish().unwrap_err();
        outstanding(failure.error(), 1, 0);
        let (lease, _) = failure.into_parts();
        assert_eq!(lease.root_count(), 4);
        drop(lease);
        drop(roots);
        drop(era);
    });
}
