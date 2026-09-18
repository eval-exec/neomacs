//! D-Bus transport matching GNU `src/dbusbind.c`.
//!
//! GNU compiles that file only under `#ifdef HAVE_DBUS`, set when
//! `configure.ac` finds `dbus-1 >= 1.0`. This module is the same switch:
//! `cfg(neomacs_have_dbus)` from `neovm-core/build.rs` (Unix). Without it the
//! subsystem still exists so startup can register nothing — GNU's
//! `--without-dbus` answers, which ledger 192 pinned.
//!
//! Lisp (`lisp/net/dbus.el`) stays GNU's. This crate owns the six primitives,
//! the nine `DEFVAR`s, `dbus-error`, `dbus-event`, and the wait-loop watches.

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;

mod fd;
mod subrs;

std::cfg_select! {
    neomacs_have_dbus => {
        mod connection;
        mod event;
        mod message;
        mod types;
        mod watch;
    }
    _ => {}
}

std::cfg_select! {
    all(test, neomacs_have_dbus) => {
        pub(crate) use subrs::SUBRS;
    }
    _ => {}
}

pub(crate) fn register_subrs(ctx: &mut Context) {
    subrs::register_subrs(ctx);
    std::cfg_select! {
        neomacs_have_dbus => subrs::install_lisp_state(ctx),
        _ => {}
    }
}

pub(crate) fn drain_events(ctx: &mut Context) -> Result<usize, Flow> {
    std::cfg_select! {
        neomacs_have_dbus => event::drain(ctx),
        _ => {
            let _ = ctx;
            Ok(0)
        }
    }
}

pub(crate) fn has_active_watches() -> bool {
    std::cfg_select! {
        neomacs_have_dbus => connection::has_buses(),
        _ => false,
    }
}

pub(crate) fn reset_thread_locals() {
    std::cfg_select! {
        neomacs_have_dbus => connection::reset(),
        _ => {}
    }
    fd::reset();
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
