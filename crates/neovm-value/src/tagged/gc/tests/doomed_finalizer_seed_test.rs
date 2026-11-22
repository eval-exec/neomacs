//! Lock: the serial stop and the stopped major yield one queued doomed
//! finalizer function through `seed_internal_runtime_roots`.
//!
//! `begin_collection` and `begin_stw_collection` both enter
//! `begin_collection_with`, which clears the gray worklist and seeds the
//! runtime roots. The doomed queue is one of those roots. The seed leaves
//! the queue in place. It does not take the function and it does not run it.

use super::*;

/// Both entries yield the same queued function, and the queue stays queued.
#[test]
fn serial_and_stopped_seeds_yield_the_same_doomed_finalizer_function() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let function = heap.alloc_cons(TaggedValue::fixnum(61), TaggedValue::NIL);
    heap.doomed_finalizer_functions.push(function);

    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.begin_collection() };
    let serial_roots = assert_doomed_function_was_seeded(&heap, function);

    heap.begin_stw_collection();
    let stopped_roots = assert_doomed_function_was_seeded(&heap, function);

    assert_eq!(
        serial_roots, stopped_roots,
        "both entries seed the same runtime-root list"
    );
}

/// The queued function is on the gray worklist, and the doomed queue is unchanged.
fn assert_doomed_function_was_seeded(heap: &TaggedHeap, function: TaggedValue) -> usize {
    assert_eq!(heap.doomed_finalizer_functions.len(), 1);
    assert_eq!(heap.doomed_finalizer_functions[0].0, function.0);
    assert!(
        heap.gray_queue.iter().any(|value| value.0 == function.0),
        "seed_internal_runtime_roots must yield the queued doomed finalizer function"
    );
    heap.last_runtime_seed_roots
}
