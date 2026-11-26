#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/driver_core.rs"]
mod driver_core;
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/sync.rs"]
mod sync;

use driver_core::{Batch, KeeperOps};

// L3/L4/L5: a batch lends one ticket. The next lend needs the batch that
// `settle` makes from the returned ticket, so the driver cannot lend before
// the last ticket came back.
fn lend_twice<O: KeeperOps, B: Batch<O>>(batch: B, first: B::Ready, next: B::Ready) {
    let _lent = batch.lend(first);
    let _ = batch.lend(next);
}

fn main() {}
