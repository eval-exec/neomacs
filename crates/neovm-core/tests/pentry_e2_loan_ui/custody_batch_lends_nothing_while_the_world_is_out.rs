#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/driver_core.rs"]
mod driver_core;
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/sync.rs"]
mod sync;

use driver_core::{Batch, KeeperOps};

// L6.7b b3: lending the World moves the batch, and only `settle_world`,
// which takes the World's return, gives it back. So the driver cannot lend
// the next head before the World came back.
fn lend_during_the_world<O: KeeperOps, B: Batch<O>>(batch: B, next: B::Ready) {
    let _world = batch.lend_world();
    let _ = batch.lend(next);
}

fn main() {}
