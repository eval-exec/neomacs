//! Canonical process-IO hook descriptor (stage-4 W4/E10).
//!
//! ProcessManager and its IO methods stay in the process module; this module
//! only assembles the one descriptor eval publishes at `Context::new_inner` /
//! `Context::from_dump`. `WaitNotifier`/host-effect slots are deferred to
//! W5/E13 per peer 13:3xZ and are not part of this table.

use crate::emacs_core::eval::process_hooks::ProcessHooks;
use crate::emacs_core::process::SystemProcessExt;
use crate::heap_types::ProcessId;

fn unlink_owned_unix_listener_for_exit(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) {
    #[cfg(unix)]
    {
        ctx.processes.unlink_owned_unix_listener_for_exit(id);
    }
    #[cfg(not(unix))]
    {
        let _ = (ctx, id);
    }
}

fn delete_process(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) -> bool {
    ctx.processes.delete_process(id)
}

fn open_channel_for_module(
    ctx: &mut crate::emacs_core::eval::Context,
    process: crate::emacs_core::value::Value,
) -> Result<std::ffi::c_int, crate::emacs_core::error::Flow> {
    ctx.processes.open_channel_for_module(process)
}

fn print_process_handle(value: &crate::emacs_core::value::Value) -> Option<String> {
    crate::emacs_core::process::print_process_handle(value)
}

static CANONICAL_PROCESS_HOOKS: ProcessHooks = ProcessHooks::new(
    unlink_owned_unix_listener_for_exit,
    delete_process,
    open_channel_for_module,
    print_process_handle,
);

/// Publish the process-IO table at the one-time construction seams.
pub(crate) fn install_process_hooks() {
    let installed =
        crate::emacs_core::eval::process_hooks::install_process_hooks(&CANONICAL_PROCESS_HOOKS);
    debug_assert!(
        installed.is_ok(),
        "a different process-IO hook table is already installed"
    );
}
