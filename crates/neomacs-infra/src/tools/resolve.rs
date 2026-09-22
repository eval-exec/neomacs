//! Resolution strategies for pinned tools.

use std::path::PathBuf;
use std::process::Command;

use super::lock::{ToolLockEntry, tool_lock_entry};

/// How a locked tool is turned into a binary on this machine.
///
/// The enum is exhaustive on purpose: every distribution strategy declares
/// how it is located, version-verified, and cached, and `resolve_tool`'s
/// match must handle each one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSource {
    /// Accept a PATH binary whose `--version` output matches the lock row.
    /// Portable but host-dependent: the CI runner image is the intended
    /// provider.
    System,
    /// Build the tool from a pinned nixpkgs attribute (`nix build`).  Used
    /// on nix machines where the host PATH git diverges from the pin.
    Nix { attribute: &'static str },
    /// Unpack a sha256-pinned release artifact into the tools cache.
    Tarball { url: &'static str },
}

/// A tool resolved to a concrete binary on this machine.
#[derive(Clone, Debug)]
pub struct ResolvedTool {
    pub name: String,
    pub version: String,
    pub strategy: ToolSource,
    /// The resolved binary itself.
    pub bin: PathBuf,
    /// The directory to prepend to `PATH` for editor sessions.
    pub bin_dir: PathBuf,
}

impl ResolvedTool {
    pub fn bin_dir(&self) -> Option<PathBuf> {
        Some(self.bin_dir.clone())
    }
}

#[derive(Debug)]
pub enum ToolsError {
    /// The lock row has no entry for this tool.
    MissingLockRow(String),
    /// A strategy needs a provider this machine lacks (e.g. no `nix`).
    Unavailable(String),
    /// The candidate binary exists but its version does not match the pin.
    VersionMismatch { expected: String, actual: String },
    /// Any other resolution failure.
    Failed(String),
}

impl std::fmt::Display for ToolsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLockRow(name) => write!(formatter, "no lock row for `{name}`"),
            Self::Unavailable(reason) => write!(formatter, "unavailable: {reason}"),
            Self::VersionMismatch { expected, actual } => write!(
                formatter,
                "version mismatch: expected {expected}, found {actual}"
            ),
            Self::Failed(reason) => write!(formatter, "{reason}"),
        }
    }
}

/// Resolve one locked tool to a concrete binary.
pub fn resolve_tool(name: &str, version: &str) -> Result<ResolvedTool, ToolsError> {
    let entry: ToolLockEntry = tool_lock_entry(name).map_err(ToolsError::Failed)?;
    if entry.version != version {
        return Err(ToolsError::Failed(format!(
            "requested {name} {version} but the lock row pins {}",
            entry.version
        )));
    }
    // Strategy selection per tool row: git resolves from the host PATH with
    // a strict version pin (the CI runner image is the provider of record);
    // nix-provisioned builds are the local upgrade path when the host PATH
    // diverges.
    match name {
        "git" | "node" => resolve_system(name, version),
        other => Err(ToolsError::Failed(format!(
            "no resolution strategy for tool `{other}`"
        ))),
    }
}

fn version_of(binary: &PathBuf, name: &str) -> Result<String, ToolsError> {
    let flag = match name {
        "git" | "node" => "--version",
        other => {
            return Err(ToolsError::Failed(format!(
                "no version probe for tool `{other}`"
            )));
        }
    };
    let output = Command::new(binary)
        .arg(flag)
        .output()
        .map_err(|error| ToolsError::Failed(format!("failed to run {name} --version: {error}")))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .map(str::to_string)
        .ok_or_else(|| ToolsError::Failed(format!("{name} --version produced no output")))
}

fn resolve_system(name: &str, version: &str) -> Result<ResolvedTool, ToolsError> {
    let which = Command::new("which")
        .arg(name)
        .output()
        .map_err(|error| ToolsError::Failed(format!("failed to probe PATH for {name}: {error}")))?;
    let candidate = PathBuf::from(String::from_utf8_lossy(&which.stdout).trim().to_string());
    if !which.status.success() || !candidate.is_file() {
        return Err(ToolsError::Unavailable(format!(
            "tool `{name}` ({version}) not found on PATH"
        )));
    }
    let actual = version_of(&candidate, name)?;
    if actual != version {
        return Err(ToolsError::VersionMismatch {
            expected: version.to_string(),
            actual,
        });
    }
    Ok(ResolvedTool {
        name: name.to_string(),
        version: version.to_string(),
        strategy: ToolSource::System,
        bin: candidate.clone(),
        bin_dir: candidate.parent().map(PathBuf::from).ok_or_else(|| {
            ToolsError::Failed(format!("resolved {name} has no parent directory"))
        })?,
    })
}
