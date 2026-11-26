//! U1: a safe call of `TaggedHeap::collect` fails with E0133.
use neovm_value::tagged::gc::TaggedHeap;

fn main() {
    let mut heap = TaggedHeap::new();
    heap.collect(std::iter::empty());
}
