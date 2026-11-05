//! Non-blocking child-status query for the process reaper (Unix only).
//!
//! `poll_child_status` is the `waitpid(WNOHANG | WUNTRACED | WCONTINUED)` probe
//! neomacs uses to notice a subprocess that exited, was signalled, stopped, or
//! continued -- the same status decode GNU Emacs does in `record_child_status`
//! (`WIFEXITED`/`WIFSIGNALED`/`WIFSTOPPED`). The `waitpid` call and every
//! `W*` status macro live here; the parent maps the returned `ChildWait` to the
//! Emacs process-status Lisp value. Unix-only (Windows waits on process handles
//! via its own backend), matching the `#[cfg(unix)]` reaper path in the caller.

/// The outcome of one non-blocking `waitpid` probe.
pub enum ChildWait {
    /// No state change yet (`waitpid` returned 0).
    Running,
    /// Nothing to wait for -- already reaped, or `ECHILD`.
    NoChild,
    /// Exited normally with this code (`WIFEXITED` -> `WEXITSTATUS`).
    Exited(i32),
    /// Killed by `sig`; `core` is whether a core was dumped (`WIFSIGNALED`).
    Signaled { sig: i32, core: bool },
    /// Stopped by `sig` (`WIFSTOPPED` -> `WSTOPSIG`).
    Stopped(i32),
    /// Continued (`WIFCONTINUED`).
    Continued,
    /// `waitpid` reported the pid but the status matched no `WIF*` class
    /// (not expected in practice; treated as "no change" by the caller).
    Undecoded,
    /// `waitpid` failed for a reason other than `ECHILD`.
    Error,
}

/// Probe `pid`'s status without blocking, reaping it if it changed state.
/// `pid` is a live child pid (always positive), so it is taken as `u32`.
pub fn poll_child_status(pid: u32) -> ChildWait {
    let mut raw_status: libc::c_int = 0;
    // SAFETY: `waitpid` writes only through the provided `&mut raw_status`.
    let result = unsafe {
        libc::waitpid(
            pid as libc::pid_t,
            &mut raw_status,
            libc::WNOHANG | libc::WUNTRACED | libc::WCONTINUED,
        )
    };
    if result == pid as libc::pid_t {
        return decode_wait_status(raw_status);
    }
    if result == 0 {
        return ChildWait::Running;
    }
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
        return ChildWait::NoChild;
    }
    ChildWait::Error
}

/// Classify a raw `waitpid`/`wait` status word (exposed for status-decode
/// tests; `poll_child_status` is the normal entry point).
pub fn decode_wait_status(status: libc::c_int) -> ChildWait {
    if libc::WIFEXITED(status) {
        return ChildWait::Exited(libc::WEXITSTATUS(status));
    }
    if libc::WIFSIGNALED(status) {
        use std::os::unix::process::ExitStatusExt;
        // GNU records the core-dump bit alongside the terminating signal.
        let core = std::process::ExitStatus::from_raw(status).core_dumped();
        return ChildWait::Signaled {
            sig: libc::WTERMSIG(status),
            core,
        };
    }
    if libc::WIFSTOPPED(status) {
        return ChildWait::Stopped(libc::WSTOPSIG(status));
    }
    if libc::WIFCONTINUED(status) {
        return ChildWait::Continued;
    }
    ChildWait::Undecoded
}
