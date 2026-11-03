//! Pure file-name conversion owned by the edit layer.
//!
//! GNU keeps file names as Lisp strings until the system-call boundary
//! (`fileio.c`'s `ENCODE_FILE` / platform I/O). The conversion binds only the
//! value-layer string type and platform APIs, so it lives here and the system
//! layer reads it downward. Operations that can run Lisp — expanding a name
//! against the default directory, file-name handler dispatch, locking — stay
//! in the system layer and are reached through the host seam.

use crate::heap_types::LispString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// Convert a Lisp file-name string to an OS path at the real filesystem boundary.
///
/// GNU keeps file names as Lisp strings until ENCODE_FILE / platform I/O.
/// On Unix, preserve the original bytes exactly so raw unibyte file names
/// survive intact.
pub(crate) fn lisp_file_name_to_path_buf(filename: &LispString) -> PathBuf {
    #[cfg(unix)]
    {
        PathBuf::from(std::ffi::OsString::from_vec(filename.as_bytes().to_vec()))
    }

    #[cfg(not(unix))]
    {
        let name = crate::emacs_core::emacs_char::to_utf8_lossy(filename.as_bytes());
        PathBuf::from(lisp_file_name_to_host_path_string(&name))
    }
}

#[cfg(not(unix))]
fn lisp_file_name_to_host_path_string(filename: &str) -> String {
    #[cfg(windows)]
    {
        filename.replace('/', "\\")
    }

    #[cfg(not(windows))]
    {
        filename.to_string()
    }
}
