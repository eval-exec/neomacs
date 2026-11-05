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

use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::Flow;

mod fd;
pub mod subrs;

std::cfg_select! {
    neomacs_have_dbus => {
        mod connection;
        mod event;
        mod message;
        pub mod types;
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

/// D-Bus's carried World state (connections and registered fds; see
/// `connection::swap_world_state`).
#[cold]
pub fn world_state_placeholder() -> Box<dyn std::any::Any> {
    std::cfg_select! {
        neomacs_have_dbus => {
            Box::new((connection::world_state_placeholder(), fd::world_state_placeholder()))
        }
        _ => {
            Box::new(((), fd::world_state_placeholder()))
        }
    }
}

#[cold]
pub fn swap_world_state(slot: &mut dyn std::any::Any) {
    std::cfg_select! {
        neomacs_have_dbus => {
            let (buses, fds) = slot
                .downcast_mut::<(Box<dyn std::any::Any>, Box<dyn std::any::Any>)>()
                .expect("D-Bus's carried state");
            connection::swap_world_state(&mut **buses);
            fd::swap_world_state(&mut **fds);
        }
        _ => {
            let ((), fds) = slot
                .downcast_mut::<((), Box<dyn std::any::Any>)>()
                .expect("D-Bus's carried state");
            fd::swap_world_state(&mut **fds);
        }
    }
}

pub fn register_subrs(ctx: &mut Context) {
    subrs::register_subrs(ctx);
}

/// GNU `syms_of_dbusbind`'s variables (see `subrs::register_bootstrap_vars`);
/// nothing in a build without libdbus.
pub fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    std::cfg_select! {
        neomacs_have_dbus => subrs::register_bootstrap_vars(obarray),
        _ => {
            let _ = obarray;
        }
    }
}

pub fn drain_events(ctx: &mut Context) -> Result<usize, Flow> {
    std::cfg_select! {
        neomacs_have_dbus => event::drain(ctx),
        _ => {
            let _ = ctx;
            Ok(0)
        }
    }
}

pub fn has_active_watches() -> bool {
    std::cfg_select! {
        neomacs_have_dbus => connection::has_buses(),
        _ => false,
    }
}

pub fn reset_thread_locals() {
    std::cfg_select! {
        neomacs_have_dbus => connection::reset(),
        _ => {}
    }
    fd::reset();
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
