//! U1: a safe call of `TaggedHeap::collect` fails with E0133.
use neovm_value::tagged::gc::{StoppedWorldProof, TaggedHeap};

fn try_collect(heap: &mut TaggedHeap, stopped: &StoppedWorldProof<'_>) {
    heap.collect(stopped, std::iter::empty());
}

fn main() {
    let _ = try_collect;
}
