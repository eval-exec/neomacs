//! Host-neutral storage contract used by synchronous Lisp file primitives.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

mod memory;
mod mounts;
mod native;
#[cfg(test)]
mod tests;
mod virtual_path;

pub use memory::MemoryFileSystem;
pub use mounts::MountTableFileSystem;
pub use native::NativeFileSystem;

pub(crate) fn default_editor_file_system() -> Box<dyn EditorFileSystem> {
    std::cfg_select! {
        target_family = "wasm" => {
            Box::new(MemoryFileSystem::new())
        }
        _ => {
            Box::new(NativeFileSystem)
        }
    }
}

/// Kind of one filesystem entry without leaking platform metadata types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileEntryKind {
    File,
    Directory,
    SymbolicLink,
    Other,
}

/// Host-neutral wall-clock timestamp used by filesystem metadata.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FileTimestamp {
    pub seconds: i64,
    pub nanoseconds: u32,
}

/// Metadata shared by native and sandboxed storage adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub kind: FileEntryKind,
    pub len: u64,
    pub modified: Option<FileTimestamp>,
    pub readonly: bool,
}

/// Access predicate requested by a Lisp filesystem primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessMode {
    Exists,
    Read,
    Write,
    Execute,
    ReadAndSearch,
}

/// Placement semantics for one complete file write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    Truncate,
    Append,
    At(u64),
    CreateNew,
}

/// One complete, synchronous editor write operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteRequest {
    pub mode: WriteMode,
    pub sync: bool,
}

/// Filesystem operations whose synchronous semantics are observable by Lisp.
///
/// Browser implementations may suspend the Wasm stack while the Worker awaits
/// an asynchronous host API, but a caller always observes one completed
/// operation or one typed `io::Error`.
pub trait EditorFileSystem {
    fn metadata(&self, path: &Path, follow_links: bool) -> io::Result<FileMetadata>;
    fn access(&self, path: &Path, mode: AccessMode) -> bool;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>>;
    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata>;
    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    fn same_file(&self, left: &Path, right: &Path) -> io::Result<bool> {
        Ok(self.canonicalize(left)? == self.canonicalize(right)?)
    }

    fn copy_file(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        let contents = self.read(from)?;
        self.write(
            to,
            &contents,
            WriteRequest {
                mode: if replace {
                    WriteMode::Truncate
                } else {
                    WriteMode::CreateNew
                },
                sync: false,
            },
        )?;
        Ok(())
    }
}
