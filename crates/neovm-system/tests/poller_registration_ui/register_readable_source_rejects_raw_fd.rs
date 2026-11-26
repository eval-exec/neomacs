//! `SystemProcessExt::register_readable_source` requires `polling::AsSource`,
//! rejecting raw integer descriptors such as `-1`.
use neovm_eval::emacs_core::eval::data::process_state::ProcessManager;
use neovm_system::emacs_core::process::SystemProcessExt;

fn check(poller: &polling::Poller) {
    let _ = unsafe { ProcessManager::register_readable_source(poller, -1i32, 1) };
}

fn main() {
    let _ = check;
}
