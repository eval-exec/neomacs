//! Host process status probes (per-facility platform module).
//!
//! `process_is_alive` is the portable "does PID exist" check used by
//! `signal-process` with signal 0 and by `process-attributes`' existence gate.
//! GNU Emacs uses `kill (pid, 0)` for this on every POSIX platform (ESRCH means
//! gone; EPERM means alive but not ours). The old implementation probed
//! `/proc/PID`, which only exists on Linux -- so on macOS it always reported the
//! process as dead. Use `kill(pid, 0)` on Unix, matching GNU and fixing macOS.

/// True if a process with `pid` currently exists.
///
/// Non-positive pids are rejected (0 and negatives address process groups under
/// `kill`, not a single process).
#[cfg(unix)]
pub fn process_is_alive(pid: i64) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // `kill(pid, 0)` performs error checking without sending a signal:
    //   0            -> the process exists and we may signal it,
    //   -1 + EPERM   -> the process exists but is owned by someone else,
    //   -1 + ESRCH   -> no such process.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Non-Unix fallback.
///
/// Windows would probe this with `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,
/// ...)` (alive iff the handle opens, or the open fails with
/// `ERROR_ACCESS_DENIED`); that native probe is left for a Windows build. The
/// `/proc` heuristic is retained meanwhile (always false off Linux).
#[cfg(not(unix))]
pub fn process_is_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}
