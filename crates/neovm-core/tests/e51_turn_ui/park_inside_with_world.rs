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

fn park_inside(mut state: OwnerSerialState<i32, Ticket>) {
    let mut turn = match state.begin(Ticket) {
        Ok(turn) => turn,
        Err(_) => return,
    };
    turn.with_world(|world| {
        *world += 1;
        let _ = turn.park("mutex");
    });
}
fn main() {}
