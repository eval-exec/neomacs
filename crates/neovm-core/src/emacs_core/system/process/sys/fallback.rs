//! Fallback `ChildStatusSource` backend for platforms without a native pollable
//! child-status edge yet (macOS, Windows, other Unix).
//!
//! `open` returns `None`, so the caller keeps using the explicit periodic
//! status poll. A native source -- kqueue `EVFILT_PROC` on macOS, a
//! job-object / process-handle wait on Windows -- would replace this with a
//! dedicated `macos` / `windows` backend selected from `sys::mod`'s
//! `cfg_select!`, mirroring GNU's per-platform `w32proc.c` implementation.

use crate::emacs_core::process::ProcessId;
use crate::emacs_core::process::{
    HostInterfaceEntry, NetworkAddressFamily, derive_network_interface_info_broadcast,
    derive_network_interface_list_broadcast, int_vector, zero_network_address,
};
use crate::emacs_core::value::Value;

/// Uninhabited: this backend never has a source, which is enforced by the type
/// system (`open` only ever returns `None`).
pub enum Source {}

pub fn open(_pid: u32) -> Option<Source> {
    None
}

impl Source {
    pub fn register(&self, _poller: &polling::Poller, _id: ProcessId) {
        match *self {}
    }

    pub fn unregister(&self, _poller: &polling::Poller) {
        match *self {}
    }
}

// ---------------------------------------------------------------------------
// network-interface-info backend (portable: the `network_interface` crate).
// A native macOS backend would instead use SIOCGIFFLAGS for real flags and
// AF_LINK/sockaddr_dl/LLADDR for the hardware address (GNU process.c:4544).
// ---------------------------------------------------------------------------

fn parse_mac_addr(mac: &str) -> Option<Value> {
    let mut bytes = Vec::new();
    for part in mac.trim().split(':') {
        if part.is_empty() {
            continue;
        }
        let byte = u8::from_str_radix(part, 16).ok()?;
        bytes.push(Value::fixnum(byte as i64));
    }
    if bytes.is_empty() {
        return None;
    }
    // hatype 1 = ARPHRD_ETHER (Ethernet), the common case
    Some(Value::cons(Value::fixnum(1), Value::vector(bytes)))
}

fn heuristic_network_interface_flags(addr: &network_interface::Addr) -> Value {
    use network_interface::Addr;

    let is_loopback = match addr {
        Addr::V4(v4) => v4.ip.is_loopback(),
        Addr::V6(v6) => v6.ip.is_loopback(),
    };
    let has_broadcast = match addr {
        Addr::V4(v4) => v4.broadcast.is_some(),
        Addr::V6(v6) => v6.broadcast.is_some(),
    };

    let mut flags = vec![Value::symbol("running"), Value::symbol("up")];
    if is_loopback {
        flags.push(Value::symbol("loopback"));
    }
    if has_broadcast {
        flags.push(Value::symbol("broadcast"));
    }
    Value::list(flags)
}

pub fn interface_snapshot() -> Option<Vec<HostInterfaceEntry>> {
    use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};

    let interfaces = NetworkInterface::show().ok()?;

    let mut entries = Vec::new();

    for iface in &interfaces {
        let hwaddr = iface
            .mac_addr
            .as_deref()
            .and_then(|mac| parse_mac_addr(mac));

        for addr in &iface.addr {
            let (family, address, netmask, raw_broadcast) = match addr {
                Addr::V4(v4) => {
                    let ip = v4.ip.octets();
                    let address =
                        int_vector(&[ip[0] as i64, ip[1] as i64, ip[2] as i64, ip[3] as i64, 0]);
                    let netmask = v4
                        .netmask
                        .map(|m| {
                            let o = m.octets();
                            int_vector(&[o[0] as i64, o[1] as i64, o[2] as i64, o[3] as i64, 0])
                        })
                        .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv4));
                    let broadcast = v4
                        .broadcast
                        .map(|b| {
                            let o = b.octets();
                            int_vector(&[o[0] as i64, o[1] as i64, o[2] as i64, o[3] as i64, 0])
                        })
                        .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv4));
                    (NetworkAddressFamily::Ipv4, address, netmask, broadcast)
                }
                Addr::V6(v6) => {
                    let segs = v6.ip.segments();
                    let mut vals = [0_i64; 9];
                    for (idx, &seg) in segs.iter().enumerate() {
                        vals[idx] = seg as i64;
                    }
                    let address = int_vector(&vals);
                    let netmask = v6
                        .netmask
                        .map(|m| {
                            let s = m.segments();
                            let mut v = [0_i64; 9];
                            for (idx, &seg) in s.iter().enumerate() {
                                v[idx] = seg as i64;
                            }
                            int_vector(&v)
                        })
                        .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv6));
                    let broadcast = v6
                        .broadcast
                        .map(|b| {
                            let s = b.segments();
                            let mut v = [0_i64; 9];
                            for (idx, &seg) in s.iter().enumerate() {
                                v[idx] = seg as i64;
                            }
                            int_vector(&v)
                        })
                        .unwrap_or_else(|| zero_network_address(NetworkAddressFamily::Ipv6));
                    (NetworkAddressFamily::Ipv6, address, netmask, broadcast)
                }
            };

            let list_broadcast =
                derive_network_interface_list_broadcast(family, &address, &netmask, &raw_broadcast);
            let info_broadcast =
                derive_network_interface_info_broadcast(family, &address, &raw_broadcast);

            let flags = heuristic_network_interface_flags(addr);

            entries.push(HostInterfaceEntry {
                name: iface.name.clone(),
                family,
                address,
                list_broadcast,
                info_broadcast,
                netmask,
                hwaddr,
                flags,
            });
        }
    }

    if entries.is_empty() {
        return None;
    }

    Some(entries)
}
