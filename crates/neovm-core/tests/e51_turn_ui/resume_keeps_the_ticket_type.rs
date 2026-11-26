#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;
include!("support/plain_host.rs");

struct Ticket;
impl TicketOwner for Ticket {
    type Owner = u8;
    fn owner(&self) -> u8 {
        0
    }
}

struct OtherTicket;
impl TicketOwner for OtherTicket {
    type Owner = u8;
    fn owner(&self) -> u8 {
        0
    }
}

fn swap_type(mut state: OwnerSerialState<i32, Ticket>) {
    let mut turn = match state.begin(Ticket) {
        Ok(turn) => turn,
        Err(_) => return,
    };
    let (parked, _ticket) = match turn.park("mutex") {
        Ok(pair) => pair,
        Err(_) => return,
    };
    let _ = parked.resume(OtherTicket);
}
fn main() {}
