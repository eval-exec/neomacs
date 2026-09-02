//! Platform capabilities used by the OS-signal facade.
//!
//! `cfg(unix)` is an ABI family, not a signal capability.  In particular,
//! Android reserves SIGUSR1/SIGUSR2 for `android_select`, Apple has no
//! `pipe2`, and errno has several target-specific symbol names.  Keep those
//! decisions here so the signal state machine consumes one projection instead
//! of rediscovering platform predicates at every call site.

use super::HandledSignal;
#[cfg(not(unix))]
use std::ffi::c_int;

#[cfg(all(unix, not(target_os = "android")))]
pub(super) const SUPPORTED_SIGNALS: &[HandledSignal] = &HandledSignal::ALL;

// GNU src/sysdep.c:init_signals deliberately leaves SIGUSR1/SIGUSR2 to
// android_select, and this port installs no other disposition since ledger 208
// took SIGCHLD out (see `HandledSignal`), so Android owns nothing here.
#[cfg(target_os = "android")]
pub(super) const SUPPORTED_SIGNALS: &[HandledSignal] = &[];

#[cfg(not(unix))]
pub(super) const SUPPORTED_SIGNALS: &[HandledSignal] = &[];

#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};

/// The owned, process-lifetime wake pipe used by the signal handler.
///
/// Ownership lives in the install report held by `OnceLock`; the handler sees
/// only the write end's raw descriptor through an atomic.  Failed setup drops
/// both `OwnedFd`s, so a partial `fcntl` sequence cannot leak descriptors.
#[cfg(unix)]
#[derive(Debug)]
pub(super) struct WakePipe {
    read: rustix::fd::OwnedFd,
    write: rustix::fd::OwnedFd,
}

#[cfg(not(unix))]
#[derive(Debug)]
pub(super) struct WakePipe;

#[cfg(unix)]
impl WakePipe {
    pub(super) fn read_fd(&self) -> Option<RawFd> {
        Some(self.read.as_raw_fd())
    }

    pub(super) fn write_fd(&self) -> RawFd {
        self.write.as_raw_fd()
    }
}

#[cfg(not(unix))]
impl WakePipe {
    pub(super) fn read_fd(&self) -> Option<c_int> {
        None
    }
}

/// GNU `child_signal_init`'s nonblocking, close-on-exec pipe, expressed with
/// portable POSIX `pipe` + `fcntl` operations.  Rustix owns the Apple/BSD/Linux
/// ABI differences and closes both ends if any step fails.
#[cfg(unix)]
pub(super) fn create_wake_pipe() -> Option<WakePipe> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use rustix::io::{FdFlags, fcntl_getfd, fcntl_setfd};

    let (read, write) = rustix::pipe::pipe().ok()?;
    for fd in [&read, &write] {
        let descriptor_flags = fcntl_getfd(fd).ok()?;
        fcntl_setfd(fd, descriptor_flags | FdFlags::CLOEXEC).ok()?;

        let status_flags = fcntl_getfl(fd).ok()?;
        fcntl_setfl(fd, status_flags | OFlags::NONBLOCK).ok()?;
    }
    Some(WakePipe { read, write })
}

#[cfg(not(unix))]
pub(super) fn create_wake_pipe() -> Option<WakePipe> {
    None
}

/// Capture and restore errno without naming a target's TLS accessor.  The
/// `errno` crate maps Apple/FreeBSD to `__error`, Android/NetBSD/OpenBSD to
/// `__errno`, Solaris/illumos to `___errno`, and Linux-like targets to
/// `__errno_location`.
#[cfg(unix)]
pub(super) type SavedErrno = errno::Errno;

#[cfg(unix)]
pub(super) fn save_errno() -> SavedErrno {
    errno::errno()
}

#[cfg(unix)]
pub(super) fn restore_errno(saved: SavedErrno) {
    errno::set_errno(saved);
}
