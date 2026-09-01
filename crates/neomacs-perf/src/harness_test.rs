use std::fs;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::Command;

use super::{
    Frontend, PerfError, PerfHarness, RunRequest, RunVerdict, ScenarioId,
    collect_editor_provenance, configure_benchmark_environment,
};

#[test]
fn invalid_scenario_output_is_persisted_but_never_accepted_as_a_sample() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 2,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    assert!(matches!(
        report.artifact.verdict,
        RunVerdict::CorrectnessMismatch { .. }
    ));
    assert!(!report.artifact.verdict.is_valid());
    assert!(
        report
            .artifact_path
            .starts_with(workspace.path().join("tmp/perf"))
    );
    assert!(report.artifact_path.is_file());

    let persisted = fs::read_to_string(&report.artifact_path).expect("read artifact");
    assert!(persisted.contains("correctness-mismatch"));
    assert!(persisted.contains("iterations"));
}

#[test]
fn fixture_overlay_count_is_checked_against_the_harness_oracle() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-oracle-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(3).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 3,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 3,
              "lsp_diagnostic_count": 4,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    match report.artifact.verdict {
        RunVerdict::CorrectnessMismatch { mismatches } => {
            assert!(
                mismatches
                    .iter()
                    .any(|mismatch| mismatch.invariant == "overlay-count")
            );
        }
        verdict => panic!("fixture-owned expectation was accepted: {verdict:?}"),
    }
}

#[test]
fn mx_tab_result_is_valid_only_when_the_real_completion_window_lifecycle_completed() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-mx-tab-result-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::MxTabCompletion,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(2).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Tui {
        rows: 40,
        columns: 120,
    });

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "mx-tab-completion",
              "status": "ok",
              "iterations": 2,
              "elapsed_us": 1000,
              "completion_help_calls": 2,
              "completion_visible": true,
              "completion_mode_correct": true,
              "known_commands_present": true,
              "completion_candidate_count": 3124,
              "candidate_count_stable": true,
              "completion_hidden_after_exit": true,
              "minibuffer_depth_restored": true,
              "selected_buffer_restored": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::Valid { measurements } = report.artifact.verdict else {
        panic!("correct M-x TAB lifecycle was rejected")
    };
    let per_completion = measurements
        .iter()
        .find(|measurement| measurement.name == super::MetricName::PerCompletionCpuTime)
        .expect("per-completion metric");
    assert_eq!(per_completion.value, 500.0);
    assert_eq!(
        per_completion.unit,
        super::MetricUnit::MicrosecondsPerCompletion
    );
}

#[test]
fn mx_tab_result_cannot_treat_a_missing_completion_window_as_a_fast_sample() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-mx-tab-mismatch-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::MxTabCompletion,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "mx-tab-completion",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 1,
              "completion_help_calls": 1,
              "completion_visible": false,
              "completion_mode_correct": true,
              "known_commands_present": true,
              "completion_candidate_count": 0,
              "candidate_count_stable": false,
              "completion_hidden_after_exit": true,
              "minibuffer_depth_restored": true,
              "selected_buffer_restored": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("missing completion window was accepted")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-window-visible")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-candidate-count-stable")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "completion-candidate-count")
    );
}

#[test]
fn bytecode_call_loop_accepts_only_the_full_interpreted_call_count() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-bytecode-call-result-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::BytecodeCallLoop,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(20).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "bytecode-call-loop",
              "status": "ok",
              "iterations": 20,
              "elapsed_us": 10,
              "bytecode_calls": 20,
              "result": 1,
              "expected_result": 1,
              "bytecode_functions_compiled": true,
              "interpreter_requested": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::Valid { measurements } = report.artifact.verdict else {
        panic!("correct bytecode call loop was rejected")
    };
    let per_call = measurements
        .iter()
        .find(|measurement| measurement.name == super::MetricName::PerBytecodeCallCpuTime)
        .expect("per-bytecode-call metric");
    assert_eq!(per_call.value, 0.5);
    assert_eq!(
        per_call.unit,
        super::MetricUnit::MicrosecondsPerBytecodeCall
    );
}

