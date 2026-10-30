//! Static tripwire for raw canonical-pin writes on the hermetic harnesses.
//!
//! [`crate::PinnedCommand`] refuses a raw canonical-pin overwrite at run time,
//! but that refusal only fires for a test the developer actually runs.  Two CI
//! jobs on main failed on exactly this class of write, in tests that a local
//! run of the same lane never exercised:
//!
//! * TUI shard 11: `EmacsRuntime::with_env("HOME", ...)` / `("TMPDIR", ...)`
//!   in the Clatter notification service (a canonical environment was already
//!   pinned; the second write panicked inside `EmacsRuntime::command`);
//! * real-GUI MELPA: `command.envs(display.env())` in `run_editor`, where the
//!   display session's environment carries the `LC_ALL` locale pin.
//!
//! This scan is the other half of the contract: it walks the harness trees
//! whose editor commands must be canonically pinned and reports
//!
//! 1. a literal canonical name through `EmacsRuntime::with_env` (the extra
//!    environment is applied by `PinnedCommand::env`, so this always panics),
//! 2. a literal canonical name written with a raw `env`/`env_remove` on the
//!    conventional pinned-command binding `command`, and
//! 3. a wholesale `.envs(<map>)` on that binding (the `run_editor` red): a
//!    map of unknown names needs the typed
//!    `CanonicalPin::from_env_name` + `override_pin` decision at the call
//!    site, not a bulk write.
//!
//! The pinned-name set is exactly [`crate::CanonicalPin::from_env_name`], so
//! the scan cannot drift from the enum.  A site that is demonstrably not a
//! pinned editor command is named in [`EXCEPTIONS`] with its reason, and
//! [`exceptions_still_match_a_raw_write`] fails when an exception goes stale.

use std::path::{Path, PathBuf};

/// Harness trees whose editor commands must go through the canonical
/// environment (the same ownership as `spawn_architecture_test.rs`).
///
/// Deliberately excluded: `neomacs-test-env` itself (it defines the contract
/// and its own unit tests assert the refusal), the product crates, the GUI
/// harness (its own Xvfb/session environment, outside the oracle/MELPA/TUI
/// parity gates), the performance harness (measurement, not parity) and build
/// tooling.
const SCANNED_ROOTS: &[&str] = &[
    "crates/neovm-oracle-tests/src",
    "crates/neovm-oracle-tests/tests",
    "crates/neomacs-melpa-test-support/src",
    "crates/neomacs-melpa-tests/src",
    "crates/neomacs-tui-tests/src",
    "crates/neomacs-tui-tests/tests",
    "crates/neomacs-infra/src/packages",
];

/// A reviewed raw write that is demonstrably not a pinned editor command.
struct Exception {
    path_suffix: &'static str,
    needle: &'static str,
    reason: &'static str,
}

const EXCEPTIONS: &[Exception] = &[
    Exception {
        path_suffix: "neomacs-tui-tests/src/lib.rs",
        needle: ".env(\"TERM\"",
        reason: "the reviewed PTY exception: this is the raw pty_process Command the \
                 canonical Tui pins are applied to first, and TERM is the per-terminal \
                 value this launch selects (see the matching EXCEPTIONS entry in \
                 spawn_architecture_test.rs)",
    },
    Exception {
        path_suffix: "gui_parity_tests/support.rs",
        needle: "envs(packages.process_environment())",
        reason: "PreparedPackageSet::process_environment returns only \
                 NEOMACS_PACKAGE_USER_DIR and NEOMACS_PACKAGE_SOURCE -- package \
                 provenance, never a canonical pin",
    },
];

/// One raw write the scan found.
struct RawWrite {
    relative: String,
    line: usize,
    snippet: String,
}

impl RawWrite {
    fn render(&self, rule: &str) -> String {
        format!("{}:{}: {rule}: {}", self.relative, self.line, self.snippet)
    }
}

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

