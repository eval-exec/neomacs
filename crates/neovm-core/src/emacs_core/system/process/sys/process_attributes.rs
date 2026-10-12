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
//! module only gathers raw OS data. Darwin reads process state through its
//! typed libproc API. Its native snapshot keeps CPU nanoseconds and epoch
//! microseconds separate from Linux's boot-relative clock ticks.
//! Additional native Darwin/BSD or Windows queries belong behind these same
//! functions.

use std::ffi::CStr;

/// Native Darwin attributes, matching GNU `sysdep.c:4273-4459`. Every query
/// owns its buffers; snapshots contain no shared or thread-confined state.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct DarwinProcessSnapshot {
    pub euid: u32,
    /// GNU reports the saved group ID (`e_pcred.p_svgid`) on Darwin.
    pub egid: u32,
    pub comm: String,
    pub state: String,
    pub ppid: i64,
    pub pgrp: i64,
    pub tpgid: i64,
    pub nice: i64,
    pub start: DarwinStartTime,
    pub tty_name: Option<String>,
    pub usage: Option<DarwinProcessUsage>,
    pub memory: Option<DarwinProcessMemory>,
    pub args: Option<String>,
}

/// An epoch timestamp; unlike `/proc` start ticks, this needs no boot time.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct DarwinStartTime {
    pub seconds: i64,
    pub microseconds: u32,
}

/// CPU duration in nanoseconds, preserving libproc's precision.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
pub struct DarwinCpuTime(u64);

#[cfg(target_os = "macos")]
impl DarwinCpuTime {
    pub fn seconds_and_nanoseconds(self) -> (u64, u32) {
        (self.0 / 1_000_000_000, (self.0 % 1_000_000_000) as u32)
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct DarwinProcessUsage {
    pub user: DarwinCpuTime,
    pub system: DarwinCpuTime,
    pub pageins: u64,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct DarwinProcessMemory {
    pub virtual_kilobytes: u64,
    pub resident_kilobytes: u64,
    pub threads: i32,
}

#[cfg(target_os = "macos")]
fn darwin_bsd_info(pid: libc::pid_t) -> Option<libc::proc_bsdinfo> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: storage is aligned and writable for the typed SDK record. arg=1
    // includes zombies without consuming their wait status.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            1,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    // SAFETY: the complete record was written; its fields are integers/arrays.
    Some(unsafe { info.assume_init() })
}

#[cfg(target_os = "macos")]
fn darwin_state(status: u32) -> &'static str {
    match status {
        libc::SRUN => "R",
        libc::SSLEEP => "S",
        libc::SZOMB => "Z",
        libc::SSTOP => "T",
        libc::SIDL => "I",
        _ => "",
    }
}

#[cfg(target_os = "macos")]
fn darwin_tty_name(device: u32) -> Option<String> {
    if device == u32::MAX {
        return None;
    }
    // libc does not expose this SDK function. The reentrant variant writes
    // caller-owned storage and avoids devname's shared static buffer.
    unsafe extern "C" {
        fn devname_r(
            dev: libc::dev_t,
            kind: libc::mode_t,
            buf: *mut libc::c_char,
            len: libc::c_int,
        ) -> *mut libc::c_char;
    }
    let mut buffer = [0u8; 1024];
    // SAFETY: the buffer is writable for the supplied length, and the ABI
    // matches Darwin Libc gen/devname.c. dev_t preserves the SDK device bits.
    let result = unsafe {
        devname_r(
            device as libc::dev_t,
            libc::S_IFCHR,
            buffer.as_mut_ptr().cast(),
            1024,
        )
    };
    if result.is_null() {
        return None;
    }
    let end = buffer.iter().position(|byte| *byte == 0)?;
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

/// Gather Darwin's supported keys. A failed mandatory BSD query returns None;
/// optional facilities remain absent as in GNU. libproc can deny BSD info for
/// another user's process, unlike GNU's sysctl implementation.
#[cfg(target_os = "macos")]
pub fn darwin_process_snapshot(pid: i64) -> Option<DarwinProcessSnapshot> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    let info = darwin_bsd_info(pid)?;
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: path is writable for exactly the supplied buffer size.
    let path_length =
        unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    let comm = if path_length > 0 {
        let end = path.iter().position(|byte| *byte == 0)?;
        let bytes = &path[..end];
        let start = bytes
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(0, |position| position + 1);
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    } else {
        let bytes: Vec<u8> = info
            .pbi_comm
            .iter()
            .map(|byte| *byte as u8)
            .take_while(|byte| *byte != 0)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v0>::zeroed();
    // SAFETY: RUSAGE_INFO_V0 writes exactly the typed v0 record. The SDK's
    // rusage_info_t* denotes the record address, not a separately allocated pointer.
    let usage =
        if unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V0, usage.as_mut_ptr().cast()) }
            == 0
        {
            // SAFETY: successful proc_pid_rusage initialized the entire v0 record.
            let usage = unsafe { usage.assume_init() };
            Some(DarwinProcessUsage {
                user: DarwinCpuTime(usage.ri_user_time),
                system: DarwinCpuTime(usage.ri_system_time),
                pageins: usage.ri_pageins,
            })
        } else {
            None
        };
    let mut memory = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let memory_size = libc::c_int::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: storage matches the flavor and is writable for memory_size bytes.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            memory.as_mut_ptr().cast(),
            memory_size,
        )
    };
    let memory = if read == memory_size {
        // SAFETY: the complete typed record was written.
        let memory = unsafe { memory.assume_init() };
        Some(DarwinProcessMemory {
            virtual_kilobytes: memory.pti_virtual_size / 1024,
            resident_kilobytes: memory.pti_resident_size / 1024,
            threads: memory.pti_threadnum,
        })
    } else {
        None
    };
    Some(DarwinProcessSnapshot {
        euid: info.pbi_uid,
        egid: info.pbi_svgid,
        comm,
        state: darwin_state(info.pbi_status).to_owned(),
        ppid: i64::from(info.pbi_ppid),
        pgrp: i64::from(info.pbi_pgid),
        tpgid: i64::from(info.e_tpgid as i32),
        nice: i64::from(info.pbi_nice),
        start: DarwinStartTime {
            seconds: i64::try_from(info.pbi_start_tvsec).ok()?,
            microseconds: u32::try_from(info.pbi_start_tvusec)
                .ok()
                .filter(|usecs| *usecs < 1_000_000)?,
        },
        tty_name: darwin_tty_name(info.e_tdev),
        usage,
        memory,
        args: darwin_process_args(pid),
    })
}

