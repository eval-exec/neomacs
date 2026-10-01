//! Pinned git repositories for package tests that drive git-backed packages.
//!
//! A fixture is created inside the sandbox the pair already owns, so it is
//! isolated from the host filesystem and removed when the sandbox drops.
//!
//! Every invocation is pinned.  The branch the first commit lands on, the
//! author and committer identity, and the signing default come from arguments
//! rather than from the host's git configuration, and the host's `GIT_*`
//! variables are cleared: a stray `GIT_DIR` or `GIT_AUTHOR_DATE` in the
//! environment must not be able to point these commands at another repository
//! or to change the commit hashes an expected grid pins.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable that carries a fixture's path to an editor's prelude.
pub const GIT_FIXTURE_ENV: &str = "NEOMACS_TUI_GIT_FIXTURE";

/// `GIT_*` variables that redirect git at another repository, index, namespace,
/// or configuration, or that replace the identity and dates a fixture pins.
///
/// Both this module's commands and the pair's editors clear them: the suite can
/// run under a shell that exported `GIT_DIR` -- a hook, or a nested command --
/// and inheriting it would aim the fixture's writes at the host's repository.
pub const GIT_ENV_LEAKS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_NAMESPACE",
    "GIT_REPLACE_REF_BASE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_COUNT",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_AUTHOR_DATE",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_COMMITTER_DATE",
];

/// The identity every fixture commit is written with.
const FIXTURE_NAME: &str = "A U Thor";
const FIXTURE_EMAIL: &str = "a.u.thor@example.com";

/// What a fixture repository contains.
#[derive(Clone, Copy)]
pub struct GitFixtureSpec {
    /// Directory created below the sandbox root that holds the repository.
    pub directory: &'static str,
    /// Branch the fixture's commits land on.
    pub branch: &'static str,
    /// File the commits touch, relative to the repository root.
    pub file: &'static str,
    /// Commits, oldest first.
    pub commits: &'static [GitCommitSpec],
    /// Contents left in the working tree AFTER the last commit, with the index
    /// untouched, so the repository starts with one unstaged modification.
    ///
    /// `None` leaves the working tree clean. This is the state a status or diff
    /// screen reads, and it is written here rather than by the editors so both
    /// peers see the same one.
    pub worktree: Option<&'static str>,
}

/// One commit in a fixture repository.
#[derive(Clone, Copy)]
pub struct GitCommitSpec {
    pub subject: &'static str,
    /// `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE`, in git's raw format.
    pub timestamp: &'static str,
    /// Contents written to the fixture's file before it is committed.
    pub contents: &'static str,
}

/// A repository created below a sandbox.
///
/// The sandbox owns the tree; this value only names it, so the sandbox's drop
/// is what removes the repository.
pub struct GitFixture {
    path: PathBuf,
}

impl GitFixture {
    /// Create `spec`'s repository below `sandbox_root`.
    pub fn create(sandbox_root: &Path, spec: &GitFixtureSpec) -> Result<Self, String> {
        let path = sandbox_root.join(spec.directory);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create git fixture {}: {error}", path.display()))?;
        let fixture = Self { path };
        fixture.git(
            &[
                "-c",
                &format!("init.defaultBranch={}", spec.branch),
                "init",
                "--quiet",
                ".",
            ],
            &[],
        )?;
        for commit in spec.commits {
            std::fs::write(fixture.path.join(spec.file), commit.contents)
                .map_err(|error| format!("write git fixture file {}: {error}", spec.file))?;
            fixture.git(&["add", "--", spec.file], &[])?;
            fixture.git(
                &[
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.autocrlf=false",
                    "commit",
                    "--quiet",
                    "-m",
                    commit.subject,
                ],
                &[
                    ("GIT_AUTHOR_DATE", commit.timestamp),
                    ("GIT_COMMITTER_DATE", commit.timestamp),
                ],
            )?;
        }
        if let Some(contents) = spec.worktree {
            std::fs::write(fixture.path.join(spec.file), contents).map_err(|error| {
                format!("write git fixture working tree {}: {error}", spec.file)
            })?;
        }
        Ok(fixture)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn git(&self, args: &[&str], envs: &[(&str, &str)]) -> Result<(), String> {
        let name = format!("user.name={FIXTURE_NAME}");
        let email = format!("user.email={FIXTURE_EMAIL}");
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.path)
            .args(["-c", name.as_str(), "-c", email.as_str()])
            .args(args)
            .env("TZ", "UTC");
        // Clear the host's redirects first: a leaked identity or date would
        // otherwise outrank the pins below and move the commit hashes.
        for leak in GIT_ENV_LEAKS {
            command.env_remove(leak);
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        for (key, value) in envs {
            command.env(key, value);
        }
        let output = command
            .output()
            .map_err(|error| format!("run git {args:?}: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git {args:?} failed in {}: {}",
                self.path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}
