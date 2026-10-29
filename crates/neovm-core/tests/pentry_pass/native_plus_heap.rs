#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

fn wrong(owner: &mut OwnerState<'_, '_, '_>) {
    let native = owner.native();
    drop(native); let native = (); let run = owner.parallel();
    drop((native, run));
}
fn main() {}
