//! Atomic extraction of packaged runtime resources into native host storage.

use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::{RUNTIME_RESOURCE_ARCHIVE_ASSET, RUNTIME_RESOURCE_ID_ASSET};

const READY_FILE_NAME: &str = ".neomacs-runtime-ready";
const REQUIRED_DIRECTORIES: [&str; 2] = ["lisp", "etc"];
const OWNED_ARCHIVE_ROOTS: [&str; 4] = ["lisp", "etc", "leim", "info"];
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether this process installed a resource tree or found it ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeResourceInstall {
    Installed,
    Reused,
}

/// Failure to validate or install packaged runtime resources.
#[derive(Debug)]
pub enum RuntimeResourceError {
    Io(io::Error),
    InvalidBundleId,
    InvalidExistingInstallation(PathBuf),
    UnownedArchivePath(PathBuf),
    UnsupportedArchiveEntry(PathBuf),
    ArchiveDigestMismatch { expected: String, actual: String },
    MissingRequiredDirectory(&'static str),
}

impl Display for RuntimeResourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, formatter),
            Self::InvalidBundleId => write!(
                formatter,
                "runtime resource bundle ID must be exactly 64 lowercase hexadecimal digits"
            ),
            Self::InvalidExistingInstallation(path) => write!(
                formatter,
                "runtime resource destination exists but is not a complete installation: {}",
                path.display()
            ),
            Self::UnownedArchivePath(path) => write!(
                formatter,
                "unowned runtime resource path in packaged archive: {}",
                path.display()
            ),
            Self::UnsupportedArchiveEntry(path) => write!(
                formatter,
                "unsupported runtime resource archive entry: {}",
                path.display()
            ),
            Self::ArchiveDigestMismatch { expected, actual } => write!(
                formatter,
                "runtime resource archive digest does not match bundle ID: expected {expected}, got {actual}"
            ),
            Self::MissingRequiredDirectory(directory) => write!(
                formatter,
                "runtime resource archive does not contain required {directory}/ directory"
            ),
        }
    }
}

impl std::error::Error for RuntimeResourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for RuntimeResourceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimeResourceBundleId(String);

impl RuntimeResourceBundleId {
    fn parse(value: &str) -> Result<Self, RuntimeResourceError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RuntimeResourceError::InvalidBundleId);
        }
        Ok(Self(value.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
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

fn read_bundle_id(source: impl Read) -> Result<RuntimeResourceBundleId, RuntimeResourceError> {
    let mut text = String::new();
    source.take(128).read_to_string(&mut text)?;
    RuntimeResourceBundleId::parse(text.trim())
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
    let digesting = DigestingReader::new(source);
    let decoder = GzDecoder::new(digesting);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        extract_entry(entry?, destination)?;
    }

    // Consume the gzip footer and any remaining compressed bytes before
    // finalizing the digest; tar stops logically at its end markers.
    let mut decoder = archive.into_inner();
    io::copy(&mut decoder, &mut io::sink())?;
    let actual = decoder.into_inner().finish_hex();
    if actual != expected.as_str() {
        return Err(RuntimeResourceError::ArchiveDigestMismatch {
            expected: expected.as_str().to_owned(),
            actual,
        });
    }
    Ok(())
}

fn extract_entry<R: Read>(
    mut entry: tar::Entry<'_, R>,
    destination: &Path,
) -> Result<(), RuntimeResourceError> {
    let relative = entry.path()?.into_owned();
    validate_archive_path(&relative)?;
    let output = destination.join(&relative);
    let entry_type = entry.header().entry_type();
    if entry_type.is_dir() {
        std::fs::create_dir_all(&output)?;
        return Ok(());
    }
    if !entry_type.is_file() {
        return Err(RuntimeResourceError::UnsupportedArchiveEntry(relative));
    }

    let parent = output
        .parent()
        .ok_or_else(|| RuntimeResourceError::UnownedArchivePath(relative.clone()))?;
    std::fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    io::copy(&mut entry, &mut file)?;
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), RuntimeResourceError> {
    let mut components = path.components();
    let Some(Component::Normal(root)) = components.next() else {
        return Err(RuntimeResourceError::UnownedArchivePath(path.to_owned()));
    };
    if !OWNED_ARCHIVE_ROOTS
        .iter()
        .any(|owned| root == std::ffi::OsStr::new(owned))
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeResourceError::UnownedArchivePath(path.to_owned()));
    }
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

struct DigestingReader<R> {
    inner: R,
    digest: Sha256,
}

impl<R> DigestingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
        }
    }

    fn finish_hex(self) -> String {
        self.digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

impl<R: Read> Read for DigestingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.digest.update(&buffer[..read]);
        Ok(read)
    }
}
