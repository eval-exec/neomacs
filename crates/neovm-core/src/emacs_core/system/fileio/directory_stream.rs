//! Per-invocation native directory streams; no shared Lisp state or TLS.
#![deny(clippy::undocumented_unsafe_blocks)]

use crate::emacs_core::error::Flow;
use crate::heap_types::LispString;
use std::ffi::{CStr, CString};
use std::io;
use std::marker::PhantomData;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;

/// Uniquely owns one open native directory until it is dropped.
///
/// Each Lisp invocation keeps its stream on its owning mutator thread; other
/// mutators open independent streams. No native entry borrow escapes a read.
#[derive(Debug)]
#[must_use = "dropping the stream closes the directory immediately"]
#[repr(transparent)]
pub(crate) struct DirectoryStream {
    handle: NonNull<libc::DIR>,
    _thread_confined: PhantomData<*mut ()>,
}

static_assertions::assert_not_impl_any!(DirectoryStream: Send, Sync, Clone, Copy);
const _: () = assert!(size_of::<DirectoryStream>() == size_of::<NonNull<libc::DIR>>());
const _: () = assert!(size_of::<Option<DirectoryStream>>() == size_of::<DirectoryStream>());

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: only a successful opendir constructs this private, non-null
        // handle. It cannot be cloned or shared, and Drop closes it exactly
        // once after the last read. No borrowed dirent survives read_once.
        // GNU's unwind also ignores the close error; Drop never panics.
        unsafe { libc::closedir(self.handle.as_ptr()) };
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DirectoryReadError {
    #[error("Opening directory: {0}")]
    Open(#[from] io::Error),
}

static_assertions::assert_impl_all!(DirectoryReadError: std::error::Error, Send, Sync);

impl DirectoryReadError {
    #[deny(clippy::wildcard_enum_match_arm)]
    pub(crate) fn into_parts(self) -> (&'static str, io::Error) {
        match self {
            Self::Open(error) => ("Opening directory", error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DirectoryNextError {
    #[error("Reading directory: {0}")]
    Io(#[from] io::Error),
    /// Propagates the quit callback's owned control flow and its GC pins.
    /// Flow is not an I/O error or a std::error::Error source.
    #[error("Directory scan interrupted by Lisp control flow")]
    Quit(Flow),
}

/// GNU dired.c:196 retries precisely these native errno values.
/// ErrorKind alone would also accept synthesized, non-OS errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, num_enum::TryFromPrimitive)]
#[repr(i32)]
enum DirectoryRetryErrno {
    Interrupted = libc::EINTR,
    TemporarilyUnavailable = libc::EAGAIN,
}

impl DirectoryStream {
    /// Open a stream or preserve the native opening error for Lisp reporting.
    /// An interior NUL is rejected with EINVAL before calling the native API.
    pub(crate) fn open(path: &Path) -> Result<Self, DirectoryReadError> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DirectoryReadError::Open(io::Error::from_raw_os_error(libc::EINVAL)))?;
        // SAFETY: CString validates that path has no interior NUL and supplies
        // the trailing NUL; its storage remains live throughout opendir.
        NonNull::new(unsafe { libc::opendir(path.as_ptr()) })
            .map(|handle| Self {
                handle,
                _thread_confined: PhantomData,
            })
            .ok_or_else(|| DirectoryReadError::Open(io::Error::last_os_error()))
    }
    fn read_once(&mut self) -> io::Result<Option<LispString>> {
        ::errno::set_errno(::errno::Errno(0));
        // SAFETY: the private handle belongs to this live stream, and &mut self
        // excludes another read or close. No callback executes until the entry
        // bytes have been copied into owned storage below.
        let entry = unsafe { libc::readdir(self.handle.as_ptr()) };
        if entry.is_null() {
            let code = ::errno::errno().0;
            return if code == 0 {
                Ok(None)
            } else {
                Err(io::Error::from_raw_os_error(code))
            };
        }
        // SAFETY: a successful readdir returned this non-null dirent; POSIX
        // guarantees a NUL-terminated d_name. The unique stream is neither
        // read nor closed while this borrowed name is copied into the result.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        Ok(Some(LispString::from_unibyte(name.to_bytes().to_vec())))
    }
    /// Return an owned filename or EOF; propagate permanent native errors and
    /// any control flow raised by the invocation's retry quit callback.
    pub(crate) fn next_name(
        &mut self,
        quit: impl FnMut() -> Result<(), Flow>,
    ) -> Result<Option<LispString>, DirectoryNextError> {
        next_with_retry(|| self.read_once(), quit)
    }
}

// GNU dired.c:188-208: a retry has its own quit point. The injected operations
// also make permanent EINTR/EAGAIN deterministic in tests without host races.
fn next_with_retry(
    mut read: impl FnMut() -> io::Result<Option<LispString>>,
    mut quit: impl FnMut() -> Result<(), Flow>,
) -> Result<Option<LispString>, DirectoryNextError> {
    loop {
        match read() {
            Ok(name) => return Ok(name),
            Err(err)
                if err
                    .raw_os_error()
                    .is_some_and(|code| DirectoryRetryErrno::try_from(code).is_ok()) =>
            {
                quit().map_err(DirectoryNextError::Quit)?
            }
            Err(err) => return Err(DirectoryNextError::Io(err)),
        }
    }
}

#[cfg(test)]
#[path = "tests/directory_stream_review.rs"]
mod tests;
