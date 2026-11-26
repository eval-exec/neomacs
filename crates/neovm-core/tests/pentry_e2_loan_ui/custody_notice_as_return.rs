#![allow(dead_code, unused_imports)]
use neovm_core::pentry_e2_custody::{CustodyFailureNotice, ReturnedSerialTicket, SuspendedKeeper};

fn probe(keeper: SuspendedKeeper<'static, 'static>, notice: CustodyFailureNotice) {
    let _ = keeper.accept_return(notice);
}

fn main() {}