#[cfg(target_os = "macos")]
fn darwin_process_args(pid: libc::pid_t) -> Option<String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut length = 0usize;
    // SAFETY: mib is readable; a null output queries the required byte count.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || length == 0
    {
        return None;
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).ok()?;
    bytes.resize(length, 0);
    // SAFETY: bytes owns length writable bytes; sysctl reports the bytes used.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || length > bytes.len()
    {
        return None;
    }
    parse_darwin_args(&bytes[..length])
}

#[cfg(target_os = "macos")]
fn parse_darwin_args(bytes: &[u8]) -> Option<String> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let argc = usize::try_from(argc).ok()?;
    let mut remaining = bytes.get(4..)?;
    let executable_end = remaining.iter().position(|byte| *byte == 0)?;
    remaining = &remaining[executable_end + 1..];
    let padding = remaining
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(remaining.len());
    remaining = &remaining[padding..];
    let mut args = Vec::new();
    for _ in 0..argc {
        let end = remaining.iter().position(|byte| *byte == 0)?;
        args.push(String::from_utf8_lossy(&remaining[..end]).into_owned());
        remaining = &remaining[end + 1..];
    }
    Some(args.join(" "))
}

#[cfg(all(test, target_os = "macos"))]
#[path = "tests/darwin_process_attributes_test.rs"]
mod darwin_tests;

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
    /// The all-zero snapshot used when native data is unavailable but the
    /// process still exists. Unsupported fields retain these fallback values.
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
/// orders them). Empty on unsupported hosts without `/proc`.
#[cfg(not(target_os = "macos"))]
pub fn list_process_ids() -> Vec<i64> {
    std::fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<i64>().ok())
        .collect()
}

/// Native Darwin enumeration, analogous to GNU's KERN_PROC_ALL query
/// (`sysdep.c:3349-3388`), without duplicating the SDK kinfo_proc ABI.
#[cfg(target_os = "macos")]
pub fn list_process_ids() -> Vec<i64> {
    // SAFETY: a null buffer queries the number of process IDs.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Some(capacity) = usize::try_from(count)
        .ok()
        .filter(|count| *count > 0)
        .and_then(|count| count.checked_add(32))
    else {
        return Vec::new();
    };
    let Some(byte_count) = capacity
        .checked_mul(std::mem::size_of::<libc::pid_t>())
        .and_then(|bytes| libc::c_int::try_from(bytes).ok())
    else {
        return Vec::new();
    };
    let mut pids = Vec::<libc::pid_t>::new();
    if pids.try_reserve_exact(capacity).is_err() {
        return Vec::new();
    }
    pids.resize(capacity, 0);
    // SAFETY: pids owns byte_count aligned writable bytes. libproc returns
    // the number of IDs written, rather than the number of bytes.
    let read = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), byte_count) };
    let Ok(read) = usize::try_from(read) else {
        return Vec::new();
    };
    if read > pids.len() {
        return Vec::new();
    }
    pids.truncate(read);
    pids.into_iter().map(i64::from).collect()
}

#[cfg(not(target_os = "macos"))]
fn parse_stat_i64_field(fields: &[&str], index: usize) -> Option<i64> {
    fields.get(index)?.parse::<i64>().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
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
#[cfg(not(target_os = "macos"))]
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

/// GNU Darwin's state letters (`sysdep.c:4337-4363`). libproc supplies the same
/// kernel `p_stat` without duplicating the SDK's `kinfo_proc` layout in Rust.
#[cfg(target_os = "macos")]
pub fn process_stat(pid: i64) -> Option<ProcStatSnapshot> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    let info = darwin_bsd_info(pid)?;
    let mut snapshot = ProcStatSnapshot::fallback(i64::from(pid));
    snapshot.state = darwin_state(info.pbi_status).to_owned();
    Some(snapshot)
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
