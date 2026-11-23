#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;

fn reach(state: &OwnerSerialState<i32>) -> &i32 {
    &state.dormant.context
}
fn main() {}
