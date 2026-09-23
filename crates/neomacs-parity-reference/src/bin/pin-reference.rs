//! `cargo run -p xtask -- pin-reference` --- deliberate re-baselining of the
//! GNU Emacs every parity number is measured against.
//!
//! It lives beside the manifest it rewrites rather than inside xtask, which
//! launches it. xtask is a command launcher; linking this in dragged the
//! parity library into every `cargo xtask fresh-build`.
//!
//! # Why re-pinning is a command and not an edit (ledger 214)
//!
//! A pin that cannot be updated is a pin people delete, so this must be
//! possible.  But the convention this project settled on is that *a rebuild of
//! the GNU mirror is its own change with its own re-baselining, never a side
//! effect of a session*, and a hand-edit of `parity-reference.toml` cannot
//! encode that: it leaves no record of what changed or why, and the next reader
//! cannot tell a considered re-baselining from a stray `make`.
//!
//! So re-pinning is one command that refuses without a `--reason`, prints the
//! full before/after of every field it is about to change, and appends a dated
//! line to the log inside the manifest.  The record is written by the same act
//! that changes the pin, which is the only way it does not get forgotten.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use neomacs_parity_reference::{
    ObservedReference, ReferenceManifest, manifest_path, parse_manifest, render_manifest_keys,
};

type Result<T> = std::result::Result<T, String>;

/// The marker in `parity-reference.toml` that new log lines are inserted after.
const LOG_HEADER: &str = "# RE-BASELINING LOG";

/// A non-empty re-baselining reason.
///
/// The `--reason` requirement is enforced by this TYPE, not by a check at the
/// one call site that happens to parse argv.  `Reason` has no public
/// constructor but [`Reason::new`], which rejects blank text, and every
/// function that writes a log entry takes a `&Reason` rather than a `&str` ---
/// so a caller added later cannot re-pin without one, and cannot pass `""`
/// either.  A `--reason` that one caller can omit would make re-baselining
/// neither explicit nor self-documenting, which is the whole requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reason(String);

impl Reason {
    fn new(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_string()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

struct Options {
    emacs: PathBuf,
    reason: Reason,
    mirror_commit: Option<String>,
    dry_run: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut emacs = None;
        let mut reason = None;
        let mut mirror_commit = None;
        let mut dry_run = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut next = |what: &str| -> Result<String> {
                args.next()
                    .map(|value| value.to_string_lossy().into_owned())
                    .ok_or_else(|| format!("pin-reference: {what} needs a value"))
            };
            match arg.to_string_lossy().as_ref() {
                "--emacs" => emacs = Some(PathBuf::from(next("--emacs")?)),
                "--reason" => reason = Some(next("--reason")?),
                "--mirror-commit" => mirror_commit = Some(next("--mirror-commit")?),
                "--dry-run" => dry_run = true,
                "--help" | "-h" => return Err(usage()),
                other => {
                    return Err(format!(
                        "pin-reference: unknown argument {other:?}\n{}",
                        usage()
                    ));
                }
            }
        }
        let emacs =
            emacs.ok_or_else(|| format!("pin-reference: --emacs is required\n{}", usage()))?;
        // The reason is mandatory because the log is the whole point.  A
        // re-baselining nobody wrote down is the side effect this command
        // exists to make impossible.
        let reason = reason.as_deref().and_then(Reason::new).ok_or_else(|| {
            format!(
                "pin-reference: --reason is required and must not be empty.\n\
                     Re-pinning the GNU reference re-baselines every parity number this \
                     project publishes; the log entry saying why is not optional.\n{}",
                usage()
            )
        })?;
        Ok(Self {
            emacs,
            reason,
            mirror_commit,
            dry_run,
        })
    }
}

