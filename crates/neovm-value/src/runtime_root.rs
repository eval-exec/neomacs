//! Runtime install-root resolution (GNU `PATH_LOADSEARCH` equivalent).
//!
//! Moved VERBATIM from the evaluator's load module (stage-3b T7). The
//! resolvers read only std environment/path state, so the lowest crate both
//! the evaluator and the text layer can name owns them; core keeps its
//! historical `emacs_core::load` paths through re-exports.
//!
//! The environment overrides and the layout candidates are documented on
//! [`runtime_root_candidates`] and [`runtime_project_root`].

use std::path::{Path, PathBuf};

pub const RUNTIME_ROOT_ENV: &str = "NEOMACS_RUNTIME_ROOT";

pub fn compile_time_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

pub fn is_runtime_root(path: &Path) -> bool {
    path.join("lisp").is_dir() && path.join("etc").is_dir()
}

/// Runtime-root candidates for a running executable, package data before the
/// legacy parent-directory fallback.
///
/// Ports GNU's dual layout support:
///
/// * The dynamic half is `init_cmdargs` (src/emacs.c): starting from the
///   invocation directory -- following symlinks, which
///   `current_exe().canonicalize()` does for us in one step -- Emacs checks
///   the executable's own directory and then its parent for the tree
///   signature, and records the hit as `installation-directory`;
///   `load_path_default` (src/lread.c) then resets the load-path to
///   `installation-directory/lisp`. GNU's signature is `lib-src` + `etc`;
///   ours is `lisp` + `etc` ([`is_runtime_root`]). This covers the release
///   tarball's flat layout (executable beside `lisp`/`etc`) and the
///   direct-sibling versioned shape (`<root>/bin/neomacs` with
///   `<root>/lisp`), reached through the `~/.local/bin/neomacs` symlink.
///   install.sh stages the versioned tree as `<ver>/bin` +
///   `<ver>/share/neomacs` so that resolver generations before this
///   walk-up existed (v0.0.15 and earlier, which only probe
///   `<grandparent>/share/neomacs`) locate it too.
/// * The configured half is GNU's compile-time `PATH_LOADSEARCH`
///   (`<prefix>/share/emacs/<version>/lisp`); we probe instead of baking a
///   prefix at build time: `<grandparent>/share/neomacs` (deb/rpm/AppImage:
///   `<prefix>/bin/neomacs` + `<prefix>/share/neomacs`) and
///   `<grandparent>/Resources/neomacs` (macOS app bundle:
///   `Contents/MacOS/neomacs` + `Contents/Resources/neomacs`).
///   These package-specific directories precede the generic parent: extracting
///   a current package over a legacy flat tree must use the new runtime data.
pub fn runtime_root_candidates(exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(4);
    if let Some(dir) = exe.parent() {
        candidates.push(dir.to_path_buf());
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join("share/neomacs"));
            candidates.push(parent.join("Resources/neomacs"));
            candidates.push(parent.to_path_buf());
        }
    }
    candidates
}

/// Probe the executable's layouts before source and remapped test fallbacks.
pub fn runtime_project_root_from_paths(
    compile_root: &Path,
    exe: Option<&Path>,
    nextest_root: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(exe) = exe {
        for candidate in runtime_root_candidates(exe) {
            if is_runtime_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    // A surviving build checkout must not shadow an installed executable's
    // own data. Development binaries still fall back to their source tree.
    if is_runtime_root(compile_root) {
        return Some(compile_root.to_path_buf());
    }
    // nextest archives retain the producer's compile-time workspace path,
    // but run under target/debug/deps in the consumer's remapped checkout.
    // Use that checkout only after checking the executable's installed tree.
    nextest_root
        .filter(|root| is_runtime_root(root))
        .map(Path::to_path_buf)
}

/// Locate runtime data, preferring a valid environment override and then the
/// running executable's installation over a surviving build checkout.
/// Panics when no candidate contains the required Lisp and data directories.
pub fn runtime_project_root() -> PathBuf {
    if let Ok(root) = std::env::var(RUNTIME_ROOT_ENV) {
        let path = PathBuf::from(root);
        if is_runtime_root(&path) {
            return path;
        }
        tracing::warn!(
            "{RUNTIME_ROOT_ENV}={} does not contain lisp/ and etc/; falling back",
            path.display()
        );
    }

    let compile_root = compile_time_project_root();
    let exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok());
    let nextest_root = std::env::var_os("NEXTEST_WORKSPACE_ROOT").map(PathBuf::from);
    if let Some(root) =
        runtime_project_root_from_paths(&compile_root, exe.as_deref(), nextest_root.as_deref())
    {
        return root;
    }

    panic!(
        "Neomacs runtime root not found. Set {RUNTIME_ROOT_ENV} to a directory containing lisp/ and etc/."
    );
}

/// Directory holding the charset `.map` files (`JISX0201.map`, ...). Resolved at
/// RUNTIME under the install data dir (`<runtime_root>/etc/charsets`) -- the
/// neomacs equivalent of GNU's `charset-map-path`
/// (`(expand-file-name "charsets" data-directory)`), reusing the same
/// `runtime_project_root()` resolver that backs `data-directory` and locates
/// `lisp/`. MUST NOT use `env!("CARGO_MANIFEST_DIR")`: that compile-time path is
/// the build machine's source tree, absent in an installed release, so charset
/// maps would silently fail to load (decode-char -> nil -> "Invalid code(s)").
pub fn charset_map_directory() -> PathBuf {
    runtime_project_root().join("etc").join("charsets")
}

/// `<runtime-root>/etc/images` -- the `data-directory/images` entry of GNU's
/// image file search path (`image_find_image_fd` -> `openp`), backing relative
/// image `:file` resolution. Resolved at RUNTIME via `runtime_project_root()`,
/// never `env!("CARGO_MANIFEST_DIR")`, for the same release-correctness reason
/// as [`charset_map_directory`].
pub fn image_data_directory() -> PathBuf {
    runtime_project_root().join("etc").join("images")
}
