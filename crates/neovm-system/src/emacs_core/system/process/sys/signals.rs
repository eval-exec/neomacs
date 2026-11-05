//! Signal-name → signal-number mapping for `signal-process` / `kill`.
//!
//! A per-facility platform module. The POSIX table is shared by every Unix
//! target (Linux and macOS alike), with a few Linux/Android-only signals
//! (`SIGPOLL`, `SIGPWR`, and the `SIGRTMIN..SIGRTMAX` realtime range) gated
//! inside; non-Unix (Windows) has only the synthetic `"EXIT"` (0). This mirrors
//! GNU Emacs's per-symbol `#ifdef` in `process.c`'s signal handling.
//!
//! Note the split here is Unix-vs-Windows, NOT linux-vs-rest like the
//! `child_status`/`interface` backends -- so this facility lives in its own
//! module with internal cfg, the way `std::sys` organizes per facility.

#[cfg(unix)]
pub fn signal_name_number(name: &str) -> Option<i32> {
    let name = name
        .strip_prefix("SIG")
        .or_else(|| name.strip_prefix("sig"))
        .unwrap_or(name);
    let name = name.to_ascii_uppercase();
    match name.as_str() {
        "EXIT" => Some(0),
        "HUP" => Some(libc::SIGHUP),
        "INT" => Some(libc::SIGINT),
        "QUIT" => Some(libc::SIGQUIT),
        "ILL" => Some(libc::SIGILL),
        "TRAP" => Some(libc::SIGTRAP),
        "ABRT" | "IOT" => Some(libc::SIGABRT),
        "BUS" => Some(libc::SIGBUS),
        "FPE" => Some(libc::SIGFPE),
        "KILL" => Some(libc::SIGKILL),
        "USR1" => Some(libc::SIGUSR1),
        "SEGV" => Some(libc::SIGSEGV),
        "USR2" => Some(libc::SIGUSR2),
        "PIPE" => Some(libc::SIGPIPE),
        "ALRM" => Some(libc::SIGALRM),
        "TERM" => Some(libc::SIGTERM),
        "CHLD" | "CLD" => Some(libc::SIGCHLD),
        "CONT" => Some(libc::SIGCONT),
        "STOP" => Some(libc::SIGSTOP),
        "TSTP" => Some(libc::SIGTSTP),
        "TTIN" => Some(libc::SIGTTIN),
        "TTOU" => Some(libc::SIGTTOU),
        "URG" => Some(libc::SIGURG),
        "XCPU" => Some(libc::SIGXCPU),
        "XFSZ" => Some(libc::SIGXFSZ),
        "VTALRM" => Some(libc::SIGVTALRM),
        "PROF" => Some(libc::SIGPROF),
        "WINCH" => Some(libc::SIGWINCH),
        #[cfg(any(target_os = "linux", target_os = "android"))]
        "POLL" | "IO" => Some(libc::SIGPOLL),
        #[cfg(any(target_os = "linux", target_os = "android"))]
        "PWR" => Some(libc::SIGPWR),
        "SYS" => Some(libc::SIGSYS),
        _ => realtime_signal_name_number(&name),
    }
}

#[cfg(unix)]
fn realtime_signal_name_number(name: &str) -> Option<i32> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let min = libc::SIGRTMIN();
        let max = libc::SIGRTMAX();
        if name == "RTMIN" {
            return Some(min);
        }
        if name == "RTMAX" {
            return Some(max);
        }
        if let Some(offset) = name
            .strip_prefix("RTMIN+")
            .and_then(|value| value.parse::<i32>().ok())
        {
            let signal = min + offset;
            return (signal <= max).then_some(signal);
        }
        if let Some(offset) = name
            .strip_prefix("RTMAX-")
            .and_then(|value| value.parse::<i32>().ok())
        {
            let signal = max - offset;
            return (signal >= min).then_some(signal);
        }
    }
    None
}