fn usage() -> String {
    "usage: cargo run -p xtask -- pin-reference --emacs PATH --reason \"...\" \
     [--mirror-commit SHA] [--dry-run]"
        .to_string()
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let options = Options::parse(args)?;
    let path = manifest_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("pin-reference: cannot read {}: {error}", path.display()))?;
    let current = parse_manifest(&text)
        .map_err(|error| format!("pin-reference: cannot parse {}: {error}", path.display()))?;

    let observed = neomacs_parity_reference::observe(&options.emacs)
        .map_err(|error| format!("pin-reference: {error}"))?;
    let proposed = ReferenceManifest {
        schema: current.schema.clone(),
        emacs_version: emacs_version(&observed.executable)?,
        mirror_commit: match &options.mirror_commit {
            Some(commit) => commit.clone(),
            None => mirror_commit(&observed.executable)?,
        },
        build_time: build_time(&observed.executable)?,
        fingerprint: observed.fingerprint.clone(),
        executable_sha256: observed.executable_sha256.clone(),
        executable_size: observed.executable_size,
        pdmp_sha256: observed.pdmp_sha256.clone(),
        pdmp_size: observed.pdmp_size,
    };

    report(&observed);
    let changes = differences(&current, &proposed);
    if changes.is_empty() {
        println!("\nThe pin already describes this build; nothing to re-baseline.");
        return Ok(());
    }

    println!("\nThis re-baselines every parity number this project publishes:");
    for (field, was, now) in &changes {
        println!("  {field}\n    was: {was}\n    now: {now}");
    }
    println!("\n  reason: {}", options.reason);

    if options.dry_run {
        println!("\n--dry-run: {} was not written.", path.display());
        return Ok(());
    }

    let rewritten = rewrite(&text, &proposed, &options.reason)?;
    // Never write a manifest this project cannot read back.
    let reparsed = parse_manifest(&rewritten)
        .map_err(|error| format!("pin-reference: refusing to write an unreadable pin: {error}"))?;
    if reparsed != proposed {
        return Err("pin-reference: the rewritten pin does not read back as intended".to_string());
    }
    std::fs::write(&path, &rewritten)
        .map_err(|error| format!("pin-reference: cannot write {}: {error}", path.display()))?;
    println!("\nRe-pinned {}.", path.display());
    println!("Commit this file on its own: a re-baselining is its own change.");
    Ok(())
}

fn report(observed: &ObservedReference) {
    println!("observed:");
    println!("  executable       {}", observed.executable.display());
    println!("  dump             {}", observed.pdmp.display());
    println!("  fingerprint      {}", observed.fingerprint);
    println!("  executable sha256 {}", observed.executable_sha256);
    println!("  dump sha256       {}", observed.pdmp_sha256);
}

fn differences(
    current: &ReferenceManifest,
    proposed: &ReferenceManifest,
) -> Vec<(&'static str, String, String)> {
    let fields: [(&'static str, String, String); 8] = [
        (
            "emacs_version",
            current.emacs_version.clone(),
            proposed.emacs_version.clone(),
        ),
        (
            "mirror_commit",
            current.mirror_commit.clone(),
            proposed.mirror_commit.clone(),
        ),
        (
            "build_time",
            current.build_time.clone(),
            proposed.build_time.clone(),
        ),
        (
            "fingerprint",
            current.fingerprint.clone(),
            proposed.fingerprint.clone(),
        ),
        (
            "executable_sha256",
            current.executable_sha256.clone(),
            proposed.executable_sha256.clone(),
        ),
        (
            "executable_size",
            current.executable_size.to_string(),
            proposed.executable_size.to_string(),
        ),
        (
            "pdmp_sha256",
            current.pdmp_sha256.clone(),
            proposed.pdmp_sha256.clone(),
        ),
        (
            "pdmp_size",
            current.pdmp_size.to_string(),
            proposed.pdmp_size.to_string(),
        ),
    ];
    fields
        .into_iter()
        .filter(|(_, was, now)| was != now)
        .collect()
}

/// Replace the key block, keeping every comment, and add the log entry.
fn rewrite(text: &str, proposed: &ReferenceManifest, reason: &Reason) -> Result<String> {
    let mut out = String::new();
    let mut keys_written = false;
    let mut log_written = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            if !log_written && trimmed.starts_with(LOG_HEADER) {
                for entry in log_entry(proposed, reason) {
                    out.push_str(&entry);
                    out.push('\n');
                }
                log_written = true;
            }
            continue;
        }
        // A key line: emit the whole new block in place of the first one and
        // drop the rest, so the order is always what `render_manifest_keys`
        // produces and the two readers never see a stale ordering.
        if !keys_written {
            out.push_str(&render_manifest_keys(proposed));
            keys_written = true;
        }
    }
    if !keys_written {
        return Err("pin-reference: the manifest has no key block to replace".to_string());
    }
    if !log_written {
        return Err(format!(
            "pin-reference: the manifest has no \"{LOG_HEADER}\" line to append to; \
             a re-baselining that cannot be recorded must not be written"
        ));
    }
    Ok(out)
}

