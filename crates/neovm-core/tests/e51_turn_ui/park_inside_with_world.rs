#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;

fn park_inside(mut state: OwnerSerialState<i32>) {
    let mut turn = state.begin(());
    turn.with_world(|world| {
        *world += 1;
        let _ = turn.park("mutex");
    });
}
fn main() {}
