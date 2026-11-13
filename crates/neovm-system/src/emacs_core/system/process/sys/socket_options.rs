//! Raw `setsockopt` socket options (per-facility platform module, Unix only).
//!
//! neomacs applies most `make-network-process` `:options` through socket2's
//! typed, portable setters (`set_broadcast`, `set_keepalive`,
//! `set_reuse_address`, `set_tcp_nodelay`, `set_out_of_band_inline`,
//! `bind_device`) directly from the dispatcher in `process.rs`. This module
//! holds only the few options socket2 does NOT wrap, which must go through raw
//! `setsockopt`:
//!   - `SO_DONTROUTE` and `SO_LINGER`: POSIX, present on every Unix
//!     (`#[cfg(unix)]`);
//!   - `SO_PRIORITY`: Linux/Android only (`#[cfg(any(linux, android))]`),
//!     matching GNU's `#ifdef SO_PRIORITY` row in process.c's `socket_options[]`.
//!
//! Design note: GNU Emacs drives ALL options from one data table
//! (`socket_options[]`) of raw `setsockopt` calls dispatched by an `opttype` tag,
//! because C has no typed socket API. neomacs deliberately does NOT mirror that
//! table -- socket2's typed setters are more portable and absorb per-OS quirks
//! (e.g. `SO_REUSEADDR`/`SO_REUSEPORT` handling) that a hand-rolled `setsockopt`
//! table would lose, so a literal port would be a regression. Only the options
//! socket2 leaves unwrapped live here as raw syscalls; the parent keeps the
//! keyword->option mapping and the typed dispatch.

use std::os::unix::io::RawFd;

fn setsockopt_raw<T>(
    fd: RawFd,
    level: libc::c_int,
    optname: libc::c_int,
    value: &T,
) -> std::io::Result<()> {
    let rc = unsafe {
        libc::setsockopt(
            fd,
            level,
            optname,
            (value as *const T).cast(),
            std::mem::size_of::<T>() as libc::socklen_t,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// `SO_DONTROUTE`: bypass the routing table (send only to directly reachable
/// hosts). socket2 does not wrap this option.
pub fn set_socket_dontroute(fd: RawFd, enabled: bool) -> std::io::Result<()> {
    let raw: libc::c_int = if enabled { 1 } else { 0 };
    setsockopt_raw(fd, libc::SOL_SOCKET, libc::SO_DONTROUTE, &raw)
}

/// `SO_LINGER`: when `onoff` is set, block `close` until queued data is flushed
/// or `linger` seconds elapse; when clear, `close` returns immediately and
/// `linger` is ignored.
pub fn set_socket_linger(fd: RawFd, onoff: bool, linger: i32) -> std::io::Result<()> {
    let raw = libc::linger {
        l_onoff: if onoff { 1 } else { 0 },
        l_linger: linger as libc::c_int,
    };
    setsockopt_raw(fd, libc::SOL_SOCKET, libc::SO_LINGER, &raw)
}

/// `SO_PRIORITY`: set the protocol-defined queueing priority of packets sent on
/// the socket. Linux/Android only, mirroring GNU's `#ifdef SO_PRIORITY`.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn set_socket_priority(fd: RawFd, priority: i32) -> std::io::Result<()> {
    let raw = priority as libc::c_int;
    setsockopt_raw(fd, libc::SOL_SOCKET, libc::SO_PRIORITY, &raw)
}
