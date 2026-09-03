//! Native operating-system implementation of editor filesystem semantics.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::{
    AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, FileMode, FileTimestamp,
    TemporaryEntry, WriteMode, WriteRequest,
};

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
        modified: metadata
            .modified()
            .ok()
            .and_then(FileTimestamp::from_system_time),
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

            let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
                return false;
            };
            let native_mode = match mode {
                AccessMode::Exists => libc::F_OK,
                AccessMode::Read => libc::R_OK,
                AccessMode::WriteOrCreate => libc::W_OK,
                AccessMode::Execute => libc::X_OK,
                AccessMode::ReadAndSearch => libc::R_OK | libc::X_OK,
            };
            if unsafe { libc::access(c_path.as_ptr(), native_mode) } == 0 {
                return true;
            }
            if mode != AccessMode::WriteOrCreate
                || io::Error::last_os_error().kind() != io::ErrorKind::NotFound
            {
                return false;
            }
            let Some(parent) = path.parent() else {
                return false;
            };
            let Ok(parent) = CString::new(parent.as_os_str().as_bytes()) else {
                return false;
            };
            unsafe { libc::access(parent.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
        }
        #[cfg(not(unix))]
        {
            match mode {
                AccessMode::Exists => path.exists(),
                AccessMode::Read => self.metadata(path, true).is_ok(),
                AccessMode::WriteOrCreate => {
                    if path.exists() {
                        OpenOptions::new().write(true).open(path).is_ok()
                    } else {
                        path.parent().is_some_and(|parent| {
                            self.metadata(parent, true)
                                .is_ok_and(|metadata| metadata.kind == FileEntryKind::Directory)
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

    fn mode(&self, path: &Path, follow_links: bool) -> io::Result<FileMode> {
        let metadata = if follow_links {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            Ok(FileMode::from_bits_truncate(metadata.permissions().mode()))
        }
        #[cfg(not(unix))]
        {
            Ok(FileMode::from_bits_truncate(
                if metadata.permissions().readonly() {
                    0o444
                } else {
                    0o644
                },
            ))
        }
    }

    fn set_mode(&self, path: &Path, mode: FileMode, follow_links: bool) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            use std::os::unix::fs::PermissionsExt;

            if !follow_links {
                let path =
                    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|error| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("embedded NUL in file name: {error}"),
                        )
                    })?;
                let result = unsafe {
                    libc::fchmodat(
                        libc::AT_FDCWD,
                        path.as_ptr(),
                        mode.bits() as libc::mode_t,
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                return if result == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                };
            }
            fs::set_permissions(path, fs::Permissions::from_mode(mode.bits()))
        }
        #[cfg(not(unix))]
        {
            let _ = follow_links;
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_readonly(mode.bits() & 0o222 == 0);
            fs::set_permissions(path, permissions)
        }
    }

    fn set_times(
        &self,
        path: &Path,
        timestamp: Option<FileTimestamp>,
        follow_links: bool,
    ) -> io::Result<()> {
        if !follow_links {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;

                let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL in file name")
                })?;
                let mut times = [
                    libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    },
                    libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    },
                ];
                if let Some(timestamp) = timestamp {
                    for time in &mut times {
                        time.tv_sec = timestamp.seconds as libc::time_t;
                        time.tv_nsec = timestamp.nanoseconds as libc::c_long;
                    }
                } else {
                    for time in &mut times {
                        time.tv_nsec = libc::UTIME_NOW as libc::c_long;
                    }
                }
                let result = unsafe {
                    libc::utimensat(
                        libc::AT_FDCWD,
                        path.as_ptr(),
                        times.as_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                return if result == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                };
            }
            #[cfg(not(unix))]
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "nofollow set-file-times is unsupported on this platform",
                ));
            }
        }

        let time = match timestamp {
            Some(timestamp) => timestamp.to_system_time().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "file timestamp is out of range",
                )
            })?,
            None => std::time::SystemTime::now(),
        };
        let times = fs::FileTimes::new().set_accessed(time).set_modified(time);
        fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .set_times(times)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        fs::read_link(path)
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
