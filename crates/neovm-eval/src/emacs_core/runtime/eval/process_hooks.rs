//! Process IO cold table (stage-4 W4/E10).
//!
//! The process authority (ProcessManager and its system extension traits)
//! stays in the core/system layer. The evaluator reaches the process IO it
//! drives — the exit-time listener/process teardown, the dynamic-module
//! channel open, the process object's printed handle, and GNU's
//! `record_kill_process` signal-plus-deferred-reap transition — through this
//! one cold PC2 table installed by core at `Context::new_inner` and
//! `Context::from_dump`.
//!
//! Frequency: per buffer/process teardown, per module channel open, per
//! process object print, per process kill — never in a Lisp loop.
//!
//! `WaitNotifier` is eval-owned (eval::data::process_state) and is carried by
//! this table. Green select and native waits execute directly in eval;
//! their production registry is the `select` child (peer 5695).

#![allow(private_interfaces)] // host_effect vocabulary becomes eval-owned at W5/E13
use super::*;
use crate::emacs_core::eval::data::process_state::{ChildOwnership, WaitNotifier};
use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWake};
use neovm_value::heap_types::ProcessId;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

pub use super::green_select::GreenSelectHandle;
pub(crate) mod select;

/// Close an owned Unix listener after kill-emacs hooks (Unix only; no-op elsewhere).
pub type UnlinkOwnedUnixListener = fn(&mut Context, ProcessId);
/// Terminate and reap one process without running its sentinel.
pub type DeleteProcess = fn(&mut Context, ProcessId) -> bool;
/// Open the file descriptor a dynamic module uses to talk to PROCESS.
pub type OpenChannelForModule = fn(&mut Context, Value) -> Result<std::ffi::c_int, Flow>;
/// The printed representation of a process object, if it is still known.
pub type PrintProcessHandle = fn(&Value) -> Option<String>;

/// Opaque process-select handle. The production eval registry keeps
/// `GreenSelectHandle` behind the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SelectHandleToken(NonZeroU64);

impl SelectHandleToken {
    /// Wrap a registry-issued id (the production registry keys tokens by it).
    pub fn from_raw(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    /// The registry id.
    pub fn raw(self) -> NonZeroU64 {
        self.0
    }
}

/// One armed native-select registration, kept behind the token by the production eval registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SelectArmToken(NonZeroU64);

impl SelectArmToken {
    /// Wrap a registry-issued registration id.
    pub fn from_raw(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    /// The registry id.
    pub fn raw(self) -> NonZeroU64 {
        self.0
    }
}

/// The result of arming the idle scheduler's select source.
#[derive(Clone, Copy, Debug)]
pub enum SelectArm {
    /// The registration is armed; use `select_wait_registered`.
    Armed(SelectArmToken),
    /// Arming failed (the lower source remembers backend loss).
    Failed,
    /// The token no longer names a live handle.
    Unavailable,
}

/// An armed select registration whose drop cancels the arm, preserving the
/// original local-registration lifetime (early returns must not leak an armed
/// wake registration).
#[derive(Debug)]
pub struct SelectArmGuard {
    arm: Option<SelectArmToken>,
}

impl SelectArmGuard {
    /// Arm TOKEN; a failed or unavailable arm yields a guard whose wait reports
    /// activity, matching the original `Some(Err(_)) => true`.
    pub fn arm(token: SelectHandleToken) -> Self {
        match select::select_arm(token) {
            SelectArm::Armed(arm) => Self { arm: Some(arm) },
            SelectArm::Failed | SelectArm::Unavailable => Self { arm: None },
        }
    }

    /// Wait for the armed registration (consumes the arm so Drop cancels nothing).
    pub fn wait(&mut self, token: SelectHandleToken, deadline: Instant) -> bool {
        match self.arm.take() {
            Some(arm) => select::select_wait_registered(token, arm, deadline),
            None => true,
        }
    }
}

impl Drop for SelectArmGuard {
    fn drop(&mut self) {
        if let Some(arm) = self.arm.take() {
            select::cancel_select_arm(arm);
        }
    }
}

/// Opaque prepared native-wait handle. The production eval registry keeps
/// `PreparedNativeWait` behind the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeWaitToken(NonZeroU64);

impl NativeWaitToken {
    /// Wrap a registry-issued wait id.
    pub fn from_raw(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    /// The registry id.
    pub fn raw(self) -> NonZeroU64 {
        self.0
    }
}

/// GNU's main-owner wait preparation, with the native wait behind a guard.
#[derive(Debug)]
pub enum MainWaitPreparation {
    Ready,
    Scheduler,
    Native(NativeWaitGuard),
}

/// One prepared native wait whose drop cancels the preparation, preserving
/// the original `PreparedNativeWait` lifetime on abandoned park requests.
#[derive(Debug)]
pub struct NativeWaitGuard {
    token: Option<NativeWaitToken>,
}

impl NativeWaitGuard {
    /// Wrap a registry-issued token (core's prepare slot).
    pub fn from_raw(token: NativeWaitToken) -> Self {
        Self { token: Some(token) }
    }

