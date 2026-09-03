//! Read-only files shipped with an embedded editor product.

use std::ffi::OsString;
use std::path::Path;

/// One immutable node stored at a mounted runtime path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeResourceNode<'a> {
    /// Complete immutable file contents.
    File(&'a [u8]),
    /// Directory marker; children are available through `directory_entries`.
    Directory,
}

/// Context-owned read-only runtime resources.
///
/// Sandboxed hosts use this narrow mount for packaged `lisp/`, `etc/`,
/// `leim/`, and `info/` content. It is deliberately not a general filesystem:
/// user documents, persistence, and directory mutation require separate host
/// capabilities instead of being made to look like native paths.
pub trait RuntimeResourceStore {
    /// Resolve an exact mounted path, or return `None` when it is not owned.
    fn node(&self, path: &Path) -> Option<RuntimeResourceNode<'_>>;

    /// Return the complete bytes of one mounted file, or `None` when this
    /// store does not own the path.
    fn file_contents(&self, path: &Path) -> Option<&[u8]> {
        match self.node(path) {
            Some(RuntimeResourceNode::File(contents)) => Some(contents),
            Some(RuntimeResourceNode::Directory) | None => None,
        }
    }

    /// Return the immediate child names of one mounted directory.
    ///
    /// `Some(empty)` distinguishes an owned empty directory from an
    /// unmounted path, allowing callers to fall back to native storage only
    /// when this capability does not own the directory.
    fn directory_entries(&self, path: &Path) -> Option<Vec<OsString>>;

    /// Whether `path` names a directory owned by this store.
    fn directory_exists(&self, path: &Path) -> bool {
        self.node(path) == Some(RuntimeResourceNode::Directory)
    }
}
