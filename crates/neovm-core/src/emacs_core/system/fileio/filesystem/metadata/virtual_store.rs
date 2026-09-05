//! Explicit single-user semantics for stores without POSIX ownership or hard links.

use super::super::EditorFileSystem;
use super::super::{FileEntryKind, FileMetadata, FileMode};
use super::{FileAttributeSnapshot, FilePrincipal};
use std::io;
use std::path::Path;

impl FileMode {
    /// Effective access for the sole virtual user, not host POSIX permissions.
    /// No execute permission for files: these stores cannot launch programs.
    /// Directories are searchable; immutable entries are never writable.
    pub fn single_user_virtual(metadata: FileMetadata) -> Self {
        let write = if metadata.readonly { 0 } else { 0o200 };
        let search = if metadata.kind == FileEntryKind::Directory {
            0o100
        } else {
            0
        };
        Self::from_bits_truncate(0o400 | write | search)
    }
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
        let principal = FilePrincipal {
            id: 0,
            name: Some("virtual".to_owned()),
        };
        attributes.user = Some(principal.clone());
        attributes.group = Some(principal);
        // Each entry has one name. These stores do not model POSIX directory
        // hard links either, so do not invent a Unix `2 + subdirectories` count.
        attributes.links = Some(1);
        Ok(attributes)
    }
}
