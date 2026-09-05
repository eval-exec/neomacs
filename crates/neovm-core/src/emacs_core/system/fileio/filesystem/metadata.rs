//! Detailed non-following metadata, independent of Lisp and native stat types.

use super::{EditorFileSystem, FileEntryKind, FileMode, FileTimestamp};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileAttributeType {
    Directory,
    SymbolicLink(PathBuf),
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePrincipal {
    pub id: i64,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    pub inode: u64,
    pub device: u64,
}

/// One attribute observation. Unknown host concepts remain `None`, not fake
/// process ownership, epoch timestamps, or a shared zero inode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileAttributeSnapshot {
    pub kind: FileAttributeType,
    pub links: Option<u64>,
    pub user: Option<FilePrincipal>,
    pub group: Option<FilePrincipal>,
    pub accessed: Option<FileTimestamp>,
    pub modified: Option<FileTimestamp>,
    pub changed: Option<FileTimestamp>,
    pub len: u64,
    pub mode: Option<FileMode>,
    pub identity: Option<FileIdentity>,
    pub legacy_group_change: bool,
}

impl FileAttributeSnapshot {
    /// Attributes for a single-user virtual store without hard links.
    /// ID zero belongs to this virtual namespace, not the host's root user.
    /// Only stores implementing these semantics opt in; native observations
    /// and the generic fallback retain their own ownership and link counts.
    pub fn read_single_user_virtual<F: EditorFileSystem + ?Sized>(
        fs: &F,
        path: &Path,
    ) -> io::Result<Self> {
        let mut attributes = Self::read(fs, path)?;
        let principal = FilePrincipal { id: 0, name: Some("virtual".to_owned()) };
        attributes.user = Some(principal.clone());
        attributes.group = Some(principal);
        // Each entry has one name. These stores do not model POSIX directory
        // hard links either, so do not invent a Unix `2 + subdirectories` count.
        attributes.links = Some(1);
        Ok(attributes)
    }

    /// Portable fallback for stores without stat-style ownership and identity.
    pub(crate) fn read<F: EditorFileSystem + ?Sized>(fs: &F, path: &Path) -> io::Result<Self> {
        let metadata = fs.metadata(path, false)?;
        let kind = match metadata.kind {
            FileEntryKind::Directory => FileAttributeType::Directory,
            FileEntryKind::SymbolicLink => FileAttributeType::SymbolicLink(fs.read_link(path)?),
            FileEntryKind::File | FileEntryKind::Other => FileAttributeType::Other,
        };
        let mode = match fs.mode(path, false) {
            Ok(mode) => Some(mode),
            Err(error) if error.kind() == io::ErrorKind::Unsupported => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            kind,
            links: None,
            user: None,
            group: None,
            accessed: None,
            modified: metadata.modified,
            changed: None,
            len: metadata.len,
            mode,
            identity: None,
            legacy_group_change: false,
        })
    }
}