    /// Run the prepared wait; the guard no longer cancels on drop.
    pub fn wait_with(
        &mut self,
        handoff: &mut super::MainNativeHandoff<'_>,
    ) -> std::io::Result<crate::host_effect::wake::ControlWakeOutcome> {
        match self.token.take() {
            Some(token) => select::wait_main_native(token, handoff),
            None => Err(std::io::Error::other("native wait token already consumed")),
        }
    }
}

impl Drop for NativeWaitGuard {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            select::cancel_native_wait(token);
        }
    }
}

/// Host-input/diagnostic wake handle for the current wait loop.
pub type WaitNotifierHook = fn(&Context) -> Option<WaitNotifier>;

/// GNU `record_kill_process`: signal the child and defer its reap.
pub type TerminateAndDeferReaping = fn(&mut ChildOwnership, i32);

/// The cold process-IO bodies eval calls.
#[derive(Clone, Copy)]
pub struct ProcessHooks {
    unlink: UnlinkOwnedUnixListener,
    delete: DeleteProcess,
    open_channel: OpenChannelForModule,
    print_handle: PrintProcessHandle,
    wait_notifier: WaitNotifierHook,
    terminate_and_defer_reaping: TerminateAndDeferReaping,
}

impl ProcessHooks {
    /// Build the one canonical descriptor core publishes.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        unlink: UnlinkOwnedUnixListener,
        delete: DeleteProcess,
        open_channel: OpenChannelForModule,
        print_handle: PrintProcessHandle,
        wait_notifier: WaitNotifierHook,
        terminate_and_defer_reaping: TerminateAndDeferReaping,
    ) -> Self {
        Self {
            unlink,
            delete,
            open_channel,
            print_handle,
            wait_notifier,
            terminate_and_defer_reaping,
        }
    }
}

static PROCESS_HOOKS: OnceLock<ProcessHooks> = OnceLock::new();
static PROCESS_HOOKS_IDENTITY: OnceLock<&'static ProcessHooks> = OnceLock::new();

/// Publish the process-IO descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting process-IO publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_process_hooks(
    hooks: &'static ProcessHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&PROCESS_HOOKS, &PROCESS_HOOKS_IDENTITY, hooks)
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

/// The wake handle for the current wait loop (host input, diagnostics,
/// asynchronous process work).
pub fn wait_notifier(ctx: &Context) -> Option<WaitNotifier> {
    (process_hooks().wait_notifier)(ctx)
}

/// GNU `record_kill_process`: signal CHILD and defer its reap.
pub fn terminate_and_defer_reaping(child: &mut ChildOwnership, kill_signal: i32) {
    (process_hooks().terminate_and_defer_reaping)(child, kill_signal)
}

/// Capture the world's process-select handle as an eval token.
pub fn select_handle(ctx: &mut Context) -> Option<SelectHandleToken> {
    select::register_manager(ctx.processes())
}

/// Wake the idle scheduler's select source.
pub fn select_notify(token: SelectHandleToken) {
    select::select_notify(token)
}

/// Nudge one native destination of the select source.
pub fn select_nudge(
    token: SelectHandleToken,
    destination: ControlDestination,
    reason: ControlNudge,
) {
    select::select_nudge(token, destination, reason)
}

/// The backend's existing readiness owner, when the token is live.
pub fn select_control_source(token: SelectHandleToken) -> Option<Arc<ControlWake>> {
    select::select_control_source(token)
}

/// Recheck select readiness without arming (a registered wait is live).
pub fn select_wait_until(token: SelectHandleToken, deadline: Instant) -> bool {
    select::select_wait_until(token, deadline)
}

/// Prepare the main owner's native wait (the handle stays behind a token).
pub fn prepare_main_wait(ctx: &mut Context, deadline: Instant) -> MainWaitPreparation {
    select::prepare_main_wait(ctx, deadline)
}
