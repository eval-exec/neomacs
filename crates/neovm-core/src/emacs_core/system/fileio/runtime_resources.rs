//! Read-only files shipped with an embedded editor product.

use std::path::Path;

/// Context-owned read-only runtime resources.
///
/// Sandboxed hosts use this narrow mount for packaged `lisp/`, `etc/`,
/// `leim/`, and `info/` content. It is deliberately not a general filesystem:
/// user documents, persistence, and directory mutation require separate host
/// capabilities instead of being made to look like native paths.
pub trait RuntimeResourceStore {
    /// Return the complete bytes of one mounted file, or `None` when this
    /// store does not own the path.
    fn file_contents(&self, path: &Path) -> Option<&[u8]>;
}
