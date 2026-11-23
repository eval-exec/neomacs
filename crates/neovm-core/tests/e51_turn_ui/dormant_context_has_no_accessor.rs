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

fn reach(state: &OwnerSerialState<i32, Ticket>) -> &i32 {
    &state.dormant.context
}
fn main() {}
