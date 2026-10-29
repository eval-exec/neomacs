#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong<'heap, 'a, 'b, 'shape>(
    run: ParallelExecution<'_, 'heap, 'a, 'shape>,
    other: &OwnerIdentity<'a>,
) {
    run.require_owner(other);
}
fn main() {}
