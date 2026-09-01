//! `PATH_EXEC`: this installation's architecture-dependent private directory.
//!
//! GNU declares one directory that is neither `bindir` nor a data directory
//! and holds everything that is both private to Emacs and specific to the
//! machine it was built for -- the helper programs and the dump image:
//!
//! ```c
//! /* The extra search path for programs to invoke.  This is appended to
//!    whatever the PATH environment variable says to set the Lisp
//!    variable exec-path and the first file name in it sets the Lisp
//!    variable exec-directory.  exec-directory is used for finding
//!    executables and other architecture-dependent files.  */
//! #define PATH_EXEC "/usr/local/libexec/emacs"
//! ```
//! -- `src/epaths.in:53-58`, filled in by `make epaths-force`
//! (`Makefile.in:473`) from `archlibdir`, which configure defines as
//!
//! ```text
//! archlibdir='${libexecdir}/emacs/${version}/${configuration}'
//! ```
//! -- `configure.ac:290`.
//!
//! It has three consumers, all ported here or by this module's callers:
//!
//! * `init_callproc_1` (`src/callproc.c:1959-1963`) seeds `exec-path` from
//!   `EMACSPATH` defaulting to `PATH_EXEC`, takes `exec-directory` from its
//!   car, and appends the whole thing to `$PATH`.
//! * `init_callproc` (`src/callproc.c:1976-1991`) overrides both with
//!   `<installation-directory>/lib-src` when Emacs is running uninstalled --
//!   "Running uninstalled, so default to tem rather than PATH_EXEC".
//! * `load_pdump` (`src/emacs.c:1046-1120`) looks for the dump image in
//!   `PATH_EXEC` once the search beside the executable has failed.
//!
//! # How this port resolves it
//!
//! GNU bakes an absolute path into `epaths.h` at configure time and repairs
//! it at runtime only where the tree is relocatable: `w32_relocate` on
//! MS-Windows and `ns_relocate` (`src/nsterm.m:524-553`) inside a macOS app
//! bundle, where `epaths-force-ns-self-contained` (`Makefile.in:511-515`)
//! has already rewritten `PATH_EXEC` to the bundle-relative
//! `Contents/MacOS/libexec`.
//!
//! Neomacs has no configure step, so -- exactly as
//! [`super::load::runtime_project_root`] does for the runtime root -- it
//! probes instead of baking, walking a fixed candidate list relative to the
//! running executable and taking the first that exists.  Every shipped
//! layout is a row in [`path_exec_candidates`] and a case in the tests.
//!
//! The probe is existence-gated on purpose: a tree that ships no `libexec`
//! at all falls through to [`PathExecSource::Uninstalled`] and behaves
//! exactly as it did before this module existed, so adding the concept
//! cannot break a layout that has not adopted it yet.

use std::path::{Path, PathBuf};

/// GNU's `${configuration}` (`configure.ac:290`): the host triple naming the
/// architecture this build targets.  Published by `build.rs` from cargo's
/// `TARGET`.
pub const HOST_TRIPLE: &str = env!("NEOVM_HOST_TRIPLE");

/// GNU's `${version}` in `archlibdir`.  The workspace version, which is also
/// what the release artifacts are named after.
pub const ARCHLIB_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `libexec` component itself -- GNU's `${libexecdir}` leaf, and the
/// whole of `ns_applibexecdir`'s tail (`configure.ac:2792`).
pub const LIBEXEC: &str = "libexec";

/// Where a running neomacs found its architecture-dependent directory.
///
/// The variant is the evidence: it names which shipped layout matched, so a
/// wrong answer is diagnosable without re-deriving the probe by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathExecSource {
    /// `<executable dir>/libexec`.
    ///
    /// GNU's `ns_applibexecdir=${ns_appdir}/Contents/MacOS/libexec`
    /// (`configure.ac:2792`), which `epaths-force-ns-self-contained`
    /// (`Makefile.in:511-515`) reduces to the bundle-relative
    /// `Contents/MacOS/libexec` and `ns_relocate` (`src/nsterm.m:524`)
    /// re-absolutises against the running bundle.  We reach the same
    /// directory by walking from the executable, which needs no bundle API
    /// and works for any self-contained tree shaped as executable + private
    /// `libexec`.
    BundleLibexec,

    /// `<prefix>/libexec/neomacs/<version>/<configuration>` reached from
    /// `<prefix>/bin/<executable>`.
    ///
    /// GNU's `archlibdir` (`configure.ac:290`) verbatim, with `neomacs` for
    /// `emacs`.  The `<version>/<configuration>` nesting is what lets two
    /// builds share one `${libexecdir}`, which is the entire reason GNU
    /// nests.
    InstalledArchLib,

    /// The directory the executable itself lives in.
    ///
    /// GNU's uninstalled branch: `init_callproc` (`src/callproc.c:1984-1991`)
    /// replaces `PATH_EXEC` with `<installation-directory>/lib-src` --
    /// "Running uninstalled, so default to tem rather than PATH_EXEC".
    /// A cargo build tree's `lib-src` is `target/<profile>`: it is where
    /// `neomacsclient` and the dump image are written, so it is what
    /// `exec-directory` must name.  This variant is also the terminal
    /// fallback, so resolution never fails.
    Uninstalled,
}

