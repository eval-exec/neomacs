//! Per-invocation native directory streams; no shared Lisp state or TLS.
use crate::emacs_core::error::Flow;
use crate::heap_types::LispString;
use std::ffi::{CStr, CString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;

pub(crate) struct DirectoryStream(NonNull<libc::DIR>);
impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: this is the uniquely owned stream returned by opendir.
        unsafe { libc::closedir(self.0.as_ptr()) };
    }
}
pub(crate) enum DirectoryReadError {
    Open(io::Error),
}
impl DirectoryReadError {
    pub(crate) fn into_parts(self) -> (&'static str, io::Error) {
        match self {
            Self::Open(error) => ("Opening directory", error),
        }
    }
}
#[derive(Debug)]
pub(crate) enum DirectoryNextError {
    Io(io::Error),
    Quit(Flow),
}

impl DirectoryStream {
    pub(crate) fn open(path: &Path) -> Result<Self, DirectoryReadError> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DirectoryReadError::Open(io::Error::from_raw_os_error(libc::EINVAL)))?;
        // SAFETY: path is NUL-terminated and remains live throughout opendir.
        NonNull::new(unsafe { libc::opendir(path.as_ptr()) })
            .map(Self)
            .ok_or_else(|| DirectoryReadError::Open(io::Error::last_os_error()))
    }
    fn read_once(&mut self) -> io::Result<Option<LispString>> {
        ::errno::set_errno(::errno::Errno(0));
        // SAFETY: this stream is unique and live; copy its bytes before readdir
        // can overwrite its entry storage or any Lisp callback can execute.
        let entry = unsafe { libc::readdir(self.0.as_ptr()) };
        if entry.is_null() {
            let code = ::errno::errno().0;
            return if code == 0 {
                Ok(None)
            } else {
                Err(io::Error::from_raw_os_error(code))
            };
        }
        // SAFETY: POSIX d_name is NUL-terminated.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        Ok(Some(LispString::from_unibyte(name.to_bytes().to_vec())))
    }
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
            Err(err) if matches!(err.raw_os_error(), Some(libc::EAGAIN | libc::EINTR)) => {
                quit().map_err(DirectoryNextError::Quit)?
            }
            Err(err) => return Err(DirectoryNextError::Io(err)),
        }
    }
}

#[cfg(test)]
#[path = "tests/directory_stream_review.rs"]
mod tests;
