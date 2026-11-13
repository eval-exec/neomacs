#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
use actual_completion::ThreadManager;
use actual_completion::completion::CompletingThread;

fn consume_twice(mut manager: ThreadManager, completion: CompletingThread) {
    let _first = manager.retire_thread_completion(completion);
    let _second = manager.retire_thread_completion(completion);
}

fn main() {}
