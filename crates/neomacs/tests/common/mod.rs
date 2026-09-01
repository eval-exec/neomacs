//! Shared helpers for the integration tests in `crates/neomacs/tests/`.
//!
//! These tests spawn the under-test `neomacs` binary and (optionally)
//! GNU `emacs` as subprocesses, capturing exit code, stdout, and stderr
//! so the harness can compare observable startup state side-by-side.
//!
//! The helpers here intentionally mirror the small slice of
//! `crates/neovm-oracle-tests/src/common.rs` that we need: oracle binary
//! discovery, an `NEOVM_FORCE_ORACLE_PATH` gate, and an RLIMIT_AS
//! cap on the spawned process. We do not pull from `neovm-oracle-tests`
//! as a crate dependency because that crate is layered for Lisp form
//! parity (in-process NeoVM evaluator) — argv parity is a different
//! shape (out-of-process binary invocation) and lives at a different
//! layer.

#![allow(dead_code)]

use std::io::{ErrorKind, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// Captured outcome of running a binary with a specific argv.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for ProbeResult {
    fn from(output: Output) -> Self {
        Self {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// Path to the GNU `emacs` binary used as the parity oracle.
///
/// Resolution order, mirroring `crates/neovm-oracle-tests/src/common.rs:125`:
/// 1. `NEOVM_FORCE_ORACLE_PATH` environment variable.
/// 2. A nearby `emacs-mirror/emacs/src/emacs` checkout, walking up to
///    four levels from the Cargo workspace root.
/// 3. Fall through to `emacs` on `$PATH`.
pub fn oracle_emacs_path() -> String {
    if let Ok(path) = std::env::var("NEOVM_FORCE_ORACLE_PATH") {
        return path;
    }
    let mut dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    for _ in 0..4 {
        let candidate = dir.join("emacs-mirror/emacs/src/emacs");
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
        if !dir.pop() {
            break;
        }
    }
    "emacs".to_string()
}

/// True when oracle parity tests should run. Mirrors
/// `crates/neovm-oracle-tests/src/common.rs:84`.
///
/// We gate on the same `NEOVM_FORCE_ORACLE_PATH` env var so a single
/// invocation flag opts the entire workspace into oracle testing.
pub fn oracle_enabled() -> bool {
    std::env::var_os("NEOVM_FORCE_ORACLE_PATH").is_some()
}

/// Skip the current test when oracle parity is not enabled.
///
/// Use as the first line of a `#[test]` body:
///
/// ```ignore
/// #[test]
/// fn parity_check() {
///     skip_unless_oracle!();
///     // …
/// }
/// ```
#[macro_export]
macro_rules! skip_unless_oracle {
    () => {
        if !$crate::common::oracle_enabled() {
            eprintln!(
                "skipping {}: set NEOVM_FORCE_ORACLE_PATH=/path/to/emacs",
                module_path!()
            );
            return;
        }
    };
}

/// Memory limit (bytes) imposed on each spawned subprocess to keep a
/// runaway evaluation from triggering the system OOM killer.
///
/// The binary argv tests load the full final pdump, which maps more memory
/// than the in-process oracle tests. Keep the limit high enough for normal
/// startup while retaining an explicit runaway ceiling; callers can still
/// override it with `NEOVM_ORACLE_MEM_LIMIT_MB`.
fn oracle_mem_limit_bytes() -> u64 {
    let mb: u64 = std::env::var("NEOVM_ORACLE_MEM_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2048);
    mb * 1024 * 1024
}

fn apply_memory_limit(cmd: &mut Command) {
    let limit = oracle_mem_limit_bytes() as libc::rlim_t;
    // Safety: `pre_exec` runs between fork and exec in the child.
    // setrlimit is async-signal-safe, so this is sound. Mirrors
    // crates/neovm-oracle-tests/src/common.rs:278.
    unsafe {
        cmd.pre_exec(move || {
            let rlim = libc::rlimit {
                rlim_cur: limit,
                rlim_max: limit,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Spawn the under-test `neomacs` binary with the given argv (passed
/// after `argv[0]`, which Cargo provides via the `CARGO_BIN_EXE_neomacs`
/// env var).
///
/// Stdin is closed; stdout and stderr are captured. The subprocess is
/// memory-limited via RLIMIT_AS the same way oracle Emacs is.
pub fn run_neomacs(argv: &[&str]) -> ProbeResult {
    run_neomacs_with_stdin(argv, "")
}

/// Spawn the under-test `neomacs` binary with text supplied on stdin.
pub fn run_neomacs_with_stdin(argv: &[&str], stdin: &str) -> ProbeResult {
    let neomacs_bin = env!("CARGO_BIN_EXE_neomacs");
    let mut cmd = Command::new(neomacs_bin);
    cmd.args(argv);
    cmd.env("HOME", std::env::temp_dir());
    cmd.env_remove("NEOMACS_LOG_FILE");
    cmd.env_remove("NEOMACS_LOG_TO_FILE");
    run_with_stdin(&mut cmd, stdin, "neomacs")
}

/// Spawn GNU oracle Emacs with the given argv. The caller is
/// responsible for `--batch` (or equivalent) so the spawn cannot block
/// on a tty.
pub fn run_oracle_emacs(argv: &[&str]) -> ProbeResult {
    run_oracle_emacs_with_stdin(argv, "")
}

/// Spawn GNU oracle Emacs with text supplied on stdin.
pub fn run_oracle_emacs_with_stdin(argv: &[&str], stdin: &str) -> ProbeResult {
    let oracle = oracle_emacs_path();
    let mut cmd = Command::new(&oracle);
    cmd.args(argv);
    cmd.env("HOME", std::env::temp_dir());
    run_with_stdin(&mut cmd, stdin, &format!("oracle emacs at {oracle}"))
}

fn run_with_stdin(cmd: &mut Command, stdin: &str, label: &str) -> ProbeResult {
    apply_memory_limit(cmd);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("failed to run {label}: {e}"));
    let mut child_stdin = child.stdin.take().expect("piped child stdin");
    let input = stdin.as_bytes().to_vec();
    let writer_label = label.to_string();
    let stdin_writer = std::thread::spawn(move || {
        if let Err(error) = child_stdin.write_all(&input)
            && error.kind() != ErrorKind::BrokenPipe
        {
            panic!("failed to write stdin for {writer_label}: {error}");
        }
    });
    let output = child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("failed to wait for {label}: {e}"));
    if let Err(payload) = stdin_writer.join() {
        std::panic::resume_unwind(payload);
    }
    ProbeResult::from(output)
}