#[test]
fn bytecode_call_loop_rejects_a_short_or_wrong_result() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-bytecode-call-mismatch-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::BytecodeCallLoop,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(20).expect("non-zero iterations"),
    );

    let report = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "bytecode-call-loop",
              "status": "ok",
              "iterations": 20,
              "elapsed_us": 1,
              "bytecode_calls": 19,
              "result": 20,
              "expected_result": 1,
              "bytecode_functions_compiled": true,
              "interpreter_requested": true,
              "error": null
            }"##,
        )
        .expect("record fixture result");

    let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
        panic!("incomplete bytecode call loop was accepted")
    };
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "bytecode-call-count")
    );
    assert!(
        mismatches
            .iter()
            .any(|mismatch| mismatch.invariant == "bytecode-result")
    );
}

#[test]
fn scenario_result_requires_every_schema_field() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-schema-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let error = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4
            }"##,
        )
        .expect_err("the required `error` field is absent");

    assert!(matches!(error, PerfError::InvalidScenarioResult { .. }));
    assert!(error.to_string().contains("missing field `error`"));
}

#[test]
fn scenario_result_rejects_a_success_status_with_an_error() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-outcome-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        workspace.path().join("fake-neomacs"),
        NonZeroU32::new(1).expect("non-zero iterations"),
    );

    let error = harness
        .record_fixture_result(
            &request,
            r##"{
              "schema_version": 1,
              "scenario": "rust-lsp-typing",
              "status": "ok",
              "iterations": 1,
              "elapsed_us": 81,
              "major_mode": "rust-ts-mode",
              "lsp_mode_loaded": true,
              "treesit_parser_language": "rust",
              "text_unchanged": true,
              "point_unchanged": true,
              "overlay_count": 4,
              "lsp_diagnostic_count": 4,
              "error": "workload failed"
            }"##,
        )
        .expect_err("success and an error message are contradictory");

    assert!(matches!(error, PerfError::InvalidScenarioResult { .. }));
    assert!(
        error
            .to_string()
            .contains("status `ok` requires a null error")
    );
}

#[test]
fn run_persists_a_missing_editor_as_an_infrastructure_failure() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let workspace = tempfile::Builder::new()
        .prefix("neomacs-perf-run-test-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local test directory");
    let harness = PerfHarness::new(workspace.path());
    let missing_editor = workspace.path().join("missing-neomacs");
    let request = RunRequest::new(
        ScenarioId::RustLspTyping,
        &missing_editor,
        NonZeroU32::new(1).expect("non-zero iterations"),
    )
    .with_frontend(Frontend::Batch);

    let report = harness
        .run(&request)
        .expect("infrastructure failures are persisted reports");

    match &report.artifact.verdict {
        RunVerdict::InfrastructureFailure { message } => {
            assert!(message.contains("missing editor executable"));
            assert!(message.contains("missing-neomacs"));
        }
        verdict => panic!("expected infrastructure failure, got {verdict:?}"),
    }
    assert!(report.artifact.verdict.measurements().is_none());
    assert!(report.artifact_path.is_file());
}

#[cfg(unix)]
#[test]
fn pty_runner_publishes_the_raw_terminal_byte_stream() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch_root = workspace_root.join("tmp");
    fs::create_dir_all(&scratch_root).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-test-")
        .tempdir_in(&scratch_root)
        .expect("create workspace-local PTY test directory");
    let sentinel = scratch.path().join("done");
    let terminal_bytes = scratch.path().join("terminal.ansi");

    let output = Command::new("python3")
        .arg(workspace_root.join("tools/bench/pty-run.py"))
        .args([
            "sh",
            "-c",
            r#"printf '\033[31mred\033[0m\n'; : > "$SENTINEL""#,
        ])
        .current_dir(&workspace_root)
        .env("SENTINEL", &sentinel)
        .env("PTY_OUTPUT", &terminal_bytes)
        .env("PTY_TIMEOUT", "5")
        .output()
        .expect("run deterministic PTY adapter");

    assert!(
        output.status.success(),
        "PTY adapter failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&terminal_bytes).expect("read raw terminal artifact"),
        b"\x1b[31mred\x1b[0m\r\n"
    );
}

