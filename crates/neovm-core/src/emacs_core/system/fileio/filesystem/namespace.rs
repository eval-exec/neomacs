//! Evaluator-owned namespace joining immutable product resources to host storage.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::super::{RuntimeResourceNode, RuntimeResourceStore};
use super::virtual_path::VirtualPath;
use super::{
    AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, FileMode, FileStability,
    FileSystemSpace, FileTimestamp, TemporaryEntry, WriteRequest,
};

/// The single filesystem boundary owned by an evaluator.
///
/// Runtime resources form an immutable layer over the host-selected mutable
/// backend. Keeping both behind this object prevents individual Lisp
/// primitives from learning which product host supplied a path.
pub(crate) struct EditorFileSystemNamespace {
    host: Box<dyn EditorFileSystem>,
    runtime_resources: Option<Box<dyn RuntimeResourceStore>>,
}

impl EditorFileSystemNamespace {
    pub(crate) fn new(host: Box<dyn EditorFileSystem>) -> Self {
        Self {
            host,
            runtime_resources: None,
        }
    }

    pub(crate) fn replace_host(&mut self, host: Box<dyn EditorFileSystem>) {
        self.host = host;
    }

    pub(crate) fn install_runtime_resources(&mut self, store: Box<dyn RuntimeResourceStore>) {
        self.runtime_resources = Some(store);
    }

    fn runtime_store_for(&self, path: &Path) -> Option<&dyn RuntimeResourceStore> {
        self.runtime_resources
            .as_deref()
            .filter(|store| path.starts_with(store.mount_root()))
    }

    fn runtime_node(&self, path: &Path) -> Option<RuntimeResourceNode<'_>> {
        self.runtime_store_for(path)?.node(path)
    }

    fn runtime_mount_child_of(&self, path: &Path) -> Option<OsString> {
        let store = self.runtime_resources.as_deref()?;
        let directory = VirtualPath::parse(path).ok()?;
        let mount_root = VirtualPath::parse(store.mount_root()).ok()?;
        let relative_mount = mount_root.strip_prefix(&directory)?;
        (!relative_mount.is_root())
            .then(|| relative_mount.first_component())
            .flatten()
    }

    fn immutable_error(action: &'static str) -> io::Error {
        io::Error::new(
            ErrorKind::PermissionDenied,
            format!("cannot {action} an immutable runtime resource"),
        )
    }

    fn runtime_metadata(node: RuntimeResourceNode<'_>) -> FileMetadata {
        let (kind, len) = match node {
            RuntimeResourceNode::File(contents) => (FileEntryKind::File, contents.len() as u64),
            RuntimeResourceNode::Directory => (FileEntryKind::Directory, 0),
        };
        FileMetadata {
            kind,
            len,
            modified: None,
            stability: FileStability::Immutable,
            readonly: true,
        }
    }

    fn require_runtime_node(&self, path: &Path) -> io::Result<RuntimeResourceNode<'_>> {
        self.runtime_node(path)
            .ok_or_else(|| io::Error::from(ErrorKind::NotFound))
    }
}

impl EditorFileSystem for EditorFileSystemNamespace {
    fn metadata(&self, path: &Path, follow_links: bool) -> io::Result<FileMetadata> {
        if self.runtime_store_for(path).is_some() {
            return self.require_runtime_node(path).map(Self::runtime_metadata);
        }
        self.host.metadata(path, follow_links)
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        if self.runtime_store_for(path).is_some() {
            return match self.runtime_node(path) {
                Some(RuntimeResourceNode::File(_)) => {
                    matches!(mode, AccessMode::Exists | AccessMode::Read)
                }
                Some(RuntimeResourceNode::Directory) => matches!(
                    mode,
                    AccessMode::Exists
                        | AccessMode::Read
                        | AccessMode::Execute
                        | AccessMode::ReadAndSearch
                ),
                None => false,
            };
        }
        self.host.access(path, mode)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if self.runtime_store_for(path).is_some() {
            return match self.require_runtime_node(path)? {
                RuntimeResourceNode::File(contents) => Ok(contents.to_vec()),
                RuntimeResourceNode::Directory => Err(io::Error::from(ErrorKind::IsADirectory)),
            };
        }
        self.host.read(path)
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>> {
        if let Some(store) = self.runtime_store_for(path) {
            return match self.require_runtime_node(path)? {
                RuntimeResourceNode::Directory => store
                    .directory_entries(path)
                    .ok_or_else(|| io::Error::from(ErrorKind::NotFound)),
                RuntimeResourceNode::File(_) => Err(io::Error::from(ErrorKind::NotADirectory)),
            };
        }
        let mut entries = self
            .host
            .read_directory(path)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        if let Some(mount_child) = self.runtime_mount_child_of(path) {
            entries.insert(mount_child);
        }
        Ok(entries.into_iter().collect())
    }

    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata> {
        if self.runtime_store_for(path).is_some() {
            return Err(Self::immutable_error("write"));
        }
        self.host.write(path, contents, request)
    }

    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            return Err(Self::immutable_error("create a directory inside"));
        }
        self.host.create_directory(path, parents)
    }

    fn create_temporary(&self, path: &Path, entry: TemporaryEntry<'_>) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            return Err(Self::immutable_error("create a temporary entry inside"));
        }
        self.host.create_temporary(path, entry)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(Self::immutable_error("remove"));
        }
        self.host.remove_file(path)
    }

    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(Self::immutable_error("remove"));
        }
        self.host.remove_directory(path, recursive)
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        if self.runtime_store_for(from).is_some() {
            self.require_runtime_node(from)?;
            return Err(Self::immutable_error("rename"));
        }
        if self.runtime_store_for(to).is_some() {
            return Err(Self::immutable_error("rename a file into"));
        }
        self.host.rename(from, to, replace)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Ok(VirtualPath::parse(path)?.to_path_buf());
        }
        self.host.canonicalize(path)
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(io::Error::from(ErrorKind::InvalidInput));
        }
        self.host.read_link(path)
    }

    fn mode(&self, path: &Path, follow_links: bool) -> io::Result<FileMode> {
        if self.runtime_store_for(path).is_some() {
            return match self.require_runtime_node(path)? {
                RuntimeResourceNode::File(_) => Ok(FileMode::from_bits_truncate(0o444)),
                RuntimeResourceNode::Directory => Ok(FileMode::from_bits_truncate(0o555)),
            };
        }
        self.host.mode(path, follow_links)
    }

    fn set_mode(&self, path: &Path, mode: FileMode, follow_links: bool) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(Self::immutable_error("change the mode of"));
        }
        self.host.set_mode(path, mode, follow_links)
    }

    fn set_times(
        &self,
        path: &Path,
        timestamp: Option<FileTimestamp>,
        follow_links: bool,
    ) -> io::Result<()> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(Self::immutable_error("change the timestamp of"));
        }
        self.host.set_times(path, timestamp, follow_links)
    }

    fn file_system_space(&self, path: &Path) -> io::Result<FileSystemSpace> {
        if self.runtime_store_for(path).is_some() {
            self.require_runtime_node(path)?;
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "immutable runtime resources do not expose filesystem capacity",
            ));
        }
        self.host.file_system_space(path)
    }

    fn same_file(&self, left: &Path, right: &Path) -> io::Result<bool> {
        let left_is_runtime = self.runtime_store_for(left).is_some();
        let right_is_runtime = self.runtime_store_for(right).is_some();
        if left_is_runtime || right_is_runtime {
            self.metadata(left, true)?;
            self.metadata(right, true)?;
            return Ok(left_is_runtime && right_is_runtime && left == right);
        }
        self.host.same_file(left, right)
    }
}
