#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;

fn after_park(mut state: OwnerSerialState<i32>) {
    let mut turn = state.begin(());
    let (_parked, _ticket) = turn.park("mutex");
    turn.with_world(|world| *world += 1);
}
fn main() {}
