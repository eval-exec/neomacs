use neovm_core::emacs_core::value::Value;
use neovm_core::tagged::gc::TaggedHeap;

fn borrow_raw_slice(heap: &TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    let _: &[Value] = items.as_slice();
    let _: &[Value] = items;
    let _ = items[0];
}

fn main() {}
