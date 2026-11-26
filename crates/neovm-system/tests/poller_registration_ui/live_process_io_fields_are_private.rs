//! `LiveProcessIo` fields and `Process::live_io` are private so safe code
//! cannot overwrite or drop poll-registered descriptors.
use neovm_eval::emacs_core::eval::data::process_state::{LiveProcessIo, Process};

fn overwrite_fields(proc: &mut Process, io: &mut LiveProcessIo) {
    io.child_stdout = None;
    io.network_socket = None;
    io.tls_stream = None;
    io.serial_port = None;
    io.pty_master = None;
    io.child_status_source = None;
    proc.live_io = LiveProcessIo::default();
}

fn main() {
    let _ = overwrite_fields;
}
