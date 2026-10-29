#![allow(dead_code)]
#[path = "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs"]
mod capability;
use capability::*;

#[path = "../pentry_ui/support/thread_bounds.rs"]
mod thread_bounds;
fn main() {
    thread_bounds::ThreadSharing::<()>::check();
}
