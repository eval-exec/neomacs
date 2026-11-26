//! value-gc C12 regression guard: ALL 9 collector-protocol `TaggedHeap` entry
//! points require `&StoppedWorldProof<'_>` and fail to compile when called
//! without a stopped-world proof.

use neovm_value::tagged::gc::TaggedHeap;

fn call_without_proof(heap: &mut TaggedHeap) {
    unsafe {
        heap.collect(std::iter::empty());
        heap.collect_exact(std::iter::empty());
        heap.begin_stw_collection();
        heap.complete_collection();
        let _ = heap.begin_minor_collection();
        heap.complete_minor_collection();
        heap.incremental_finish(0, std::time::Instant::now());
        let _ = heap.incremental_sweep_slice(1);
        heap.finish_incremental_sweep_now();
    }
}

fn main() {}
