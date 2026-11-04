//! Pure file-name conversion owned by the edit layer.
//!
//! GNU keeps file names as Lisp strings until the system-call boundary
//! (`fileio.c`'s `ENCODE_FILE` / platform I/O). The conversion binds only the
//! value-layer string type and platform APIs, so it lives here and the system
//! layer reads it downward. Operations that can run Lisp — expanding a name
//! against the default directory, file-name handler dispatch, locking — stay
//! in the system layer and are reached through the host seam.

use crate::buffer::{BufferId, BufferManager};
use neovm_value::heap_types::LispString;
use neovm_value::value::ValueKind;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// Convert a Lisp file-name string to an OS path at the real filesystem boundary.
///
/// GNU keeps file names as Lisp strings until ENCODE_FILE / platform I/O.
/// On Unix, preserve the original bytes exactly so raw unibyte file names
/// survive intact.
#[doc(hidden)]
pub fn lisp_file_name_to_path_buf(filename: &LispString) -> PathBuf {
    #[cfg(unix)]
    {
        PathBuf::from(std::ffi::OsString::from_vec(filename.as_bytes().to_vec()))
    }

    #[cfg(not(unix))]
    {
        let name = neovm_value::emacs_char::to_utf8_lossy(filename.as_bytes());
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

/// The file name whose lock governs BUFFER's first text change.
///
/// GNU hands BVAR (b, file_truename) to Flock_file untouched; a buffer without
/// both file-name slots has no lock target.
pub(crate) fn buffer_file_lock_target(
    buffers: &BufferManager,
    buffer_id: BufferId,
) -> Option<LispString> {
    let root_id = buffers.modified_state_root_id(buffer_id)?;
    let buffer = buffers.get(root_id)?;
    let file_name = buffer.buffer_local_value("buffer-file-name")?;
    let file_truename = buffer.buffer_local_value("buffer-file-truename")?;
    match (file_name.kind(), file_truename.kind()) {
        (ValueKind::String, ValueKind::String) => file_truename.as_lisp_string().cloned(),
        _ => None,
    }
}

/// The lock target of the current buffer, when the buffer is clean.
///
/// GNU `prepare_to_modify_buffer_1` locks only a clean file-visiting base
/// buffer. These native checks run before any before-change hook and never
/// invalidate text geometry; the caller publishes that boundary and then asks
/// the host to take the lock, which can run Lisp.
pub(crate) fn clean_current_buffer_file_lock_target(buffers: &BufferManager) -> Option<LispString> {
    let buffer_id = buffers.current_buffer_id()?;
    let root_id = buffers.modified_state_root_id(buffer_id)?;
    let buffer = buffers.get(root_id)?;
    if !buffer.modified_state_value().is_nil() {
        return None;
    }
    buffer_file_lock_target(buffers, buffer_id)
}
