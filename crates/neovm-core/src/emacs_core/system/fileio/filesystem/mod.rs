//! Host-neutral storage contract used by synchronous Lisp file primitives.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

mod browser_layout;
mod memory;
mod mounts;
mod native;
#[cfg(test)]
mod tests;
mod virtual_path;

pub use browser_layout::BrowserFileSystemLayout;
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

impl FileTimestamp {
    pub(crate) fn from_system_time(time: std::time::SystemTime) -> Option<Self> {
        match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => Some(Self {
                seconds: i64::try_from(duration.as_secs()).ok()?,
                nanoseconds: duration.subsec_nanos(),
            }),
            Err(error) => {
                let duration = error.duration();
                let seconds = i64::try_from(duration.as_secs()).ok()?;
                if duration.subsec_nanos() == 0 {
                    Some(Self {
                        seconds: -seconds,
                        nanoseconds: 0,
                    })
                } else {
                    Some(Self {
                        seconds: seconds.checked_neg()?.checked_sub(1)?,
                        nanoseconds: 1_000_000_000 - duration.subsec_nanos(),
                    })
                }
            }
        }
    }

    pub(crate) fn to_system_time(self) -> Option<std::time::SystemTime> {
        if self.seconds >= 0 {
            return std::time::UNIX_EPOCH.checked_add(std::time::Duration::new(
                self.seconds as u64,
                self.nanoseconds,
            ));
        }

        let seconds_before_epoch = self.seconds.unsigned_abs();
        let duration = if self.nanoseconds == 0 {
            std::time::Duration::from_secs(seconds_before_epoch)
        } else {
            std::time::Duration::new(
                seconds_before_epoch.checked_sub(1)?,
                1_000_000_000_u32.checked_sub(self.nanoseconds)?,
            )
        };
        std::time::UNIX_EPOCH.checked_sub(duration)
    }
}

/// Portable representation of GNU-visible file permission bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMode(u32);

impl FileMode {
    const PERMISSION_MASK: u32 = 0o7777;

    #[must_use]
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & Self::PERMISSION_MASK)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
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
    /// The entry is writable, or a missing entry can be created in its parent.
    WriteOrCreate,
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

/// Kind and initial contents of an exclusively created temporary entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporaryEntry<'a> {
    File(&'a [u8]),
    Directory,
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
    fn create_temporary(&self, path: &Path, entry: TemporaryEntry<'_>) -> io::Result<()> {
        match entry {
            TemporaryEntry::File(contents) => {
                self.write(
                    path,
                    contents,
                    WriteRequest {
                        mode: WriteMode::CreateNew,
                        sync: false,
                    },
                )?;
                Ok(())
            }
            TemporaryEntry::Directory => self.create_directory(path, false),
        }
    }
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_link(&self, _path: &Path) -> io::Result<PathBuf> {
        Err(io::Error::from(io::ErrorKind::InvalidInput))
    }
    fn mode(&self, _path: &Path, _follow_links: bool) -> io::Result<FileMode> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "file permission modes are unsupported by this storage backend",
        ))
    }
    fn set_mode(&self, _path: &Path, _mode: FileMode, _follow_links: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "changing file permission modes is unsupported by this storage backend",
        ))
    }
    fn set_times(
        &self,
        _path: &Path,
        _timestamp: Option<FileTimestamp>,
        _follow_links: bool,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "changing file timestamps is unsupported by this storage backend",
        ))
    }

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
