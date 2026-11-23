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

struct World {
    buffer: i32,
}
impl SerialHost for World {
    type Detached = ();
    type Refusal = ();
    fn detach(&mut self, _: HostTransition) {}
    fn reattach(&mut self, _: (), _: HostTransition) -> Result<(), ((), ())> {
        Ok(())
    }
}

fn escape<'t>(turn: &'t mut SerialTurn<'_, World, Ticket>) -> &'t i32 {
    turn.with_world(|world| &world.buffer)
}
fn main() {}
