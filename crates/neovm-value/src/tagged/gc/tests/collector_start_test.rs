//! A refused thread start preserves marking and retries on the next request.

use super::*;
use collector_test_support::{SpawnOutcome, SpawnPlan};
use std::time::Instant;

fn refusal_then_recovery(heap: HeapIdentity) -> SpawnPlan {
    SpawnPlan::install(
        heap,
        [
            SpawnOutcome::Unavailable(std::io::ErrorKind::WouldBlock),
            SpawnOutcome::Unavailable(std::io::ErrorKind::WouldBlock),
            SpawnOutcome::System,
        ],
    )
}

#[test]
fn mark_all_survives_repeated_start_refusal_and_recovers_on_next_request() {
    let mut heap = TestHeap::new();
    let root = heap.alloc_cons(TaggedValue::fixnum(37), TaggedValue::NIL);
    let plan = refusal_then_recovery(heap.heap_identity());
    let failures = collector_start_failures();
    for cycle in 0..4 {
        heap.collect_exact([root].into_iter());
        assert_eq!(root.cons_car(), TaggedValue::fixnum(37));
        assert_eq!(collector_is_registered(heap.heap_identity()), cycle >= 2);
    }
    plan.assert_consumed();
    assert_eq!(collector_start_failures() - failures, 2);
}

#[derive(Clone, Copy)]
enum ClaimPolicy {
    Legacy,
    Enabled,
}

fn concurrent_start_refusal(policy: ClaimPolicy) {
    knobs::set_concurrent_claims_for_test(Some(matches!(policy, ClaimPolicy::Enabled)));
    let mut heap = TestHeap::new();
    knobs::set_concurrent_claims_for_test(None);
    let child = heap.alloc_string(crate::heap_types::LispString::from_utf8(
        "inline worker retained",
    ));
    let mut spine = child;
    // More than the worker's stop quantum: the inline completion must publish
    // readiness even when its gray queue is handed off before the idle branch.
    for n in 0..1536 {
        spine = heap.alloc_cons(TaggedValue::fixnum(n), spine);
    }
    let root = heap.alloc_vector(vec![spine]);
    let plan = refusal_then_recovery(heap.heap_identity());
    let failures = collector_start_failures();
    for cycle in 0..4 {
        heap.concurrent_begin();
        heap.seed_root(root);
        heap.launch_concurrent_mark();
        if cycle < 2 {
            assert!(!collector_is_registered(heap.heap_identity()));
            assert!(
                heap.concurrent_mark_done(),
                "the inline result is already ready for termination"
            );
            let result = heap
                .gc_exited
                .take()
                .unwrap()
                .try_recv()
                .expect("inline dispatch must return with its result queued");
            if cycle == 0 {
                assert!(
                    !result.deferred.is_empty(),
                    "the quantum stop must retain residual work"
                );
            }
            // Inspect without consuming the actual termination protocol.
            let (send, receive) = std::sync::mpsc::channel();
            send.send(result).unwrap();
            heap.gc_exited = Some(receive);
        } else {
            assert!(collector_is_registered(heap.heap_identity()));
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        heap.seed_root(root);
        heap.incremental_drain_all();
        let live_bytes = heap.live_bytes();
        heap.incremental_finish(live_bytes, Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(heap.owns_heap_value_for_test(root));
        assert!(heap.owns_heap_value_for_test(child));
        assert_eq!(child.as_str_owned().unwrap(), "inline worker retained");
        assert_eq!(
            root.as_vector_data().unwrap()[0].cons_car(),
            TaggedValue::fixnum(1535)
        );
    }
    plan.assert_consumed();
    assert_eq!(collector_start_failures() - failures, 2);
}

#[test]
fn concurrent_mark_survives_start_refusal_with_residual_handoff_and_retry() {
    concurrent_start_refusal(ClaimPolicy::Legacy);
}

#[test]
fn enabled_concurrent_mark_survives_start_refusal_with_residual_handoff_and_retry() {
    concurrent_start_refusal(ClaimPolicy::Enabled);
}

#[test]
fn inline_only_handle_retires_without_a_thread_to_join() {
    let mut heap = TestHeap::new();
    let plan = SpawnPlan::install(
        heap.heap_identity(),
        [SpawnOutcome::Unavailable(std::io::ErrorKind::WouldBlock)],
    );
    heap.collect_exact(std::iter::empty());
    plan.assert_consumed();
    assert!(matches!(
        collectors().get(&heap.heap_identity()),
        Some(CollectorHandle::Inline)
    ));
    assert!(retire_collector(heap.heap_identity()).is_none());
    assert!(!collectors().contains_key(&heap.heap_identity()));
}
