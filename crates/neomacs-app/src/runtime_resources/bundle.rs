//! Authentication and structural validation shared by every resource host.

use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use crate::content_id::ContentId;

pub(super) const REQUIRED_DIRECTORIES: [&str; 2] = ["lisp", "etc"];
const OWNED_ARCHIVE_ROOTS: [&str; 4] = ["lisp", "etc", "leim", "info"];

pub(super) type RuntimeResourceBundleId = ContentId;

/// Compressed runtime archive paired with a canonical packaged identity.
///
/// Constructing this value validates the identity representation. Mounting or
/// extraction then streams the archive through SHA-256 before publishing any
/// resources.
#[derive(Debug)]
pub struct RuntimeResourceBundle<'a> {
    archive: &'a [u8],
    expected: RuntimeResourceBundleId,
}

impl<'a> RuntimeResourceBundle<'a> {
    /// Pair archive bytes with their canonical lowercase SHA-256 identity.
    pub fn from_assets(archive: &'a [u8], bundle_id: &[u8]) -> Result<Self, RuntimeResourceError> {
        Ok(Self {
            archive,
            expected: read_bundle_id(bundle_id)?,
        })
    }

    pub(super) fn archive(&self) -> &'a [u8] {
        self.archive
    }

    pub(super) fn expected(&self) -> &RuntimeResourceBundleId {
        &self.expected
    }
}

/// Failure to authenticate, validate, or provision packaged runtime resources.
#[derive(Debug)]
pub enum RuntimeResourceError {
    Io(io::Error),
    InvalidBundleId,
    InvalidExistingInstallation(PathBuf),
    UnownedArchivePath(PathBuf),
    DuplicateArchivePath(PathBuf),
    ConflictingArchivePath(PathBuf),
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
            Self::DuplicateArchivePath(path) => write!(
                formatter,
                "duplicate runtime resource path in packaged archive: {}",
                path.display()
            ),
            Self::ConflictingArchivePath(path) => write!(
                formatter,
                "runtime resource archive uses a file as a directory: {}",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArchiveEntryKind {
    Directory,
    File,
}

/// Validates that archive entries describe one coherent directory tree.
///
/// Tar entries are ordered, so a file/directory conflict must be rejected
/// whether the parent file or its descendant appears first.
#[derive(Default)]
struct ArchivePathRegistry {
    entries: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
    parents: BTreeSet<PathBuf>,
}

impl ArchivePathRegistry {
    fn insert(&mut self, path: &Path, kind: ArchiveEntryKind) -> Result<(), RuntimeResourceError> {
        if !self.entries.insert(path.to_owned()) {
            return Err(RuntimeResourceError::DuplicateArchivePath(path.to_owned()));
        }
        if kind == ArchiveEntryKind::File && self.parents.contains(path) {
            return Err(RuntimeResourceError::ConflictingArchivePath(
                path.to_owned(),
            ));
        }

        let ancestors = || {
            path.ancestors()
                .skip(1)
                .take_while(|ancestor| !ancestor.as_os_str().is_empty())
        };
        if let Some(file) = ancestors().find(|ancestor| self.files.contains(*ancestor)) {
            return Err(RuntimeResourceError::ConflictingArchivePath(
                file.to_owned(),
            ));
        }

        if kind == ArchiveEntryKind::File {
            self.files.insert(path.to_owned());
        }
        self.parents.extend(ancestors().map(Path::to_owned));
        Ok(())
    }
}

pub(super) struct ValidatedArchiveEntry {
    path: PathBuf,
    kind: ArchiveEntryKind,
    size: u64,
}

impl ValidatedArchiveEntry {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) const fn kind(&self) -> ArchiveEntryKind {
        self.kind
    }

    pub(super) const fn size(&self) -> u64 {
        self.size
    }
}

pub(super) fn read_bundle_id(
    source: impl Read,
) -> Result<RuntimeResourceBundleId, RuntimeResourceError> {
    let mut text = String::new();
    source.take(128).read_to_string(&mut text)?;
    RuntimeResourceBundleId::parse(text.trim()).map_err(|_| RuntimeResourceError::InvalidBundleId)
}

/// Authenticate one compressed archive and visit only validated entries.
///
/// The compressed input is streamed through SHA-256 as it is decoded. Entry
/// paths and types are checked before the visitor sees them, keeping native
/// extraction and in-memory mounts on exactly the same trust boundary.
pub(super) fn visit_authenticated_archive(
    source: impl Read,
    expected: &RuntimeResourceBundleId,
    mut visit: impl FnMut(&ValidatedArchiveEntry, &mut dyn Read) -> Result<(), RuntimeResourceError>,
) -> Result<(), RuntimeResourceError> {
    let digesting = DigestingReader::new(source);
    let decoder = GzDecoder::new(digesting);
    let mut archive = tar::Archive::new(decoder);
    let mut archive_paths = ArchivePathRegistry::default();
    let mut seen_required_directories = [false; REQUIRED_DIRECTORIES.len()];

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        validate_archive_path(&path)?;
        let entry_type = entry.header().entry_type();
        let kind = if entry_type.is_dir() {
            ArchiveEntryKind::Directory
        } else if entry_type.is_file() {
            ArchiveEntryKind::File
        } else {
            return Err(RuntimeResourceError::UnsupportedArchiveEntry(path));
        };
        archive_paths.insert(&path, kind)?;
        mark_required_directory(&path, kind, &mut seen_required_directories);
        let validated = ValidatedArchiveEntry {
            path,
            kind,
            size: entry.size(),
        };
        visit(&validated, &mut entry)?;
    }

    // Tar stops at its logical end markers. Consume the gzip footer and any
    // remaining compressed bytes before finalizing the archive identity.
    let mut decoder = archive.into_inner();
    io::copy(&mut decoder, &mut io::sink())?;
    let actual = decoder.into_inner().finish_hex();
    if actual != expected.as_str() {
        return Err(RuntimeResourceError::ArchiveDigestMismatch {
            expected: expected.as_str().to_owned(),
            actual,
        });
    }

    for (index, directory) in REQUIRED_DIRECTORIES.iter().enumerate() {
        if !seen_required_directories[index] {
            return Err(RuntimeResourceError::MissingRequiredDirectory(directory));
        }
    }
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

fn mark_required_directory(
    path: &Path,
    kind: ArchiveEntryKind,
    seen: &mut [bool; REQUIRED_DIRECTORIES.len()],
) {
    let mut components = path.components();
    let Some(Component::Normal(root)) = components.next() else {
        return;
    };
    let names_a_tree = kind == ArchiveEntryKind::Directory || components.next().is_some();
    if !names_a_tree {
        return;
    }
    for (index, required) in REQUIRED_DIRECTORIES.iter().enumerate() {
        if root == std::ffi::OsStr::new(required) {
            seen[index] = true;
        }
    }
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
