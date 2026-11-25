//! `register_child_stdin_writable_with_poller` is crate-private
//! (`ChildPipePollerExt`), not exposed on `pub trait SystemProcessExt`.
use neovm_eval::emacs_core::eval::data::process_state::ProcessManager;
use neovm_system::emacs_core::process::SystemProcessExt;

fn check(poller: &polling::Poller, stdin: &std::fs::File) {
    ProcessManager::register_child_stdin_writable_with_poller(poller, stdin, 1);
}

fn main() {
    let _ = check;
}
