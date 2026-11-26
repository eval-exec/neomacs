//! A real clear in a different heap must not reconstruct surviving geometry.
//!
//! Both heaps execute on this test's mutator. The certificate stays !Send, and
//! its owner remains allocated in the live heap while only the other heap is
//! collected. The counter is test-only metadata inside the existing State.

use super::super::{CompiledJournalMode, capture, force_compiled_journal_for_test, with_state};
use crate::tagged::gc::{
    TaggedHeap, collection_observation_epoch, collection_observed_metadata,
    mark_collection_observed, set_tagged_heap,
};
use crate::tagged::value::TaggedValue;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn geometry_rebuilds() -> usize {
    with_state(|state| state.borrow().observed.geometry_rebuilds_for_test())
}

#[test]
fn unrelated_heap_reclamation_preserves_live_envelope_geometry() {
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut live_heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut live_heap);
    let owner = live_heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL);
    let (_, reads) = capture(|| owner.cons_car());
    let reads = reads.expect("the retained owner supplies a real certificate");
    assert!(collection_observed_metadata(owner.bits()));
    let bounds = with_state(|state| state.borrow().observed.full_bounds());
    let window = live_heap.jit_barrier_window_for_test();
    let rebuilds = geometry_rebuilds();

    let mut other_heap = Box::new(TaggedHeap::new());
    // SAFETY: bare `TaggedHeap` unit test switching heaps on this thread.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&live_heap) };
    set_tagged_heap(&mut other_heap);
    let garbage = other_heap.alloc_cons(TaggedValue::make_int(7), TaggedValue::NIL);
    // A shared mark does not insert a dependency into this mutator's ledger.
    // Its actual reclamation must advance the shared epoch, not merely invoke
    // a sweep publisher with no marks removed.
    assert!(unsafe { mark_collection_observed(garbage.bits()) });
    let epoch = collection_observation_epoch();
    other_heap.collect_rooted(std::iter::empty());
    assert_ne!(collection_observation_epoch(), epoch);
    assert!(!collection_observed_metadata(garbage.bits()));
    // SAFETY: bare `TaggedHeap` unit test restoring `live_heap`.
    unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(&other_heap) };
    set_tagged_heap(&mut live_heap);

    assert_eq!(
        geometry_rebuilds(),
        rebuilds,
        "an unrelated real mark clear must not reconstruct the live address tree"
    );
    assert_eq!(
        with_state(|state| state.borrow().observed.full_bounds()),
        bounds
    );
    assert_eq!(live_heap.jit_barrier_window_for_test(), window);
    assert!(live_heap.owns_heap_value_for_test(owner));
    assert!(reads.unchanged());
    owner.set_car(TaggedValue::make_int(23));
    assert!(
        !reads.unchanged(),
        "the retained dependency still invalidates"
    );
}

#[test]
fn deferred_prune_refresh_retains_pre_acquired_epoch_across_concurrent_reclaim() {
    // Regression for Release/Acquire ordering in `ObservedEnvelope::refresh`:
    // Writer executes: (W0) clear mark [Release] -> (W1) RECLAIMED += cleared [Release]
    //                  -> (W2) OBSERVATION_EPOCH += 1 [Release].
    // `refresh` must Acquire `OBSERVATION_EPOCH` BEFORE `RECLAIMED_OBSERVATIONS`
    // and store that pre-acquired epoch on deferred prune, never re-loading a
    // newer epoch after `RECLAIMED_OBSERVATIONS` has already been read.
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut heap = crate::tagged::gc::TestHeap::new();
    heap.set_gc_threshold(usize::MAX);

    let owners: Vec<TaggedValue> = (0..64)
        .map(|i| heap.alloc_vector(vec![TaggedValue::make_int(i)]))
        .collect();
    // Publish sticky marks for all 64 vectors via real capture reads.
    let (_, cert) = capture(|| {
        for owner in &owners {
            std::hint::black_box(owner.as_vector_data().unwrap().at(0));
        }
    });
    drop(cert);

    // Snapshot an ObservedEnvelope at the pre-reclamation epoch (modeling a
    // peer envelope that has not yet refreshed after the first collection).
    let mut envelope = super::ObservedEnvelope::new();
    for owner in &owners {
        envelope.insert(owner.bits(), super::NativeWindowCollectionOwner::Slots);
    }
    let e0 = collection_observation_epoch();
    assert_eq!(envelope.epoch_for_test(), Some(e0));

    // Reclaim owners[0]: debt = 1, 1 * 4 < 64 -> triggers deferred-prune branch.
    heap.collect_rooted(owners[1..].iter().copied());
    let e1 = collection_observation_epoch();
    assert!(e1 > e0, "first reclamation must advance epoch");
    let mut e2 = 0;

    // Interleave a second reclamation (owners[1]) right after `refresh_reclaimed`
    // Acquires `RECLAIMED_OBSERVATIONS`. The deferred-prune branch must record
    // the pre-acquired `e1`, NOT the post-reclaim `e2`.
    let dump = crate::tagged::gc::current_collection_dump_window();
    envelope.refresh_with_concurrent_reclaim_for_test(dump, || {
        heap.collect_rooted(owners[2..].iter().copied());
        e2 = collection_observation_epoch();
    });
    assert!(e2 > e1, "interleaved reclamation must advance epoch");
    assert_eq!(
        envelope.epoch_for_test(),
        Some(e1),
        "deferred prune must retain the epoch Acquired before RECLAIMED, never a newer epoch"
    );
    assert!(
        !envelope.was_refreshed_at(e2),
        "envelope must not claim to be refreshed at the post-reclaim epoch e2"
    );
}

