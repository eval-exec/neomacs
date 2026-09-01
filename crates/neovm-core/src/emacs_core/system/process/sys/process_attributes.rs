//! Host process introspection for `list-system-processes` and
//! `process-attributes` (per-facility platform module).
//!
//! This is the neomacs analogue of GNU Emacs's `system_process_attributes` /
//! `list_system_processes` in `sysdep.c`, which are `#ifdef`'d per OS: Linux /
//! Cygwin / Android read `/proc`, Darwin and the BSDs use `sysctl`, Solaris uses
//! `/proc/<pid>` procfs structs, and Windows delegates to `w32.c`. Here the
//! `/proc` reads are portable Rust (`std::fs` compiles everywhere and simply
//! yields empty results where `/proc` is absent), so only the handful of
//! libc-backed queries carry `cfg`:
//!   - `clock_ticks_per_second` / `page_size_kb`: `sysconf` on Unix, constants
//!     on Windows;
//!   - `user_name` / `group_name`: `getpwuid` / `getgrgid` on non-Windows,
//!     `None` on Windows.
//!
//! The parent `process` module keeps the Lisp side (turning a `ProcStatSnapshot`
//! into the `process-attributes` alist and the time/percentage math); this
//! module only gathers raw OS data. A native Darwin/BSD `sysctl` backend or a
//! Windows toolhelp backend would slot in here behind the same functions.

use std::ffi::CStr;

/// Parsed fields of `/proc/<pid>/stat` (plus the derived tty name), in the raw
/// units the kernel reports. The parent converts ticks/pages to the Lisp
/// `process-attributes` representation.
#[derive(Clone, Debug)]
pub struct ProcStatSnapshot {
    pub comm: String,
    pub state: String,
    pub ppid: i64,
    pub pgrp: i64,
    pub sess: i64,
    pub tpgid: i64,
    pub minflt: i64,
    pub majflt: i64,
    pub cminflt: i64,
    pub cmajflt: i64,
    pub utime_ticks: i64,
    pub stime_ticks: i64,
    pub cutime_ticks: i64,
    pub cstime_ticks: i64,
    pub pri: i64,
    pub nice: i64,
    pub thcount: i64,
    pub start_ticks: i64,
    pub vsize: i64,
    pub rss: i64,
    pub ttname: String,
}

impl ProcStatSnapshot {
    /// The all-zero snapshot used when `/proc/<pid>/stat` is unreadable but the
    /// process still exists (matches GNU returning a sparse alist).
    pub fn fallback(_pid: i64) -> Self {
        Self {
            comm: String::new(),
            state: String::new(),
            ppid: 0,
            pgrp: 0,
            sess: 0,
            tpgid: 0,
            minflt: 0,
            majflt: 0,
            cminflt: 0,
            cmajflt: 0,
            utime_ticks: 0,
            stime_ticks: 0,
            cutime_ticks: 0,
            cstime_ticks: 0,
            pri: 0,
            nice: 0,
            thcount: 0,
            start_ticks: 0,
            vsize: 0,
            rss: 0,
            ttname: procfs_ttyname(0),
        }
    }
}

/// Enumerate the PIDs of every process on the host (unsorted; the caller
/// orders them). Empty where `/proc` is unavailable.
pub fn list_process_ids() -> Vec<i64> {
    std::fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<i64>().ok())
        .collect()
}

fn parse_stat_i64_field(fields: &[&str], index: usize) -> Option<i64> {
    fields.get(index)?.parse::<i64>().ok()
}

#[cfg(unix)]
fn page_size_kb() -> i64 {
    let page_size_bytes = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size_bytes <= 0 {
        4
    } else {
        ((page_size_bytes as i64) / 1024).max(1)
    }
}

#[cfg(not(unix))]
fn page_size_kb() -> i64 {
    4
}

#[cfg(not(target_os = "windows"))]
pub fn clock_ticks_per_second() -> i64 {
    // SAFETY: `sysconf(_SC_CLK_TCK)` has no additional preconditions.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks <= 0 { 100 } else { ticks as i64 }
}

#[cfg(target_os = "windows")]
pub fn clock_ticks_per_second() -> i64 {
    100
}

fn procfs_dev_major(rdev: i64) -> u64 {
    let dev = rdev as u64;
    (dev >> 8) & 0xfff
}

fn procfs_dev_minor(rdev: i64) -> u64 {
    let dev = rdev as u64;
    (dev & 0xff) | ((dev & 0xfff00000) >> 12)
}

fn parse_proc_minor_range(value: &str) -> Option<(u64, u64)> {
    if let Some((start, end)) = value.split_once('-') {
        Some((start.parse().ok()?, end.parse().ok()?))
    } else {
        let minor = value.parse().ok()?;
        Some((minor, minor))
    }
}

fn procfs_ttyname(rdev: i64) -> String {
    if rdev <= 0 {
        return String::new();
    }

    let major = procfs_dev_major(rdev);
    let minor = procfs_dev_minor(rdev);
    let Ok(drivers) = std::fs::read_to_string("/proc/tty/drivers") else {
        return String::new();
    };

    for line in drivers.lines() {
        let mut fields = line.split_whitespace();
        let _driver = fields.next();
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(line_major) = fields.next().and_then(|field| field.parse::<u64>().ok()) else {
            continue;
        };
        let Some((minor_start, minor_end)) = fields.next().and_then(parse_proc_minor_range) else {
            continue;
        };
        if line_major == major && minor_start <= minor && minor <= minor_end {
            return format!("{name}{minor}");
        }
    }

    String::new()
}

