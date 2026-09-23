//! GNU's errno-to-text boundary.
//!
//! [`emacs_strerror`] is GNU's `emacs_strerror` (`src/emacs.c:3254`), and it is
//! what every GNU error message that quotes an errno is built from:
//! `report_file_notify_error` (`src/fileio.c:307`), the file-error paths in
//! `fileio.c`, the process diagnostics in `process.c`. Its text is the bare
//! `strerror` string -- "No such file or directory", "Invalid argument".
//!
//! Rust's equivalent is NOT that string. `io::Error`, and every error type
//! that wraps it (`rustix::io::Errno`), append " (os error N)". So a Rust
//! error carried across an interface and stringified at the Lisp boundary
//! emits text GNU never produces:
//!
//! ```text
//!   GNU:   "No such file or directory"
//!   Rust:  "No such file or directory (os error 2)"
//! ```
//!
//! That is a divergence which only appears at the boundary, which is why it
//! keeps recurring: the error is correct where it is raised, and only becomes
//! wrong where it is formatted. [`Errno`] exists so a boundary cannot make
//! that mistake -- it is the only shape the file-notify error constructors
//! accept for an OS failure, and its only rendering is `strerror`.
//!
//! Not a mirror module in the four-population sense, for two reasons: GNU has
//! no `src/errno.c` for a port to mirror (`emacs_strerror` sits among
//! `src/emacs.c`'s other host plumbing), and this has no elisp surface at all
//! -- no subr and no variable. It is the shared machinery three subsystems
//! each used to duplicate by hand (`system/fileio`, `system/process`, and
//! `lisp/native/fns`, whose copy was a live defect rather than a duplicate).

use std::ffi::CStr;

/// An errno, rendered the way GNU's Lisp boundary renders one.
///
/// Not a `String`, deliberately. A caller cannot pass pre-formatted text where
/// an `Errno` is expected, so Rust's `"... (os error N)"` cannot reach Lisp
/// through any error path that takes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Errno(libc::c_int);

impl Errno {
    pub const fn new(errno: libc::c_int) -> Self {
        Self(errno)
    }

    /// Recover the code from an `io::Error`, which is the last point at which
    /// it is still known.
    ///
    /// Callers that need GNU's kind-keyed classification for an error carrying
    /// no OS code own that mapping themselves -- `system/fileio`'s
    /// `errno_for_kind` is the existing one, and it deliberately differs from a
    /// naive map, so it is not folded in here.
    pub fn from_io(error: &std::io::Error) -> Self {
        Self(error.raw_os_error().unwrap_or(libc::EIO))
    }

    pub const fn get(self) -> libc::c_int {
        self.0
    }

    /// The bare `strerror` text, exactly GNU's `emacs_strerror`.
    pub fn message(self) -> String {
        emacs_strerror(self.0)
    }
}

impl std::fmt::Display for Errno {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&emacs_strerror(self.0))
    }
}

/// The bare `strerror` text for `errno`, matching GNU's `emacs_strerror`
/// (`src/emacs.c:3254`; e.g. `ENOENT` -> "No such file or directory").
///
/// Rust's `io::Error::to_string()` appends "(os error N)", which GNU never
/// emits, so go through libc directly.
///
/// One implementation for every platform, as in GNU: `libc::strerror` is the
/// CRT's on Windows just as GNU's `WINDOWSNT` build calls the same function.
pub fn emacs_strerror(errno: libc::c_int) -> String {
    // SAFETY: `strerror` returns a pointer to a static (per-thread) C string,
    // valid until the next `strerror` call on this thread; the `CStr` borrow
    // and the `to_string_lossy().into_owned()` copy both end before that.
    unsafe {
        let ptr = libc::strerror(errno);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[cfg(test)]
#[path = "tests/errno_test.rs"]
mod tests;
