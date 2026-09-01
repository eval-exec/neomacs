//! Typed build provenance embedded by `build.rs`.
//!
//! Keep this deliberately smaller than a general telemetry fingerprint. The
//! source revision/date identify the code, while target/profile/rustc identify
//! the build domain. Wall-clock build time is omitted so rebuilding identical
//! source does not change the binary merely because time passed.

use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceRevision {
    Git {
        sha: &'static str,
        commit_timestamp: Option<&'static str>,
        worktree: WorktreeState,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeState {
    Clean,
    Dirty,
    Unknown,
}

impl WorktreeState {
    fn embedded() -> Self {
        match usable_embedded_value(option_env!("VERGEN_GIT_DIRTY")) {
            Some("false") => Self::Clean,
            Some("true") => Self::Dirty,
            Some(_) | None => Self::Unknown,
        }
    }

    const fn suffix(self) -> &'static str {
        match self {
            Self::Clean => "",
            Self::Dirty => " (dirty)",
            Self::Unknown => " (worktree state unknown)",
        }
    }
}

impl SourceRevision {
    fn embedded() -> Self {
        match usable_embedded_value(option_env!("VERGEN_GIT_SHA")) {
            None => Self::Unknown,
            Some(sha) => Self::Git {
                sha,
                commit_timestamp: usable_embedded_value(option_env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
                worktree: WorktreeState::embedded(),
            },
        }
    }

    const fn sha(self) -> &'static str {
        match self {
            Self::Git { sha, .. } => sha,
            Self::Unknown => "unknown",
        }
    }

    const fn worktree_suffix(self) -> &'static str {
        match self {
            Self::Git { worktree, .. } => worktree.suffix(),
            Self::Unknown => "",
        }
    }

    const fn commit_timestamp(self) -> &'static str {
        match self {
            Self::Git {
                commit_timestamp: Some(timestamp),
                ..
            } => timestamp,
            Self::Git {
                commit_timestamp: None,
                ..
            }
            | Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildProfile(&'static str);

impl BuildProfile {
    fn embedded() -> Self {
        Self(
            match usable_embedded_value(option_env!("NEOMACS_BUILD_PROFILE")) {
                Some(profile) => profile,
                None => "unknown-profile",
            },
        )
    }

    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildProvenance {
    source: SourceRevision,
    target: &'static str,
    profile: BuildProfile,
    rustc: &'static str,
}

impl BuildProvenance {
    fn embedded() -> Self {
        Self {
            source: SourceRevision::embedded(),
            target: match usable_embedded_value(option_env!("VERGEN_CARGO_TARGET_TRIPLE")) {
                Some(target) => target,
                None => "unknown-target",
            },
            profile: BuildProfile::embedded(),
            rustc: match usable_embedded_value(option_env!("VERGEN_RUSTC_SEMVER")) {
                Some(rustc) => rustc,
                None => "unknown",
            },
        }
    }

    fn write_to(self, output: &mut String) {
        let _ = writeln!(
            output,
            "Git commit: {}{}",
            self.source.sha(),
            self.source.worktree_suffix()
        );
        let _ = writeln!(output, "Source date: {}", self.source.commit_timestamp());
        let _ = writeln!(
            output,
            "Build: {} for {} with rustc {}",
            self.profile.as_str(),
            self.target,
            self.rustc
        );
    }
}

fn usable_embedded_value(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some("") | Some("VERGEN_IDEMPOTENT_OUTPUT") | None => None,
        Some(value) => Some(value),
    }
}

pub(crate) fn write_build_provenance(output: &mut String) {
    BuildProvenance::embedded().write_to(output);
}

#[cfg(test)]
#[path = "build_info_test.rs"]
mod tests;
