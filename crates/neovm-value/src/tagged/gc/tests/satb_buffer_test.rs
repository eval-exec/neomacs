use super::*;
use crate::tagged::value::TaggedValue;
use std::mem::{align_of, offset_of, size_of};
use std::sync::{Arc, Barrier};

fn word(ordinal: usize) -> MarkWord {
    MarkWord::of(TaggedValue::fixnum(ordinal as i64))
}

fn values(chunks: Vec<SatbChunk>) -> Vec<usize> {
    chunks
        .into_iter()
        .flat_map(SatbChunk::into_words)
        .map(|word| word.value().as_fixnum().unwrap() as usize)
        .collect()
}

#[test]
fn satb_boundaries_preserve_exact_words_and_fixed_capacity() {
    for count in [SATB_CHUNK_WORDS - 1, SATB_CHUNK_WORDS, SATB_CHUNK_WORDS + 1] {
        let handoff = SatbHandoff::new();
        let mut buffer = SatbBuffer::new();
        let initial_capacity = buffer.words.capacity();
        for ordinal in 0..count {
            buffer.push(word(ordinal), &handoff);
        }
        assert_eq!(buffer.words.capacity(), initial_capacity);
        let usage = handoff.storage_usage().unwrap();
        assert_eq!(usage.chunks, usize::from(count > SATB_CHUNK_WORDS));
        let mut actual = values(handoff.take_all().unwrap());
        if let Some(chunk) = buffer.take_partial(&StoppedMutatorsProof::for_test()) {
            actual.extend(values(vec![chunk]));
        }
        assert_eq!(actual, (0..count).collect::<Vec<_>>());
        assert!(buffer.is_empty());
        assert!(handoff.is_empty().unwrap());
    }
}

#[test]
fn satb_retire_delivers_nonempty_partial_exactly_once() {
    let handoff = SatbHandoff::new();
    let mut buffer = SatbBuffer::new();
    buffer.push(word(71), &handoff);
    buffer.retire(&handoff).unwrap().acknowledge();
    assert_eq!(values(handoff.take_all().unwrap()), vec![71]);
    assert!(handoff.take_all().unwrap().is_empty());
}

#[test]
fn satb_pending_and_chunks_agree_under_the_publish_lock() {
    let handoff = SatbHandoff::new();
    let mut buffer = SatbBuffer::new();
    for ordinal in 0..SATB_CHUNK_WORDS * 3 + 1 {
        buffer.push(word(ordinal), &handoff);
    }
    {
        let chunks = handoff.chunks.lock().unwrap();
        assert_eq!(handoff.pending.load(Ordering::Acquire), chunks.len());
        assert!(!chunks.is_empty());
    }
    buffer.retire(&handoff).unwrap().acknowledge();
    assert_eq!(
        handoff.storage_usage().unwrap().payload.words,
        SATB_CHUNK_WORDS * 3 + 1
    );
    assert_eq!(
        values(handoff.take_all().unwrap()),
        (0..SATB_CHUNK_WORDS * 3 + 1).collect::<Vec<_>>()
    );
    assert_eq!(handoff.pending.load(Ordering::Acquire), 0);
}

#[test]
fn satb_k_mutators_publish_concurrently_without_loss_or_duplicate_words() {
    const MUTATORS: usize = 4;
    const WORDS: usize = 10003;
    let handoff = Arc::new(SatbHandoff::new());
    let start = Arc::new(Barrier::new(MUTATORS + 1));
    let mut workers = Vec::new();
    for mutator in 0..MUTATORS {
        let handoff = handoff.clone();
        let start = start.clone();
        workers.push(std::thread::spawn(move || {
            // Construction happens on the owner thread; a !Send buffer never
            // crosses the spawn boundary. Only Send chunks enter the handoff.
            let mut buffer = SatbBuffer::new();
            start.wait();
            for ordinal in 0..WORDS {
                buffer.push(word(mutator * WORDS + ordinal), &handoff);
            }
            buffer.retire(&handoff).unwrap().acknowledge();
        }));
    }
    start.wait();
    let mut actual = Vec::new();
    while workers.iter().any(|worker| !worker.is_finished()) {
        actual.extend(values(handoff.take_all().unwrap()));
        std::thread::yield_now();
    }
    for worker in workers {
        worker.join().unwrap();
    }
    actual.extend(values(handoff.take_all().unwrap()));
    actual.sort_unstable();
    assert_eq!(actual, (0..MUTATORS * WORDS).collect::<Vec<_>>());
    assert!(handoff.is_empty().unwrap());
}

