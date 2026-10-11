use neovm_core::tagged::gc::TaggedHeap;

fn main() {
    let mut heap = TaggedHeap::new();
    heap.shutdown().unwrap();
    heap.finish_concurrent_mark().unwrap();
}
