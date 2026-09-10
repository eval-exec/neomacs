//! Timestamp metadata operations must not require writing file contents.

use super::super::FileTimestamp;
use std::{io, path::Path};

pub(super) fn set(
    path: &Path,
    timestamp: Option<FileTimestamp>,
    follow_links: bool,
) -> io::Result<()> {
    if timestamp.is_some_and(|time| time.nanoseconds >= 1_000_000_000) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid nanosecond timestamp",
        ));
    }
    std::cfg_select! {
        unix => {
            use std::os::unix::ffi::OsStrExt;
            let path = std::ffi::CString::new(path.as_os_str().as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL in file name"))?;
            let time = match timestamp {
                Some(time) => libc::timespec {
                    tv_sec: time.seconds.try_into().map_err(|_| io::Error::new(
                        io::ErrorKind::InvalidInput, "timestamp exceeds platform time_t"))?,
                    // Validated below one billion above, so this also fits a
                    // signed 32-bit c_long on 32-bit Unix targets.
                    tv_nsec: time.nanoseconds as libc::c_long,
                },
                None => libc::timespec { tv_sec: 0, tv_nsec: libc::UTIME_NOW as _ },
            };
            let times = [time; 2];
            let flags = if follow_links { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
            // SAFETY: path is NUL-terminated and times holds the two initialized
            // timespec values required by utimensat for the duration of the call.
            let result = unsafe { libc::utimensat(libc::AT_FDCWD, path.as_ptr(), times.as_ptr(), flags) };
            if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
        }
        windows => {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
            };
            // GNU w32.c's utimensat uses metadata-only CreateFileW access,
            // including directory handles and optional reparse-point traversal.
            let flags = FILE_FLAG_BACKUP_SEMANTICS
                | if follow_links { 0 } else { FILE_FLAG_OPEN_REPARSE_POINT };
            let time = match timestamp {
                Some(time) => time.to_system_time().ok_or_else(|| io::Error::new(
                    io::ErrorKind::InvalidInput, "file timestamp is out of range"))?,
                None => std::time::SystemTime::now(),
            };
            std::fs::OpenOptions::new()
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(flags)
                .open(path)?
                .set_times(std::fs::FileTimes::new().set_accessed(time).set_modified(time))
        }
        _ => {
            if !follow_links {
                return Err(io::Error::new(io::ErrorKind::Unsupported,
                    "nofollow set-file-times is unsupported on this platform"));
            }
            let time = match timestamp {
                Some(time) => time.to_system_time().ok_or_else(|| io::Error::new(
                    io::ErrorKind::InvalidInput, "file timestamp is out of range"))?,
                None => std::time::SystemTime::now(),
            };
            std::fs::OpenOptions::new().write(true).open(path)?
                .set_times(std::fs::FileTimes::new().set_accessed(time).set_modified(time))
        }
    }
}
