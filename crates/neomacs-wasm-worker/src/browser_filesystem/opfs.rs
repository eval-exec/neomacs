//! EditorFileSystem implementation backed by origin-private browser storage.

use super::protocol::{JAVASCRIPT_MAX_SAFE_INTEGER, complete, current_metadata, path_string, read_result_bytes};
use crate::browser_host;
use neovm_core::emacs_core::fileio::{
    AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, WriteMode, WriteRequest,
};
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// Persistent OPFS root supplied by the browser Worker.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BrowserOpfsFileSystem;

impl EditorFileSystem for BrowserOpfsFileSystem {
    fn attributes(
        &self,
        path: &Path,
    ) -> io::Result<neovm_core::emacs_core::fileio::FileAttributeSnapshot> {
        neovm_core::emacs_core::fileio::FileAttributeSnapshot::read_single_user_virtual(self, path)
    }

    fn metadata(&self, path: &Path, _follow_links: bool) -> io::Result<FileMetadata> {
        complete(browser_host::filesystem_stat(path_string(path)?))?;
        current_metadata()
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        match self.metadata(path, true) {
            Ok(metadata) => match mode {
                AccessMode::Existing(permissions) => permissions.is_satisfied_by(
                    true,
                    !metadata.readonly,
                    metadata.kind == FileEntryKind::Directory,
                ),
                AccessMode::Exists | AccessMode::Read | AccessMode::WriteOrCreate => true,
                AccessMode::Execute | AccessMode::ReadAndSearch => {
                    metadata.kind == FileEntryKind::Directory
                }
            },
            Err(error)
                if error.kind() == ErrorKind::NotFound && mode == AccessMode::WriteOrCreate =>
            {
                path.parent().is_some_and(|parent| {
                    self.metadata(parent, true)
                        .is_ok_and(|metadata| metadata.kind == FileEntryKind::Directory)
                })
            }
            Err(_) => false,
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
