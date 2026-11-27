//! The JIT's exit report and report sink, end to end: these run the real
//! executable, so they need a release build with a matching pdump
//! (`NEOMACS_GUI_TEST_BINARY` overrides the path, like `batch_startup.rs`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    neomacs_infra::crate_root!().join("../..")
}

fn binary() -> PathBuf {
    let path = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"));
    assert!(
        path.is_file(),
        "release-pdump-tests needs a release neomacs at {}; build it with \
         `cargo xtask fresh-build --release` or set NEOMACS_GUI_TEST_BINARY",
        path.display(),
    );
    path
}

/// A byte-compiled hot loop, so the JIT compiles something, then `tail`.
const HOT: &str = "(progn (defun jit-obs-probe (n) (let ((s 0)) (dotimes (i n) (setq s (+ s i))) s)) \
                   (byte-compile 'jit-obs-probe) (dotimes (_ 200) (jit-obs-probe 50)))";

fn run(envs: &[(&str, &str)], tail: &str) -> Output {
    let mut cmd = Command::new(binary());
    cmd.current_dir(root())
        .env("RUST_LOG", "off")
        .env_remove("NEOVM_JIT_COMPILE_STATS")
        .env_remove("NEOVM_JIT_STATS_FILE")
        .env_remove("NEOVM_JIT_PROFILE")
        .env("NEOVM_JIT_THRESHOLD", "1")
        .args(["-Q", "--batch", "--eval", HOT, "--eval", tail]);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run neomacs --batch")
}

fn report_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| l.starts_with("[neovm-jit-"))
        .collect()
}

#[test]
#[cfg(feature = "release-pdump-tests")]
fn jit_final_report_on_kill_emacs() {
    let out = run(
        &[("NEOVM_JIT_COMPILE_STATS", "1")],
        "(progn (princ \"payload\") (kill-emacs 3))",
    );
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let lines = report_lines(&stderr);
    assert!(
        lines.iter().any(|l| l.starts_with("[neovm-jit-final] ")),
        "{stderr}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[neovm-jit-final-runs] ")),
        "{stderr}"
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "payload",
        "the report never touches stdout"
    );
}

#[test]
#[cfg(feature = "release-pdump-tests")]
fn jit_final_report_on_batch_error_255() {
    let out = run(
        &[("NEOVM_JIT_COMPILE_STATS", "1")],
        "(error \"jit-obs-probe-error\")",
    );
    assert_eq!(out.status.code(), Some(255), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        report_lines(&stderr)
            .iter()
            .any(|l| l.starts_with("[neovm-jit-final] ")),
        "{stderr}"
    );
}

#[test]
#[cfg(feature = "release-pdump-tests")]
fn jit_stats_file_sink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jit-report.txt");
    let out = run(
        &[("NEOVM_JIT_STATS_FILE", path.to_str().expect("utf-8 path"))],
        "(kill-emacs 0)",
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(report_lines(&stderr).is_empty(), "{stderr}");
    let file = std::fs::read_to_string(Path::new(&path)).expect("report file");
    assert!(
        report_lines(&file)
            .iter()
            .any(|l| l.starts_with("[neovm-jit-final] ")),
        "{file}"
    );
}

#[test]
#[cfg(feature = "release-pdump-tests")]
fn jit_report_silent_without_knobs() {
    let out = run(&[], "(kill-emacs 0)");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(report_lines(&stderr).is_empty(), "{stderr}");
}

/// Under `PERF_BUILDID_DIR` (set by `perf record -- cmd`), cranelift-jit
/// writes `/tmp/perf-<pid>.map`, and the JIT now declares each leaf under
/// `lisp:<fn>#<id>:<tier>` instead of one shared anonymous name.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn jit_perf_map_names_under_perf_buildid_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let child = Command::new(binary())
        .current_dir(root())
        .env("RUST_LOG", "off")
        .env("NEOVM_JIT_THRESHOLD", "1")
        .env("PERF_BUILDID_DIR", dir.path())
        .args(["-Q", "--batch", "--eval", HOT, "--eval", "(kill-emacs 0)"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn neomacs");
    let pid = child.id();
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let map_path = PathBuf::from(format!("/tmp/perf-{pid}.map"));
    let map = std::fs::read_to_string(&map_path).expect("perf map written");
    let _ = std::fs::remove_file(&map_path);
    assert!(
        map.lines().any(|l| l.contains(" lisp:jit-obs-probe#")),
        "{map}"
    );
    assert!(
        !map.lines().any(|l| l.ends_with(" __neovm_jit_leaf")),
        "no anonymous leaves under naming: {map}"
    );
}

/// #865: Cranelift logs every compiled function's whole IR at INFO
/// (`cranelift_jit::backend`: `info!("defining function {}: {}", id, clif)`),
/// so a plain `RUST_LOG=info` launch used to carry ~1 MB of CLIF dump. Every
/// Cranelift target must stay at WARN unless `RUST_LOG` names one.
///
/// Batch mode selects `LogTarget::File` (the TTY frontend writes tracing to a
/// file, never to the pty), and that writer is silent without
/// `NEOMACS_LOG_FILE` — which is what makes this row display-free. The same
/// process compiles `HOT` and prints its own `[neovm-jit-final]` line to
/// stderr; requiring `ok>=1` there is the anti-vacuity guard, because the
/// negative half would otherwise also hold on a host where nothing compiled.
#[test]
#[cfg(feature = "release-pdump-tests")]
fn cranelift_ir_dump_obeys_rust_log() {
    /// The log the process wrote, plus how many leaves it really compiled.
    struct Run {
        log: String,
        jit_ok: u64,
    }

    fn run(rust_log: &str) -> Run {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("neomacs.log");
        let out = Command::new(binary())
            .current_dir(root())
            .env("RUST_LOG", rust_log)
            .env("NEOMACS_LOG_FILE", &log_path)
            .env("NEOVM_JIT_THRESHOLD", "1")
            .env("NEOVM_JIT_COMPILE_STATS", "1")
            .args(["-Q", "--batch", "--eval", HOT, "--eval", "(kill-emacs 0)"])
            .output()
            .expect("run neomacs --batch");
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        let final_line = report_lines(&stderr)
            .into_iter()
            .find(|l| l.starts_with("[neovm-jit-final] "))
            .unwrap_or_else(|| panic!("no `[neovm-jit-final]` line in stderr: {stderr}"));
        let jit_ok = final_line
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix("ok="))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `ok=<n>` in `{final_line}`"));
        Run {
            log: std::fs::read_to_string(&log_path).expect("NEOMACS_LOG_FILE written"),
            jit_ok,
        }
    }

    let global_info = run("info");
    assert!(
        global_info.jit_ok >= 1,
        "the stimulus must compile at least one leaf, or the assertion below is vacuous"
    );
    assert!(
        !global_info.log.contains("defining function"),
        "a global `RUST_LOG=info` must not enable Cranelift's IR dump (#865); \
         the log is {} lines",
        global_info.log.lines().count()
    );

    // The other direction: an explicit Cranelift directive is the user's
    // choice and must still deliver the IR. Without this control the filter
    // could silence the target entirely and the row above would stay green.
    let explicit = run("info,cranelift_jit=info");
    assert!(
        explicit.jit_ok >= 1,
        "the explicit control must compile at least one leaf"
    );
    assert!(
        explicit.log.contains("defining function"),
        "`RUST_LOG=info,cranelift_jit=info` must still deliver the IR; \
         the log is {} lines",
        explicit.log.lines().count()
    );
}
