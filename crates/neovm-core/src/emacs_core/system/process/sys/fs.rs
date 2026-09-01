//! Filesystem access checks (per-facility platform module).
//!
//! `executable_path_access` answers "can this process execute this file", which
//! is an effective-access question. On Unix it must go through `faccessat(path,
//! X_OK, AT_EACCESS)` so the kernel honors effective uid/gid and ACLs; a raw
//! permission-bit inspection would get this wrong. The returned errno is part
//! of GNU `openp`'s observable error contract, so callers must not reduce it to
//! a boolean.

use std::path::Path;

/// Probe `path` for executability, preserving the OS errno on failure.
///
/// GNU `openp` treats an executable/searchable directory as `EISDIR`, not as
/// a runnable program, so perform that check after a successful `faccessat`.
#[cfg(unix)]
pub fn executable_path_access(path: &Path) -> Result<(), libc::c_int> {
    match rustix::fs::accessat(
        rustix::fs::CWD,
        path,
        rustix::fs::Access::EXEC_OK,
        rustix::fs::AtFlags::EACCESS,
    ) {
        Ok(()) if path.is_dir() => Err(libc::EISDIR),
        Ok(()) => Ok(()),
        Err(errno) => Err(errno.raw_os_error()),
    }
}

#[cfg(not(unix))]
pub fn executable_path_access(path: &Path) -> Result<(), libc::c_int> {
    if path.is_dir() {
        Err(libc::EISDIR)
    } else if path.exists() {
        Ok(())
    } else {
        Err(libc::ENOENT)
    }
}
