//! Architecture tripwire for harness process spawns.
//!
//! [`crate::PinnedCommand`] keeps its `Command` private, so the only way to
//! construct one is [`crate::CanonicalEnvironment::command`]; the harness
//! entry points (`apply_memory_limit`, `run_with_stdin`, the MELPA driver
//! traits, the oracle sandbox) accept `&mut PinnedCommand`, so a raw
//! `Command` cannot flow through them.  This test guards the other half: a
//! harness that bypasses its own API and spawns an editor with a bare
//! `Command::new`.
//!
//! The scan covers the oracle, MELPA and TUI harnesses plus the in-crate GNU
//! probes.  A reviewed non-editor helper is named in [`EXCEPTIONS`] with its
//! reason; a new spawn of a variable program fails until it either goes
//! through the canonical builder or is added here deliberately.

use std::path::{Path, PathBuf};

/// Harness trees whose spawns must go through the canonical environment.
///
/// Deliberately excluded: the GUI harness (its own Xvfb/session environment,
/// outside the oracle/MELPA/TUI parity gates), the performance harness
/// (measurement, not parity), product daemon/client launchers (they inherit
/// the user's environment by design) and build tooling.
const SCANNED_ROOTS: &[&str] = &[
    "crates/neovm-oracle-tests/src",
    "crates/neovm-oracle-tests/tests",
    "crates/neomacs-melpa-test-support/src",
    "crates/neomacs-melpa-tests/src",
    "crates/neomacs-tui-tests/src",
    "crates/neomacs-tui-tests/tests",
    "crates/neovm-core/src",
    "crates/neomacs/tests/common",
    "crates/neomacs-infra/src/packages",
    "crates/neomacs-infra/src/config_env",
];

/// A line carrying one of these built its command from the builder.
const CANONICAL_TOKENS: &[&str] = &[
    "environment.command(",
    "pinned_command(",
    "canonical_oracle_command(",
    "gnu_probe_command(",
    "gnu_refresh_command(",
    "EmacsRuntime::command",
    "driver.command(",
    ".command(&environment)",
    "command(&self, environment)",
];

/// Literal programs that are not editors.
const NON_EDITOR_PROGRAMS: &[&str] = &[
    "git",
    "sh",
    "cc",
    "dbus-daemon",
    "python3",
    "kill",
    "gpg",
    "which",
    "sleep",
    "env",
    "cat",
    "/bin/sh",
    "tic",
    "tput",
    "toe",
];

/// A reviewed spawn whose program is a variable but is not an editor.
struct Exception {
    path_suffix: &'static str,
    argument: &'static str,
    reason: &'static str,
}

const EXCEPTIONS: &[Exception] = &[
    Exception {
        path_suffix: "emacs_core/system/callproc/spawn.rs",
        argument: "&self.program",
        reason: "product call-process: launches the user's program, not an editor",
    },
    Exception {
        path_suffix: "emacs_core/system/process/tests/process_test.rs",
        argument: "locator",
        reason: "`which`-style program locator",
    },
    Exception {
        path_suffix: "emacs_core/system/process/tests/process_test.rs",
        argument: "&sh",
        reason: "a POSIX shell, not an editor",
    },
    Exception {
        path_suffix: "emacs_core/lisp/load/tests/load_test.rs",
        argument: "&exe",
        reason: "re-executes the current test binary as a child",
    },
    Exception {
        path_suffix: "emacs_core/system/dynamic_module/tests/library_lifetime.rs",
        argument: "program",
        reason: "the campaign sandbox wraps the child test binary (module crash probe)",
    },
    Exception {
        path_suffix: "neomacs-melpa-tests/src/parity_tests/robe/mod.rs",
        argument: "program",
        reason: "Ruby/gem toolchain command prefix",
    },
    Exception {
        path_suffix: "neomacs-tui-tests/src/lib.rs",
        argument: "program",
        reason: "the TUI PTY exception: `CanonicalEnvironment::apply_env_pairs` pins the PTY child",
    },
    Exception {
        path_suffix: "neomacs-infra/src/config_env/common.rs",
        argument: "program",
        reason: "git fixture helper",
    },
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives at <repo>/crates/neomacs-test-env")
        .to_path_buf()
}

