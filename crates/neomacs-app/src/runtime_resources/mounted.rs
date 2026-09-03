//! Read-only runtime resources mounted in an evaluator's virtual root.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use neovm_core::emacs_core::fileio::RuntimeResourceStore;

use super::bundle::{
    ArchiveEntryKind, RuntimeResourceError, read_bundle_id, visit_authenticated_archive,
};

/// An authenticated runtime resource bundle expanded into linear memory.
///
/// This is intended for hosts without a native filesystem, principally a
/// browser Worker. It owns only immutable product resources; user documents
/// belong to a separate host storage capability.
#[derive(Debug)]
pub struct MountedRuntimeResources {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl MountedRuntimeResources {
    /// Authenticate and mount a deterministic runtime resource bundle.
    pub fn from_bundle(
        mount_root: &Path,
        archive: &[u8],
        bundle_id: &[u8],
    ) -> Result<Self, RuntimeResourceError> {
        let expected = read_bundle_id(Cursor::new(bundle_id))?;
        let mut files = BTreeMap::new();
        visit_authenticated_archive(Cursor::new(archive), &expected, |entry, contents| {
            if entry.kind() == ArchiveEntryKind::File {
                let capacity = usize::try_from(entry.size()).map_err(|_| {
                    RuntimeResourceError::Io(std::io::Error::other(
                        "runtime resource entry does not fit address space",
                    ))
                })?;
                let mut bytes = Vec::with_capacity(capacity);
                contents.read_to_end(&mut bytes)?;
                files.insert(mount_root.join(entry.path()), bytes);
            }
            Ok(())
        })?;
        Ok(Self { files })
    }
}

impl RuntimeResourceStore for MountedRuntimeResources {
    fn file_contents(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
}
