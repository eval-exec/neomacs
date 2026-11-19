//! Killed-buffer, retired-process, and deleted-frame ephemerons drain to one
//! joint fixpoint. A field of any kind can hold the key object of another, so
//! a chain that alternates through all three must survive as long as its
//! first object is reachable.
//!
//! The three mark helpers run inside `mark_reached_ephemeron_records_to_fixpoint`
//! and again inside the weak-table fixpoint. Splitting the helper into three
//! sequential per-kind loops lets a chain of this length die: a full
//! collection runs the helper only a few times.

use super::*;
use crate::heap_types::ProcessId;
use crate::value::buffer_vocab::BufferId;

/// Alternations of each kind. Sequential per-kind loops advance the chain by
/// only a few links per call, and a full collection runs only a few such
/// calls, so this length outruns them.
const LINKS: u64 = 6;

/// Builds root -> B0 -> P0 -> F0 -> B1 -> P1 -> F1 ... -> X and arms every
/// buffer, process, and frame as an ephemeron record. Returns the root
/// (B0's object) and the ids in the chain.
fn alternating_chain(
    heap: &mut TaggedHeap,
) -> (TaggedValue, Vec<BufferId>, Vec<ProcessId>, Vec<u64>) {
    let tail = heap.alloc_cons(TaggedValue::fixnum(42), TaggedValue::fixnum(0));
    let buffers: Vec<BufferId> = (0..LINKS).map(|i| BufferId(100 + i)).collect();
    let processes: Vec<ProcessId> = (0..LINKS).map(|i| 500 + i).collect();
    let frames: Vec<u64> = (0..LINKS).map(|i| 800 + i).collect();
    let buffer_objects: Vec<TaggedValue> =
        buffers.iter().map(|&id| heap.alloc_buffer(id)).collect();
    let process_objects: Vec<TaggedValue> =
        processes.iter().map(|&id| heap.alloc_process(id)).collect();
    let frame_objects: Vec<TaggedValue> = frames.iter().map(|&id| heap.alloc_frame(id)).collect();
    for (&id, &object) in buffers.iter().zip(&buffer_objects) {
        heap.register_buffer_value(id, object);
    }
    for (&id, &object) in processes.iter().zip(&process_objects) {
        heap.register_process_value(id, object);
    }
    for (&id, &object) in frames.iter().zip(&frame_objects) {
        heap.register_frame_value(id, object);
    }
    // B_i's field is P_i; P_i's field is F_i; F_i's field is B_{i+1}; the
    // last frame holds TAIL.
    let killed = buffers
        .iter()
        .zip(&process_objects)
        .map(|(&id, &process)| (id, vec![process]))
        .collect();
    let retired = processes
        .iter()
        .zip(&frame_objects)
        .map(|(&id, &frame)| (id, vec![frame]))
        .collect();
    let deleted = frames
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let field = buffer_objects.get(i + 1).copied().unwrap_or(tail);
            (id, vec![field])
        })
        .collect();
    heap.arm_killed_buffer_ephemerons(killed);
    heap.arm_retired_process_ephemerons(retired);
    heap.arm_deleted_frame_ephemerons(deleted);
    (buffer_objects[0], buffers, processes, frames)
}

#[test]
fn alternating_killed_buffer_process_and_frame_chain_survives_a_full_collection() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (root, buffers, processes, frames) = alternating_chain(&mut heap);

    heap.collect_exact(std::iter::once(root));

    assert_eq!(heap.take_reclaimed_buffer_ids(), Vec::<BufferId>::new());
    assert_eq!(heap.take_reclaimed_process_ids(), Vec::<ProcessId>::new());
    assert_eq!(heap.take_reclaimed_frame_ids(), Vec::<u64>::new());
    for &id in &buffers {
        assert!(
            heap.buffer_value(id).is_some(),
            "buffer {id:?} lost its registry entry"
        );
    }
    for &id in &processes {
        assert!(
            heap.process_value(id).is_some(),
            "process {id} lost its registry entry"
        );
    }
    for &id in &frames {
        assert!(
            heap.frame_value(id).is_some(),
            "frame {id} lost its registry entry"
        );
    }
    // The tail cons is reachable only through the last frame's field.
    assert_eq!(heap.cons_live_count_exact(), 1);
}

#[test]
fn unreached_alternating_chain_is_reclaimed_whole() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (_root, buffers, processes, frames) = alternating_chain(&mut heap);

    heap.collect_exact(std::iter::empty());

    let mut reclaimed_buffers = heap.take_reclaimed_buffer_ids();
    reclaimed_buffers.sort_by_key(|id| id.0);
    let mut reclaimed_processes = heap.take_reclaimed_process_ids();
    reclaimed_processes.sort_unstable();
    let mut reclaimed_frames = heap.take_reclaimed_frame_ids();
    reclaimed_frames.sort_unstable();
    println!("reclaimed buffer ids: {reclaimed_buffers:?}");
    println!("reclaimed process ids: {reclaimed_processes:?}");
    println!("reclaimed frame ids: {reclaimed_frames:?}");
    assert_eq!(reclaimed_buffers, buffers);
    assert_eq!(reclaimed_processes, processes);
    assert_eq!(reclaimed_frames, frames);
    assert_eq!(heap.cons_live_count_exact(), 0);
    for &id in &frames {
        assert!(heap.frame_value(id).is_none());
    }
}

#[test]
fn reclaimed_frame_id_refuses_make_frame_and_does_not_mint() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let id = 4242_u64;
    let object = heap.alloc_frame(id);
    heap.register_frame_value(id, object);
    heap.arm_deleted_frame_ephemerons(vec![(id, Vec::new())]);

    heap.collect_exact(std::iter::empty());

    let reclaimed = heap.take_reclaimed_frame_ids();
    assert_eq!(reclaimed, vec![id]);
    assert!(heap.frame_value(id).is_none());

    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| TaggedValue::make_frame(id)));
    #[cfg(debug_assertions)]
    assert!(
        outcome.is_err(),
        "debug builds trip debug_assert on a reclaimed frame id"
    );
    #[cfg(not(debug_assertions))]
    assert!(
        outcome.expect("release make_frame returns").is_nil(),
        "a reclaimed frame id returns nil"
    );
    assert!(
        heap.frame_value(id).is_none(),
        "make_frame minted a new object for a reclaimed id"
    );

    let never = 999_999_u64;
    let missing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        TaggedValue::make_frame(never)
    }));
    #[cfg(debug_assertions)]
    assert!(missing.is_err(), "a never-created frame id refuses");
    #[cfg(not(debug_assertions))]
    assert!(missing.expect("release make_frame returns").is_nil());
    assert!(heap.frame_value(never).is_none());
}
