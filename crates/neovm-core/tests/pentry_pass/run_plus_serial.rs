#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong<'heap, 'owner, 'shape>(
    owner: &mut OwnerState<'heap, 'owner, 'shape>,
    witness: &BatonWitness<'_, 'heap, 'owner>,
) {
    let run = owner.parallel();
    drop(run); let run = (); let serial = owner.serial(witness);
    drop((run, serial));
}
fn main() {}
