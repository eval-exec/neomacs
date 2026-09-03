//! Deterministic in-memory storage for session mounts and adapter tests.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use super::{AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, WriteMode, WriteRequest};

#[derive(Clone, Debug)]
enum MemoryNode {
    Directory {
        modified: SystemTime,
    },
    File {
        contents: Vec<u8>,
        modified: SystemTime,
    },
}

impl MemoryNode {
    fn metadata(&self) -> FileMetadata {
        match self {
            Self::Directory { modified } => FileMetadata {
                kind: FileEntryKind::Directory,
                len: 0,
                modified: Some(*modified),
                readonly: false,
            },
            Self::File { contents, modified } => FileMetadata {
                kind: FileEntryKind::File,
                len: contents.len() as u64,
                modified: Some(*modified),
                readonly: false,
            },
        }
    }
}

/// A rooted, path-safe filesystem kept entirely in memory.
#[derive(Debug)]
pub struct MemoryFileSystem {
    nodes: RwLock<BTreeMap<PathBuf, MemoryNode>>,
}

impl Default for MemoryFileSystem {
    fn default() -> Self {
        let now = SystemTime::now();
        Self {
            nodes: RwLock::new(BTreeMap::from([(
                PathBuf::from("/"),
                MemoryNode::Directory { modified: now },
            )])),
        }
    }
}

impl MemoryFileSystem {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn invalid_path(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

fn normalize(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid_path("virtual filesystem paths must be absolute"));
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                if normalized == Path::new("/") {
                    return Err(invalid_path("virtual filesystem path escapes its root"));
                }
                normalized.pop();
            }
            Component::Prefix(_) => {
                return Err(invalid_path("virtual filesystem path has a native prefix"));
            }
        }
    }
    Ok(normalized)
}

fn parent_directory<'a>(
    nodes: &'a BTreeMap<PathBuf, MemoryNode>,
    path: &Path,
) -> io::Result<&'a MemoryNode> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_path("virtual filesystem root has no parent"))?;
    match nodes.get(parent) {
        Some(node @ MemoryNode::Directory { .. }) => Ok(node),
        Some(MemoryNode::File { .. }) => Err(io::Error::from(ErrorKind::NotADirectory)),
        None => Err(io::Error::from(ErrorKind::NotFound)),
    }
}

fn remove_tree(nodes: &mut BTreeMap<PathBuf, MemoryNode>, root: &Path) {
    let descendants = nodes
        .keys()
        .filter(|candidate| *candidate == root || candidate.starts_with(root))
        .cloned()
        .collect::<Vec<_>>();
    for path in descendants {
        nodes.remove(&path);
    }
}

