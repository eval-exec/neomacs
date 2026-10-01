//! Pinned external toolchains for the package suites.
//!
//! Package tests shell out to real programs — git (git-gutter+, forge,
//! helm-git-grep), Python EPC servers (jedi), node, ruby.  Their outputs and
//! behaviors vary across tool versions, so a recorded expected outcome is
//! only valid for the tool identity it was recorded against.  This module
//! gives every suite the same answer to "which git/python/node?":
//!
//! 1. a lock row pins the tool identity (name, version, per-strategy
//!    resolution data, integrity hash);
//! 2. [`resolve`] produces a `ResolvedTool` whose `bin_dir` suites prepend
//!    to `PATH` at session boot;
//! 3. a mismatch fails loudly (or is a documented skip) — never a silent
//!    use of whichever binary happens to be first on PATH.
//!
//! Resolution strategies form the [`ToolSource`] enum: `System` accepts a
//! PATH binary whose version matches the lock row; `Nix` builds a pinned
//! nixpkgs attribute; `Tarball` unpacks a sha256-pinned release artifact.
//! The match is exhaustive on purpose — a new distribution strategy must
//! declare how it is resolved, verified, and cached.

pub mod lock;
pub mod resolve;

pub use lock::ToolLockEntry;
pub use resolve::{ResolvedTool, ToolSource, ToolsError, resolve_locked, resolve_tool};

use std::path::PathBuf;
use std::sync::OnceLock;

/// The pinned external tools every package-suite session needs on `PATH`.
///
/// Keep this in step with the suites' actual tool requirements; a test that
/// grows a new external dependency adds its row here rather than reaching
/// for whatever the host happens to have.
pub const REQUIRED_TOOLS: &[(&str, &str)] = &[("git", "*"), ("java", "*"), ("node", "*")];

/// A row whose version is `*` asks only that the tool be *provided*.
///
/// The suites' expected values were taken against particular builds, but
/// requiring the same build everywhere costs more than it buys: it makes
/// every host that has moved on skip the suite instead of running it.
///
/// The consequence is deliberate: a suite whose records need a particular
/// build now *runs* and fails on a host that provides a different one,
/// rather than passing while testing nothing.  Those failures are the
/// standing reminder that a provider carrying the pinned versions -- a
/// dedicated nixpkgs input, the way `nix-wpe-webkit` is pinned -- is what
/// they are waiting for.  They are not to be re-recorded against whatever
/// the host happens to have.
pub const ANY_VERSION: &str = "*";

/// Directories to prepend to `PATH` for editor sessions, best effort.
///
/// A row that resolves contributes its `bin` directory; a row that cannot
/// resolve on this machine contributes nothing and is reported on stderr.
/// This is deliberately softer than [`resolve_tool`], which suites call when
/// they need a *gate*: a suite that cannot run here should skip with a reason
/// rather than have the whole session fail to boot.
pub fn provisioned_bin_dirs() -> Vec<PathBuf> {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        let mut dirs = Vec::new();
        for (name, version) in REQUIRED_TOOLS {
            match resolve_locked(name, version) {
                Ok(tool) => {
                    if let Some(dir) = tool.bin_dir() {
                        if !dirs.contains(&dir) {
                            dirs.push(dir);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("tool provision: {name} {version} not provided here: {error}");
                }
            }
        }
        dirs
    })
    .clone()
}

/// Put the pinned tools at the front of this process's `PATH`, once.
///
/// A suite's own tool gate and the editor sessions it spawns must read the
/// same resolution: a gate that asked `which git` would otherwise see the
/// host's git while the sessions were handed the pinned one, and skip a suite
/// that could have run.  Nextest gives each test its own process, so the
/// mutation stays inside the session that asked for it.
pub fn provision_pinned_tools_once() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        let provisioned = provisioned_bin_dirs();
        if provisioned.is_empty() {
            return;
        }
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        match std::env::join_paths(
            provisioned
                .iter()
                .cloned()
                .chain(std::env::split_paths(&inherited)),
        ) {
            // SAFETY: nextest runs each test in its own process, so this
            // cannot race another test's environment.
            Ok(joined) => unsafe { std::env::set_var("PATH", joined) },
            Err(error) => eprintln!("tool provision: cannot prepend to PATH: {error}"),
        }
    });
}

/// Resolve every required tool; returns the directories whose union must be
/// prepended to `PATH` for editor sessions, plus the resolved binaries.
pub fn provision_tools() -> Result<ToolsBin, ToolsError> {
    let mut bin_dirs: Vec<PathBuf> = Vec::new();
    let mut resolved = Vec::new();
    for (name, version) in REQUIRED_TOOLS {
        let tool = resolve_tool(name, version)?;
        if let Some(dir) = tool.bin_dir() {
            if !bin_dirs.contains(&dir) {
                bin_dirs.push(dir);
            }
        }
        resolved.push(tool);
    }
    Ok(ToolsBin { bin_dirs, resolved })
}

/// A set of resolved tools sharing prepended `PATH` directories.
#[derive(Clone, Debug, Default)]
pub struct ToolsBin {
    bin_dirs: Vec<PathBuf>,
    resolved: Vec<ResolvedTool>,
}

impl ToolsBin {
    /// Directories to prepend to `PATH` for editor sessions, in priority
    /// order (first entry wins).
    pub fn bin_dirs(&self) -> &[PathBuf] {
        &self.bin_dirs
    }

    /// The resolved tools behind this set.
    pub fn resolved(&self) -> &[ResolvedTool] {
        &self.resolved
    }
}
