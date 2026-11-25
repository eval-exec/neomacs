//! Pass control: safe read-only `LiveProcessIo` inspection and `SystemProcessExt`
//! imports compile cleanly.
use neovm_eval::emacs_core::eval::data::process_state::{LiveProcessIo, Process, ProcessManager};
use neovm_system::emacs_core::process::SystemProcessExt;

fn inspect(proc: &Process, io: &LiveProcessIo) {
    let _ = proc.live_io().child_stdout();
    let _ = proc.live_io().network_socket();
    let _ = proc.live_io().tls_stream();
    let _ = proc.live_io().serial_port();
    let _ = io.child();
    let _ = ProcessManager::unregister_process_poll_sources(None, proc);
}

fn main() {
    let _ = inspect;
}
