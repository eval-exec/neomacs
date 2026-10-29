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
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::sync::OnceLock;

use neomacs_test_env::{CanonicalEnvironment, CanonicalProfile, PinnedCommand, bounded_temp_dir};

/// The canonical environment both editors run under in this test process.
///
/// These argv/startup probes compare the two engines live, so the pins only
/// have to be equal -- but they must be *pinned*: an inherited passwd GECOS or
/// time zone can otherwise turn a comparison into a property of the host.
struct CanonicalProbes {
    _home: tempfile::TempDir,
    _tmp: tempfile::TempDir,
    environment: CanonicalEnvironment,
}

fn canonical_probes() -> &'static CanonicalProbes {
    static PROBES: OnceLock<CanonicalProbes> = OnceLock::new();
    PROBES.get_or_init(|| {
        let home = bounded_temp_dir("nv-probes-home-").expect("create canonical probes HOME");
        let tmp = bounded_temp_dir("nv-probes-tmp-").expect("create canonical probes TMPDIR");
        let environment = CanonicalEnvironment::new(
            CanonicalProfile::Oracle,
            home.path(),
            tmp.path(),
            neomacs_infra::workspace_root(),
        )
        .expect("canonical probes environment");
        CanonicalProbes {
            _home: home,
            _tmp: tmp,
            environment,
        }
    })
}

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

/// The attested GNU `emacs` every parity test in this crate runs against.
///
/// Resolution and identity attestation are centralized in
/// `neomacs_test_oracle::gnu`: `NEOVM_FORCE_ORACLE_PATH` when set, otherwise
/// the pinned-reference check on the `emacs` found on `PATH`.  A missing or
/// mismatched oracle PANICS in the helper below; a parity test must never
/// skip or compare against an unpinned editor.
pub fn oracle_emacs_path() -> PathBuf {
    neomacs_test_oracle::gnu::gnu_oracle().executable_path()
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

#[cfg(unix)]
fn apply_memory_limit(cmd: &mut PinnedCommand) {
    if !neomacs_infra::ADDRESS_SPACE_LIMIT_SUPPORTED {
        return;
    }
    cmd.limit_address_space(oracle_mem_limit_bytes());
}

/// RLIMIT_AS is Unix-only; non-Unix hosts run spawned probes unlimited.
#[cfg(not(unix))]
fn apply_memory_limit(_cmd: &mut PinnedCommand) {}

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
    let mut cmd = canonical_probes().environment.command(neomacs_bin);
    cmd.args(argv);
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
    let mut cmd = canonical_probes().environment.command(&oracle);
    cmd.args(argv);
    run_with_stdin(
        &mut cmd,
        stdin,
        &format!("oracle emacs at {}", oracle.display()),
    )
}

fn run_with_stdin(cmd: &mut PinnedCommand, stdin: &str, label: &str) -> ProbeResult {
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
