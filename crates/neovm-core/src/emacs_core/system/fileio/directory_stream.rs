//! The Unix directory stream GNU's directory primitives consume.

use crate::heap_types::LispString;
use std::ffi::{CStr, CString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;

/// Each invocation owns one independent native directory stream. Streams are
/// never shared across mutators and retain no Lisp values or evaluator state.
struct DirectoryStream(NonNull<libc::DIR>);

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: this is the uniquely owned stream returned by opendir.
        unsafe { libc::closedir(self.0.as_ptr()) };
    }
}

/// An owned host error, with no Lisp state; each caller constructs its signal.
pub(crate) enum DirectoryReadError {
    Open(io::Error),
    Read(io::Error),
}

impl DirectoryReadError {
    pub(crate) fn into_parts(self) -> (&'static str, io::Error) {
        match self {
            Self::Open(error) => ("Opening directory", error),
            Self::Read(error) => ("Reading directory", error),
        }
    }
}

/// Read every entry, including dots, at its actual position in the stream.
/// GNU src/dired.c:186-213 retries interrupted reads; directory_files_internal
/// consumes the resulting order before COUNT or sorting (dired.c:297-371).
pub(crate) fn read_names(path: &Path) -> Result<Vec<LispString>, DirectoryReadError> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| DirectoryReadError::Open(io::Error::from_raw_os_error(libc::EINVAL)))?;
    // SAFETY: path is NUL-terminated and remains live for the call.
    let stream = NonNull::new(unsafe { libc::opendir(path.as_ptr()) })
        .map(DirectoryStream)
        .ok_or_else(|| DirectoryReadError::Open(io::Error::last_os_error()))?;
    let mut names = Vec::new();
    loop {
        ::errno::set_errno(::errno::Errno(0));
        // SAFETY: the stream is live and unique to this invocation. Copy the
        // name before the next call overwrites readdir's native entry storage.
        let entry = unsafe { libc::readdir(stream.0.as_ptr()) };
        if entry.is_null() {
            let code = ::errno::errno().0;
            match code {
                0 => return Ok(names),
                libc::EAGAIN | libc::EINTR => continue,
                _ => return Err(DirectoryReadError::Read(io::Error::from_raw_os_error(code))),
            }
        }
        // SAFETY: POSIX readdir returns a NUL-terminated d_name in this entry.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        names.push(LispString::from_unibyte(name.to_bytes().to_vec()));
    }
}