#[test]
fn full_prune_refresh_acquires_epoch_before_reclaimed_across_concurrent_reclaim() {
    // Full-prune branch must also Acquire `OBSERVATION_EPOCH` BEFORE
    // `RECLAIMED_OBSERVATIONS` so `validated_reclaimed` never lags behind
    // `self.epoch` across an interleaved mark-clearing reclamation.
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut heap = crate::tagged::gc::TestHeap::new();
    heap.set_gc_threshold(usize::MAX);

    let owners: Vec<TaggedValue> = (0..4)
        .map(|i| heap.alloc_vector(vec![TaggedValue::make_int(i)]))
        .collect();
    let (_, cert) = capture(|| {
        for owner in &owners {
            std::hint::black_box(owner.as_vector_data().unwrap().at(0));
        }
    });
    drop(cert);

    let mut envelope = super::ObservedEnvelope::new();
    for owner in &owners {
        envelope.insert(owner.bits(), super::NativeWindowCollectionOwner::Slots);
    }

    // Reclaim 2 of 4 owners: debt = 2, 2 * 4 >= 4 -> triggers full-prune branch.
    heap.collect_rooted(owners[2..].iter().copied());
    let r1 = crate::tagged::gc::reclaimed_collection_observation_count();
    let mut e2 = 0;
    let mut r2 = 0;

    let dump = crate::tagged::gc::current_collection_dump_window();
    envelope.refresh_with_concurrent_reclaim_for_test(dump, || {
        heap.collect_rooted(owners[3..].iter().copied());
        e2 = collection_observation_epoch();
        r2 = crate::tagged::gc::reclaimed_collection_observation_count();
    });
    assert!(
        r2 > r1,
        "interleaved reclamation must advance reclaimed count"
    );
    assert_eq!(envelope.epoch_for_test(), Some(e2));
    assert!(
        envelope.validated_reclaimed_for_test() >= r2,
        "full prune advancing self.epoch to e2 must record validated_reclaimed >= r2 (got {}, expected >= {})",
        envelope.validated_reclaimed_for_test(),
        r2
    );
}

#[test]
fn full_prune_refresh_interleaved_between_epoch_and_reclaimed_catches_load_swap() {
    // Interleave a mark-clearing collection directly BETWEEN the two full-prune
    // loads (`epoch = collection_observation_epoch()` and
    // `reclaimed = reclaimed_collection_observation_count()`).
    // With the correct EPOCH-before-RECLAIMED order:
    //   - `self.epoch` stays at pre-interleave `Some(e1)` (never `e2`), and
    //   - `validated_reclaimed` absorbs the post-interleave `r2` (and `retain()`
    //     prunes `owners[2]` so only the 1 live owner remains).
    // Swapping the two loads causes `validated_reclaimed` to record stale `r1 < r2`
    // while `self.epoch` advances to `Some(e2)`, failing both assertions.
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut heap = crate::tagged::gc::TestHeap::new();
    heap.set_gc_threshold(usize::MAX);

    let owners: Vec<TaggedValue> = (0..4)
        .map(|i| heap.alloc_vector(vec![TaggedValue::make_int(i)]))
        .collect();
    let (_, cert) = capture(|| {
        for owner in &owners {
            std::hint::black_box(owner.as_vector_data().unwrap().at(0));
        }
    });
    drop(cert);

    let mut envelope = super::ObservedEnvelope::new();
    for owner in &owners {
        envelope.insert(owner.bits(), super::NativeWindowCollectionOwner::Slots);
    }

    // Reclaim 2 of 4 owners: debt = 2, 2 * 4 >= 4 -> enters full-prune branch.
    heap.collect_rooted(owners[2..].iter().copied());
    let e1 = collection_observation_epoch();
    let r1 = crate::tagged::gc::reclaimed_collection_observation_count();
    let mut e2 = 0;
    let mut r2 = 0;

    let dump = crate::tagged::gc::current_collection_dump_window();
    envelope.refresh_with_full_prune_interleave_for_test(dump, || {
        heap.collect_rooted(owners[3..].iter().copied());
        e2 = collection_observation_epoch();
        r2 = crate::tagged::gc::reclaimed_collection_observation_count();
    });
    assert!(e2 > e1, "interleaved reclamation must advance epoch");
    assert!(
        r2 > r1,
        "interleaved reclamation must advance reclaimed count"
    );
    assert_eq!(
        envelope.epoch_for_test(),
        Some(e1),
        "full prune must acquire epoch (e1) BEFORE reclaimed (r2); swapping the loads advances epoch to e2"
    );
    assert!(
        envelope.validated_reclaimed_for_test() >= r2,
        "full prune must acquire reclaimed (r2) AFTER epoch (e1); swapping the loads leaves stale r1 ({} < {})",
        envelope.validated_reclaimed_for_test(),
        r2
    );
    assert_eq!(
        envelope.owner_count_for_test(),
        1,
        "retain() must observe the interleaved clear and prune all 3 dead owners"
    );
}