/// The process's command line from `/proc/<pid>/cmdline`, NUL-separated
/// arguments rendered with whitespace/backslash escaped and joined by spaces.
/// Falls back to `[comm]` when the command line is empty (kernel threads).
pub fn process_cmdline(pid: i64, comm: &str) -> String {
    let bytes = match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(bytes) => bytes,
        Err(_) => return String::new(),
    };

    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    if end == 0 {
        return format!("[{comm}]");
    }

    let mut quoted = Vec::with_capacity(end);
    for &byte in &bytes[..end] {
        if byte == 0 {
            quoted.push(b' ');
        } else {
            if byte.is_ascii_whitespace() || byte == b'\\' {
                quoted.push(b'\\');
            }
            quoted.push(byte);
        }
    }
    String::from_utf8_lossy(&quoted).into_owned()
}

/// The host boot time in epoch seconds, from `/proc/stat`'s `btime` line.
pub fn boot_time_secs() -> Option<i64> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    for line in stat.lines() {
        if let Some(rest) = line.strip_prefix("btime ") {
            return rest.trim().parse::<i64>().ok();
        }
    }
    None
}

/// Total physical memory in kilobytes, from `/proc/meminfo`'s `MemTotal` line.
pub fn total_memory_kb() -> Option<i64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb = rest.split_whitespace().next()?.parse::<i64>().ok()?;
            return Some(kb);
        }
    }
    None
}

/// Parse `/proc/<pid>/stat`. The `comm` field is delimited by parentheses and
/// may itself contain spaces, so it is extracted between the first `(` and last
/// `)` before the remaining space-separated fields are indexed.
pub fn process_stat(pid: i64) -> Option<ProcStatSnapshot> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open_paren = stat.find('(')?;
    let close_paren = stat.rfind(')')?;
    if close_paren <= open_paren {
        return None;
    }

    let comm = stat.get((open_paren + 1)..close_paren)?.to_string();
    let trailing = stat.get((close_paren + 1)..)?.trim_start();
    let fields: Vec<&str> = trailing.split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }

    let state = fields[0].to_string();
    let ppid = parse_stat_i64_field(&fields, 1)?;
    let pgrp = parse_stat_i64_field(&fields, 2)?;
    let sess = parse_stat_i64_field(&fields, 3)?;
    let tty_nr = parse_stat_i64_field(&fields, 4)?;
    let tpgid = parse_stat_i64_field(&fields, 5)?;
    let minflt = parse_stat_i64_field(&fields, 7)?;
    let cminflt = parse_stat_i64_field(&fields, 8)?;
    let majflt = parse_stat_i64_field(&fields, 9)?;
    let cmajflt = parse_stat_i64_field(&fields, 10)?;
    let utime_ticks = parse_stat_i64_field(&fields, 11)?;
    let stime_ticks = parse_stat_i64_field(&fields, 12)?;
    let cutime_ticks = parse_stat_i64_field(&fields, 13)?;
    let cstime_ticks = parse_stat_i64_field(&fields, 14)?;
    let pri = parse_stat_i64_field(&fields, 15)?;
    let nice = parse_stat_i64_field(&fields, 16)?;
    let thcount = parse_stat_i64_field(&fields, 17)?;
    let start_ticks = parse_stat_i64_field(&fields, 19)?;
    let vsize = parse_stat_i64_field(&fields, 20)?;
    let rss_pages = parse_stat_i64_field(&fields, 21)?;
    let rss = rss_pages.saturating_mul(page_size_kb());
    let ttname = procfs_ttyname(tty_nr);

    Some(ProcStatSnapshot {
        comm,
        state,
        ppid,
        pgrp,
        sess,
        tpgid,
        minflt,
        majflt,
        cminflt,
        cmajflt,
        utime_ticks,
        stime_ticks,
        cutime_ticks,
        cstime_ticks,
        pri,
        nice,
        thcount,
        start_ticks,
        vsize,
        rss,
        ttname,
    })
}

/// The process's effective `(uid, gid)` from `/proc/<pid>/status`' `Uid:` /
/// `Gid:` lines (field index 1 is the effective id).
pub fn process_effective_ids(pid: i64) -> Option<(u32, u32)> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let mut euid = None;
    let mut egid = None;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() >= 2 {
                euid = fields[1].parse::<u32>().ok();
            }
        } else if let Some(rest) = line.strip_prefix("Gid:") {
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() >= 2 {
                egid = fields[1].parse::<u32>().ok();
            }
        }
        if euid.is_some() && egid.is_some() {
            break;
        }
    }
    Some((euid?, egid?))
}

#[cfg(not(target_os = "windows"))]
pub fn user_name(uid: u32) -> Option<String> {
    // SAFETY: libc returns either null or a valid passwd struct pointer.
    let user = unsafe { libc::getpwuid(uid as libc::uid_t) };
    if user.is_null() {
        return None;
    }
    // SAFETY: `user` is non-null and `pw_name` is a valid C string pointer.
    let name_ptr = unsafe { (*user).pw_name };
    if name_ptr.is_null() {
        return None;
    }
    // SAFETY: `name_ptr` is a valid NUL-terminated C string.
    Some(
        unsafe { CStr::from_ptr(name_ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(target_os = "windows")]
pub fn user_name(_uid: u32) -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn group_name(gid: u32) -> Option<String> {
    // SAFETY: libc returns either null or a valid group struct pointer.
    let group = unsafe { libc::getgrgid(gid as libc::gid_t) };
    if group.is_null() {
        return None;
    }
    // SAFETY: `group` is non-null and `gr_name` is a valid C string pointer.
    let name_ptr = unsafe { (*group).gr_name };
    if name_ptr.is_null() {
        return None;
    }
    // SAFETY: `name_ptr` is a valid NUL-terminated C string.
    Some(
        unsafe { CStr::from_ptr(name_ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(target_os = "windows")]
pub fn group_name(_gid: u32) -> Option<String> {
    None
}
