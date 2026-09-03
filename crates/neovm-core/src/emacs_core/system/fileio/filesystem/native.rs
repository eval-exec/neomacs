//! Native operating-system implementation of editor filesystem semantics.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::{
    AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, FileTimestamp, TemporaryEntry,
    WriteMode, WriteRequest,
};

fn timestamp_from_native(time: std::time::SystemTime) -> Option<FileTimestamp> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => Some(FileTimestamp {
            seconds: i64::try_from(duration.as_secs()).ok()?,
            nanoseconds: duration.subsec_nanos(),
        }),
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).ok()?;
            if duration.subsec_nanos() == 0 {
                Some(FileTimestamp {
                    seconds: -seconds,
                    nanoseconds: 0,
                })
            } else {
                Some(FileTimestamp {
                    seconds: seconds.checked_neg()?.checked_sub(1)?,
                    nanoseconds: 1_000_000_000 - duration.subsec_nanos(),
                })
            }
        }
    }
}

/// Direct access to the current process's native filesystem namespace.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFileSystem;

fn metadata_from_native(metadata: fs::Metadata) -> FileMetadata {
    let file_type = metadata.file_type();
    let kind = if file_type.is_file() {
        FileEntryKind::File
    } else if file_type.is_dir() {
        FileEntryKind::Directory
    } else if file_type.is_symlink() {
        FileEntryKind::SymbolicLink
    } else {
        FileEntryKind::Other
    };
    FileMetadata {
        kind,
        len: metadata.len(),
        modified: metadata.modified().ok().and_then(timestamp_from_native),
        readonly: metadata.permissions().readonly(),
    }
}

impl EditorFileSystem for NativeFileSystem {
    fn metadata(&self, path: &Path, follow_links: bool) -> io::Result<FileMetadata> {
        let metadata = if follow_links {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }?;
        Ok(metadata_from_native(metadata))
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        if mode == AccessMode::Exists {
            return path.exists();
        }
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
                return false;
            };
            let mode = match mode {
                AccessMode::Exists => libc::F_OK,
                AccessMode::Read => libc::R_OK,
                AccessMode::Write => libc::W_OK,
                AccessMode::Execute => libc::X_OK,
                AccessMode::ReadAndSearch => libc::R_OK | libc::X_OK,
            };
            unsafe { libc::access(path.as_ptr(), mode) == 0 }
        }
        #[cfg(not(unix))]
        {
            match mode {
                AccessMode::Exists => path.exists(),
                AccessMode::Read => self.metadata(path, true).is_ok(),
                AccessMode::Write => {
                    if path.exists() {
                        OpenOptions::new().write(true).open(path).is_ok()
                    } else {
                        path.parent().is_some_and(|parent| {
                            self.metadata(parent, true).is_ok_and(|metadata| {
                                metadata.kind == FileEntryKind::Directory && !metadata.readonly
                            })
                        })
                    }
                }
                AccessMode::Execute | AccessMode::ReadAndSearch => path.is_dir(),
            }
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect()
    }

    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata> {
        let mut options = OpenOptions::new();
        options.write(true);
        match request.mode {
            WriteMode::Truncate => {
                options.create(true).truncate(true);
            }
            WriteMode::Append => {
                options.create(true).append(true);
            }
            WriteMode::At(_) => {
                options.create(true);
            }
            WriteMode::CreateNew => {
                options.create_new(true);
            }
        }
        let mut file = options.open(path)?;
        if let WriteMode::At(offset) = request.mode {
            file.seek(SeekFrom::Start(offset))?;
        }
        file.write_all(contents)?;
        if request.sync {
            file.sync_all()?;
        }
        drop(file);
        self.metadata(path, true)
    }

    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()> {
        if parents {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        }
    }

    fn create_temporary(&self, path: &Path, entry: TemporaryEntry<'_>) -> io::Result<()> {
        match entry {
            TemporaryEntry::File(contents) => {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(path)?;
                file.write_all(contents)
            }
            TemporaryEntry::Directory => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    fs::DirBuilder::new().mode(0o700).create(path)
                }
                #[cfg(not(unix))]
                {
                    fs::create_dir(path)
                }
            }
        }
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        if !replace && fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        super::super::rename_path_with_cross_device_fallback(from, to, replace, |from, to| {
            fs::rename(from, to)
        })
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn same_file(&self, left: &Path, right: &Path) -> io::Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let left = fs::metadata(left)?;
            let right = fs::metadata(right)?;
            Ok(left.dev() == right.dev() && left.ino() == right.ino())
        }
        #[cfg(not(unix))]
        {
            Ok(fs::canonicalize(left)? == fs::canonicalize(right)?)
        }
    }

    fn copy_file(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        if !replace && fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        fs::copy(from, to).map(|_| ())
    }
}
