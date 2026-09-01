use std::fs;
#[cfg(any(unix, windows))]
use std::process::{Command, Stdio};
#[cfg(any(unix, windows))]
use std::time::Duration;

use neomacs_test_oracle::EvalOutcome;

#[cfg(any(unix, windows))]
use super::DirectEditorChild;
use super::{
    DIRECT_PROBE_MAX_BYTES, DIRECT_PROBE_SCHEMA, direct_log_elisp_string,
    parse_direct_probe_outcome, read_direct_probe_outcome,
};
#[cfg(unix)]
use super::{run_direct_editor_probe, wrap_direct_probe_logs};
#[cfg(unix)]
use crate::EmacsRuntime;
use crate::MelpaSandbox;

fn envelope(case_id: &str, kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(DIRECT_PROBE_SCHEMA.as_bytes());
    encoded.push(b'\n');
    encoded.extend_from_slice(case_id.len().to_string().as_bytes());
    encoded.push(b'\n');
    encoded.extend_from_slice(case_id.as_bytes());
    encoded.push(b'\n');
    encoded.extend_from_slice(kind.as_bytes());
    encoded.push(b'\n');
    encoded.extend_from_slice(payload.len().to_string().as_bytes());
    encoded.push(b'\n');
    encoded.extend_from_slice(payload);
    encoded
}

