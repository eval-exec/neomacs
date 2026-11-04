//! Process IO cold table (stage-4 W4/E10).
//!
//! The process authority (ProcessManager and its system extension traits)
//! stays in the core/system layer. The evaluator reaches the process IO it
//! drives — the exit-time listener/process teardown, the dynamic-module
//! channel open, and the process object's printed handle — through this one
//! cold PC2 table installed by core at `Context::new_inner` and
//! `Context::from_dump`.
//!
//! Frequency: per buffer/process teardown, per module channel open, per
//! process object print — never in a Lisp loop.
//!
//! `WaitNotifier` and the host-effect-typed readiness slots are deliberately
//! NOT here: peer ruling 13:3xZ moves those types with eval at W5/E13, so
//! they cannot appear in an eval-declared signature yet; their call sites
//! stay direct until then.

use super::*;
use crate::heap_types::ProcessId;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// Close an owned Unix listener after kill-emacs hooks (Unix only; no-op elsewhere).
pub type UnlinkOwnedUnixListener = fn(&mut Context, ProcessId);
/// Terminate and reap one process without running its sentinel.
pub type DeleteProcess = fn(&mut Context, ProcessId) -> bool;
/// Open the file descriptor a dynamic module uses to talk to PROCESS.
pub type OpenChannelForModule = fn(&mut Context, Value) -> Result<std::ffi::c_int, Flow>;
/// The printed representation of a process object, if it is still known.
pub type PrintProcessHandle = fn(&Value) -> Option<String>;

/// The four cold process-IO bodies eval calls.
#[derive(Clone, Copy)]
pub struct ProcessHooks {
    unlink: UnlinkOwnedUnixListener,
    delete: DeleteProcess,
    open_channel: OpenChannelForModule,
    print_handle: PrintProcessHandle,
}

impl ProcessHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        unlink: UnlinkOwnedUnixListener,
        delete: DeleteProcess,
        open_channel: OpenChannelForModule,
        print_handle: PrintProcessHandle,
    ) -> Self {
        Self {
            unlink,
            delete,
            open_channel,
            print_handle,
        }
    }
}

static PROCESS_HOOKS: OnceLock<ProcessHooks> = OnceLock::new();
static PROCESS_HOOKS_IDENTITY: OnceLock<&'static ProcessHooks> = OnceLock::new();

/// Publish the process-IO descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Core calls
/// this at `Context::new_inner` and `Context::from_dump`.
#[must_use = "handle a conflicting process-IO publication"]
#[cold]
#[inline(never)]
pub fn install_process_hooks(
    hooks: &'static ProcessHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = PROCESS_HOOKS_IDENTITY.get_or_init(|| hooks);
    if !std::ptr::eq(*selected, hooks) {
        return Err(HookInstallError::DifferentTable);
    }
    match PROCESS_HOOKS.set(**selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

/// The installed descriptor.
///
/// Fail-closed: every call site runs during evaluator lifetime, so a missing
/// table means the construction seam is broken and process state must not be
/// leaked or silently mishandled.
fn process_hooks() -> &'static ProcessHooks {
    PROCESS_HOOKS
        .get()
        .expect("process hooks install at Context construction")
}

/// Close an owned Unix listener for ID after kill-emacs hooks.
pub fn unlink_owned_unix_listener_for_exit(ctx: &mut Context, id: ProcessId) {
    (process_hooks().unlink)(ctx, id)
}

/// Terminate and reap ID without running its sentinel.
pub fn delete_process(ctx: &mut Context, id: ProcessId) -> bool {
    (process_hooks().delete)(ctx, id)
}

/// Open the dynamic-module channel to PROCESS.
pub fn open_channel_for_module(ctx: &mut Context, process: Value) -> Result<std::ffi::c_int, Flow> {
    (process_hooks().open_channel)(ctx, process)
}

/// The printed representation of a process object.
pub fn print_process_handle(value: &Value) -> Option<String> {
    (process_hooks().print_handle)(value)
}
