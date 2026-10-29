#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong<'a, 'b, 'owner, 'shape>(
    run: ParallelExecution<'_, 'a, 'owner, 'shape>,
    other: &HeapIdentity<'b>,
) {
    run.require_heap(other);
}
fn main() {}
