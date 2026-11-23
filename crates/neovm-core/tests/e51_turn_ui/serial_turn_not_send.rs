#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;

#[path = "support/thread_bounds.rs"]
mod thread_bounds;
fn main() {
    thread_bounds::ThreadTransfer::<SerialTurn<'static, i32, ()>>::check();
}
