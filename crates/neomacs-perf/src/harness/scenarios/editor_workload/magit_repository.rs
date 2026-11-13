//! Deterministic Magit fixture repository preparation: one git environment,
//! one history generator, and the minimal/working repository shapes. Moved
//! verbatim out of the editor-workload family module to keep the family file
//! under its architecture ceiling; the scenarios, names, constants and
//! generated fixture bytes are unchanged.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::harness::ScenarioId;

/// How much history and working-tree change the Magit fixture repository
/// carries.
///
/// `magit-status`'s repository has always been one file, one commit and one
/// modified line, which is the SMALLEST status Magit can render: no staged
/// changes, no untracked files, no stashes, no second commit in the log. Magit
/// spends its time parsing `git diff` output into sections and propertizing
/// them, and that repository gives it one line of diff to parse.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MagitRepositoryShape {
    /// One file, one commit, one modified line.
    Minimal,
    /// A working repository: real history, files of real length, and every
    /// status section populated.
    Working,
}

impl MagitRepositoryShape {
    const fn of(scenario: ScenarioId) -> Self {
        match scenario {
            ScenarioId::MagitStatusHeavy => Self::Working,
            _ => Self::Minimal,
        }
    }
}

/// How far apart two revised lines must be to land in SEPARATE diff hunks.
///
/// `git diff` carries three lines of context either side, so changes closer
/// than seven lines merge into one hunk. Magit builds a section per hunk, so
/// a stride below this measures the diff TEXT without measuring the section
/// machinery -- a 400-line file came out as one 401-line hunk. At this stride
/// each revised line is its own hunk and a file yields twenty of them.
const MAGIT_HUNK_STRIDE: u32 = 20;

/// Deterministic file body: `lines` lines seeded by `(name, revision)`, so a
/// rewrite at a later revision differs from the previous one throughout rather
/// than only at the end, which is what gives `git diff` scattered hunks
/// instead of one appended block.
fn magit_fixture_file(name: &str, revision: u32, lines: u32) -> String {
    let mut out = String::with_capacity(lines as usize * 48);
    out.push_str(&format!(
        "// {name} -- generated fixture, revision {revision}\n"
    ));
    for line in 0..lines {
        // A line's content depends on the revision only once per stride, so a
        // bumped revision rewrites a scattered slice of the file.
        if line % MAGIT_HUNK_STRIDE == revision % MAGIT_HUNK_STRIDE {
            out.push_str(&format!(
                "    let value_{line} = compute_{name}({line}, {revision}); // revised\n"
            ));
        } else {
            out.push_str(&format!("    let value_{line} = compute({line});\n"));
        }
    }
    out
}

