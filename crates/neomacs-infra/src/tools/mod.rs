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
pub use resolve::{ResolvedTool, ToolSource, ToolsError, resolve_tool};

use std::path::PathBuf;

/// The pinned external tools every package-suite session needs on `PATH`.
///
/// Keep this in step with the suites' actual tool requirements; a test that
/// grows a new external dependency adds its row here rather than reaching
/// for whatever the host happens to have.
pub const REQUIRED_TOOLS: &[(&str, &str)] = &[("git", "2.51.2"), ("node", "v22.22.2")];

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
