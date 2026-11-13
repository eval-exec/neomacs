#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
use actual_completion::completion::RetiredCompletion;

#[path = "../thread_safety_ui/support/thread_bounds.rs"]
mod thread_bounds;
use thread_bounds::ThreadTransfer;

fn main() {
    ThreadTransfer::<RetiredCompletion>::check();
}
