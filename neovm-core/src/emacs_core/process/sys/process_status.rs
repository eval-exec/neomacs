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

#[cfg(windows)]
pub fn process_is_alive(pid: i64) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let Ok(pid) = u32::try_from(pid) else {
        return false;
    };
    if pid == 0 {
        return false;
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
    }
    unsafe {
        CloseHandle(process);
    }
    true
}

#[cfg(not(any(unix, windows)))]
pub fn process_is_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(process_is_alive(i64::from(std::process::id())));
    }
}
