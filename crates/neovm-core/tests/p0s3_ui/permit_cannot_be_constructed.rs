use neovm_core::tagged::gc::{ConcurrentMarkPermit, TaggedHeap};
use std::marker::PhantomData;

fn fabricate(heap: &mut TaggedHeap) -> ConcurrentMarkPermit<'_> {
    let heap_identity = heap.heap_identity();
    ConcurrentMarkPermit {
        heap,
        heap_identity,
        _owner: PhantomData,
    }
}

fn main() {}
