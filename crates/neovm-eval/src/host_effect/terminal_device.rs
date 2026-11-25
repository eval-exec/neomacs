//! Typed native payloads for text-terminal device effects.
//!
//! `suspend-tty`, `resume-tty`, `send-string-to-terminal`, `delete-terminal`
//! and the winsize query are evaluated by their calling Lisp thread. Only
//! the device effect crosses to the owner of the display registry, which
//! holds every terminal's native device; no `Value`, root or evaluator
//! borrow crosses.
//!
//! The types are defined in [`super::contracts`] so the path-included
//! host-effect compile fixtures can name them; this module is the stable
//! production import path.

pub use super::contracts::{TerminalDeviceEffect, TerminalDeviceReply, TerminalDeviceRequest};
