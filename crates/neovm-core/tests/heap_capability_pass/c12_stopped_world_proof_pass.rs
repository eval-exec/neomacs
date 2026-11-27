//! Positive control for value-gc C12: minting `&StoppedWorldProof<'_>`
//! via `unsafe { StoppedWorldProof::with_stopped_world(heap, |heap, stopped| ...) }` and
//! passing it to ALL 10 public collector-protocol entry points compiles cleanly.

use neovm_value::tagged::gc::{StoppedWorldProof, TaggedHeap};

fn drive_all_public_entries(heap: &mut TaggedHeap) {
    unsafe {
        StoppedWorldProof::with_stopped_world(heap, |heap, stopped| {
            let _ = stopped.heap_identity();
            heap.collect(stopped, std::iter::empty());
            heap.collect_exact(stopped, std::iter::empty());
            heap.begin_stw_collection(stopped);
            heap.complete_collection(stopped);
            let _ = heap.begin_minor_collection(stopped);
            heap.complete_minor_collection(stopped);
            heap.incremental_finish(stopped, 0, std::time::Instant::now());
            let _ = heap.incremental_sweep_slice(stopped, 1);
            heap.finish_incremental_sweep_now(stopped);
            heap.finish_first_partition_cycle(stopped);
        });
    }
}

fn main() {
    let _ = drive_all_public_entries;
}
