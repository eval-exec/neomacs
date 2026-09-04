//! Safe-facing Windows filesystem operations with GNU Emacs semantics.
//!
//! Rust's standard library deliberately gives `remove_file` POSIX-like
//! behavior on modern Windows: after `DeleteFileW` rejects a read-only file,
//! it retries with `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE`. GNU's
//! `sys_unlink` instead clears the read-only attribute with `_wchmod` before
//! unlinking. Besides affecting failure behavior, that preparation is
//! observable through `ReadDirectoryChangesW` and is part of GNU's
//! file-notification contract.

use std::fs;
use std::path::Path;

/// Delete one non-directory filesystem entry like GNU's `sys_unlink`.
///
/// Keep the Windows policy here so portable file-I/O callers cannot silently
/// bypass it by choosing `std::fs::remove_file` directly. The operation uses
/// safe standard-library APIs; `Permissions` preserves all other Windows file
/// attributes while changing only the read-only bit.
pub(crate) fn unlink(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_symlink() && metadata.permissions().readonly() {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    fs::remove_file(path)
}