impl EditorFileSystem for MemoryFileSystem {
    fn metadata(&self, path: &Path, _follow_links: bool) -> io::Result<FileMetadata> {
        let path = normalize(path)?;
        self.nodes
            .read()
            .expect("memory filesystem read lock poisoned")
            .get(&path)
            .map(MemoryNode::metadata)
            .ok_or_else(|| io::Error::from(ErrorKind::NotFound))
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        let Ok(metadata) = self.metadata(path, true) else {
            return false;
        };
        match mode {
            AccessMode::Exists | AccessMode::Read => true,
            AccessMode::Write => !metadata.readonly,
            AccessMode::Execute | AccessMode::ReadAndSearch => {
                metadata.kind == FileEntryKind::Directory
            }
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path)?;
        match self
            .nodes
            .read()
            .expect("memory filesystem read lock poisoned")
            .get(&path)
        {
            Some(MemoryNode::File { contents, .. }) => Ok(contents.clone()),
            Some(MemoryNode::Directory { .. }) => Err(io::Error::from(ErrorKind::IsADirectory)),
            None => Err(io::Error::from(ErrorKind::NotFound)),
        }
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>> {
        let path = normalize(path)?;
        let nodes = self
            .nodes
            .read()
            .expect("memory filesystem read lock poisoned");
        match nodes.get(&path) {
            Some(MemoryNode::File { .. }) => return Err(io::Error::from(ErrorKind::NotADirectory)),
            None => return Err(io::Error::from(ErrorKind::NotFound)),
            Some(MemoryNode::Directory { .. }) => {}
        }
        Ok(nodes
            .keys()
            .filter_map(|candidate| {
                (candidate.parent() == Some(path.as_path()))
                    .then(|| candidate.file_name().map(ToOwned::to_owned))
                    .flatten()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata> {
        let path = normalize(path)?;
        if path == Path::new("/") {
            return Err(io::Error::from(ErrorKind::IsADirectory));
        }
        let mut nodes = self
            .nodes
            .write()
            .expect("memory filesystem write lock poisoned");
        parent_directory(&nodes, &path)?;
        let previous = match nodes.get(&path) {
            Some(MemoryNode::Directory { .. }) => {
                return Err(io::Error::from(ErrorKind::IsADirectory));
            }
            Some(MemoryNode::File { contents, .. }) => Some(contents.clone()),
            None => None,
        };
        if request.mode == WriteMode::CreateNew && previous.is_some() {
            return Err(io::Error::from(ErrorKind::AlreadyExists));
        }
        let mut next = match request.mode {
            WriteMode::Truncate | WriteMode::CreateNew => Vec::new(),
            WriteMode::Append | WriteMode::At(_) => previous.unwrap_or_default(),
        };
        match request.mode {
            WriteMode::Append => next.extend_from_slice(contents),
            WriteMode::At(offset) => {
                let offset = usize::try_from(offset)
                    .map_err(|_| invalid_path("write offset does not fit address space"))?;
                if next.len() < offset {
                    next.resize(offset, 0);
                }
                let end = offset
                    .checked_add(contents.len())
                    .ok_or_else(|| invalid_path("write length overflows address space"))?;
                if next.len() < end {
                    next.resize(end, 0);
                }
                next[offset..end].copy_from_slice(contents);
            }
            WriteMode::Truncate | WriteMode::CreateNew => next.extend_from_slice(contents),
        }
        let node = MemoryNode::File {
            contents: next,
            modified: SystemTime::now(),
        };
        let metadata = node.metadata();
        nodes.insert(path, node);
        Ok(metadata)
    }

    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()> {
        let path = normalize(path)?;
        if path == Path::new("/") {
            return if parents {
                Ok(())
            } else {
                Err(io::Error::from(ErrorKind::AlreadyExists))
            };
        }
        let mut nodes = self
            .nodes
            .write()
            .expect("memory filesystem write lock poisoned");
        if nodes.contains_key(&path) {
            return if parents && matches!(nodes.get(&path), Some(MemoryNode::Directory { .. })) {
                Ok(())
            } else {
                Err(io::Error::from(ErrorKind::AlreadyExists))
            };
        }
        let now = SystemTime::now();
        if parents {
            let mut ancestors = path.ancestors().take_while(|path| *path != Path::new("/"));
            let mut missing = ancestors.by_ref().map(Path::to_owned).collect::<Vec<_>>();
            missing.reverse();
            for directory in missing {
                if matches!(nodes.get(&directory), Some(MemoryNode::File { .. })) {
                    return Err(io::Error::from(ErrorKind::NotADirectory));
                }
                nodes
                    .entry(directory)
                    .or_insert(MemoryNode::Directory { modified: now });
            }
        } else {
            parent_directory(&nodes, &path)?;
            nodes.insert(path, MemoryNode::Directory { modified: now });
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let path = normalize(path)?;
        let mut nodes = self
            .nodes
            .write()
            .expect("memory filesystem write lock poisoned");
        match nodes.get(&path) {
            Some(MemoryNode::File { .. }) => {
                nodes.remove(&path);
                Ok(())
            }
            Some(MemoryNode::Directory { .. }) => Err(io::Error::from(ErrorKind::IsADirectory)),
            None => Err(io::Error::from(ErrorKind::NotFound)),
        }
    }

    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        let path = normalize(path)?;
        if path == Path::new("/") {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "cannot remove virtual filesystem root",
            ));
        }
        let mut nodes = self
            .nodes
            .write()
            .expect("memory filesystem write lock poisoned");
        match nodes.get(&path) {
            Some(MemoryNode::File { .. }) => return Err(io::Error::from(ErrorKind::NotADirectory)),
            None => return Err(io::Error::from(ErrorKind::NotFound)),
            Some(MemoryNode::Directory { .. }) => {}
        }
        let has_children = nodes
            .keys()
            .any(|candidate| candidate.parent() == Some(&path));
        if has_children && !recursive {
            return Err(io::Error::from(ErrorKind::DirectoryNotEmpty));
        }
        remove_tree(&mut nodes, &path);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        let from = normalize(from)?;
        let to = normalize(to)?;
        if from == Path::new("/") || to == Path::new("/") || to.starts_with(&from) {
            return Err(invalid_path("invalid virtual filesystem rename"));
        }
        let mut nodes = self
            .nodes
            .write()
            .expect("memory filesystem write lock poisoned");
        if !nodes.contains_key(&from) {
            return Err(io::Error::from(ErrorKind::NotFound));
        }
        parent_directory(&nodes, &to)?;
        if nodes.contains_key(&to) {
            if !replace {
                return Err(io::Error::from(ErrorKind::AlreadyExists));
            }
            remove_tree(&mut nodes, &to);
        }
        let moving = nodes
            .iter()
            .filter(|(candidate, _)| **candidate == from || candidate.starts_with(&from))
            .map(|(path, node)| (path.clone(), node.clone()))
            .collect::<Vec<_>>();
        for (old_path, _) in &moving {
            nodes.remove(old_path);
        }
        for (old_path, node) in moving {
            let suffix = old_path
                .strip_prefix(&from)
                .expect("selected rename descendant has source prefix");
            nodes.insert(to.join(suffix), node);
        }
        Ok(())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let path = normalize(path)?;
        self.metadata(&path, true)?;
        Ok(path)
    }
}