#[test]
fn satb_k_stopped_partials_and_midway_retirement_are_all_delivered() {
    let handoff = SatbHandoff::new();
    let proof = StoppedMutatorsProof::for_test();
    let mut mutators: Vec<_> = (0..4).map(|_| Some(SatbBuffer::new())).collect();
    for (id, buffer) in mutators.iter_mut().enumerate() {
        buffer.as_mut().unwrap().push(word(id * 10), &handoff);
    }
    mutators[1]
        .take()
        .unwrap()
        .retire(&handoff)
        .unwrap()
        .acknowledge();
    for (id, buffer) in mutators.iter_mut().enumerate() {
        if let Some(buffer) = buffer {
            buffer.push(word(id * 10 + 1), &handoff);
        }
    }
    let mut actual = values(handoff.take_all().unwrap());
    for buffer in mutators.iter_mut().flatten() {
        if let Some(chunk) = buffer.take_partial(&proof) {
            actual.extend(values(vec![chunk]));
        }
    }
    actual.sort_unstable();
    assert_eq!(actual, vec![0, 1, 10, 20, 21, 30, 31]);
    assert!(mutators.iter().flatten().all(SatbBuffer::is_empty));
}

#[test]
fn satb_poison_is_reported_even_when_pending_is_zero() {
    let handoff = SatbHandoff::new();
    handoff.poison_for_test();
    assert_eq!(handoff.pending.load(Ordering::Acquire), 0);
    assert!(handoff.is_poisoned());
    assert!(handoff.take_all().is_err());
    assert!(handoff.is_empty().is_err());
    assert!(handoff.storage_usage().is_err());
    assert!(SatbBuffer::new().retire(&handoff).is_err());
}

#[test]
fn satb_abandonment_is_lock_free_even_with_a_poisoned_handoff() {
    let handoff = SatbHandoff::new();
    let mut buffer = SatbBuffer::new();
    buffer.push(word(5), &handoff);
    handoff.poison_for_test();
    let _locked = handoff.chunks.lock().unwrap_err().into_inner();
    buffer.abandon();
    assert!(buffer.is_empty());
}

#[test]
fn satb_handoff_reserves_both_lines_from_fields_and_arc_refcounts() {
    #[repr(C)]
    struct Adjacent {
        before: u8,
        handoff: SatbHandoff,
        after: u8,
    }
    #[repr(C)]
    struct ArcAllocationLayout {
        strong: AtomicUsize,
        weak: AtomicUsize,
        payload: SatbHandoff,
    }
    assert_eq!(align_of::<SatbHandoff>(), 128);
    assert_eq!(size_of::<SatbHandoff>(), 128);
    assert_eq!(offset_of!(Adjacent, handoff), 128);
    assert_eq!(offset_of!(Adjacent, after), 256);
    // Pin the repr(C) ArcInner header model without reading any private Arc
    // memory. The std implementation's padded payload offset must remain this
    // model when reviewing a compiler/toolchain change.
    assert_eq!(offset_of!(ArcAllocationLayout, payload), 128);
    assert_eq!(size_of::<ArcAllocationLayout>(), 256);
    assert!(offset_of!(ArcAllocationLayout, weak) + size_of::<AtomicUsize>() <= 128);
    let handoff = Arc::new(SatbHandoff::new());
    let payload = Arc::as_ptr(&handoff) as usize;
    assert_eq!(payload % 128, 0);
    // The actual payload is aligned as the model requires; cloning cannot
    // relocate it. No assumptions about private refcount addresses are read.
    let other = handoff.clone();
    assert_eq!(Arc::as_ptr(&other) as usize, payload);
    assert!(offset_of!(SatbHandoff, chunks) + size_of::<Mutex<Vec<SatbChunk>>>() <= 128);
    assert!(offset_of!(SatbHandoff, pending) + size_of::<AtomicUsize>() <= 128);
}

#[test]
#[cfg(debug_assertions)]
fn satb_unretired_nonempty_buffer_trips_debug_drop_check() {
    let handoff = SatbHandoff::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buffer = SatbBuffer::new();
        buffer.push(word(1), &handoff);
        drop(buffer);
    }));
    assert!(result.is_err());
}

#[test]
fn satb_buffer_drop_during_unwind_never_panics_twice() {
    let handoff = SatbHandoff::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buffer = SatbBuffer::new();
        buffer.push(word(1), &handoff);
        panic!("fixture owner unwinding");
    }));
    assert!(result.is_err());
}
