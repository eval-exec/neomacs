//! `SystemProcessExt::register_readable_source` requires `unsafe`.
use neovm_eval::emacs_core::eval::data::process_state::ProcessManager;
use neovm_system::emacs_core::process::SystemProcessExt;

fn check(poller: &polling::Poller, stream: &std::net::TcpStream) {
    let _ = ProcessManager::register_readable_source(poller, stream, 1);
}

fn main() {
    let _ = check;
}
