//! Linux `ChildStatusSource` backend.
//!
//! `pidfd_open(2)` (Linux 5.3+) returns a descriptor that becomes readable when
//! the target process terminates, so the wait poller can wake on child exit
//! with a plain readable registration. Stop/continue transitions still come
//! from the periodic `waitpid(WUNTRACED | WCONTINUED)` scan. `open` returns
//! `None` on older kernels, where every transition uses that periodic scan.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};

use crate::emacs_core::process::{ProcessId, ProcessManager};
use std::ffi::CStr;

use crate::emacs_core::process::{
    HostInterfaceEntry, NetworkAddressFamily, derive_network_interface_info_broadcast,
    derive_network_interface_list_broadcast, int_vector, zero_network_address,
};
use crate::emacs_core::value::Value;

pub struct Source {
    pidfd: OwnedFd,
}

pub fn open(pid: u32) -> Option<Source> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        return None;
    }
    // SAFETY: `pidfd_open` returned a fresh owned descriptor.
    Some(Source {
        pidfd: unsafe { OwnedFd::from_raw_fd(fd as RawFd) },
    })
}

impl Source {
    pub fn register(&self, poller: &polling::Poller, id: ProcessId) {
        // Reuse the ProcessManager registration policy (level-triggered
        // readable, keyed by process id) so pidfd sources and process-output
        // sources are registered identically.
        let _ = ProcessManager::register_readable_source(poller, &self.pidfd, id);
    }

    pub fn unregister(&self, poller: &polling::Poller) {
        let _ = poller.delete(&self.pidfd);
    }
}

// ---------------------------------------------------------------------------
// network-interface-info backend (getifaddrs + SIOCGIFFLAGS/SIOCGIFHWADDR).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct LinuxInterfaceMetadata {
    hwaddr: Option<Value>,
    flags: Option<Value>,
}

fn linux_ifreq(ifname: &str) -> Option<libc::ifreq> {
    let bytes = ifname.as_bytes();
    if bytes.len() >= libc::IFNAMSIZ {
        return None;
    }

    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    for (idx, byte) in bytes.iter().enumerate() {
        request.ifr_name[idx] = *byte as libc::c_char;
    }
    Some(request)
}

fn linux_ifflags_to_value(raw_flags: libc::c_short) -> Value {
    let mut flags = raw_flags as i32;
    if flags < 0 {
        flags = u16::from_ne_bytes(raw_flags.to_ne_bytes()) as i32;
    }
    linux_ifflags_bits_to_value(flags)
}

fn linux_ifflags_bits_to_value(mut flags: i32) -> Value {
    let table = [
        (libc::IFF_UP, "up"),
        (libc::IFF_BROADCAST, "broadcast"),
        (libc::IFF_DEBUG, "debug"),
        (libc::IFF_LOOPBACK, "loopback"),
        (libc::IFF_POINTOPOINT, "pointopoint"),
        (libc::IFF_RUNNING, "running"),
        (libc::IFF_NOARP, "noarp"),
        (libc::IFF_PROMISC, "promisc"),
        (libc::IFF_NOTRAILERS, "notrailers"),
        (libc::IFF_ALLMULTI, "allmulti"),
        (libc::IFF_MASTER, "master"),
        (libc::IFF_SLAVE, "slave"),
        (libc::IFF_MULTICAST, "multicast"),
        (libc::IFF_PORTSEL, "portsel"),
        (libc::IFF_AUTOMEDIA, "automedia"),
        (libc::IFF_DYNAMIC, "dynamic"),
    ];

    let mut values = Vec::new();
    for (bit, name) in table {
        if flags & bit != 0 {
            values.insert(0, Value::symbol(name));
            flags -= bit;
        }
    }

    let mut fnum = 0;
    while flags != 0 && fnum < 32 {
        if flags & 1 != 0 {
            values.insert(0, Value::fixnum(fnum));
        }
        flags >>= 1;
        fnum += 1;
    }

    Value::list(values)
}

struct LinuxIfaddrsGuard(*mut libc::ifaddrs);

impl Drop for LinuxIfaddrsGuard {
    fn drop(&mut self) {
        unsafe {
            libc::freeifaddrs(self.0);
        }
    }
}

fn linux_ipv4_addr_to_value(addr: libc::in_addr) -> Value {
    let octets = addr.s_addr.to_ne_bytes();
    int_vector(&[
        octets[0] as i64,
        octets[1] as i64,
        octets[2] as i64,
        octets[3] as i64,
        0,
    ])
}

fn linux_ipv6_addr_to_value(addr: libc::in6_addr) -> Value {
    let mut vals = [0_i64; 9];
    for (idx, chunk) in addr.s6_addr.chunks_exact(2).enumerate() {
        vals[idx] = u16::from_be_bytes([chunk[0], chunk[1]]) as i64;
    }
    int_vector(&vals)
}

