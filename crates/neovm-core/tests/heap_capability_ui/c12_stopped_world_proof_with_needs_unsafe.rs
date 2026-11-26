//! value-gc C12 regression guard: `StoppedWorldProof::with_stopped_world`
//! requires an `unsafe` block; safe code cannot mint a stopped-world proof.

use neovm_value::tagged::gc::{StoppedWorldProof, TaggedHeap};

fn try_mint_safely(heap: &mut TaggedHeap) {
    StoppedWorldProof::with_stopped_world(heap, |_heap, _stopped| {});
}

fn main() {}
