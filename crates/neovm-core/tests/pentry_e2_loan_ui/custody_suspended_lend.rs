#![allow(dead_code, unused_imports)]
use neovm_core::pentry_e2_custody::{KeeperReadyRequest, StoppedKeeper, SuspendedKeeper};

fn probe(keeper: SuspendedKeeper<'static, 'static>, request: KeeperReadyRequest<'static>) {
    let _ = keeper.lend_serial(request);
}

fn main() {}
