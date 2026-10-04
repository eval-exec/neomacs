//! Existing observation metadata must remain readable during cold insertion.

use super::*;
use crate::tagged::gc::ConsBlock;
use crate::tagged::header::ConsCdrOrNext;
use crate::tagged::value::TaggedValue;
use std::sync::mpsc;
use std::time::Duration;

fn marked_cons() -> (ConsBlock, usize) {
    let mut block = ConsBlock::new();
    assert_eq!(block.reserve_tail(1), (0, 1));
    // The block stays live until the worker has joined. Its owner is never
    // mutated or reclaimed while another thread queries its atomic metadata.
    unsafe {
        block.cells_ptr().write(ConsCell {
            car: TaggedValue::NIL,
            cdr_or_next: ConsCdrOrNext {
                cdr: TaggedValue::NIL,
            },
        });
    }
    let bits = unsafe { TaggedValue::from_cons_ptr(block.cells_ptr()) }.bits();
    assert!(mark_collection_observed(bits));
    (block, bits)
}

fn while_registry_locked(operation: impl FnOnce() -> bool + Send + 'static) -> bool {
    let (worker, completed_while_locked) = with_collection_observed_registry_locked(|| {
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = operation();
            // On the red path the receiver times out and goes away before
            // the lock is released. The worker must still join normally.
            let _ = sender.send(result);
            result
        });
        let completed = receiver.recv_timeout(Duration::from_secs(1));
        (worker, completed)
    });
    // Always unlock before joining or asserting, including the red timeout.
    let result = worker.join().expect("observation metadata worker");
    assert!(
        completed_while_locked.is_ok(),
        "an existing observation must not wait for the cold registry insertion lock"
    );
    result
}

#[test]
fn observed_cons_query_does_not_wait_for_registry_insertion_lock() {
    crate::test_utils::init_test_tracing();
    let (_block, bits) = marked_cons();
    assert!(while_registry_locked(move || collection_observed(bits)));
}

#[test]
fn repeated_cons_observation_does_not_wait_for_registry_insertion_lock() {
    crate::test_utils::init_test_tracing();
    let (_block, bits) = marked_cons();
    assert!(!while_registry_locked(move || mark_collection_observed(
        bits
    )));
}
