use neovm_value::tagged::gc::heap_transfer::HeapRootDesignator;
use neovm_value::tagged::gc::TaggedHeap;
use std::ptr::NonNull;

fn main() {
    let _root = HeapRootDesignator::from_original(NonNull::<TaggedHeap>::dangling());
}
