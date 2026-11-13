#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
use actual_completion::completion::RetiredCompletion;
use threading::ThreadId;

fn forge_retired(thread: ThreadId) {
    let forged = RetiredCompletion {
        thread,
        _confined: Default::default(),
    };
    let _ = forged.into_thread_id();
}

fn main() {}
