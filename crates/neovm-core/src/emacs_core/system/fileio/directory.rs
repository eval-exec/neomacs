//! Shared byte-preserving directory enumeration for Lisp file operations.

use super::{EditorFileSystem, lisp_file_name_to_path_buf, path_to_lisp_file_name};
use crate::heap_types::LispString;
use std::io;
use std::path::Path;

/// Return raw entry names, including dot entries, in backend traversal order.
/// Callers apply GNU filename decoding before matching or returning names.
pub(crate) fn read_directory_names_lisp(
    dir: &LispString,
    filesystem: &dyn EditorFileSystem,
) -> io::Result<Vec<LispString>> {
    let entries = filesystem.read_directory(&lisp_file_name_to_path_buf(dir))?;
    let mut names = vec![
        LispString::from_unibyte(b".".to_vec()),
        LispString::from_unibyte(b"..".to_vec()),
    ];
    names.extend(
        entries
            .into_iter()
            .map(|entry| path_to_lisp_file_name(Path::new(&entry))),
    );
    Ok(names)
}
