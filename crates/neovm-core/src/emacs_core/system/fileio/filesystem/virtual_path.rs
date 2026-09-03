//! Target-independent POSIX paths for mounted editor filesystems.

use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct VirtualPath {
    components: Vec<String>,
}

impl VirtualPath {
    pub(super) fn parse(path: &Path) -> io::Result<Self> {
        let path = path.to_str().ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                "virtual filesystem path is not UTF-8",
            )
        })?;
        if !path.starts_with('/') {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "virtual filesystem paths must be absolute",
            ));
        }
        let mut components = Vec::new();
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            "virtual filesystem path escapes its root",
                        ));
                    }
                }
                component => components.push(component.to_owned()),
            }
        }
        Ok(Self { components })
    }

    pub(super) fn is_root(&self) -> bool {
        self.components.is_empty()
    }

    pub(super) fn parent(&self) -> Option<Self> {
        (!self.is_root()).then(|| Self {
            components: self.components[..self.components.len() - 1].to_vec(),
        })
    }

    pub(super) fn file_name(&self) -> Option<OsString> {
        self.components.last().map(OsString::from)
    }

    pub(super) fn first_component(&self) -> Option<OsString> {
        self.components.first().map(OsString::from)
    }

    pub(super) fn depth(&self) -> usize {
        self.components.len()
    }

    pub(super) fn starts_with(&self, prefix: &Self) -> bool {
        self.components.starts_with(&prefix.components)
    }

    pub(super) fn strip_prefix(&self, prefix: &Self) -> Option<Self> {
        self.starts_with(prefix).then(|| Self {
            components: self.components[prefix.components.len()..].to_vec(),
        })
    }

    pub(super) fn join(&self, suffix: &Self) -> Self {
        let mut components = self.components.clone();
        components.extend_from_slice(&suffix.components);
        Self { components }
    }

    pub(super) fn ancestors_without_root(&self) -> Vec<Self> {
        (1..=self.components.len())
            .map(|length| Self {
                components: self.components[..length].to_vec(),
            })
            .collect()
    }

    pub(super) fn to_path_buf(&self) -> PathBuf {
        if self.is_root() {
            PathBuf::from("/")
        } else {
            PathBuf::from(format!("/{}", self.components.join("/")))
        }
    }
}
