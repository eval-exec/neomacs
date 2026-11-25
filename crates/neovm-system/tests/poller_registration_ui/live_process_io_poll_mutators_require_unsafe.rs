//! Mutating or clearing poll-registered sources on `LiveProcessIo` / `Process`
//! requires `unsafe`.
use neovm_eval::emacs_core::eval::data::process_state::{LiveProcessIo, Process};

fn mutate_without_unsafe(proc: &mut Process, io: &mut LiveProcessIo) {
    io.set_child_stdout(None);
    io.set_network_socket(None);
    let _ = proc.live_io_mut();
    proc.clear_live_io();
}

fn main() {
    let _ = mutate_without_unsafe;
}