#[cfg(not(unix))]
pub fn signal_name_number(name: &str) -> Option<i32> {
    match name
        .strip_prefix("SIG")
        .or_else(|| name.strip_prefix("sig"))
        .unwrap_or(name)
        .to_ascii_uppercase()
        .as_str()
    {
        "EXIT" => Some(0),
        _ => None,
    }
}

/// A signal number's `strsignal` description with the first character
/// down-cased, matching GNU's `status_message` ("Terminated" -> "terminated").
/// Falls back to "unknown" when `strsignal` yields NULL or off Unix.
pub fn signal_description(signum: i32) -> String {
    #[cfg(unix)]
    {
        // SAFETY: `strsignal` returns NULL or a valid static C string.
        let raw = unsafe {
            let p = libc::strsignal(signum);
            if p.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        };
        if let Some(s) = raw {
            let mut chars = s.chars();
            return match chars.next() {
                Some(first) => {
                    let lowered: String = first.to_lowercase().collect();
                    format!("{lowered}{}", chars.as_str())
                }
                None => s,
            };
        }
    }
    let _ = signum;
    "unknown".to_string()
}

/// An errno's `strerror` text, exactly as GNU's `emacs_strerror`
/// (src/sysdep.c) hands it to `emacs_perror`: `strerror` verbatim, with a
/// fixed fallback when the number has no description.
pub fn errno_description(errno: i32) -> String {
    #[cfg(unix)]
    {
        // SAFETY: `strerror` returns NULL or a valid C string; the same
        // contract `signal_description` relies on for `strsignal`.
        let raw = unsafe {
            let p = libc::strerror(errno);
            if p.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        };
        if let Some(text) = raw {
            return text;
        }
    }
    let _ = errno;
    "Invalid error number".to_string()
}

/// Map a `strsignal`-style description (as produced by `portable_pty`'s
/// `ExitStatus`) back to a signal number by scanning the platform's signal
/// table. Both the PTY layer and this lookup call `strsignal`, so the
/// descriptions match exactly.
#[cfg(unix)]
pub fn signal_number_from_description(name: &str) -> Option<i32> {
    // portable_pty falls back to "Signal N" when strsignal yields NULL.
    if let Some(rest) = name.strip_prefix("Signal ")
        && let Ok(n) = rest.trim().parse::<i32>()
    {
        return Some(n);
    }
    for signum in 1..=64i32 {
        // SAFETY: `strsignal` returns NULL or a valid static C string.
        let desc = unsafe {
            let p = libc::strsignal(signum);
            if p.is_null() {
                continue;
            }
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        };
        if desc == name {
            return Some(signum);
        }
    }
    None
}

/// Deliver `signal_num` to a single process. On Unix this is
/// `kill(pid, signal_num)`; where `kill` is unavailable it degrades to a
/// liveness probe (0 if the process exists, -1 otherwise).
#[cfg(unix)]
pub fn send_signal(pid: i64, signal_num: i32) -> i32 {
    // SAFETY: `kill` takes no pointer arguments.
    unsafe { libc::kill(pid as libc::pid_t, signal_num) }
}

#[cfg(not(unix))]
pub fn send_signal(pid: i64, signal_num: i32) -> i32 {
    let _ = signal_num;
    if super::process_is_alive(pid) { 0 } else { -1 }
}

/// Deliver `signal_num` to the process group led by `pid` -- GNU signals the
/// whole group for interrupt/quit/stop when CURRENT-GROUP is set. On Unix this
/// is `kill(-pid, signal_num)`; elsewhere it falls back to `send_signal`.
#[cfg(unix)]
pub fn send_signal_to_group(pid: i64, signal_num: i32) -> i32 {
    // SAFETY: `kill` takes no pointer arguments.
    unsafe { libc::kill(-(pid as libc::pid_t), signal_num) }
}

#[cfg(not(unix))]
pub fn send_signal_to_group(pid: i64, signal_num: i32) -> i32 {
    send_signal(pid, signal_num)
}
