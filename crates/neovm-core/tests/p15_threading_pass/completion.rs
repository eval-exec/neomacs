#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{tagged, threading};

#[path = "../p15_threading_support/actual_completion.rs"]
mod actual_completion;
#[path = "../p15_threading_support/actual_completion_exit.rs"]
mod actual_completion_exit;
use actual_completion::ThreadManager;
use actual_completion::completion::{CompletionError, CompletionExit};
use actual_completion_exit::emacs_core;
use actual_completion_exit::{Baton, Control};
use threading::ThreadId;

fn completion_client(
    mut manager: ThreadManager,
    thread: ThreadId,
) -> Result<ThreadId, CompletionError> {
    let completion = manager.prepare_thread_completion(thread, CompletionExit::Executed)?;
    // This is the actual production consuming signature, not a fixture foil.
    let retired = manager.retire_thread_completion(completion);
    Ok(retired.into_thread_id())
}

fn exit_client(
    mut manager: ThreadManager,
    thread: ThreadId,
    control: &Control,
    baton: Baton,
) -> Result<(), CompletionError> {
    let active = manager.prepare_thread_completion(thread, CompletionExit::Executed)?;
    let retired = manager.retire_thread_completion(active);
    control.finish(retired, baton);
    Ok(())
}

fn main() {
    // Type-check only; acquiring actual owner/Baton authority is a runtime
    // test obligation, never a constructor supplied by this adapter.
    let _: fn(ThreadManager, ThreadId) -> Result<ThreadId, CompletionError> = completion_client;
    let _: fn(ThreadManager, ThreadId, &Control, Baton) -> Result<(), CompletionError> =
        exit_client;
}