#[test]
fn direct_envelope_preserves_escaped_symbol_and_signal_payloads() {
    assert_eq!(
        parse_direct_probe_outcome(
            "escaped-symbol",
            &envelope("escaped-symbol", "value", br"\)")
        ),
        Ok(EvalOutcome::Value(r"\)".to_string()))
    );
    assert_eq!(
        parse_direct_probe_outcome(
            "signalled",
            &envelope("signalled", "signal", br#"(error "failed ) safely")"#),
        ),
        Ok(EvalOutcome::Signal(
            r#"(error "failed ) safely")"#.to_string()
        ))
    );
}

#[test]
fn direct_envelope_rejects_malformed_partial_and_trailing_records() {
    let valid = envelope("case", "value", b"42");
    let malformed = [
        (b"".as_slice(), "schema line is incomplete"),
        (
            b"wrong\n4\ncase\nvalue\n2\n42".as_slice(),
            "expected schema",
        ),
        (
            b"neomacs-melpa-direct-v1\n04\ncase\nvalue\n2\n42".as_slice(),
            "not canonical decimal",
        ),
        (
            b"neomacs-melpa-direct-v1\n4\ncasevalue\n2\n42".as_slice(),
            "case id is not followed",
        ),
        (
            b"neomacs-melpa-direct-v1\n4\ncase\nother\n2\n42".as_slice(),
            "unknown outcome kind",
        ),
        (
            b"neomacs-melpa-direct-v1\n4\ncase\nvalue\n3\n42".as_slice(),
            "declares 3 bytes",
        ),
    ];
    for (record, error_fragment) in malformed {
        let error = parse_direct_probe_outcome("case", record).expect_err("record must fail");
        assert!(
            error.contains(error_fragment),
            "expected `{error}` to contain `{error_fragment}`"
        );
    }

    let mut trailing = valid;
    trailing.extend_from_slice(b"\nignored");
    assert!(
        parse_direct_probe_outcome("case", &trailing)
            .expect_err("trailing bytes must fail")
            .contains("trailing bytes")
    );

    assert!(
        parse_direct_probe_outcome("case", &envelope("other", "value", b"42"))
            .expect_err("case identity mismatch must fail")
            .contains("expected case id `case`, got `other`")
    );
    assert!(
        parse_direct_probe_outcome("case", &envelope("case", "value", &[0xff]))
            .expect_err("non-UTF-8 payload must fail")
            .contains("payload is not UTF-8")
    );
}

#[test]
fn direct_outcome_reader_rejects_partial_and_oversized_files() {
    let sandbox = MelpaSandbox::new("direct-outcome-reader-contract")
        .expect("create workspace-local reader sandbox");
    let outcome = sandbox.root().join("outcome");
    let partial = sandbox.root().join("outcome.partial");
    fs::write(&outcome, envelope("case", "value", b"42")).expect("write valid outcome");
    fs::write(&partial, b"incomplete").expect("write partial outcome");
    assert!(
        read_direct_probe_outcome("case", &outcome, &partial)
            .expect_err("partial outcome must fail")
            .contains("incomplete atomic outcome")
    );

    fs::remove_file(&partial).expect("remove partial outcome");
    fs::write(
        &outcome,
        vec![b'x'; usize::try_from(DIRECT_PROBE_MAX_BYTES).unwrap() + 1],
    )
    .expect("write oversized outcome");
    assert!(
        read_direct_probe_outcome("case", &outcome, &partial)
            .expect_err("oversized outcome must fail")
            .contains("protocol limit")
    );
}

#[test]
fn direct_log_observation_uses_canonical_elisp_string_syntax() {
    assert_eq!(
        direct_log_elisp_string("line 1\n\t\"quoted\"\r\u{1b}\0 \0\u{37}\u{7}7\u{8}7\u{c}\u{7f} Ω"),
        r#""line 1\n\11\"quoted\"\15\33\0 \0007\0077\0107\f\177 Ω""#
    );
}

#[test]
#[cfg(unix)]
fn direct_log_observation_discards_ascii_whitespace_only_terminal_noise() {
    let sandbox = MelpaSandbox::new("direct-log-whitespace-noise")
        .expect("create workspace-local direct-log sandbox");
    assert_eq!(
        wrap_direct_probe_logs(
            EvalOutcome::Value("ok".to_string()),
            " \t\n\u{b}".to_string(),
            "\r\n\u{c}".to_string(),
            &sandbox,
        ),
        EvalOutcome::Value(r#"(:value ok :stdout "" :stderr "")"#.to_string())
    );
    assert_eq!(
        wrap_direct_probe_logs(
            EvalOutcome::Value("ok".to_string()),
            " \tvisible\n".to_string(),
            "\nwarning\r".to_string(),
            &sandbox,
        ),
        EvalOutcome::Value(
            r#"(:value ok :stdout " \11visible\n" :stderr "\nwarning\15")"#.to_string()
        )
    );
}

#[cfg(unix)]
#[test]
fn direct_wrapped_logs_round_trip_through_gnu_reader_without_byte_loss() {
    use neomacs_melpa_test_support::elisp_string;

    let sandbox = MelpaSandbox::new("direct-log-elisp-round-trip")
        .expect("create workspace-local log round-trip sandbox");
    let stdout = "line 1\n\t\"quoted\"\r\u{1b}\0 \0\u{37}\u{7}7\u{8}7\u{c}\u{7f} Ω";
    let stderr = "failure\\path\n\u{1}2\u{85} λ";
    let wrapped = wrap_direct_probe_logs(
        EvalOutcome::Value(r#"\)"#.to_string()),
        stdout.to_string(),
        stderr.to_string(),
        &sandbox,
    );
    let EvalOutcome::Value(record) = wrapped else {
        panic!("value outcome must remain a value after log wrapping");
    };
    assert_eq!(
        record,
        r#"(:value \) :stdout "line 1\n\11\"quoted\"\15\33\0 \0007\0077\0107\f\177 Ω" :stderr "failure\\path\n\0012 λ")"#
    );

    let script = sandbox.root().join("read-wrapped-logs.el");
    let stdout_file = sandbox.root().join("decoded.stdout");
    let stderr_file = sandbox.root().join("decoded.stderr");
    fs::write(
        &script,
        format!(
            r####";;; -*- lexical-binding: t; -*-
(let* ((read-eval nil)
       (encoded {})
       (decoded (read-from-string encoded))
       (record (car decoded))
       (trailing (substring encoded (cdr decoded)))
       (coding-system-for-write 'utf-8-unix))
  (unless (string-match-p "\\`[[:space:]]*\\'" trailing)
    (error "Wrapped log record has trailing input"))
  (with-temp-file {}
    (insert (plist-get record :stdout)))
  (with-temp-file {}
    (insert (plist-get record :stderr))))
"####,
            elisp_string(&record),
            elisp_string(&stdout_file.to_string_lossy()),
            elisp_string(&stderr_file.to_string_lossy()),
        ),
    )
    .expect("write GNU log round-trip script");

    let mut command = EmacsRuntime::gnu_emacs().command();
    sandbox.configure(&mut command);
    let output = command
        .arg("--quick")
        .arg("--batch")
        .arg("--load")
        .arg(script)
        .output()
        .expect("run GNU read-from-string log round trip");
    assert!(
        output.status.success(),
        "GNU log round trip failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
    assert_eq!(
        fs::read(stdout_file).expect("read decoded stdout"),
        stdout.as_bytes()
    );
    assert_eq!(
        fs::read(stderr_file).expect("read decoded stderr"),
        stderr.as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn direct_editor_executes_canonical_escaped_symbol_payload() {
    let runtime = EmacsRuntime::gnu_emacs().with_timeout(Duration::from_secs(10));
    let outcome = run_direct_editor_probe(&runtime, "escaped-symbol", "", r#"(intern ")")"#)
        .expect("run escaped-symbol probe through the direct editor adapter");
    assert_eq!(
        outcome,
        EvalOutcome::Value(r#"(:value \) :stdout "" :stderr "")"#.to_string())
    );

    let outcome =
        run_direct_editor_probe(&runtime, "signalled", "", r#"(error "direct\nfailure")"#)
            .expect("run signal probe through the direct editor adapter");
    assert_eq!(
        outcome,
        EvalOutcome::Signal(
            r#"(:signal (error "direct\nfailure") :stdout "" :stderr "")"#.to_string()
        )
    );
}

#[cfg(unix)]
#[test]
fn direct_editor_timeout_reaps_its_process_tree() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("sleep 30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = DirectEditorChild::spawn(&mut command).expect("spawn timeout fixture");
    let error = child
        .wait_for_exit(Duration::from_millis(25))
        .expect_err("fixture must time out");
    assert!(error.contains("timed out"), "unexpected error: {error}");
    assert!(!child.process_tree_has_members());
}

#[cfg(unix)]
#[test]
fn direct_editor_reaps_live_descendant_after_parent_exit() {
    let sandbox = MelpaSandbox::new("direct-descendant-contract")
        .expect("create workspace-local descendant sandbox");
    let pid_file = sandbox.root().join("descendant.pid");
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("sleep 30 & printf '%s' \"$!\" > \"$1\"")
        .arg("direct-descendant-gate")
        .arg(&pid_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = DirectEditorChild::spawn(&mut command).expect("spawn descendant fixture");
    let error = child
        .wait_for_exit(Duration::from_secs(5))
        .expect_err("live descendant must be rejected and reaped");
    assert!(
        error.contains("descendant processes were still live"),
        "unexpected error: {error}"
    );
    let pid: i32 = fs::read_to_string(pid_file)
        .expect("read descendant pid")
        .parse()
        .expect("parse descendant pid");
    // SAFETY: signal 0 only probes the exact positive pid written by our
    // adapter-owned fixture; it cannot deliver a signal.
    let result = unsafe { libc::kill(pid, 0) };
    assert_eq!(result, -1, "descendant {pid} unexpectedly remains live");
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[cfg(windows)]
const WINDOWS_JOB_FIXTURE_MODE: &str = "NEOMACS_DIRECT_WINDOWS_JOB_FIXTURE_MODE";
#[cfg(windows)]
const WINDOWS_JOB_FIXTURE_READY: &str = "NEOMACS_DIRECT_WINDOWS_JOB_FIXTURE_READY";

#[cfg(windows)]
fn windows_job_fixture_command(mode: &str, ready: Option<&std::path::Path>) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("locate current test binary"));
    command
        .arg("direct_adapter_tests::direct_windows_job_fixture_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(WINDOWS_JOB_FIXTURE_MODE, mode)
        .env_remove(WINDOWS_JOB_FIXTURE_READY)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(ready) = ready {
        command.env(WINDOWS_JOB_FIXTURE_READY, ready);
    }
    command
}

#[cfg(windows)]
fn wait_for_windows_job_fixture(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "Windows Job Object fixture did not publish {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Child mode for the Windows Job Object runtime contracts below.
///
/// Launching this test binary suspended lets the adapter assign it to the Job
/// Object before this function can create its own descendant.
#[cfg(windows)]
#[test]
fn direct_windows_job_fixture_child() {
    let Ok(mode) = std::env::var(WINDOWS_JOB_FIXTURE_MODE) else {
        return;
    };
    match mode.as_str() {
        "success" => {}
        "sleep" => std::thread::sleep(Duration::from_secs(30)),
        "descendant-and-sleep" | "descendant-and-exit" => {
            let descendant = windows_job_fixture_command("sleep", None)
                .spawn()
                .expect("spawn same-binary Windows Job Object descendant");
            let ready = std::env::var_os(WINDOWS_JOB_FIXTURE_READY)
                .expect("descendant fixture requires a ready marker");
            fs::write(&ready, descendant.id().to_string())
                .expect("publish Windows Job Object descendant pid");
            if mode == "descendant-and-sleep" {
                std::thread::sleep(Duration::from_secs(30));
            }
            // Dropping Child closes only our process handle. The process stays
            // live and remains owned by the inherited Job Object.
            drop(descendant);
        }
        unexpected => panic!("unexpected Windows Job Object fixture mode `{unexpected}`"),
    }
}

#[cfg(windows)]
#[test]
fn direct_windows_suspended_child_resumes_and_exits_with_empty_job() {
    let mut command = windows_job_fixture_command("success", None);
    let mut child = DirectEditorChild::spawn(&mut command)
        .expect("spawn suspended child and assign its Job Object");
    let status = child
        .wait_for_exit(Duration::from_secs(10))
        .expect("resumed Windows fixture must exit");
    assert!(status.success());
    assert_eq!(
        child
            .active_windows_job_processes()
            .expect("query successful fixture Job Object"),
        0
    );
}

#[cfg(windows)]
#[test]
fn direct_windows_timeout_terminates_descendant_and_empties_job() {
    let sandbox = MelpaSandbox::new("direct-windows-timeout-contract")
        .expect("create workspace-local Windows timeout sandbox");
    let ready = sandbox.root().join("descendant.pid");
    let mut command = windows_job_fixture_command("descendant-and-sleep", Some(&ready));
    let mut child =
        DirectEditorChild::spawn(&mut command).expect("spawn suspended Windows timeout fixture");
    wait_for_windows_job_fixture(&ready);
    let error = child
        .wait_for_exit(Duration::from_millis(25))
        .expect_err("Windows fixture with descendant must time out");
    assert!(error.contains("timed out"), "unexpected error: {error}");
    assert_eq!(
        child
            .active_windows_job_processes()
            .expect("query timed-out fixture Job Object"),
        0
    );
}

#[cfg(windows)]
#[test]
fn direct_windows_rejects_parent_exit_with_live_descendant_and_empties_job() {
    let sandbox = MelpaSandbox::new("direct-windows-descendant-contract")
        .expect("create workspace-local Windows descendant sandbox");
    let ready = sandbox.root().join("descendant.pid");
    let mut command = windows_job_fixture_command("descendant-and-exit", Some(&ready));
    let mut child =
        DirectEditorChild::spawn(&mut command).expect("spawn suspended Windows descendant fixture");
    wait_for_windows_job_fixture(&ready);
    let error = child
        .wait_for_exit(Duration::from_secs(10))
        .expect_err("live descendant after parent exit must be rejected");
    assert!(
        error.contains("descendant processes were still live"),
        "unexpected error: {error}"
    );
    assert_eq!(
        child
            .active_windows_job_processes()
            .expect("query descendant fixture Job Object"),
        0
    );
}
