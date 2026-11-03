#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
use actual_completion::completion::CompletingThread;

fn forge_completion(completion: CompletingThread) {
    // Struct update cannot duplicate the real token's sealed fields.
    let forged = CompletingThread { ..completion };
    std::mem::forget(forged);
    std::mem::forget(completion);
}

fn main() {}
