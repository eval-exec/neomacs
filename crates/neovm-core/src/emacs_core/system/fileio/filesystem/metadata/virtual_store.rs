//! Explicit single-user semantics for stores without POSIX ownership or hard links.

use super::super::EditorFileSystem;
use super::{FileAttributeSnapshot, FilePrincipal};
use std::io;
use std::path::Path;

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
