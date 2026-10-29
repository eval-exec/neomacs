#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong<'heap, 'owner, 'before, 'after>(
    run: ParallelExecution<'_, 'heap, 'owner, 'after>,
    stale: &BindingShape<'after>,
) {
    run.require_shape(stale);
}
fn main() {}
