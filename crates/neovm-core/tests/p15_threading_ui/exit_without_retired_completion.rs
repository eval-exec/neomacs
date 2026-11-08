#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{tagged, threading, value};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
#[path = "../p15_threading_support/actual_completion_exit.rs"]
mod actual_completion_exit;
use actual_completion_exit::emacs_core;

use actual_completion_exit::{Baton, Control};
use threading::ThreadId;

fn publish_without_retirement(control: &Control, baton: Baton, thread: ThreadId) {
    // Resolves to the included production Control::finish signature.
    control.finish(thread, baton);
}

fn main() {}
