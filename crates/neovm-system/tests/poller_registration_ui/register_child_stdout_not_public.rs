//! `register_child_stdout_with_poller` is crate-private (`ChildPipePollerExt`),
//! not exposed on `pub trait SystemProcessExt`.
use neovm_eval::emacs_core::eval::data::process_state::{ChildOutputReader, ProcessManager};
use neovm_system::emacs_core::process::SystemProcessExt;

fn check(poller: &polling::Poller, stdout: &ChildOutputReader) {
    ProcessManager::register_child_stdout_with_poller(poller, stdout, 1);
}

fn main() {
    let _ = check;
}
