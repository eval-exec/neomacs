#![deny(unused_must_use)]

use neovm_core::tagged::gc::TaggedHeap;

fn discard_completion(heap: &mut TaggedHeap) {
    heap.finish_concurrent_mark();
}

fn main() {}
