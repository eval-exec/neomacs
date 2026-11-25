//! Main-owner text-terminal device effects.
//!
//! The service half of `suspend-tty`, `resume-tty`, `send-string-to-terminal`,
//! `delete-terminal` and the winsize query when a Green worker calls them.
//! Main's display registry holds every text terminal's native device
//! (`docs/design/host-effect-projections.md`: native backing stays on main),
//! in every session, hosted or not, so this family is served whatever display
//! adapter the pump has. It receives only the typed request and main's
//! `DisplayTerminals`: no `Context`, so it cannot run Lisp or touch
//! owner-carried state while main is parked. The device calls are the ones
//! the serial path makes (`DisplayTerminals::perform_tty_device`), and a
//! device error becomes the serial path's `(error MESSAGE)` on the requester.

use crate::emacs_core::eval::{DisplayTerminals, TerminalNumber};
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_outcome::{NativeOutcome, NativeReply};
use crate::host_effect::terminal_device::{TerminalDeviceReply, TerminalDeviceRequest};

/// Serve REQUEST on TERMINAL's device in DISPLAYS.
pub(super) fn perform_terminal_device(
    displays: &mut DisplayTerminals,
    terminal: TerminalNumber,
    request: TerminalDeviceRequest,
) -> NativeOutcome {
    NativeOutcome::Reply(match displays.perform_tty_device(terminal, request) {
        TerminalDeviceReply::HostError(message) => NativeReply::LispError(
            OwnedNativeData::TerminalDeviceReply(TerminalDeviceReply::HostError(message)),
        ),
        reply => NativeReply::Returned(OwnedNativeData::TerminalDeviceReply(reply)),
    })
}
