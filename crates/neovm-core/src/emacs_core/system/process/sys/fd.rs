//! File-descriptor mode helpers (per-facility platform module, Unix only).
//!
//! Setting a descriptor non-blocking is `fcntl(F_GETFL)` + `fcntl(F_SETFL, ...
//! | O_NONBLOCK)` on Unix; Windows uses `ioctlsocket(FIONBIO)` on sockets and
//! has no `fcntl`, so this is a Unix-only facility (the call sites -- socket and
//! PTY setup -- are themselves Unix paths). Kept in the PAL so `process.rs`
//! never issues a raw `fcntl`.

use std::os::unix::io::RawFd;

/// Put `fd` into non-blocking mode.
///
/// Unlike the open-coded `F_GETFL`/`F_SETFL` pair this replaces, the read-back
/// is checked: a failed `F_GETFL` returns -1, and OR-ing `O_NONBLOCK` into that
/// would have written an all-ones flag word back. On any valid descriptor the
/// success path is identical to the previous code.
pub fn set_fd_nonblocking(fd: RawFd) -> std::io::Result<()> {
    // SAFETY: `F_GETFL`/`F_SETFL` take no pointer arguments; the caller owns a
    // valid `fd` for the duration of the call.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Duplicate `fd` (`dup(2)`), returning the new descriptor or `None` on failure.
pub fn dup_fd(fd: RawFd) -> Option<RawFd> {
    // SAFETY: `dup` takes no pointer arguments.
    let new_fd = unsafe { libc::dup(fd) };
    (new_fd != -1).then_some(new_fd)
}
