#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/keeper_custody.rs"]
mod keeper_custody;

use keeper_custody::{KeeperReadyRequest, StoppedKeeper};

fn lend<'inner, 'outer: 'inner, 'era>(
    keeper: StoppedKeeper<'inner, 'era>,
    request: KeeperReadyRequest<'outer>,
) {
    let _ = keeper.lend_serial(request);
}

fn main() {}
