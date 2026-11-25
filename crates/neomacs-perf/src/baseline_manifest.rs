//! Immutable performance baseline manifest loader and verifier.
//!
//! Guard suites compare a candidate build against a frozen release binary plus
//! its matching portable dump.  This module reads the committed JSON manifest
//! (for example `.github/perf-baselines/v0.0.21.json`) and verifies the exact
//! byte size and SHA-256 digest of BOTH the baseline executable and its sibling
//! `.pdump` before any benchmark runs.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::PerfError;

pub(crate) const BASELINE_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineAssetEntry {
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineBinaryEntry {
    pub archive_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselinePdumpEntry {
    pub archive_path: String,
    pub relative_to_binary: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineManifest {
    pub schema_version: u32,
    pub version: String,
    pub git_commit: String,
    pub target: String,
    pub release_asset: BaselineAssetEntry,
    pub binary: BaselineBinaryEntry,
    pub pdump: BaselinePdumpEntry,
}

/// Immutable provenance recorded in `suite.json` after verifying a baseline
/// executable and its sibling portable dump against a committed manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedBaselineManifest {
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
    pub version: String,
    pub git_commit: String,
    pub binary_path: PathBuf,
    pub binary_sha256: String,
    pub binary_size_bytes: u64,
    pub pdump_path: PathBuf,
    pub pdump_sha256: String,
    pub pdump_size_bytes: u64,
}

impl BaselineManifest {
    pub fn load(path: &Path) -> Result<(Self, String), PerfError> {
        let bytes = fs::read(path).map_err(|error| PerfError::InvalidBaselineManifest {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let manifest: Self =
            serde_json::from_slice(&bytes).map_err(|error| PerfError::InvalidBaselineManifest {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        if manifest.schema_version != BASELINE_MANIFEST_SCHEMA_VERSION {
            return Err(PerfError::InvalidBaselineManifest {
                path: path.to_path_buf(),
                message: format!(
                    "unsupported baseline manifest schema version {}; expected {}",
                    manifest.schema_version, BASELINE_MANIFEST_SCHEMA_VERSION
                ),
            });
        }
        validate_hex_sha256(path, "release_asset.sha256", &manifest.release_asset.sha256)?;
        validate_hex_sha256(path, "binary.sha256", &manifest.binary.sha256)?;
        validate_hex_sha256(path, "pdump.sha256", &manifest.pdump.sha256)?;
        if manifest.pdump.relative_to_binary.as_os_str().is_empty()
            || manifest.pdump.relative_to_binary.is_absolute()
        {
            return Err(PerfError::InvalidBaselineManifest {
                path: path.to_path_buf(),
                message: format!(
                    "pdump.relative_to_binary must be a non-empty relative path, got {:?}",
                    manifest.pdump.relative_to_binary
                ),
            });
        }
        Ok((manifest, sha256_hex(&bytes)))
    }

    pub fn verify_editor(
        &self,
        manifest_path: &Path,
        manifest_sha256: String,
        baseline_editor: &Path,
    ) -> Result<VerifiedBaselineManifest, PerfError> {
        let binary_path = fs::canonicalize(baseline_editor).map_err(|error| {
            PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "binary",
                path: baseline_editor.to_path_buf(),
                reason: format!("cannot resolve baseline binary: {error}"),
            }
        })?;
        let (binary_sha256, binary_size_bytes) =
            hash_and_size(&binary_path).map_err(|error| PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "binary",
                path: binary_path.clone(),
                reason: error,
            })?;
        if binary_size_bytes != self.binary.size_bytes || binary_sha256 != self.binary.sha256 {
            return Err(PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "binary",
                path: binary_path,
                reason: format!(
                    "expected sha256={} ({} B), observed sha256={} ({} B)",
                    self.binary.sha256, self.binary.size_bytes, binary_sha256, binary_size_bytes
                ),
            });
        }

        let Some(binary_dir) = binary_path.parent() else {
            return Err(PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "pdump",
                path: binary_path,
                reason: "baseline binary has no parent directory for sibling pdump lookup"
                    .to_owned(),
            });
        };
        let pdump_candidate = binary_dir.join(&self.pdump.relative_to_binary);
        let pdump_path = fs::canonicalize(&pdump_candidate).map_err(|error| {
            PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "pdump",
                path: pdump_candidate.clone(),
                reason: format!("missing sibling portable dump: {error}"),
            }
        })?;
        let (pdump_sha256, pdump_size_bytes) =
            hash_and_size(&pdump_path).map_err(|error| PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "pdump",
                path: pdump_path.clone(),
                reason: error,
            })?;
        if pdump_size_bytes != self.pdump.size_bytes || pdump_sha256 != self.pdump.sha256 {
            return Err(PerfError::BaselineManifestMismatch {
                manifest: manifest_path.to_path_buf(),
                subject: "pdump",
                path: pdump_path,
                reason: format!(
                    "expected sha256={} ({} B), observed sha256={} ({} B)",
                    self.pdump.sha256, self.pdump.size_bytes, pdump_sha256, pdump_size_bytes
                ),
            });
        }

        Ok(VerifiedBaselineManifest {
            manifest_path: manifest_path.to_path_buf(),
            manifest_sha256,
            version: self.version.clone(),
            git_commit: self.git_commit.clone(),
            binary_path,
            binary_sha256,
            binary_size_bytes,
            pdump_path,
            pdump_sha256,
            pdump_size_bytes,
        })
    }
}

