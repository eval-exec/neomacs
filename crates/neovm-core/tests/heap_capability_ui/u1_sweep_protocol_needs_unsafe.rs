//! U1 protocol spelling: a safe call of the sweep entries fails with E0133.
use neovm_value::tagged::gc::{StoppedWorldProof, TaggedHeap};

fn try_sweep(heap: &mut TaggedHeap, stopped: &StoppedWorldProof<'_>) {
    heap.begin_stw_collection(stopped);
    heap.complete_collection(stopped);
}

fn main() {
    let _ = try_sweep;
}
