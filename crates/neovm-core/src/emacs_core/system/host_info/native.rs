//! Operating-system-backed host inventory.

use std::num::NonZeroU64;
use std::sync::OnceLock;

use super::{BootTime, LoadAverage};

pub(crate) fn system_name() -> Option<String> {
    hostname::get()
        .ok()
        .map(|name| name.to_string_lossy().into_owned())
}

pub(crate) fn operating_system_release() -> Option<String> {
    sysinfo::System::kernel_version()
}

#[cfg(unix)]
pub(crate) fn load_average() -> Option<LoadAverage> {
    let load = sysinfo::System::load_average();
    Some(LoadAverage::new(load.one, load.five, load.fifteen))
}

#[cfg(not(unix))]
pub(crate) fn load_average() -> Option<LoadAverage> {
    None
}

pub(crate) fn configured_processor_count() -> Option<NonZeroU64> {
    let mut system = sysinfo::System::new();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    NonZeroU64::new(system.cpus().len() as u64)
}

pub(crate) fn boot_time() -> Option<BootTime> {
    static BOOT_TIME: OnceLock<Option<BootTime>> = OnceLock::new();
    *BOOT_TIME.get_or_init(query_boot_time)
}

#[cfg(windows)]
fn query_boot_time() -> Option<BootTime> {
    // GNU gnulib checks this boot-touched file before falling back to
    // current time minus GetTickCount64 uptime.
    std::fs::metadata(r"C:\pagefile.sys")
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .or_else(|| i64::try_from(sysinfo::System::boot_time()).ok())
        .and_then(BootTime::from_unix_seconds)
}

/// The utmp BOOT_TIME record, GNU gnulib's primary boot-time source
/// (`lib/boot-time.c get_boot_time_uncached`). Systemd stamps this record
/// seconds later than the kernel's `/proc/stat` btime, and GNU's staleness
/// check tolerates only one second of skew.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn boot_time_from_utmp() -> Option<BootTime> {
    // getutxent is not thread-safe. This executes once behind BOOT_TIME, and
    // nothing else in the process reads utmp.
    unsafe {
        libc::setutxent();
        let mut found = None;
        loop {
            let entry = libc::getutxent();
            if entry.is_null() {
                break;
            }
            if (*entry).ut_type == libc::BOOT_TIME {
                found = BootTime::from_unix_seconds((*entry).ut_tv.tv_sec as i64);
            }
        }
        libc::endutxent();
        found
    }
}

#[cfg(not(windows))]
fn query_boot_time() -> Option<BootTime> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Some(utmp_boot) = boot_time_from_utmp() {
        return Some(utmp_boot);
    }
    i64::try_from(sysinfo::System::boot_time())
        .ok()
        .and_then(BootTime::from_unix_seconds)
}
