//! U1 protocol spelling: a safe call of the sweep entries fails with E0133.
use neovm_value::tagged::gc::TaggedHeap;

fn main() {
    let mut heap = TaggedHeap::new();
    heap.begin_stw_collection();
    heap.complete_collection();
}
