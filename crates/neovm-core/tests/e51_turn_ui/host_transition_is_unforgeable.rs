#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"]
mod serial_turn;
use serial_turn::*;
include!("support/plain_host.rs");

// Outside serial_turn.rs no code can detach a host, so no detached proof
// exists that could be reattached into another host.
fn detach_outside(host: &mut i32) {
    host.detach(HostTransition(()));
}
fn main() {}
