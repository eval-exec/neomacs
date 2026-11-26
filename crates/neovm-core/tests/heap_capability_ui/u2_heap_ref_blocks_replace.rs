//! Phase 1c.0 U2: holding a `HeapRef` payload borrow across a `HeapMut` bulk
//! replacement fails with E0502.
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::value::Value;

fn borrow_across_replace(heap: &mut TaggedHeap, v: Value) {
    let mut hm = heap.heap_mut();
    let items = v.vector_data_in(hm.as_ref()).unwrap();
    let _ = v.replace_vector_data_in(&mut hm, Vec::new());
    let _ = items.len();
}

fn main() {
    let _ = borrow_across_replace;
}
