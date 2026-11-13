//! Canonical input-wait/OS-signal hook descriptor (stage-4 W4/E10 batch 3).
//!
//! The interrupt policy stays in `commands/keyboard` (`CommandLoopHost`) and
//! the signal latch/drain in `system::os_signal`; this module only assembles
//! the one descriptor eval publishes at `Context::new_inner` /
//! `Context::from_dump`.

use crate::emacs_core::eval::input_hooks::{InputHooks, SignalWakeSource};
use crate::emacs_core::keyboard::CommandLoopHost;

fn interrupt_for_input_event_if_requested(
    ctx: &mut crate::emacs_core::eval::Context,
    event: crate::keyboard::InputEvent,
) -> Result<bool, crate::emacs_core::error::Flow> {
    ctx.interrupt_for_input_event_if_requested(event)
}

fn os_signal_pending() -> bool {
    crate::emacs_core::os_signal::pending()
}

fn take_termination_signal() -> Option<i32> {
    crate::emacs_core::os_signal::take_termination_signal()
}

fn drain_pending_os_signals(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), crate::emacs_core::error::Flow> {
    crate::emacs_core::os_signal::drain_pending_os_signals(ctx).map(|_| ())
}

/// POSIX self-pipe read end. GNU w32 has no such pipe.
#[cfg(unix)]
fn unix_signal_wake_read_fd() -> Option<i32> {
    crate::emacs_core::os_signal::install().self_pipe_read_fd()
}

/// Drain the POSIX self-pipe. GNU w32 wakes `MsgWaitForMultipleObjects` instead.
#[cfg(unix)]
fn unix_signal_wake_drain(fd: i32) {
    crate::emacs_core::os_signal::drain_wake_pipe(fd)
}

const fn canonical_signal_wake() -> SignalWakeSource {
    #[cfg(unix)]
    {
        SignalWakeSource::SelfPipe {
            read_fd: unix_signal_wake_read_fd,
            drain: unix_signal_wake_drain,
        }
    }
    #[cfg(not(unix))]
    {
        // Explicitly absent. `w32xfns.c` `input_available` is a manual-reset
        // event, not a descriptor this poller owns.
        SignalWakeSource::Absent
    }
}

static CANONICAL_INPUT_HOOKS: InputHooks = InputHooks::new(
    interrupt_for_input_event_if_requested,
    os_signal_pending,
    take_termination_signal,
    drain_pending_os_signals,
    canonical_signal_wake(),
);

/// The one canonical input-wait descriptor (published only by the one core
/// boot entry, peer follow-up F2).
pub(crate) fn canonical_input_hooks() -> &'static InputHooks {
    &CANONICAL_INPUT_HOOKS
}
