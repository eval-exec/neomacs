//! Classification of filesystem errors into GNU's `file-error` family.
//!
//! GNU `fileio.c` `get_file_errno_data` keys the condition symbol on the raw
//! errno (`EEXIST`, `ENOENT`, `EACCES`, everything else) and carries
//! `emacs_strerror (errno)` as the message. Rust's Unix I/O errors expose that
//! libc errno vocabulary directly. Other targets—including Windows, whose raw
//! I/O codes are Win32 errors rather than CRT errno values—classify by
//! [`io::ErrorKind`] and carry the standard library's error text. This module
//! is the one place that knows which representation is available, so `fileio`
//! and `filelock` stay target-agnostic.

use std::io::{self, ErrorKind};

/// The GNU condition a filesystem failure signals.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum FileErrorClass {
    /// `EEXIST`: `(file-already-exists STRERROR . NAME)`, with no ACTION.
    FileAlreadyExists,
    /// `ENOENT`.
    FileMissing,
    /// `EACCES` only. `EPERM` stays a plain `file-error`, exactly as in GNU,
    /// even though Rust folds both into `ErrorKind::PermissionDenied`.
    PermissionDenied,
    /// Every other failure.
    FileError,
}

impl FileErrorClass {
    /// The condition symbol GNU signals for this class.
    pub(crate) fn condition_symbol(self) -> &'static str {
        self.into()
    }

    /// GNU's errno table (`fileio.c` `get_file_errno_data`).
    #[cfg(unix)]
    fn from_errno(errno: i32) -> Self {
        match errno {
            libc::EEXIST => Self::FileAlreadyExists,
            libc::ENOENT => Self::FileMissing,
            libc::EACCES => Self::PermissionDenied,
            _ => Self::FileError,
        }
    }

    /// The same table keyed on the portable [`ErrorKind`], for errors that
    /// carry no raw OS code and for hosts without errno.
    pub(crate) fn from_kind(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::AlreadyExists => Self::FileAlreadyExists,
            ErrorKind::NotFound => Self::FileMissing,
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            _ => Self::FileError,
        }
    }
}

/// What `report_file_errno` needs: the condition and its message text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileErrorReport {
    pub(crate) class: FileErrorClass,
    /// GNU's bare `strerror` text on errno hosts (never Rust's
    /// "(os error N)" suffix); the standard library's text elsewhere.
    pub(crate) strerror: String,
}

/// Classify an I/O failure the way GNU's `report_file_errno` would.
#[cfg(unix)]
pub(crate) fn classify(err: &io::Error) -> FileErrorReport {
    let errno = err
        .raw_os_error()
        .unwrap_or_else(|| errno_for_kind(err.kind()));
    FileErrorReport {
        class: FileErrorClass::from_errno(errno),
        strerror: errno_strerror(errno),
    }
}

/// Classify an I/O failure on a host without errno: browser storage reports
/// typed errors, so only the [`ErrorKind`] carries meaning here.
#[cfg(not(unix))]
pub(crate) fn classify(err: &io::Error) -> FileErrorReport {
    FileErrorReport {
        class: FileErrorClass::from_kind(err.kind()),
        strerror: err.to_string(),
    }
}

/// An `EINVAL`-shaped error, so `report_file_errno` renders GNU's "Invalid
/// argument" text where the OS vocabulary exists.
pub(crate) fn invalid_argument_error() -> io::Error {
    #[cfg(unix)]
    {
        io::Error::from_raw_os_error(libc::EINVAL)
    }
    #[cfg(not(unix))]
    {
        io::Error::new(ErrorKind::InvalidInput, "invalid argument")
    }
}

/// Best-effort errno for an `io::Error` without a raw OS code, so kind-only
/// errors classify and render exactly as their errno counterparts.
#[cfg(unix)]
fn errno_for_kind(kind: ErrorKind) -> i32 {
    match kind {
        ErrorKind::NotFound => libc::ENOENT,
        ErrorKind::AlreadyExists => libc::EEXIST,
        ErrorKind::PermissionDenied => libc::EACCES,
        _ => libc::EIO,
    }
}

/// The bare `strerror` text for an errno, matching GNU's `emacs_strerror`
/// (e.g. ENOENT -> "No such file or directory").  Rust's
/// `io::Error::to_string()` appends "(os error N)", which GNU never emits, so
/// go through libc `strerror` directly.
#[cfg(unix)]
fn errno_strerror(errno: i32) -> String {
    // SAFETY: `strerror` returns a pointer to a static (per-thread) C string.
    unsafe {
        let ptr = libc::strerror(errno);
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_table_matches_gnu_condition_symbols() {
        assert_eq!(
            FileErrorClass::from_kind(ErrorKind::NotFound).condition_symbol(),
            "file-missing"
        );
        assert_eq!(
            FileErrorClass::from_kind(ErrorKind::AlreadyExists).condition_symbol(),
            "file-already-exists"
        );
        assert_eq!(
            FileErrorClass::from_kind(ErrorKind::PermissionDenied).condition_symbol(),
            "permission-denied"
        );
        assert_eq!(
            FileErrorClass::from_kind(ErrorKind::InvalidInput).condition_symbol(),
            "file-error"
        );
    }

    #[cfg(unix)]
    #[test]
    fn errno_table_keeps_eperm_a_plain_file_error_like_gnu() {
        let report = classify(&io::Error::from_raw_os_error(libc::EPERM));
        assert_eq!(report.class, FileErrorClass::FileError);
        assert_eq!(report.strerror, "Operation not permitted");

        let report = classify(&io::Error::from_raw_os_error(libc::EACCES));
        assert_eq!(report.class, FileErrorClass::PermissionDenied);
        assert_eq!(report.strerror, "Permission denied");

        let report = classify(&io::Error::from_raw_os_error(libc::ENOENT));
        assert_eq!(report.class, FileErrorClass::FileMissing);
        assert_eq!(report.strerror, "No such file or directory");

        let report = classify(&io::Error::from_raw_os_error(libc::EEXIST));
        assert_eq!(report.class, FileErrorClass::FileAlreadyExists);
        assert_eq!(report.strerror, "File exists");

        let report = classify(&io::Error::from_raw_os_error(libc::ENOTDIR));
        assert_eq!(report.class, FileErrorClass::FileError);
        assert!(!report.strerror.contains("os error"));
    }

    #[cfg(unix)]
    #[test]
    fn kind_only_errors_render_like_their_errno_counterparts() {
        let by_kind = classify(&io::Error::from(ErrorKind::PermissionDenied));
        let by_errno = classify(&io::Error::from_raw_os_error(libc::EACCES));
        assert_eq!(by_kind, by_errno);

        let by_kind = classify(&io::Error::new(ErrorKind::Other, "custom"));
        assert_eq!(by_kind.class, FileErrorClass::FileError);
        assert_eq!(
            by_kind.strerror,
            classify(&io::Error::from_raw_os_error(libc::EIO)).strerror
        );
    }

    #[cfg(unix)]
    #[test]
    fn invalid_argument_error_carries_einval() {
        assert_eq!(invalid_argument_error().raw_os_error(), Some(libc::EINVAL));
        assert_eq!(
            classify(&invalid_argument_error()).strerror,
            "Invalid argument"
        );
    }
}