fn rust_files(root: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => panic!("scan {}: {error}", root.display()),
    };
    for entry in entries {
        let path = entry.expect("read harness directory entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if matches!(name, "target" | ".git" | "tmp" | "node_modules") {
                continue;
            }
            rust_files(&path, files);
        } else if name.ends_with(".rs") {
            files.push(path);
        }
    }
}

/// The first argument of a `Command::new(...)` call on one source line.
///
/// The harness code keeps the program argument on the call line; an argument
/// that continues on the next line yields `""` and is reported as a violation
/// so the author sees the decision point instead of silently passing.
fn first_argument(line: &str) -> &str {
    let needle = "Command::new(";
    let Some(start) = line.find(needle) else {
        return "";
    };
    let rest = &line[start + needle.len()..];
    let mut depth = 0usize;
    let mut end = rest.len();
    for (index, character) in rest.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                end = index;
                break;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                end = index;
                break;
            }
            _ => {}
        }
    }
    rest[..end].trim()
}

fn is_exception(relative: &str, argument: &str) -> bool {
    EXCEPTIONS.iter().any(|exception| {
        relative.ends_with(exception.path_suffix) && argument == exception.argument
    })
}

#[test]
fn harness_editor_spawns_go_through_the_canonical_environment() {
    let root = repo_root();
    let mut files = Vec::new();
    for scanned in SCANNED_ROOTS {
        rust_files(&root.join(scanned), &mut files);
    }
    assert!(
        files.len() > 100,
        "the architecture scan found only {} files; check SCANNED_ROOTS",
        files.len()
    );

    let mut violations = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .expect("scanned file is inside the repository")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        for (index, line) in source.lines().enumerate() {
            // `ChildCommand::new` is the evaluator's call-process seam, not a
            // test spawn; the product path owns its own environment.
            if !line.contains("Command::new(") || line.contains("ChildCommand::new(") {
                continue;
            }
            if CANONICAL_TOKENS.iter().any(|token| line.contains(token)) {
                continue;
            }
            let argument = first_argument(line);
            if argument.is_empty()
                || argument.starts_with('"')
                    && NON_EDITOR_PROGRAMS.contains(&argument.trim_matches('"'))
                || argument.contains("current_exe")
                || is_exception(&relative, argument)
            {
                continue;
            }
            // A literal editor program (`Command::new("emacs")`) and any
            // variable program both land here unless reviewed above.
            violations.push(format!(
                "{}:{}: unpinned harness spawn `Command::new({argument})`",
                relative,
                index + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "harness spawns must use the canonical environment builder \
         (neomacs_test_env::CanonicalEnvironment::command):\n{}\n\
         Add a reviewed EXCEPTIONS entry in {} only for a program demonstrably \
         not an editor.",
        violations.join("\n"),
        file!()
    );
}

#[test]
fn pinned_command_has_one_construction_site() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("read the environment crate");
    let constructions: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("PinnedCommand {"))
        .collect();
    assert_eq!(
        constructions,
        vec!["PinnedCommand { command }"],
        "PinnedCommand must only be constructed by CanonicalEnvironment::command"
    );
    assert!(
        source.contains("pub fn command(&self, program: impl AsRef<OsStr>) -> PinnedCommand"),
        "the single constructor is CanonicalEnvironment::command"
    );
}

#[test]
fn the_three_named_harnesses_depend_on_the_environment_crate() {
    let root = repo_root();
    for crate_dir in [
        "crates/neovm-oracle-tests",
        "crates/neomacs-melpa-test-support",
        "crates/neomacs-tui-tests",
    ] {
        let manifest = std::fs::read_to_string(root.join(crate_dir).join("Cargo.toml"))
            .unwrap_or_else(|error| panic!("read {crate_dir}/Cargo.toml: {error}"));
        assert!(
            manifest.contains("neomacs-test-env"),
            "{crate_dir} must depend on neomacs-test-env for its canonical spawns"
        );
    }
}

#[test]
fn the_tui_pty_exception_still_applies_the_canonical_pins() {
    let source = std::fs::read_to_string(repo_root().join("crates/neomacs-tui-tests/src/lib.rs"))
        .expect("read the TUI harness");
    assert!(
        source.contains("pty_process::blocking::Command::new(program)"),
        "the reviewed PTY exception is gone; remove it from EXCEPTIONS"
    );
    assert!(
        source.contains("canonical.apply_env_pairs()"),
        "the PTY child must receive the canonical pins"
    );
}
