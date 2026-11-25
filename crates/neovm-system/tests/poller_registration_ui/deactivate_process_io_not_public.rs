//! `deactivate_process_io` is crate-private (`ChildPipePollerExt`) and `unsafe`,
//! so external safe code cannot pass `None` to drop registered poll sources.
use neovm_eval::emacs_core::eval::data::process_state::{Process, ProcessManager};

fn check(proc: &mut Process) {
    ProcessManager::deactivate_process_io(None, proc);
}

fn main() {
    let _ = check;
}