fn validate_hex_sha256(path: &Path, field: &str, digest: &str) -> Result<(), PerfError> {
    let valid = digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(PerfError::InvalidBaselineManifest {
            path: path.to_path_buf(),
            message: format!(
                "{field} must be a 64-character lowercase hex SHA-256, got {digest:?}"
            ),
        });
    }
    Ok(())
}

fn hash_and_size(path: &Path) -> Result<(String, u64), String> {
    let metadata = fs::metadata(path).map_err(|error| format!("cannot stat file: {error}"))?;
    if !metadata.is_file() {
        return Err("path is not a regular file".to_owned());
    }
    let mut file = fs::File::open(path).map_err(|error| format!("cannot open file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read file: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size_bytes += read as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((hex, size_bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Canonical absolute paths to external host tools (`perf`, `taskset`) used by
/// a manifest-pinned guard comparison. Resolved once from `PATH` with shadowing
/// checks so guard runs never spawn tools by bare name through an ambient `PATH`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedGuardTools {
    pub perf_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taskset_path: Option<PathBuf>,
}

/// Resolve a host tool to one unambiguous canonical absolute executable in `PATH`.
///
/// Rejects relative `PATH` directories providing the tool and rejects any
/// shadowed tool where two distinct canonical executables share the same name
/// in `PATH`.
pub fn resolve_pinned_tool(tool: &str, path_env: Option<&OsStr>) -> Result<PathBuf, String> {
    let Some(path_env) = path_env else {
        return Err(format!(
            "PATH is unset; cannot resolve required guard tool `{tool}`"
        ));
    };
    let mut distinct: Vec<PathBuf> = Vec::new();
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(tool);
        if !is_executable_file(&candidate) {
            continue;
        }
        if !dir.is_absolute() {
            return Err(format!(
                "PATH contains relative directory `{}` providing `{tool}`; guard tools must resolve from absolute PATH entries",
                dir.display()
            ));
        }
        let canonical = fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "cannot canonicalize guard tool `{tool}` at {}: {error}",
                candidate.display()
            )
        })?;
        if !distinct.contains(&canonical) {
            distinct.push(canonical);
        }
    }
    match distinct.as_slice() {
        [] => Err(format!("required guard tool `{tool}` not found in PATH")),
        [only] => Ok(only.clone()),
        [first, second, ..] => Err(format!(
            "PATH shadows `{tool}`: {} shadows {}",
            first.display(),
            second.display()
        )),
    }
}

/// Resolve the pinned external host tools required by a manifest-pinned guard.
pub fn resolve_guard_tools(
    path_env: Option<&OsStr>,
    require_taskset: bool,
) -> Result<ResolvedGuardTools, PerfError> {
    let perf_path = resolve_pinned_tool("perf", path_env).map_err(|reason| {
        PerfError::GuardToolPathViolation {
            tool: "perf",
            reason,
        }
    })?;
    let taskset_path = if require_taskset {
        Some(resolve_pinned_tool("taskset", path_env).map_err(|reason| {
            PerfError::GuardToolPathViolation {
                tool: "taskset",
                reason,
            }
        })?)
    } else {
        None
    };
    Ok(ResolvedGuardTools {
        perf_path,
        taskset_path,
    })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
#[path = "baseline_manifest/tests/baseline_manifest_test.rs"]
mod tests;
