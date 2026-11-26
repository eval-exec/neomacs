use neovm_core::emacs_core::value::Value;
use neovm_core::tagged::gc::TaggedHeap;

fn borrow_iter_raw_slice(heap: &TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    let _ = items.iter().as_slice();
}

fn main() {}
