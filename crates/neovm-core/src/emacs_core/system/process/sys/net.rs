//! Network address-family, service, and connect-state platform facility.
//!
//! `make-network-process` runs on socket2 / std / the `dns_lookup` crate for the
//! actual socket and resolver syscalls; this module owns the remaining raw
//! platform values those crates don't surface: the address-family constants
//! (`AF_*`), the `getaddrinfo` numeric-host flag, `SOCK_SEQPACKET`, the
//! `sockaddr_*` payload sizes for Emacs's address-vector encoding, the
//! `getservbyname` service lookup, and the errno classification that decides
//! whether a non-blocking `connect` is still pending.
//!
//! Each carries the platform split its API actually has: the constants are
//! Unix-`libc` vs Windows-`windows_sys` (GNU's `AF_*`/`AI_*` macros); the
//! `getservbyname` lookup and `SOCK_SEQPACKET` are Unix-only (socket2/std cover
//! the rest on Windows). Keeping them here means `process.rs` never names a
//! `libc`/`windows_sys` networking constant.

/// A resolved address family, platform-independent. [`classify_family`] maps a
/// raw `AF_*` value (which differs by OS) into this so the parent never matches
/// on `libc`/`windows_sys` constants.
pub enum NetFamily {
    Unspecified,
    Ipv4,
    Ipv6,
    Local,
    Other(i32),
}

// --- forward: family -> raw AF_* (for addrinfo hints / sockaddr construction) ---

/// `AF_UNIX` (local domain), or 0 where unavailable.
#[cfg(unix)]
pub fn af_local() -> i32 {
    libc::AF_UNIX
}
#[cfg(not(unix))]
pub fn af_local() -> i32 {
    0
}

/// `AF_INET` (IPv4).
#[cfg(unix)]
pub fn af_inet() -> i32 {
    libc::AF_INET
}
#[cfg(windows)]
pub fn af_inet() -> i32 {
    windows_sys::Win32::Networking::WinSock::AF_INET as i32
}
#[cfg(all(not(unix), not(windows)))]
pub fn af_inet() -> i32 {
    0
}

/// `AF_INET6` (IPv6).
#[cfg(unix)]
pub fn af_inet6() -> i32 {
    libc::AF_INET6
}
#[cfg(windows)]
pub fn af_inet6() -> i32 {
    windows_sys::Win32::Networking::WinSock::AF_INET6 as i32
}
#[cfg(all(not(unix), not(windows)))]
pub fn af_inet6() -> i32 {
    0
}

// --- reverse: raw AF_* -> NetFamily (per-platform to avoid 0-value collisions) ---

/// Classify a raw address-family value. Done per-platform rather than by
/// comparing against the forward accessors, because several families collapse to
/// 0 on platforms that lack them and would otherwise alias.
#[cfg(unix)]
pub fn classify_family(raw: i32) -> NetFamily {
    match raw {
        r if r == libc::AF_UNSPEC => NetFamily::Unspecified,
        r if r == libc::AF_INET => NetFamily::Ipv4,
        r if r == libc::AF_INET6 => NetFamily::Ipv6,
        r if r == libc::AF_UNIX => NetFamily::Local,
        r => NetFamily::Other(r),
    }
}

#[cfg(windows)]
pub fn classify_family(raw: i32) -> NetFamily {
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC};
    match raw {
        r if r == AF_UNSPEC as i32 => NetFamily::Unspecified,
        r if r == AF_INET as i32 => NetFamily::Ipv4,
        r if r == AF_INET6 as i32 => NetFamily::Ipv6,
        r => NetFamily::Other(r),
    }
}

#[cfg(all(not(unix), not(windows)))]
pub fn classify_family(raw: i32) -> NetFamily {
    NetFamily::Other(raw)
}

/// The `getaddrinfo` `AI_NUMERICHOST` hint flag.
#[cfg(unix)]
pub fn ai_numerichost() -> i32 {
    libc::AI_NUMERICHOST
}
#[cfg(windows)]
pub fn ai_numerichost() -> i32 {
    windows_sys::Win32::Networking::WinSock::AI_NUMERICHOST as i32
}
#[cfg(all(not(unix), not(windows)))]
pub fn ai_numerichost() -> i32 {
    0
}

/// The `getaddrinfo` socktype value for a SOCK_SEQPACKET lookup (Unix only;
/// `dns_lookup`'s `SockType` has no seqpacket variant).
#[cfg(unix)]
pub fn sock_seqpacket() -> i32 {
    libc::SOCK_SEQPACKET
}

/// Length of an IPv4 `sockaddr_in` past its family field, i.e. the size of the
/// raw address payload Emacs stores in a network-address vector.
pub fn sockaddr_in_payload_len() -> usize {
    cfg_select! {
        unix => {
            std::mem::size_of::<libc::sockaddr_in>() - std::mem::size_of::<libc::sa_family_t>()
        }
        _ => { 14 }
    }
}

/// Length of an IPv6 `sockaddr_in6` past its family field.
pub fn sockaddr_in6_payload_len() -> usize {
    cfg_select! {
        unix => {
            std::mem::size_of::<libc::sockaddr_in6>() - std::mem::size_of::<libc::sa_family_t>()
        }
        _ => { 26 }
    }
}

/// Length of a `sockaddr_un` past its family field (Unix local sockets).
#[cfg(unix)]
pub fn sockaddr_un_payload_len() -> usize {
    std::mem::size_of::<libc::sockaddr_un>() - std::mem::size_of::<libc::sa_family_t>()
}

/// Resolve a service name (e.g. "http") for a protocol ("tcp"/"udp") to its
/// port via `getservbyname`. `None` off Unix (std/socket2 have no equivalent).
#[cfg(unix)]
pub fn service_port(service: &str, protocol: &str) -> Option<u16> {
    let service = std::ffi::CString::new(service).ok()?;
    let protocol = std::ffi::CString::new(protocol).ok()?;
    // SAFETY: both pointers are valid C strings; getservbyname returns NULL or a
    // pointer to a static (per-process) servent whose s_port we copy immediately.
    let entry = unsafe { libc::getservbyname(service.as_ptr(), protocol.as_ptr()) };
    if entry.is_null() {
        None
    } else {
        Some(u16::from_be(unsafe { (*entry).s_port as u16 }))
    }
}

#[cfg(not(unix))]
pub fn service_port(_service: &str, _protocol: &str) -> Option<u16> {
    None
}

/// Whether a failed non-blocking `connect` is merely still in progress (so the
/// caller should wait for writability) rather than a hard failure. Matches GNU's
/// `is_non_blocking_client && errno == EINPROGRESS` handling.
pub fn connect_is_pending(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            err.raw_os_error(),
            Some(code)
                if code == libc::EINPROGRESS
                    || code == libc::EWOULDBLOCK
                    || code == libc::EALREADY
        )
    }
    #[cfg(not(unix))]
    {
        false
    }
}