impl PathExecSource {
    /// Whether this directory is a real installation's private archlib, as
    /// opposed to GNU's uninstalled build-tree stand-in.
    pub const fn is_installed(self) -> bool {
        match self {
            Self::BundleLibexec | Self::InstalledArchLib => true,
            Self::Uninstalled => false,
        }
    }
}

/// A resolved `PATH_EXEC`, with the layout that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathExec {
    dir: PathBuf,
    source: PathExecSource,
}

impl PathExec {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub const fn source(&self) -> PathExecSource {
        self.source
    }

    pub fn into_dir(self) -> PathBuf {
        self.dir
    }
}

/// GNU's `archlibdir` tail: `libexec/neomacs/<version>/<configuration>`,
/// relative to an install prefix.
///
/// Packaging scripts stage exactly this path under their prefix; keeping the
/// spelling in one place is what lets a script verify its own staging by
/// asking the staged binary for `exec-directory`.
pub fn archlib_relative_path() -> PathBuf {
    Path::new(LIBEXEC)
        .join("neomacs")
        .join(ARCHLIB_VERSION)
        .join(HOST_TRIPLE)
}

/// The ordered `PATH_EXEC` candidates for an executable at `exe`, nearest
/// layout first.  The last entry is the uninstalled fallback and always
/// exists, so resolution is total.
///
/// `exe` should already be symlink-resolved: GNU follows the chain in
/// `init_cmdargs` (`src/emacs.c:628-637`) precisely so that a
/// `~/.local/bin/neomacs` symlink resolves against the real tree, and
/// [`resolve`] does the same with `canonicalize`.
pub fn path_exec_candidates(exe: &Path) -> Vec<(PathBuf, PathExecSource)> {
    // Same normalisation as `runtime_image_path_for_executable`: a bare
    // relative program name has an empty parent, which is the current
    // directory.
    let dir = match exe.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut candidates = Vec::with_capacity(3);
    candidates.push((dir.join(LIBEXEC), PathExecSource::BundleLibexec));
    if let Some(prefix) = dir.parent() {
        candidates.push((
            prefix.join(archlib_relative_path()),
            PathExecSource::InstalledArchLib,
        ));
    }
    candidates.push((dir.to_path_buf(), PathExecSource::Uninstalled));
    candidates
}

/// Resolve `PATH_EXEC` for an executable at `exe`.
///
/// Existence-gated, first hit wins; the terminal candidate is the
/// executable's own directory, so this never fails.
pub fn path_exec_for_executable(exe: &Path) -> PathExec {
    let candidates = path_exec_candidates(exe);
    let last = candidates.len() - 1;
    for (index, (dir, source)) in candidates.into_iter().enumerate() {
        if index == last || dir.is_dir() {
            return PathExec { dir, source };
        }
    }
    unreachable!("path_exec_candidates always ends with the uninstalled fallback")
}

/// Resolve `PATH_EXEC` for the running executable, or `None` when the OS
/// will not say where that executable is.
///
/// Mirrors GNU's symlink chasing in `init_cmdargs` (`src/emacs.c:628-637`)
/// by canonicalising first, so `~/.local/bin/neomacs -> .../versions/<v>/bin/
/// neomacs` resolves against the installed tree rather than `~/.local/bin`.
///
/// `None` rather than a `"."` stand-in: GNU reaches this code with an
/// absolute compile-time constant and has no such case, and answering the
/// process's working directory would put an arbitrary directory on
/// `exec-path`.  Callers leave `exec-directory` alone instead, which is what
/// `load_pdump` does when `find_emacs_executable` comes back empty
/// (`src/emacs.c:1000-1006`).
pub fn resolve() -> Option<PathExec> {
    let exe = std::env::current_exe().ok()?;
    let resolved = exe.canonicalize().unwrap_or(exe);
    Some(path_exec_for_executable(&resolved))
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
