//! Input wait/pending and OS-signal cold table (stage-4 W4/E10 batch 3).
//!
//! The command-input interrupt policy (`CommandLoopHost`), the OS-signal
//! latch, and the pending-signal drain stay in the core command/system layer;
//! the evaluator's read/wait paths reach them through this one cold PC2 table
//! installed by core at `Context::new_inner` and `Context::from_dump`.
//!
//! `CommandInputWaitOutcome` is plain data and lives here (TYPE-DOWN): the
//! wait implementation in `system::wait` re-exports it.
//!
//! Frequency: per special input event and per wait/quit poll — never per
//! bytecode step.

use super::*;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// Why a command-input wait returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandInputWaitOutcome {
    InputPending,
    Interrupted,
    DeadlineElapsed,
}

/// Apply GNU's command-loop interrupt policy to one special input event.
pub type InterruptForInputEvent =
    fn(&mut Context, super::keyboard::InputEvent) -> Result<bool, Flow>;
/// Whether a pending OS signal is latched.
pub type OsSignalPending = fn() -> bool;
/// Take the latched termination signal, if any.
pub type TakeTerminationSignal = fn() -> Option<i32>;
/// Drain pending OS signals through the evaluator.
pub type DrainPendingOsSignals = fn(&mut Context) -> Result<(), Flow>;
/// The process-lifetime signal self-pipe read end for evaluator wait pollers.
pub type SignalWakePipeReadFd = fn() -> Option<i32>;
/// Drain one wake byte from the signal self-pipe.
pub type DrainSignalWakePipe = fn(i32);

/// How an OS signal wakes an evaluator wait.
///
/// GNU's POSIX port writes a self-pipe so `pselect` returns. The w32 port
/// does not: `sys_select` waits in `MsgWaitForMultipleObjects`
/// (`w32inevt.c`), and GUI input sets the manual-reset event
/// `input_available` (`w32xfns.c` `notify_msg_ready`). That event is not
/// this poller's descriptor, so Windows publishes [`SignalWakeSource::Absent`].
/// The slot is present on every platform.
#[derive(Clone, Copy)]
pub enum SignalWakeSource {
    /// Process-lifetime nonblocking self-pipe.
    SelfPipe {
        read_fd: SignalWakePipeReadFd,
        drain: DrainSignalWakePipe,
    },
    /// No POSIX self-pipe. GNU w32 wakes `MsgWaitForMultipleObjects` instead.
    Absent,
}

/// The cold input-wait/OS-signal bodies eval calls.
#[derive(Clone, Copy)]
pub struct InputHooks {
    interrupt_for_input_event_if_requested: InterruptForInputEvent,
    os_signal_pending: OsSignalPending,
    take_termination_signal: TakeTerminationSignal,
    drain_pending_os_signals: DrainPendingOsSignals,
    signal_wake: SignalWakeSource,
}

impl InputHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        interrupt_for_input_event_if_requested: InterruptForInputEvent,
        os_signal_pending: OsSignalPending,
        take_termination_signal: TakeTerminationSignal,
        drain_pending_os_signals: DrainPendingOsSignals,
        signal_wake: SignalWakeSource,
    ) -> Self {
        Self {
            interrupt_for_input_event_if_requested,
            os_signal_pending,
            take_termination_signal,
            drain_pending_os_signals,
            signal_wake,
        }
    }
}

static INPUT_HOOKS: OnceLock<InputHooks> = OnceLock::new();
static INPUT_HOOKS_IDENTITY: OnceLock<&'static InputHooks> = OnceLock::new();

/// Publish the input-wait descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting input-wait publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_input_hooks(
    hooks: &'static InputHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&INPUT_HOOKS, &INPUT_HOOKS_IDENTITY, hooks)
}

/// The installed descriptor.
///
/// Fail-closed: the read/wait paths run only after construction/dump, so a
/// missing table means the construction seam is broken; the interrupt policy
/// must never silently default.
fn input_hooks() -> &'static InputHooks {
    INPUT_HOOKS
        .get()
        .expect("input hooks install at Context construction")
}

/// Apply the command-loop interrupt policy to EVENT.
pub fn interrupt_for_input_event_if_requested(
    ctx: &mut Context,
    event: super::keyboard::InputEvent,
) -> Result<bool, Flow> {
    (input_hooks().interrupt_for_input_event_if_requested)(ctx, event)
}

/// Whether a pending OS signal is latched.
pub fn os_signal_pending() -> bool {
    (input_hooks().os_signal_pending)()
}

/// Take the latched termination signal, if any.
pub fn take_termination_signal() -> Option<i32> {
    (input_hooks().take_termination_signal)()
}

/// Drain pending OS signals through the evaluator.
pub fn drain_pending_os_signals(ctx: &mut Context) -> Result<(), Flow> {
    (input_hooks().drain_pending_os_signals)(ctx)
}

/// The process-lifetime signal self-pipe read end, or `None` when the
/// installed source is not a self-pipe.
pub fn signal_wake_pipe_read_fd() -> Option<i32> {
    match input_hooks().signal_wake {
        SignalWakeSource::SelfPipe { read_fd, .. } => read_fd(),
        SignalWakeSource::Absent => None,
    }
}

/// Drain one wake byte from the signal self-pipe. An absent source has no byte.
pub fn drain_signal_wake_pipe(fd: i32) {
    if let SignalWakeSource::SelfPipe { drain, .. } = input_hooks().signal_wake {
        drain(fd);
    }
}
