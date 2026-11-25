//! `ProcessManager::processes` and `deleted_processes` are private so safe code
//! outside `neovm-eval` cannot remove, clear, or overwrite a `Process` while
//! its `LiveProcessIo` descriptors remain registered on a poller.
use neovm_eval::emacs_core::eval::data::process_state::ProcessManager;

fn drop_via_map(pm: &mut ProcessManager) {
    let _ = pm.processes.remove(&1);
    pm.processes.clear();
    pm.deleted_processes.clear();
}

fn main() {
    let _ = drop_via_map;
}
