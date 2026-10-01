//! Resolution strategies for pinned tools.

use std::path::PathBuf;
use std::process::Command;

use super::lock::{ToolLockEntry, tool_lock_entry};

/// How a locked tool is turned into a binary on this machine.
///
/// The enum is exhaustive on purpose: every distribution strategy declares
/// how it is located, version-verified, and cached, and `resolve_tool`'s
/// match must handle each one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolSource {
    /// Accept a PATH binary whose `--version` output matches the lock row.
    /// Portable but host-dependent: the CI runner image is the intended
    /// provider.
    System,
    /// Build the tool from a pinned nixpkgs attribute (`nix build`).  Used
    /// on nix machines where the host PATH git diverges from the pin.
    Nix { attribute: String },
    /// Unpack a sha256-pinned release artifact into the tools cache.
    Tarball { url: String },
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
///
/// Provisions the pinned tools into this process's `PATH` first, so a gate and
/// the session it guards agree on which build of a tool is in play.
pub fn resolve_tool(name: &str, version: &str) -> Result<ResolvedTool, ToolsError> {
    crate::tools::provision_pinned_tools_once();
    resolve_locked(name, version)
}

/// As [`resolve_tool`], without triggering provisioning.
///
/// Provisioning itself resolves rows through here: the public entry point
/// would otherwise re-enter it.
pub fn resolve_locked(name: &str, version: &str) -> Result<ResolvedTool, ToolsError> {
    let entry: ToolLockEntry = tool_lock_entry(name).map_err(ToolsError::Failed)?;
    if version != crate::tools::ANY_VERSION && entry.version != version {
        return Err(ToolsError::Failed(format!(
            "requested {name} {version} but the lock row pins {}",
            entry.version
        )));
    }
    // The row names its own strategy, so adding a tool is a row rather than a
    // match arm.
    match entry.source {
        "system" => resolve_system(name, version),
        source if source.starts_with("nix:") => {
            let attribute = &source["nix:".len()..];
            resolve_nix(name, version, attribute)
        }
        other => Err(ToolsError::Failed(format!(
            "no resolution strategy for source `{other}`"
        ))),
    }
}

/// Build a pinned attribute of the workspace flake and use its `bin`.
///
/// The flake is the workspace's own (`.#<attribute>`), so the tool identity is
/// the nixpkgs revision the repository already locks, not whichever build the
/// host's package manager has.  This is the strategy for a host whose PATH
/// diverges from the pin, or which has no such tool at all.
fn resolve_nix(name: &str, version: &str, attribute: &str) -> Result<ResolvedTool, ToolsError> {
    if !command_succeeds("nix", &["--version"]) {
        return Err(ToolsError::Unavailable(format!(
            "nix is not installed, so the pinned `{attribute}` cannot be built"
        )));
    }
    let flake = format!("{}#{attribute}", crate::workspace_root().display());
    let output = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command flakes",
            "build",
            "--no-link",
            "--print-out-paths",
            &flake,
        ])
        .output()
        .map_err(|error| ToolsError::Failed(format!("failed to run nix build {flake}: {error}")))?;
    if !output.status.success() {
        return Err(ToolsError::Failed(format!(
            "nix build {flake} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let store_path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
        .map(PathBuf::from)
        .ok_or_else(|| ToolsError::Failed(format!("nix build {flake} printed no store path")))?;
    let bin_dir = store_path.join("bin");
    let bin = bin_dir.join(name);
    if !bin.is_file() {
        return Err(ToolsError::Failed(format!(
            "{flake} has no `bin/{name}` (looked in {})",
            bin_dir.display()
        )));
    }
    // A nix row's *pin* is the flake's locked nixpkgs revision, so the column
    // is a prefix sanity check rather than an exact match: the revision
    // decides the patch level, and recording it here would mean editing the
    // lock whenever the flake updates.  `system` rows keep the exact match,
    // because nothing pins a host binary but this column.
    let actual = version_of(&bin, name)?;
    if version != crate::tools::ANY_VERSION && !actual.starts_with(version) {
        return Err(ToolsError::VersionMismatch {
            expected: format!("{version} (as a prefix)"),
            actual,
        });
    }
    Ok(ResolvedTool {
        name: name.to_string(),
        version: actual.clone(),
        strategy: ToolSource::Nix {
            attribute: attribute.to_string(),
        },
        bin,
        bin_dir,
    })
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// How to ask a tool for its version, and where it answers.
///
/// Most tools print to stdout with `--version`; the JDK prints to stderr and
/// spells the flag differently, and says `openjdk version "27"` rather than a
/// bare number.  A tool with no row here cannot be version-verified, which is
/// a failure rather than a guess.
fn version_probe(name: &str) -> Option<(&'static str, bool, &'static str)> {
    // (flag, version is on stderr, prefix to strip)
    match name {
        "git" => Some(("--version", false, "git version ")),
        "node" => Some(("--version", false, "v")),
        "java" => Some(("-version", true, "openjdk version \"")),
        "python3" => Some(("--version", false, "Python ")),
        "rg" => Some(("--version", false, "ripgrep ")),
        _ => None,
    }
}

fn version_of(binary: &PathBuf, name: &str) -> Result<String, ToolsError> {
    let Some((flag, on_stderr, strip)) = version_probe(name) else {
        return Err(ToolsError::Failed(format!(
            "no version probe for tool `{name}`"
        )));
    };
    let output = Command::new(binary)
        .arg(flag)
        .output()
        .map_err(|error| ToolsError::Failed(format!("failed to run {name} {flag}: {error}")))?;
    let stream = if on_stderr {
        &output.stderr
    } else {
        &output.stdout
    };
    let line = String::from_utf8_lossy(stream)
        .lines()
        .next()
        .map(str::trim)
        .map(str::to_string)
        .ok_or_else(|| ToolsError::Failed(format!("{name} {flag} produced no output")))?;
    let line = line.strip_prefix(strip).unwrap_or(&line).to_string();
    // A JDK ends its version with a quote and build metadata: `21.0.5" 2024-10-15`.
    let line = line
        .split(|character: char| character == '"' || character == ' ')
        .next()
        .unwrap_or(&line)
        .to_string();
    Ok(line)
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
    if version != crate::tools::ANY_VERSION && actual != version {
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