#[cfg(unix)]
#[test]
fn pty_runner_allows_a_profile_wrapper_to_finalize_after_the_sentinel() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch_root = workspace_root.join("tmp");
    fs::create_dir_all(&scratch_root).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-pty-finalize-test-")
        .tempdir_in(&scratch_root)
        .expect("create workspace-local PTY test directory");
    let sentinel = scratch.path().join("done");
    let finalized = scratch.path().join("profile-finalized");

    let output = Command::new("python3")
        .arg(workspace_root.join("tools/bench/pty-run.py"))
        .args([
            "sh",
            "-c",
            r##": > "$SENTINEL"; sleep 1; : > "$FINALIZED""##,
        ])
        .current_dir(&workspace_root)
        .env("SENTINEL", &sentinel)
        .env("FINALIZED", &finalized)
        .env("PTY_TIMEOUT", "5")
        .output()
        .expect("run PTY adapter around a finalizing wrapper");

    assert!(
        output.status.success(),
        "PTY adapter failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        finalized.is_file(),
        "PTY adapter killed the wrapper before profile finalization"
    );
}

#[test]
fn gui_runner_keeps_logs_workspace_local_and_owns_its_viewport_size() {
    let runner = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/tools/bench/gui-run.sh"
    ));
    assert!(!runner.contains(">/tmp/"));
    assert!(runner.contains("GUI_WESTON_LOG"));
    assert!(runner.contains("GUI_APP_LOG"));
    assert!(runner.contains("GUI_WIDTH"));
    assert!(runner.contains("GUI_HEIGHT"));
}

#[test]
fn benchmark_environment_does_not_inherit_logging_or_allocator_controls() {
    let sandbox = neomacs_melpa_test_support::MelpaSandbox::new("perf-environment")
        .expect("create package sandbox");
    let mut command = Command::new("env");
    command
        .env("RUST_LOG", "debug")
        .env("MIMALLOC_ARENA_EAGER_COMMIT", "2")
        .env("NEOMACS_DUMP_FRAME_GLYPHS", "1");

    configure_benchmark_environment(&mut command, &sandbox);
    let output = command.output().expect("print benchmark environment");
    let environment = String::from_utf8(output.stdout).expect("environment is UTF-8");

    assert!(!environment.contains("RUST_LOG="));
    assert!(!environment.contains("MIMALLOC_ARENA_EAGER_COMMIT="));
    assert!(!environment.contains("NEOMACS_DUMP_FRAME_GLYPHS="));
    assert!(environment.contains("NEOMACS_TEST_SANDBOX_ROOT="));
}

#[cfg(unix)]
#[test]
fn editor_provenance_uses_content_and_pdump_fingerprints() {
    use std::os::unix::fs::PermissionsExt;

    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-editor-provenance-")
        .tempdir_in(workspace_root.join("tmp"))
        .expect("create workspace-local provenance scratch directory");
    let editor = scratch.path().join("fake-neomacs");
    fs::write(
        &editor,
        r##"#!/bin/sh
case "$1" in
  --fingerprint) printf '%s\n' 'PDUMP-FINGERPRINT' ;;
  --version) printf '%s\n' 'Neomacs test-build' ;;
  *) exit 64 ;;
esac
"##,
    )
    .expect("write fake editor");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))
        .expect("make fake editor executable");
    let sandbox = neomacs_melpa_test_support::MelpaSandbox::new("editor-provenance")
        .expect("create package sandbox");

    let before = collect_editor_provenance(&editor, &sandbox).expect("collect editor identity");
    assert_eq!(before.pdump_fingerprint, "PDUMP-FINGERPRINT");
    assert_eq!(before.version, "Neomacs test-build");
    assert_eq!(before.executable_sha256.len(), 64);
    assert!(
        before
            .executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );

    let mut changed = fs::read(&editor).expect("read fake editor");
    changed.extend_from_slice(b"# changed build\n");
    fs::write(&editor, changed).expect("change fake editor contents");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755))
        .expect("keep fake editor executable");
    let after = collect_editor_provenance(&editor, &sandbox).expect("collect changed identity");
    assert_ne!(before.executable_sha256, after.executable_sha256);
}
