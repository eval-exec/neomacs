//! Platform abstraction layer (PAL) for OS-specific subprocess facilities.
//!
//! This is the neomacs analogue of GNU Emacs's `sysdep.c` / `w32proc.c` split
//! and of Rust std's `sys::pal`: the portable process logic in the parent
//! `process` module calls this small, stable interface, and exactly one backend
//! module implements it for the target platform. The platform choice is made
//! ONCE here, at the module boundary -- no `cfg` leaks into the caller.
//!
//! Extension points: add a `macos` backend (kqueue `EVFILT_PROC`) or a
//! `windows` backend (job-object / process-handle wait) and route to it from
//! the `cfg_select!` below, exactly as GNU adds an `#elif`/`w32proc.c` path.
//! The `fallback` backend is the portable poll-only path used until then.
//!
//! Each per-facility module below is here because GNU splits the SAME facility
//! the same way. `serial` is the clearest case: `serial_open` and
//! `serial_configure` are declared once in GNU's `src/systty.h:90-91` and
//! implemented twice, in `src/sysdep.c` and `src/w32.c`.

use crate::emacs_core::process::{HostInterfaceEntry, ProcessId};

mod signals;
#[cfg(unix)]
pub use signals::signal_number_from_description;
pub use signals::{
    errno_description, send_signal, send_signal_to_group, signal_description, signal_name_number,
};

mod process_status;
pub use process_status::process_is_alive;

mod process_attributes;
pub use process_attributes::{
    ProcStatSnapshot, boot_time_secs, clock_ticks_per_second, group_name, list_process_ids,
    process_cmdline, process_effective_ids, process_stat, total_memory_kb, user_name,
};

#[cfg(unix)]
mod tty;
#[cfg(unix)]
pub use tty::{
    configure_child_pty_tty, establish_pty_controlling_terminal, fd_foreground_pgrp,
    tty_path_foreground_pgrp,
};

#[cfg(unix)]
mod fd;
#[cfg(unix)]
pub use fd::{dup_fd, set_fd_nonblocking};

mod fs;
pub use fs::executable_path_access;

mod serial;
pub use serial::{
    SerialAttributes, SerialByteSize, SerialConfigureFailure, SerialConfigureStep,
    SerialFlowControl, SerialParity, SerialPort, SerialStopBits,
};

pub mod net;

#[cfg(unix)]
mod child_wait;
#[cfg(unix)]
pub use child_wait::{ChildWait, poll_child_status};
// The raw-status decode entry point is currently only exercised by status-decode
// tests; production reaps through `poll_child_status`.
#[cfg(all(unix, test))]
pub use child_wait::decode_wait_status;

#[cfg(unix)]
mod socket_options;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use socket_options::set_socket_priority;
#[cfg(unix)]
pub use socket_options::{set_socket_dontroute, set_socket_linger};

cfg_select! {
    target_os = "linux" => {
        mod linux;
        use self::linux as backend;
    }
    _ => {
        mod fallback;
        use self::fallback as backend;
    }
}

/// A pollable terminal-status edge for one subprocess.
///
/// GNU Emacs waits for subprocess I/O and child status in a single primitive:
/// Unix multiplexes a SIGCHLD self-pipe into `wait_reading_process_output`,
/// while w32 waits on subprocess handles alongside its pipe-reader events.
/// Linux `pidfd`s wake the poller on child termination. Stop/continue changes
/// are harvested by the explicit `waitpid` scan because pidfds do not report
/// those transitions. Platforms without a native terminal source use that
/// scan for every status transition (see the `fallback` backend).
pub struct ChildStatusSource(backend::Source);

impl ChildStatusSource {
    /// Open a status source for `pid`, or `None` when this platform has no
    /// native pollable source (the caller then keeps using the periodic poll).
    pub fn open(pid: u32) -> Option<Self> {
        backend::open(pid).map(Self)
    }

    /// Register this source's readable edge with the wait `poller`. A no-op
    /// when there is no poller or no native source.
    pub fn register_with_poller(&self, poller: Option<&polling::Poller>, id: ProcessId) {
        if let Some(poller) = poller {
            self.0.register(poller, id);
        }
    }

    /// Remove this source from the wait `poller`.
    pub fn unregister_from_poller(&self, poller: &polling::Poller) {
        self.0.unregister(poller);
    }
}

/// Snapshot of the host's network interfaces for `network-interface-list` /
/// `network-interface-info`. Linux uses native `getifaddrs`+ioctls; other
/// platforms use the portable `network_interface` crate (see the backends).
pub fn interface_snapshot() -> Option<Vec<HostInterfaceEntry>> {
    backend::interface_snapshot()
}
