//! Atomic extraction of packaged runtime resources into native host storage.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{RUNTIME_RESOURCE_ARCHIVE_ASSET, RUNTIME_RESOURCE_ID_ASSET};
use super::bundle::{
    ArchiveEntryKind, REQUIRED_DIRECTORIES, RuntimeResourceBundleId, RuntimeResourceError,
    ValidatedArchiveEntry, read_bundle_id, visit_authenticated_archive,
};

const READY_FILE_NAME: &str = ".neomacs-runtime-ready";
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether this process installed a resource tree or found it ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeResourceInstall {
    Installed,
    Reused,
}

/// A complete runtime resource root in app-private native storage.
#[derive(Debug)]
pub struct RuntimeResourceRoot {
    path: PathBuf,
    install: RuntimeResourceInstall,
}

impl RuntimeResourceRoot {
    /// Install the packaged runtime tree selected by its content ID.
    ///
    /// Only the small ID asset is opened on a reuse. A first installation
    /// streams and authenticates the gzip archive while extracting it into a
    /// private staging directory, then publishes the complete tree with one
    /// rename. The archive may own only `lisp/`, `etc/`, `leim/`, and `info/`.
    pub fn prepare<R: Read>(
        storage: &Path,
        mut open_asset: impl FnMut(&str) -> io::Result<R>,
    ) -> Result<Self, RuntimeResourceError> {
        let id = read_bundle_id(open_asset(RUNTIME_RESOURCE_ID_ASSET)?)?;
        std::fs::create_dir_all(storage)?;
        let destination = storage.join(id.as_str());
        if is_complete_installation(&destination, &id) {
            return Ok(Self {
                path: destination,
                install: RuntimeResourceInstall::Reused,
            });
        }
        if destination.exists() {
            return Err(RuntimeResourceError::InvalidExistingInstallation(
                destination,
            ));
        }

        let staging = create_staging_directory(storage, &id)?;
        let result = (|| {
            let archive = open_asset(RUNTIME_RESOURCE_ARCHIVE_ASSET)?;
            extract_authenticated_archive(archive, &staging, &id)?;
            validate_required_directories(&staging)?;
            write_ready_file(&staging, &id)?;
            publish_staging_directory(&staging, &destination, &id)
        })();
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }

        let install = result?;
        Ok(Self {
            path: destination,
            install,
        })
    }

    /// Parent of the installed `lisp/`, `etc/`, `leim/`, and `info/` trees.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn install(&self) -> RuntimeResourceInstall {
        self.install
    }
}

fn create_staging_directory(
    storage: &Path,
    id: &RuntimeResourceBundleId,
) -> Result<PathBuf, RuntimeResourceError> {
    loop {
        let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = storage.join(format!(
            ".{}.{}.{}.partial",
            id.as_str(),
            std::process::id(),
            sequence
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn extract_authenticated_archive(
    source: impl Read,
    destination: &Path,
    expected: &RuntimeResourceBundleId,
) -> Result<(), RuntimeResourceError> {
    visit_authenticated_archive(source, expected, |entry, contents| {
        extract_entry(entry, contents, destination)
    })
}

fn extract_entry(
    entry: &ValidatedArchiveEntry,
    contents: &mut dyn Read,
    destination: &Path,
) -> Result<(), RuntimeResourceError> {
    let output = destination.join(entry.path());
    if entry.kind() == ArchiveEntryKind::Directory {
        std::fs::create_dir_all(&output)?;
        return Ok(());
    }

    let parent = output
        .parent()
        .ok_or_else(|| RuntimeResourceError::UnownedArchivePath(entry.path().to_owned()))?;
    std::fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    io::copy(contents, &mut file)?;
    Ok(())
}

fn validate_required_directories(path: &Path) -> Result<(), RuntimeResourceError> {
    for directory in REQUIRED_DIRECTORIES {
        if !path.join(directory).is_dir() {
            return Err(RuntimeResourceError::MissingRequiredDirectory(directory));
        }
    }
    Ok(())
}

fn write_ready_file(
    destination: &Path,
    id: &RuntimeResourceBundleId,
) -> Result<(), RuntimeResourceError> {
    let path = destination.join(READY_FILE_NAME);
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(file, "{}", id.as_str())?;
    file.sync_all()?;
    sync_directory(destination)?;
    Ok(())
}

fn is_complete_installation(path: &Path, id: &RuntimeResourceBundleId) -> bool {
    REQUIRED_DIRECTORIES
        .iter()
        .all(|directory| path.join(directory).is_dir())
        && std::fs::read_to_string(path.join(READY_FILE_NAME))
            .is_ok_and(|ready| ready.trim() == id.as_str())
}

fn publish_staging_directory(
    staging: &Path,
    destination: &Path,
    id: &RuntimeResourceBundleId,
) -> Result<RuntimeResourceInstall, RuntimeResourceError> {
    match std::fs::rename(staging, destination) {
        Ok(()) => {
            if let Some(parent) = destination.parent() {
                sync_directory(parent)?;
            }
            Ok(RuntimeResourceInstall::Installed)
        }
        Err(_) if is_complete_installation(destination, id) => Ok(RuntimeResourceInstall::Reused),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}
