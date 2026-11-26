use neovm_core::emacs_core::value::Value;
use neovm_core::tagged::gc::TaggedHeap;

fn borrow_ref_index(heap: &TaggedHeap, v: Value) {
    let items = v.vector_data_in(heap.heap_ref()).unwrap();
    let _: &Value = &items[0];
}

fn main() {}
