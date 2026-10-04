//! Existing observation metadata must remain readable during cold insertion.

use super::*;
use crate::tagged::gc::ConsBlock;
use crate::tagged::header::{ConsCdrOrNext, HeapObjectKind};
use crate::tagged::value::TaggedValue;
use std::sync::mpsc;
use std::time::Duration;

fn unobserved_cons() -> (ConsBlock, usize) {
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
    (block, bits)
}

fn marked_cons() -> (ConsBlock, usize) {
    let (block, bits) = unobserved_cons();
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

#[test]
fn compiled_cons_observation_word_survives_clear_and_reuse() {
    crate::test_utils::init_test_tracing();
    let (mut block, bits) = marked_cons();
    let (word, mask) = cons_collection_observed_word(bits);
    assert_ne!(word.load(Ordering::Acquire) & mask, 0);
    let before = collection_observation_epoch();
    let mut free_list = std::ptr::null_mut();
    assert_eq!(block.sweep(&mut free_list), 0);
    assert_eq!(free_list, block.cells_ptr());
    assert!(collection_observation_epoch() > before);
    assert_eq!(word.load(Ordering::Acquire) & mask, 0);
    assert!(!collection_observed_metadata(bits));

    // Reinitialize the same retained free cell, then observe its new lifetime.
    unsafe {
        free_list.write(ConsCell {
            car: TaggedValue::NIL,
            cdr_or_next: ConsCdrOrNext {
                cdr: TaggedValue::NIL,
            },
        });
    }
    let (reused_word, reused_mask) = cons_collection_observed_word(bits);
    assert!(std::ptr::eq(word, reused_word));
    assert_eq!(mask, reused_mask);
    assert!(mark_collection_observed(bits));
    assert!(collection_observed_metadata(bits));
    drop(block);
    // Native code's permanent word and stale-safe ledger lookup remain valid
    // after the owner storage dies, and expose no prior-lifetime mark.
    assert_eq!(word.load(Ordering::Acquire) & mask, 0);
    assert!(!collection_observed_metadata(bits));
}

#[test]
fn noncons_observation_metadata_is_safe_after_header_storage_dies() {
    crate::test_utils::init_test_tracing();
    // This fixture exercises only header observation APIs, never a FloatObj
    // payload or Lisp pointer projection.
    let header = Box::new(GcHeader::new(HeapObjectKind::Float));
    let bits = (&*header as *const GcHeader as usize) | TAG_FLOAT;
    assert!(mark_collection_observed(bits));
    assert!(header.collection_observed());
    assert!(collection_observed_metadata(bits));
    let before = collection_observation_epoch();
    header.clear_collection_observed();
    assert!(!header.collection_observed());
    assert!(!collection_observed_metadata(bits));
    assert!(collection_observation_epoch() > before);
    drop(header);
    assert!(!collection_observed_metadata(bits));
}

#[test]
fn compiled_cons_observation_registration_is_stable_across_threads() {
    crate::test_utils::init_test_tracing();
    let (_block, bits) = unobserved_cons();
    let words = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(move || {
                    let (word, mask) = cons_collection_observed_word(bits);
                    (word as *const AtomicU64 as usize, mask)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("bitmap registration worker"))
            .collect::<Vec<_>>()
    });
    assert!(words.iter().all(|word| *word == words[0]));
    assert!(!collection_observed_metadata(bits));
    let (word, mask) = cons_collection_observed_word(bits);
    assert_eq!(word.load(Ordering::Acquire) & mask, 0);
    assert!(mark_collection_observed(bits));
    assert_ne!(word.load(Ordering::Acquire) & mask, 0);
}