fn linux_sockaddr_ipv4(sockaddr: *const libc::sockaddr) -> Option<Value> {
    if sockaddr.is_null() {
        return None;
    }
    let sockaddr = unsafe { &*(sockaddr as *const libc::sockaddr_in) };
    Some(linux_ipv4_addr_to_value(sockaddr.sin_addr))
}

fn linux_sockaddr_ipv6(sockaddr: *const libc::sockaddr) -> Option<Value> {
    if sockaddr.is_null() {
        return None;
    }
    let sockaddr = unsafe { &*(sockaddr as *const libc::sockaddr_in6) };
    Some(linux_ipv6_addr_to_value(sockaddr.sin6_addr))
}

fn linux_sockaddr_hwaddr(sockaddr: libc::sockaddr) -> Value {
    let mut bytes = [0_i64; 6];
    for (idx, byte) in bytes.iter_mut().enumerate() {
        *byte = sockaddr.sa_data[idx] as u8 as i64;
    }
    Value::cons(Value::fixnum(sockaddr.sa_family as i64), int_vector(&bytes))
}

fn linux_interface_metadata(ifname: &str) -> LinuxInterfaceMetadata {
    let Some(mut flags_request) = linux_ifreq(ifname) else {
        return LinuxInterfaceMetadata {
            hwaddr: None,
            flags: None,
        };
    };
    let Some(mut hwaddr_request) = linux_ifreq(ifname) else {
        return LinuxInterfaceMetadata {
            hwaddr: None,
            flags: None,
        };
    };

    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return LinuxInterfaceMetadata {
            hwaddr: None,
            flags: None,
        };
    }

    let flags = if unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS, &mut flags_request) } == 0 {
        Some(linux_ifflags_to_value(unsafe {
            flags_request.ifr_ifru.ifru_flags
        }))
    } else {
        None
    };
    let hwaddr = if unsafe { libc::ioctl(fd, libc::SIOCGIFHWADDR, &mut hwaddr_request) } == 0 {
        Some(linux_sockaddr_hwaddr(unsafe {
            hwaddr_request.ifr_ifru.ifru_hwaddr
        }))
    } else {
        None
    };

    unsafe {
        libc::close(fd);
    }

    LinuxInterfaceMetadata { hwaddr, flags }
}

pub fn interface_snapshot() -> Option<Vec<HostInterfaceEntry>> {
    let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut ifaddrs) } == -1 {
        return None;
    }
    let _guard = LinuxIfaddrsGuard(ifaddrs);

    let mut entries = Vec::new();
    let mut cursor = ifaddrs;
    while !cursor.is_null() {
        let item = unsafe { &*cursor };
        cursor = item.ifa_next;

        if item.ifa_addr.is_null() {
            continue;
        }

        let family = unsafe { (*item.ifa_addr).sa_family as i32 };
        let Some(name) = (!item.ifa_name.is_null()).then(|| {
            unsafe { CStr::from_ptr(item.ifa_name) }
                .to_string_lossy()
                .into_owned()
        }) else {
            continue;
        };

        let (family, address, netmask, raw_broadcast) = match family {
            libc::AF_INET => {
                let address = linux_sockaddr_ipv4(item.ifa_addr)?;
                let netmask = linux_sockaddr_ipv4(item.ifa_netmask)
                    .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv4));
                let raw_broadcast = linux_sockaddr_ipv4(item.ifa_ifu)
                    .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv4));
                (NetworkAddressFamily::Ipv4, address, netmask, raw_broadcast)
            }
            libc::AF_INET6 => {
                let address = linux_sockaddr_ipv6(item.ifa_addr)?;
                let netmask = linux_sockaddr_ipv6(item.ifa_netmask)
                    .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv6));
                let raw_broadcast = linux_sockaddr_ipv6(item.ifa_ifu)
                    .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv6));
                (NetworkAddressFamily::Ipv6, address, netmask, raw_broadcast)
            }
            _ => continue,
        };

        let linux_metadata = linux_interface_metadata(&name);
        let list_broadcast =
            derive_network_interface_list_broadcast(family, &address, &netmask, &raw_broadcast);
        let info_broadcast =
            derive_network_interface_info_broadcast(family, &address, &raw_broadcast);
        let flags = linux_metadata
            .flags
            .unwrap_or_else(|| linux_ifflags_bits_to_value(item.ifa_flags as i32));

        entries.push(HostInterfaceEntry {
            name,
            family,
            address,
            list_broadcast,
            info_broadcast,
            netmask,
            hwaddr: linux_metadata.hwaddr,
            flags,
        });
    }

    if entries.is_empty() {
        return None;
    }

    Some(entries)
}
