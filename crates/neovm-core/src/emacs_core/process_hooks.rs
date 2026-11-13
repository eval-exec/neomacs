//! Canonical process-IO hook descriptor (stage-4 W4/E10).
//!
//! ProcessManager and its IO methods stay in the process module; this module
//! only assembles the one descriptor eval publishes at `Context::new_inner` /
//! `Context::from_dump`. `WaitNotifier` is eval-owned since A3 and has a slot.
//! Green select and native waits live directly in eval (peer 5695).

#![allow(private_interfaces)] // host_effect/switch vocabulary becomes eval-owned at W5/E13
use crate::emacs_core::eval::process_hooks::ProcessHooks;
use crate::emacs_core::process::SystemProcessExt;
use crate::heap_types::ProcessId;

fn unlink_owned_unix_listener_for_exit(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) {
    #[cfg(unix)]
    {
        ctx.processes_mut().unlink_owned_unix_listener_for_exit(id);
    }
    #[cfg(not(unix))]
    {
        let _ = (ctx, id);
    }
}

fn delete_process(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) -> bool {
    ctx.processes_mut().delete_process(id)
}

fn open_channel_for_module(
    ctx: &mut crate::emacs_core::eval::Context,
    process: crate::emacs_core::value::Value,
) -> Result<std::ffi::c_int, crate::emacs_core::error::Flow> {
    ctx.processes().open_channel_for_module(process)
}

fn print_process_handle(value: &crate::emacs_core::value::Value) -> Option<String> {
    crate::emacs_core::process::print_process_handle(value)
}

fn wait_notifier(
    ctx: &crate::emacs_core::eval::Context,
) -> Option<crate::emacs_core::eval::data::process_state::WaitNotifier> {
    ctx.processes().wait_notifier()
}

fn terminate_and_defer_reaping(
    child: &mut crate::emacs_core::eval::data::process_state::ChildOwnership,
    kill_signal: i32,
) {
    use crate::emacs_core::process::reap::ChildOwnershipIo;
    child.terminate_and_defer_reaping(kill_signal);
}

static CANONICAL_PROCESS_HOOKS: ProcessHooks = ProcessHooks::new(
    unlink_owned_unix_listener_for_exit,
    delete_process,
    open_channel_for_module,
    print_process_handle,
    wait_notifier,
    terminate_and_defer_reaping,
);

/// The one canonical process descriptor (published only by the one core
/// boot entry, peer follow-up F2).
pub(crate) fn canonical_process_hooks() -> &'static ProcessHooks {
    &CANONICAL_PROCESS_HOOKS
}
