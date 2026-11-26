//! `deactivate_process_io` is crate-private (`ChildPipePollerExt`), so
//! external safe code cannot tear down poll-registered I/O.
use neovm_eval::emacs_core::eval::data::process_state::{Process, ProcessManager};

fn check(proc: &mut Process) {
    ProcessManager::deactivate_process_io(proc);
}

fn main() {
    let _ = check;
}
