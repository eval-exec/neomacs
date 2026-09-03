//! `EditorFileSystem` adapter for origin-private browser storage.

use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use neovm_core::emacs_core::fileio::{
    AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, WriteMode, WriteRequest,
};

use crate::browser_host;

const MAX_RESULT_BYTES: usize = 512 * 1024 * 1024;
const JAVASCRIPT_MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
enum HostStatus {
    Ok = 0,
    NotFound = 1,
    AlreadyExists = 2,
    PermissionDenied = 3,
    QuotaExceeded = 4,
    InvalidInput = 5,
    IsDirectory = 6,
    NotADirectory = 7,
    DirectoryNotEmpty = 8,
    Unsupported = 9,
    Other = 10,
}

impl HostStatus {
    fn parse(value: u32) -> io::Result<Self> {
        match value {
            0 => Ok(Self::Ok),
            1 => Ok(Self::NotFound),
            2 => Ok(Self::AlreadyExists),
            3 => Ok(Self::PermissionDenied),
            4 => Ok(Self::QuotaExceeded),
            5 => Ok(Self::InvalidInput),
            6 => Ok(Self::IsDirectory),
            7 => Ok(Self::NotADirectory),
            8 => Ok(Self::DirectoryNotEmpty),
            9 => Ok(Self::Unsupported),
            10 => Ok(Self::Other),
            unknown => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("browser filesystem returned unknown status {unknown}"),
            )),
        }
    }

    fn error_kind(self) -> ErrorKind {
        match self {
            Self::Ok => ErrorKind::Other,
            Self::NotFound => ErrorKind::NotFound,
            Self::AlreadyExists => ErrorKind::AlreadyExists,
            Self::PermissionDenied => ErrorKind::PermissionDenied,
            Self::QuotaExceeded => ErrorKind::StorageFull,
            Self::InvalidInput => ErrorKind::InvalidInput,
            Self::IsDirectory => ErrorKind::IsADirectory,
            Self::NotADirectory => ErrorKind::NotADirectory,
            Self::DirectoryNotEmpty => ErrorKind::DirectoryNotEmpty,
            Self::Unsupported => ErrorKind::Unsupported,
            Self::Other => ErrorKind::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
enum HostFileKind {
    File = 1,
    Directory = 2,
    SymbolicLink = 3,
    Other = 4,
}

impl HostFileKind {
    fn current() -> io::Result<Self> {
        match browser_host::filesystem_result_kind() {
            1 => Ok(Self::File),
            2 => Ok(Self::Directory),
            3 => Ok(Self::SymbolicLink),
            4 => Ok(Self::Other),
            unknown => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("browser filesystem returned unknown entry kind {unknown}"),
            )),
        }
    }
}

/// Persistent OPFS root supplied by the browser Worker.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BrowserOpfsFileSystem;

fn path_string(path: &Path) -> io::Result<&str> {
    let path = path.to_str().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "browser filesystem paths must be valid UTF-8",
        )
    })?;
    u32::try_from(path.len()).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "browser filesystem path exceeds the Wasm32 transfer limit",
        )
    })?;
    Ok(path)
}

fn complete(status: u32) -> io::Result<()> {
    let status = HostStatus::parse(status)?;
    if status == HostStatus::Ok {
        return Ok(());
    }
    let message = browser_host::filesystem_result_error()
        .unwrap_or_else(|protocol_error| format!("{status:?}: {protocol_error}"));
    Err(io::Error::new(status.error_kind(), message))
}

fn result_len() -> io::Result<u64> {
    let length = browser_host::filesystem_result_len();
    if !length.is_finite()
        || length < 0.0
        || length.fract() != 0.0
        || length > JAVASCRIPT_MAX_SAFE_INTEGER as f64
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("invalid browser filesystem result length {length}"),
        ));
    }
    Ok(length as u64)
}

fn current_metadata() -> io::Result<FileMetadata> {
    let kind = match HostFileKind::current()? {
        HostFileKind::File => FileEntryKind::File,
        HostFileKind::Directory => FileEntryKind::Directory,
        HostFileKind::SymbolicLink => FileEntryKind::SymbolicLink,
        HostFileKind::Other => FileEntryKind::Other,
    };
    let modified = browser_host::filesystem_result_modified_milliseconds();
    let modified = (modified.is_finite() && modified >= 0.0)
        .then(|| Duration::try_from_secs_f64(modified / 1000.0).ok())
        .flatten()
        .and_then(|duration| UNIX_EPOCH.checked_add(duration));
    Ok(FileMetadata {
        kind,
        len: result_len()?,
        modified,
        readonly: false,
    })
}

fn read_result_bytes() -> io::Result<Vec<u8>> {
    let length = usize::try_from(result_len()?).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidData,
            "browser filesystem result exceeds the target address space",
        )
    })?;
    if length > MAX_RESULT_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("browser filesystem result is {length} bytes; maximum is {MAX_RESULT_BYTES}"),
        ));
    }
    browser_host::filesystem_result_bytes(length)
        .map_err(|message| io::Error::new(ErrorKind::InvalidData, message))
}

impl EditorFileSystem for BrowserOpfsFileSystem {
    fn metadata(&self, path: &Path, _follow_links: bool) -> io::Result<FileMetadata> {
        complete(browser_host::filesystem_stat(path_string(path)?))?;
        current_metadata()
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        let Ok(metadata) = self.metadata(path, true) else {
            return false;
        };
        match mode {
            AccessMode::Exists | AccessMode::Read | AccessMode::Write => true,
            AccessMode::Execute | AccessMode::ReadAndSearch => {
                metadata.kind == FileEntryKind::Directory
            }
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        complete(browser_host::filesystem_read(path_string(path)?))?;
        read_result_bytes()
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>> {
        complete(browser_host::filesystem_read_directory(path_string(path)?))?;
        let bytes = read_result_bytes()?;
        serde_json::from_slice::<Vec<String>>(&bytes)
            .map(|names| names.into_iter().map(OsString::from).collect())
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
    }

    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata> {
        let (mode, offset) = match request.mode {
            WriteMode::Truncate => (1, 0),
            WriteMode::Append => (2, 0),
            WriteMode::At(offset) => (3, offset),
            WriteMode::CreateNew => (4, 0),
        };
        u32::try_from(contents.len()).map_err(|_| {
            io::Error::new(
                ErrorKind::InvalidInput,
                "browser filesystem write exceeds the Wasm32 transfer limit",
            )
        })?;
        if offset > JAVASCRIPT_MAX_SAFE_INTEGER {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "browser filesystem offset exceeds JavaScript's exact integer range",
            ));
        }
        complete(browser_host::filesystem_write(
            path_string(path)?,
            contents,
            mode,
            offset,
            request.sync,
        ))?;
        current_metadata()
    }

    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()> {
        complete(browser_host::filesystem_create_directory(
            path_string(path)?,
            parents,
        ))
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        complete(browser_host::filesystem_remove_file(path_string(path)?))
    }

    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        complete(browser_host::filesystem_remove_directory(
            path_string(path)?,
            recursive,
        ))
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        complete(browser_host::filesystem_rename(
            path_string(from)?,
            path_string(to)?,
            replace,
        ))
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        complete(browser_host::filesystem_canonicalize(path_string(path)?))?;
        String::from_utf8(read_result_bytes()?)
            .map(PathBuf::from)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
    }
}
