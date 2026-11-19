//! Killed-buffer and retired-process ephemerons drain to one joint fixpoint.
//! A killed buffer's field can hold a deleted process and a deleted process's
//! field can hold a killed buffer, so a chain that alternates between the two
//! kinds must survive as long as its first object is reachable.

use super::*;
use crate::heap_types::ProcessId;
use crate::value::buffer_vocab::BufferId;

/// Alternations in the chain. Each sequential pair of per-kind loops would
/// advance the chain by at most two links, and a full collection runs only a
/// few such pairs, so this length outruns them.
const LINKS: u64 = 6;

/// Builds root -> B0 -[field]-> P0 -[field]-> B1 -[field]-> P1 ... -> X and
/// arms every buffer and process as an ephemeron record. Returns the root
/// (B0's object) and the ids in the chain.
fn alternating_chain(heap: &mut TaggedHeap) -> (TaggedValue, Vec<BufferId>, Vec<ProcessId>) {
    let tail = heap.alloc_cons(TaggedValue::fixnum(42), TaggedValue::fixnum(0));
    let buffers: Vec<BufferId> = (0..LINKS).map(|i| BufferId(100 + i)).collect();
    let processes: Vec<ProcessId> = (0..LINKS).map(|i| 500 + i).collect();
    let buffer_objects: Vec<TaggedValue> =
        buffers.iter().map(|&id| heap.alloc_buffer(id)).collect();
    let process_objects: Vec<TaggedValue> =
        processes.iter().map(|&id| heap.alloc_process(id)).collect();
    for (&id, &object) in buffers.iter().zip(&buffer_objects) {
        heap.register_buffer_value(id, object);
    }
    for (&id, &object) in processes.iter().zip(&process_objects) {
        heap.register_process_value(id, object);
    }
    // B_i's field is P_i; P_i's field is B_{i+1}; the last process holds TAIL.
    let killed = buffers
        .iter()
        .zip(&process_objects)
        .map(|(&id, &process)| (id, vec![process]))
        .collect();
    let retired = processes
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let field = buffer_objects.get(i + 1).copied().unwrap_or(tail);
            (id, vec![field])
        })
        .collect();
    heap.arm_killed_buffer_ephemerons(killed);
    heap.arm_retired_process_ephemerons(retired);
    (buffer_objects[0], buffers, processes)
}

#[test]
fn alternating_killed_buffer_and_process_chain_survives_a_full_collection() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (root, buffers, processes) = alternating_chain(&mut heap);

    heap.collect_exact(std::iter::once(root));

    assert_eq!(heap.take_reclaimed_buffer_ids(), Vec::<BufferId>::new());
    assert_eq!(heap.take_reclaimed_process_ids(), Vec::<ProcessId>::new());
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
    // The tail cons is reachable only through the last process's field.
    assert_eq!(heap.cons_live_count_exact(), 1);
}

#[test]
fn unreached_alternating_chain_is_reclaimed_whole() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let (_root, buffers, processes) = alternating_chain(&mut heap);

    heap.collect_exact(std::iter::empty());

    let mut reclaimed_buffers = heap.take_reclaimed_buffer_ids();
    reclaimed_buffers.sort_by_key(|id| id.0);
    let mut reclaimed_processes = heap.take_reclaimed_process_ids();
    reclaimed_processes.sort_unstable();
    assert_eq!(reclaimed_buffers, buffers);
    assert_eq!(reclaimed_processes, processes);
    assert_eq!(heap.cons_live_count_exact(), 0);
}
