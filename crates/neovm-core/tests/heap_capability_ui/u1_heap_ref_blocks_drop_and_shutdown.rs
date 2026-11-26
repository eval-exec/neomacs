//! Phase 1c.0 U1: holding a `HeapRef` payload borrow across `drop(heap)` or
//! `heap.shutdown()` fails with E0505.
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::value::Value;

fn borrow_across_drop(heap: TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    drop(heap);
    let _ = items.len();
}

fn borrow_across_shutdown(heap: TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    let _ = heap.shutdown();
    let _ = items.len();
}

fn main() {
    let _ = (borrow_across_drop, borrow_across_shutdown);
}
