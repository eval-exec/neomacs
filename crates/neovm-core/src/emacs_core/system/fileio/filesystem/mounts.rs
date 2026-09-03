//! A virtual namespace that routes absolute editor paths to isolated mounts.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

use super::{AccessMode, EditorFileSystem, FileEntryKind, FileMetadata, WriteRequest};

struct Mount {
    path: PathBuf,
    filesystem: Box<dyn EditorFileSystem>,
}

/// An immutable-after-construction table of rooted filesystem adapters.
///
/// Backends always receive an absolute path rooted at `/`; mount prefixes are
/// implementation details of this namespace and cannot escape into a host.
#[derive(Default)]
pub struct MountTableFileSystem {
    mounts: Vec<Mount>,
}

impl MountTableFileSystem {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one mount. Mount paths must be absolute, normalized, and unique.
    pub fn mount(&mut self, path: &Path, filesystem: Box<dyn EditorFileSystem>) -> io::Result<()> {
        let normalized = normalize(path)?;
        if normalized == Path::new("/") {
            return Err(invalid_path(
                "the virtual namespace root cannot be replaced",
            ));
        }
        if normalized != path {
            return Err(invalid_path("mount path must already be normalized"));
        }
        if self.mounts.iter().any(|mount| mount.path == normalized) {
            return Err(io::Error::from(ErrorKind::AlreadyExists));
        }
        self.mounts.push(Mount {
            path: normalized,
            filesystem,
        });
        self.mounts.sort_by(|left, right| {
            right
                .path
                .components()
                .count()
                .cmp(&left.path.components().count())
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(())
    }

    fn route(&self, path: &Path) -> io::Result<(&Mount, PathBuf)> {
        let path = normalize(path)?;
        let mount = self
            .mounts
            .iter()
            .find(|mount| path.starts_with(&mount.path))
            .ok_or_else(|| io::Error::from(ErrorKind::NotFound))?;
        let relative = path
            .strip_prefix(&mount.path)
            .expect("selected mount must prefix routed path");
        Ok((mount, Path::new("/").join(relative)))
    }

    fn namespace_directory(&self, path: &Path) -> io::Result<bool> {
        let path = normalize(path)?;
        Ok(path == Path::new("/")
            || self
                .mounts
                .iter()
                .any(|mount| mount.path != path && mount.path.starts_with(&path)))
    }

    fn namespace_children(&self, path: &Path) -> io::Result<Vec<OsString>> {
        let path = normalize(path)?;
        if !self.namespace_directory(&path)? {
            return Err(io::Error::from(ErrorKind::NotFound));
        }
        Ok(self
            .mounts
            .iter()
            .filter_map(|mount| {
                let relative = mount.path.strip_prefix(&path).ok()?;
                relative.components().find_map(|component| match component {
                    Component::Normal(name) => Some(name.to_owned()),
                    _ => None,
                })
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}

fn invalid_path(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

fn normalize(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid_path("mounted filesystem paths must be absolute"));
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                if normalized == Path::new("/") {
                    return Err(invalid_path("mounted filesystem path escapes its root"));
                }
                normalized.pop();
            }
            Component::Prefix(_) => {
                return Err(invalid_path("mounted filesystem path has a native prefix"));
            }
        }
    }
    Ok(normalized)
}

fn namespace_metadata() -> FileMetadata {
    FileMetadata {
        kind: FileEntryKind::Directory,
        len: 0,
        modified: None,
        readonly: true,
    }
}

impl EditorFileSystem for MountTableFileSystem {
    fn metadata(&self, path: &Path, follow_links: bool) -> io::Result<FileMetadata> {
        match self.route(path) {
            Ok((mount, relative)) => mount.filesystem.metadata(&relative, follow_links),
            Err(error)
                if error.kind() == ErrorKind::NotFound && self.namespace_directory(path)? =>
            {
                Ok(namespace_metadata())
            }
            Err(error) => Err(error),
        }
    }

    fn access(&self, path: &Path, mode: AccessMode) -> bool {
        match self.route(path) {
            Ok((mount, relative)) => mount.filesystem.access(&relative, mode),
            Err(_) => {
                self.namespace_directory(path).unwrap_or(false)
                    && matches!(
                        mode,
                        AccessMode::Exists | AccessMode::Read | AccessMode::ReadAndSearch
                    )
            }
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let (mount, relative) = self.route(path)?;
        mount.filesystem.read(&relative)
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<OsString>> {
        match self.route(path) {
            Ok((mount, relative)) => mount.filesystem.read_directory(&relative),
            Err(error)
                if error.kind() == ErrorKind::NotFound && self.namespace_directory(path)? =>
            {
                self.namespace_children(path)
            }
            Err(error) => Err(error),
        }
    }

    fn write(
        &self,
        path: &Path,
        contents: &[u8],
        request: WriteRequest,
    ) -> io::Result<FileMetadata> {
        let (mount, relative) = self.route(path)?;
        mount.filesystem.write(&relative, contents, request)
    }

    fn create_directory(&self, path: &Path, parents: bool) -> io::Result<()> {
        let (mount, relative) = self.route(path)?;
        mount.filesystem.create_directory(&relative, parents)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let (mount, relative) = self.route(path)?;
        mount.filesystem.remove_file(&relative)
    }

    fn remove_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        let (mount, relative) = self.route(path)?;
        if relative == Path::new("/") {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "cannot remove a filesystem mount",
            ));
        }
        mount.filesystem.remove_directory(&relative, recursive)
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        let (from_mount, from_relative) = self.route(from)?;
        let (to_mount, to_relative) = self.route(to)?;
        if !std::ptr::eq(from_mount, to_mount) {
            return Err(io::Error::from(ErrorKind::CrossesDevices));
        }
        from_mount
            .filesystem
            .rename(&from_relative, &to_relative, replace)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let (mount, relative) = self.route(path)?;
        let canonical = mount.filesystem.canonicalize(&relative)?;
        let relative = canonical
            .strip_prefix("/")
            .map_err(|_| invalid_path("mounted backend returned a non-rooted canonical path"))?;
        Ok(mount.path.join(relative))
    }
}
