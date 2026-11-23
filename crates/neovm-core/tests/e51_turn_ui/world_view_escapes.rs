#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;

struct Ticket;
impl TicketOwner for Ticket {
    type Owner = u8;
    fn owner(&self) -> u8 {
        0
    }
}

struct World {
    buffer: i32,
}

fn escape<'t>(turn: &'t mut SerialTurn<'_, World, Ticket>) -> &'t i32 {
    turn.with_world(|world| &world.buffer)
}
fn main() {}