/// Blank comment lines while preserving every byte offset and line number.
///
/// The harness doc comments quote the very calls this scan looks for (this
/// module's own documentation included), so the scan must not read them as
/// code; replacing each comment character with a space keeps `line_of` and
/// the reported snippets anchored to the original file.
fn blank_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_block = false;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let blank = in_block
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || (trimmed.starts_with('*') && !trimmed.starts_with("*/"));
        if in_block {
            if trimmed.contains("*/") {
                in_block = false;
            }
        } else if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block = true;
        }
        if blank {
            for character in line.chars() {
                out.push(if character == '\n' { '\n' } else { ' ' });
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

fn is_pinned_name(name: &str) -> bool {
    crate::CanonicalPin::from_env_name(name).is_some()
}

/// The first argument as a string literal: `("<name>", ...)`.
fn first_argument_literal(arguments: &str) -> Option<&str> {
    let rest = arguments.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The first quoted string inside `text` (used for `.envs([("A", ..), ..])`).
fn first_quoted(text: &str) -> Option<&str> {
    let start = text.find('"')?;
    let rest = &text[start + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The first argument's text up to its balanced closing parenthesis.
fn first_argument(arguments: &str) -> &str {
    let mut depth = 0usize;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return &arguments[..index],
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    arguments
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn snippet_of(text: &str, offset: usize) -> String {
    let tail = &text[offset..];
    let mut collapsed = String::new();
    for word in tail.split_whitespace().take(12) {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
        if collapsed.len() >= 90 {
            break;
        }
    }
    collapsed
}

/// Scan one source file for raw canonical writes.
fn raw_writes(relative: &str, source: &str) -> (Vec<RawWrite>, Vec<RawWrite>) {
    let code = blank_comments(source);
    let mut literals = Vec::new();
    let mut maps = Vec::new();

    for (offset, _) in code.match_indices(".with_env(") {
        let arguments = &code[offset + ".with_env(".len()..];
        if let Some(name) = first_argument_literal(arguments) {
            if is_pinned_name(name) {
                literals.push(RawWrite {
                    relative: relative.to_owned(),
                    line: line_of(&code, offset),
                    snippet: snippet_of(&code, offset),
                });
            }
        }
    }

    for (offset, _) in code.match_indices("command") {
        if code[..offset].chars().next_back().is_some_and(|previous| {
            previous.is_alphanumeric() || previous == '_' || previous == '.'
        }) {
            continue;
        }
        let after = code[offset + "command".len()..].trim_start();
        let Some(after) = after.strip_prefix('.') else {
            continue;
        };
        let after = after.trim_start();
        // Longest first: `env_remove`/`envs` are prefixes of `env`.
        let (method, after) = if let Some(rest) = after.strip_prefix("env_remove") {
            ("env_remove", rest)
        } else if let Some(rest) = after.strip_prefix("envs") {
            ("envs", rest)
        } else if let Some(rest) = after.strip_prefix("env") {
            ("env", rest)
        } else {
            continue;
        };
        let Some(arguments) = after.trim_start().strip_prefix('(') else {
            continue;
        };
        if method == "envs" {
            let first = arguments.trim_start().chars().next();
            match first {
                Some(character)
                    if character.is_ascii_alphabetic() || character == '_' || character == '&' =>
                {
                    maps.push(RawWrite {
                        relative: relative.to_owned(),
                        line: line_of(&code, offset),
                        snippet: snippet_of(&code, offset),
                    });
                }
                Some(_) => {
                    if let Some(name) = first_quoted(first_argument(arguments)) {
                        if is_pinned_name(name) {
                            literals.push(RawWrite {
                                relative: relative.to_owned(),
                                line: line_of(&code, offset),
                                snippet: snippet_of(&code, offset),
                            });
                        }
                    }
                }
                None => {}
            }
        } else if let Some(name) = first_argument_literal(arguments) {
            if is_pinned_name(name) {
                literals.push(RawWrite {
                    relative: relative.to_owned(),
                    line: line_of(&code, offset),
                    snippet: snippet_of(&code, offset),
                });
            }
        }
    }

    (literals, maps)
}

fn scan() -> (usize, Vec<RawWrite>, Vec<RawWrite>) {
    let root = repo_root();
    let mut files = Vec::new();
    for scanned in SCANNED_ROOTS {
        rust_files(&root.join(scanned), &mut files);
    }
    let mut literals = Vec::new();
    let mut maps = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .expect("scanned file is inside the repository")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        let (file_literals, file_maps) = raw_writes(&relative, &source);
        literals.extend(file_literals);
        maps.extend(file_maps);
    }
    (files.len(), literals, maps)
}

fn is_reviewed(write: &RawWrite) -> bool {
    EXCEPTIONS.iter().any(|exception| {
        write.relative.ends_with(exception.path_suffix) && write.snippet.contains(exception.needle)
    })
}

#[test]
fn canonical_pins_are_never_written_raw_on_the_harness_paths() {
    let (scanned, literals, maps) = scan();
    assert!(
        scanned > 500,
        "the pin scan found only {scanned} files; check SCANNED_ROOTS"
    );

    let mut violations = Vec::new();
    for write in literals.iter().filter(|write| !is_reviewed(write)) {
        violations.push(write.render(
            "raw canonical pin (use override_pin(CanonicalPin::..), or with_env \
             only for non-pinned names)",
        ));
    }
    for write in maps.iter().filter(|write| !is_reviewed(write)) {
        violations.push(write.render(
            "wholesale .envs(<map>) on the pinned command (classify each name with \
             CanonicalPin::from_env_name: override_pin for pins, env for the rest)",
        ));
    }

    assert!(
        violations.is_empty(),
        "canonical environment variables must not be written raw on a \
         PinnedCommand path; the typed layer would panic when the site runs:\n{}\n\
         Add a reviewed EXCEPTIONS entry in {} only for a write demonstrably not \
         aimed at a pinned editor command.",
        violations.join("\n"),
        file!()
    );
}

/// Every exception must still describe a write the scan actually finds: a
/// stale exception silently widens the hole it was meant to document.
#[test]
fn exceptions_still_match_a_raw_write() {
    let (_, literals, maps) = scan();
    let mut stale = Vec::new();
    for exception in EXCEPTIONS {
        let matched = literals
            .iter()
            .chain(maps.iter())
            .any(|write| is_reviewed_for(write, exception));
        if !matched {
            stale.push(format!(
                "{} `{}`: {}",
                exception.path_suffix, exception.needle, exception.reason
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "these pin-bypass exceptions no longer match a raw write; remove or \
         update them:\n{}",
        stale.join("\n")
    );
}

fn is_reviewed_for(write: &RawWrite, exception: &Exception) -> bool {
    write.relative.ends_with(exception.path_suffix) && write.snippet.contains(exception.needle)
}
