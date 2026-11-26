use neovm_core::emacs_core::value::Value;
use neovm_core::tagged::gc::TaggedHeap;

fn borrow_as_ref_raw_slice(heap: &TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    let _: &[Value] = items.as_ref();
}

fn main() {}
