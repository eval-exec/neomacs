#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
use actual_completion::completion::CompletingThread;

fn clone_completion(completion: CompletingThread) {
    let duplicate = completion.clone();
    std::mem::forget(duplicate);
    std::mem::forget(completion);
}

fn main() {}