/// Every git invocation that builds the Magit fixture runs in this one
/// environment, so the repository is a function of the workload alone.
///
/// Commit IDs hash the author and committer identities and timestamps. Without
/// these pins every run minted a fresh HEAD, fresh commit dates and a stash
/// signed by whoever ran the board, so the status buffer Magit rendered (and
/// the work it did to render it) differed between the arms of one A/B and
/// between machines. The host's global and system git config are shut out for
/// the same reason (`commit.gpgsign`, `init.defaultBranch`, hooks, ...).
const MAGIT_FIXTURE_GIT_ENV: [(&str, &str); 8] = [
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_AUTHOR_NAME", "neomacs-perf"),
    ("GIT_AUTHOR_EMAIL", "perf@example.invalid"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+0000"),
    ("GIT_COMMITTER_NAME", "neomacs-perf"),
    ("GIT_COMMITTER_EMAIL", "perf@example.invalid"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+0000"),
];

fn run_git(repository: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .envs(MAGIT_FIXTURE_GIT_ENV)
        .output()
        .map_err(|error| format!("failed to launch git for Magit fixture: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to prepare Magit fixture repository ({}): {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn git_commit(repository: &Path, message: &str) -> Result<(), String> {
    run_git(repository, &["commit", "--quiet", "-m", message])
}

/// Files, commits, and per-section working-tree changes in the `Working`
/// repository. Sized so `magit-refresh` parses thousands of diff lines across
/// every section it renders, not one line in one.
const MAGIT_HEAVY_FILES: u32 = 40;
const MAGIT_HEAVY_FILE_LINES: u32 = 400;
const MAGIT_HEAVY_COMMITS: u32 = 50;
const MAGIT_HEAVY_UNSTAGED: u32 = 12;
const MAGIT_HEAVY_STAGED: u32 = 8;
const MAGIT_HEAVY_UNTRACKED: u32 = 10;

pub(super) fn prepare_magit_repository(
    run_directory: &Path,
    scenario: ScenarioId,
) -> Result<PathBuf, String> {
    let repository = run_directory.join("magit-repository");
    fs::create_dir_all(&repository).map_err(|error| {
        format!(
            "failed to create Magit fixture repository {}: {error}",
            repository.display()
        )
    })?;
    run_git(&repository, &["init", "--quiet", "--initial-branch=main"])?;
    // Pin everything about `git diff` that the HOST could otherwise decide.
    //
    // Magit renders a section per hunk, and how many hunks a change becomes is
    // `diff.context`: a developer machine with `diff.context = 30` merges what
    // git's default `-U3` reports as forty hunks into one, so the same fixture
    // would hand Magit forty times fewer sections to build there than on CI.
    // That is a property of whoever ran the benchmark, not of the workload, so
    // the repository states it and stops inheriting it.
    for (key, value) in [
        ("diff.context", "3"),
        ("diff.algorithm", "myers"),
        ("diff.noprefix", "false"),
        ("diff.renames", "true"),
        ("core.autocrlf", "false"),
    ] {
        run_git(&repository, &["config", key, value])?;
    }
    match MagitRepositoryShape::of(scenario) {
        MagitRepositoryShape::Minimal => {
            fs::write(repository.join("README.md"), "# neomacs-perf\n")
                .map_err(|error| format!("failed to write Magit fixture: {error}"))?;
            run_git(&repository, &["add", "README.md"])?;
            git_commit(&repository, "fixture")?;
            fs::write(repository.join("README.md"), "# neomacs-perf\nmodified\n")
                .map_err(|error| format!("failed to modify Magit fixture: {error}"))?;
        }
        MagitRepositoryShape::Working => {
            prepare_working_magit_repository(&repository)?;
        }
    }
    Ok(repository)
}

/// Build history and then a working tree that populates every status section.
///
/// Magit's default status renders untracked files, unstaged changes, staged
/// changes, stashes, and the recent-commit log. Each is filled here, so
/// `magit-refresh` runs the section machinery it runs in a session instead of
/// short-circuiting on empty output.
fn prepare_working_magit_repository(repository: &Path) -> Result<(), String> {
    let name_of = |index: u32| format!("src/module_{index:02}.rs");
    fs::create_dir_all(repository.join("src"))
        .map_err(|error| format!("failed to create Magit fixture source directory: {error}"))?;

    // History. The first commit lands every file; the rest each rewrite a
    // scattered slice of a rotating few, so the log has real diffs behind it.
    for index in 0..MAGIT_HEAVY_FILES {
        fs::write(
            repository.join(name_of(index)),
            magit_fixture_file(&format!("module_{index:02}"), 0, MAGIT_HEAVY_FILE_LINES),
        )
        .map_err(|error| format!("failed to write Magit fixture source: {error}"))?;
    }
    run_git(repository, &["add", "."])?;
    git_commit(repository, "fixture: import the tree")?;
    for commit in 1..=MAGIT_HEAVY_COMMITS {
        for slot in 0..3 {
            let index = (commit * 3 + slot) % MAGIT_HEAVY_FILES;
            fs::write(
                repository.join(name_of(index)),
                magit_fixture_file(
                    &format!("module_{index:02}"),
                    commit,
                    MAGIT_HEAVY_FILE_LINES,
                ),
            )
            .map_err(|error| format!("failed to rewrite Magit fixture source: {error}"))?;
        }
        run_git(repository, &["add", "."])?;
        git_commit(repository, &format!("fixture: revision {commit}"))?;
    }

    // A stash, so the stash section is non-empty. Made from a change that is
    // then put away, which is what a stash is.
    fs::write(
        repository.join(name_of(0)),
        magit_fixture_file(
            "module_00",
            MAGIT_HEAVY_COMMITS + 100,
            MAGIT_HEAVY_FILE_LINES,
        ),
    )
    .map_err(|error| format!("failed to write Magit stash fixture: {error}"))?;
    run_git(
        repository,
        &["stash", "push", "--quiet", "-m", "fixture stash"],
    )?;

    // Staged changes.
    for index in 0..MAGIT_HEAVY_STAGED {
        fs::write(
            repository.join(name_of(index)),
            magit_fixture_file(
                &format!("module_{index:02}"),
                MAGIT_HEAVY_COMMITS + 1,
                MAGIT_HEAVY_FILE_LINES,
            ),
        )
        .map_err(|error| format!("failed to write staged Magit fixture: {error}"))?;
    }
    run_git(repository, &["add", "."])?;

    // Unstaged changes, on a disjoint set so both sections stay populated.
    for offset in 0..MAGIT_HEAVY_UNSTAGED {
        let index = MAGIT_HEAVY_STAGED + offset;
        fs::write(
            repository.join(name_of(index)),
            magit_fixture_file(
                &format!("module_{index:02}"),
                MAGIT_HEAVY_COMMITS + 2,
                MAGIT_HEAVY_FILE_LINES,
            ),
        )
        .map_err(|error| format!("failed to write unstaged Magit fixture: {error}"))?;
    }

    // Untracked files.
    for index in 0..MAGIT_HEAVY_UNTRACKED {
        fs::write(
            repository.join(format!("src/scratch_{index:02}.rs")),
            magit_fixture_file(&format!("scratch_{index:02}"), 0, 60),
        )
        .map_err(|error| format!("failed to write untracked Magit fixture: {error}"))?;
    }
    Ok(())
}
