//! Image `:file` path resolution.
//!
//! GNU resolves an image spec's `:file` via `image_find_image_fd`, which runs
//! `openp` over a search path of `[data-directory/images, ...x-bitmap-file-path]`
//! (src/image.c). A relative name like `splash.svg` is therefore found under
//! `etc/images/`, not the process working directory. This module mirrors that
//! contract as one pure, testable resolver and one typed classification that
//! the evaluator thread and the off-thread image submission worker share, so
//! the two consumers can never drift out of sync.

use crate::emacs_core::fileio::{
    expand_file_name, expand_file_name_with_home, file_name_absolute_p,
};
use std::path::Path;

/// Resolve an image `:file` the way GNU's `image_find_image_fd` does: try each
/// `search_path` element in order, forming `expand_file_name(file, dir)`, and
/// return the first candidate that is a readable, non-directory file.
///
/// `search_path` is expected to hold absolute directories (the production path
/// is `[data-directory/images, ...x-bitmap-file-path]`, both absolute). GNU's
/// `openp` has a `default-directory` fallback for *relative* path elements, but
/// `expand-file-name` already absolutizes a relative element against the
/// buffer's `default-directory`, so that branch is dormant in practice; any
/// future relative element should be absolutized at the call site.
///
/// A leading `/:` (the quoted-file-name prefix) is stripped, matching `openp`.
pub(crate) fn image_find_image_file(file: &str, search_path: &[String]) -> Option<String> {
    /// `openp`'s per-element step: strip the `/:` quote prefix, then probe the
    /// filesystem. Directories do not count (GNU treats them as `EISDIR`).
    fn probe(candidate: String) -> Option<String> {
        let probe = candidate
            .strip_prefix("/:")
            .map(|rest| rest.to_owned())
            .unwrap_or(candidate);
        Path::new(&probe).is_file().then_some(probe)
    }

    // openp: a nil/empty path means "use str as-is".
    if search_path.is_empty() {
        return probe(file.to_owned());
    }

    for dir in search_path {
        // expand_file_name ignores `dir` when `file` is already absolute.
        let candidate = expand_file_name(file, Some(dir));
        if let Some(found) = probe(candidate) {
            return Some(found);
        }
    }
    None
}

/// One image `:file`, classified by how it becomes a filesystem path.
///
/// This is the single decode of the resolution policy. The evaluator thread
/// calls [`ImageFileRequest::classify`]; the catalog keys its cache on
/// [`ImageFileRequest::cache_key`] and routes [`ImageFileRequest::needs_off_thread`]
/// entries to the submission worker, which calls [`ImageFileRequest::resolve`].
#[derive(Clone, Debug)]
pub enum ImageFileRequest {
    /// Open verbatim -- already absolute, or `~/`-expanded against a cached
    /// home. No resolution or I/O is required.
    Direct(String),
    /// A `~`-prefixed name that could not be expanded inline (either `~user`,
    /// which may consult NSS/LDAP, or `~/...` with no cached home). Resolved
    /// off-thread via lexical `expand-file-name`.
    ExpandHome(String),
    /// A relative name that must be searched against the image search path
    /// (GNU `image_find_image_fd`). Resolved off-thread by probing the
    /// filesystem, which must not happen on the evaluator thread.
    Search {
        name: String,
        search_path: Vec<String>,
    },
}

impl ImageFileRequest {
    /// Pure classification -- no I/O, no NSS. `home` is the cached `$HOME`
    /// (so `~/...` and `~` resolve inline); `search_path` is captured for the
    /// relative case.
    pub fn classify(file: &str, home: Option<&str>, search_path: Vec<String>) -> Self {
        // `~/...` and `~` expand against a cached home with no I/O.
        if let Some(home) = home
            && (file == "~" || file.starts_with("~/"))
        {
            return ImageFileRequest::Direct(expand_file_name_with_home(file, None, Some(home)));
        }
        // Any other `~`-prefix (`~user`, or `~/...` with no cached home) may
        // need NSS or $HOME lookup: defer to the worker.
        if file.starts_with('~') {
            return ImageFileRequest::ExpandHome(file.to_owned());
        }
        // Absolute: open verbatim.
        if file_name_absolute_p(file) {
            return ImageFileRequest::Direct(file.to_owned());
        }
        // Relative: GNU image_find_image_fd search, off-thread.
        ImageFileRequest::Search {
            name: file.to_owned(),
            search_path,
        }
    }

    /// Whether the evaluator thread must hand this to the submission worker
    /// (because resolution may touch the filesystem or NSS).
    pub fn needs_off_thread(&self) -> bool {
        !matches!(self, ImageFileRequest::Direct(_))
    }

    /// The stable string the image catalog dedups entries on: the expanded
    /// path for inline-resolved `Direct`, otherwise the raw name.
    pub fn cache_key(&self) -> &str {
        match self {
            ImageFileRequest::Direct(s) | ImageFileRequest::ExpandHome(s) => s,
            ImageFileRequest::Search { name, .. } => name,
        }
    }

    /// Produce the absolute filesystem path, or `None` if no search-path
    /// element matches. `Direct` is the identity; `ExpandHome` is a lexical
    /// `expand-file-name`; `Search` probes the filesystem.
    pub fn resolve(&self) -> Option<String> {
        match self {
            ImageFileRequest::Direct(s) => Some(s.clone()),
            ImageFileRequest::ExpandHome(s) => Some(expand_file_name(s, None)),
            ImageFileRequest::Search { name, search_path } => {
                image_find_image_file(name, search_path)
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