fn log_entry(proposed: &ReferenceManifest, reason: &Reason) -> Vec<String> {
    let date = today();
    let mut lines = vec![format!(
        "#   {date}  GNU Emacs {} built {} from mirror commit {}.",
        proposed.emacs_version,
        proposed.build_time,
        &proposed.mirror_commit[..11],
    )];
    // Wrap the reason so the manifest stays readable in a terminal.
    let mut current = String::from("#               ");
    for word in reason.as_str().split_whitespace() {
        if current.len() + word.len() + 1 > 78 && current.trim_start_matches('#').trim() != "" {
            lines.push(current.trim_end().to_string());
            current = String::from("#               ");
        }
        current.push_str(word);
        current.push(' ');
    }
    if current.trim_start_matches('#').trim() != "" {
        lines.push(current.trim_end().to_string());
    }
    lines
}

/// Today, as `YYYY-MM-DD`, without pulling in a date crate.
fn today() -> String {
    let output = Command::new("date").arg("+%Y-%m-%d").output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown-date".to_string(),
    }
}

fn emacs_version(executable: &Path) -> Result<String> {
    let output = Command::new(executable)
        .args([
            "--batch",
            "--no-site-file",
            "--eval",
            "(princ emacs-version)",
        ])
        .output()
        .map_err(|error| {
            format!(
                "pin-reference: cannot run {}: {error}",
                executable.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "pin-reference: {} could not report emacs-version: {}",
            executable.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err("pin-reference: emacs-version came back empty".to_string());
    }
    Ok(version)
}

fn build_time(executable: &Path) -> Result<String> {
    let output = Command::new(executable)
        .args([
            "--batch",
            "--no-site-file",
            "--eval",
            "(princ (format-time-string \"%Y-%m-%dT%H:%M:%S%z\" emacs-build-time))",
        ])
        .output()
        .map_err(|error| {
            format!(
                "pin-reference: cannot run {}: {error}",
                executable.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "pin-reference: {} could not report emacs-build-time",
            executable.display()
        ));
    }
    let stamp = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stamp.is_empty() {
        return Err("pin-reference: emacs-build-time came back empty".to_string());
    }
    // The manifest format has no escapes; a `+`-form offset is plain enough.
    Ok(insert_offset_colon(&stamp))
}

/// `+0500` -> `+05:00`, matching the checked-in `build_time` shape.
fn insert_offset_colon(stamp: &str) -> String {
    let bytes = stamp.as_bytes();
    if bytes.len() >= 5 {
        let split = bytes.len() - 5;
        if matches!(bytes[split], b'+' | b'-') && bytes[split + 1..].iter().all(u8::is_ascii_digit)
        {
            return format!(
                "{}{}:{}",
                &stamp[..split + 1],
                &stamp[split + 1..split + 3],
                &stamp[split + 3..]
            );
        }
    }
    stamp.to_string()
}

fn mirror_commit(executable: &Path) -> Result<String> {
    let directory = executable
        .parent()
        .ok_or_else(|| "pin-reference: the executable has no parent directory".to_string())?;
    let output = Command::new("git")
        .args(["-C"])
        .arg(directory)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("pin-reference: cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "pin-reference: {} is not in a git checkout, so the mirror commit cannot be \
             recorded; pass --mirror-commit SHA",
            directory.display()
        ));
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if commit.len() != 40 {
        return Err(format!("pin-reference: git gave an odd commit {commit:?}"));
    }

    // A dirty mirror cannot be recorded honestly: the commit would name a tree
    // the binary was not built from.
    let status = Command::new("git")
        .args(["-C"])
        .arg(directory)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|error| format!("pin-reference: cannot run git status: {error}"))?;
    if !String::from_utf8_lossy(&status.stdout).trim().is_empty() {
        return Err(format!(
            "pin-reference: the mirror at {} has uncommitted changes, so `{commit}' would not \
             describe what was built.  Commit or clean the mirror, or pass --mirror-commit \
             to record the commit deliberately.",
            directory.display()
        ));
    }
    Ok(commit)
}

fn main() -> std::process::ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "tests/pin_reference_test.rs"]
mod tests;
