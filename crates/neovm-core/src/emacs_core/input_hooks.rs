//! Canonical input-wait/OS-signal hook descriptor (stage-4 W4/E10 batch 3).
//!
//! The interrupt policy stays in `commands/keyboard` (`CommandLoopHost`) and
//! the signal latch/drain in `system::os_signal`; this module only assembles
//! the one descriptor eval publishes at `Context::new_inner` /
//! `Context::from_dump`.

use crate::emacs_core::eval::input_hooks::InputHooks;
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

fn signal_wake_pipe_read_fd() -> Option<i32> {
    crate::emacs_core::os_signal::install().self_pipe_read_fd()
}

fn drain_signal_wake_pipe(fd: i32) {
    crate::emacs_core::os_signal::drain_wake_pipe(fd)
}

static CANONICAL_INPUT_HOOKS: InputHooks = InputHooks::new(
    interrupt_for_input_event_if_requested,
    os_signal_pending,
    take_termination_signal,
    drain_pending_os_signals,
    signal_wake_pipe_read_fd,
    drain_signal_wake_pipe,
);

/// Publish the input-wait table at the one-time construction seams.
/// The one canonical input-wait descriptor.
pub(crate) fn canonical_input_hooks() -> &'static InputHooks {
    &CANONICAL_INPUT_HOOKS
}

pub(crate) fn install_input_hooks() {
    let installed =
        crate::emacs_core::eval::input_hooks::install_input_hooks(&CANONICAL_INPUT_HOOKS);
    debug_assert!(
        installed.is_ok(),
        "a different input-wait hook table is already installed"
    );
}
