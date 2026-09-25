//! The JIT's exit report and report sink, end to end: these run the real
//! executable, so they need a release build with a matching pdump
//! (`NEOMACS_GUI_TEST_BINARY` overrides the path, like `batch_startup.rs`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"))
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
#[ignore = "requires release executable with matching pdump"]
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
#[ignore = "requires release executable with matching pdump"]
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
#[ignore = "requires release executable with matching pdump"]
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
#[ignore = "requires release executable with matching pdump"]
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
#[ignore = "requires release executable with matching pdump"]
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
