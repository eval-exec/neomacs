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

#[path = "support/thread_bounds.rs"]
mod thread_bounds;
fn main() {
    thread_bounds::ThreadSharing::<OwnerSerialState<i32, Ticket>>::check();
}
