//! Read-only runtime resources mounted in an evaluator's virtual root.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use neovm_core::emacs_core::fileio::{RuntimeResourceNode, RuntimeResourceStore};

use super::bundle::{
    ArchiveEntryKind, RuntimeResourceBundle, RuntimeResourceError, visit_authenticated_archive,
};

/// An authenticated runtime resource bundle expanded into linear memory.
///
/// This is intended for hosts without a native filesystem, principally a
/// browser Worker. It owns only immutable product resources; user documents
/// belong to a separate host storage capability.
#[derive(Debug)]
pub struct MountedRuntimeResources {
    mount_root: PathBuf,
    directories: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl MountedRuntimeResources {
    /// Authenticate and mount a deterministic runtime resource bundle.
    pub fn from_bundle(
        mount_root: &Path,
        bundle: RuntimeResourceBundle<'_>,
    ) -> Result<Self, RuntimeResourceError> {
        let mut directories = BTreeSet::from([mount_root.to_owned()]);
        let mut files = BTreeMap::new();
        visit_authenticated_archive(
            Cursor::new(bundle.archive()),
            bundle.expected(),
            |entry, contents| {
                let mounted_path = mount_root.join(entry.path());
                if entry.kind() == ArchiveEntryKind::Directory {
                    directories.insert(mounted_path);
                    return Ok(());
                }

                let capacity = usize::try_from(entry.size()).map_err(|_| {
                    RuntimeResourceError::Io(std::io::Error::other(
                        "runtime resource entry does not fit address space",
                    ))
                })?;
                let mut bytes = Vec::with_capacity(capacity);
                contents.read_to_end(&mut bytes)?;
                let mut parent = mounted_path.parent();
                while let Some(directory) = parent {
                    if !directory.starts_with(mount_root) {
                        break;
                    }
                    directories.insert(directory.to_owned());
                    if directory == mount_root {
                        break;
                    }
                    parent = directory.parent();
                }
                files.insert(mounted_path, bytes);
                Ok(())
            },
        )?;
        Ok(Self {
            mount_root: mount_root.to_owned(),
            directories,
            files,
        })
    }

    /// Virtual parent of the mounted `lisp/`, `etc/`, `leim/`, and `info/`
    /// trees.
    #[must_use]
    pub fn mount_root(&self) -> &Path {
        &self.mount_root
    }
}

impl RuntimeResourceStore for MountedRuntimeResources {
    fn mount_root(&self) -> &Path {
        &self.mount_root
    }

    fn node(&self, path: &Path) -> Option<RuntimeResourceNode<'_>> {
        if let Some(contents) = self.files.get(path) {
            Some(RuntimeResourceNode::File(contents))
        } else if self.directories.contains(path) {
            Some(RuntimeResourceNode::Directory)
        } else {
            None
        }
    }

    fn directory_entries(&self, path: &Path) -> Option<Vec<OsString>> {
        if !self.directories.contains(path) {
            return None;
        }
        let entries = self
            .directories
            .iter()
            .chain(self.files.keys())
            .filter_map(|entry| {
                (entry.parent() == Some(path))
                    .then(|| entry.file_name().map(ToOwned::to_owned))
                    .flatten()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Some(entries)
    }
}
