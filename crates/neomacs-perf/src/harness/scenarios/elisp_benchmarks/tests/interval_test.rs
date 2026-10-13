//! The complete suite owns one measured interval; process startup is separate.

use super::*;
use crate::{MetricName, PerfError, PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

fn valid_result() -> Value {
    json!({
        "schema_version": 1, "scenario": "elisp-benchmarks", "status": "ok",
        "iterations": 2, "elapsed_us": 100, "elapsed_wall_us": 240,
        "benchmark_count": 17, "gcs_done_start": 12, "gcs_done_end": 15,
        "gcs_done_delta": 3, "gc_elapsed_us_start": 100,
        "gc_elapsed_us_end": 180, "gc_elapsed_us_delta": 80, "error": null
    })
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).expect("scratch root");
    tempfile::Builder::new()
        .prefix("elb-interval-result-")
        .tempdir_in(root)
        .expect("workspace-local scratch")
}

fn request() -> RunRequest {
    RunRequest::new(
        ScenarioId::ElispBenchmarks,
        "/unused/editor",
        NonZeroU32::new(2).expect("nonzero repetitions"),
    )
}

#[test]
fn elb_interval_publishes_same_window_wall_cpu_and_gc_measurements() {
    let result: ElispBenchmarksResult = serde_json::from_value(valid_result()).unwrap();
    assert!(validate_elisp_benchmarks_result(&request(), &result).is_empty());
    let measurements = valid_elisp_benchmarks_measurements(&result, 9_999);
    for (name, expected) in [
        (MetricName::ProcessWallTime, 9_999.0),
        (MetricName::WorkloadWallTime, 240.0),
        (MetricName::PerOperationWallTime, 120.0),
        (MetricName::WorkloadCpuTime, 100.0),
        (MetricName::PerOperationCpuTime, 50.0),
        (MetricName::GcsDoneStart, 12.0),
        (MetricName::GcsDoneEnd, 15.0),
        (MetricName::GcsDoneDelta, 3.0),
        (MetricName::GcElapsedStart, 100.0),
        (MetricName::GcElapsedEnd, 180.0),
        (MetricName::GcElapsedDelta, 80.0),
    ] {
        let found: Vec<_> = measurements.iter().filter(|m| m.name == name).collect();
        assert_eq!(found.len(), 1, "one {name:?} measurement");
        assert_eq!(found[0].value, expected);
        assert_eq!(found[0].unit, name.canonical_unit());
    }
}

#[test]
fn elb_interval_requires_wall_and_every_boundary_field() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let valid = valid_result();
    for key in valid.as_object().unwrap().keys() {
        let mut incomplete = valid.clone();
        incomplete.as_object_mut().unwrap().remove(key);
        assert!(
            matches!(
                harness.record_fixture_result(&request(), &incomplete.to_string()),
                Err(PerfError::InvalidScenarioResult { .. })
            ),
            "missing {key} must not publish a sample"
        );
    }
    let mut old = valid;
    for key in [
        "elapsed_wall_us",
        "gcs_done_start",
        "gcs_done_end",
        "gcs_done_delta",
        "gc_elapsed_us_start",
        "gc_elapsed_us_end",
        "gc_elapsed_us_delta",
    ] {
        old.as_object_mut().unwrap().remove(key);
    }
    assert!(serde_json::from_value::<ElispBenchmarksResult>(old).is_err());
}

#[test]
fn elb_interval_rejects_zero_timers_changed_suite_and_partial_work() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for (key, wrong) in [
        ("elapsed_us", json!(0)),
        ("elapsed_wall_us", json!(0)),
        ("iterations", json!(1)),
        ("benchmark_count", json!(16)),
        ("schema_version", json!(2)),
        ("scenario", json!("rust-lsp-typing")),
    ] {
        let mut result = valid_result();
        result[key] = wrong;
        let report = harness
            .record_fixture_result(&request(), &result.to_string())
            .unwrap();
        assert!(
            matches!(
                report.artifact.verdict,
                RunVerdict::CorrectnessMismatch { .. }
            ),
            "{key} must not publish timings"
        );
    }
    let mut failed = valid_result();
    failed["status"] = json!("error");
    failed["error"] = json!("sampling gate rejected disable");
    let report = harness
        .record_fixture_result(&request(), &failed.to_string())
        .unwrap();
    assert!(matches!(
        report.artifact.verdict,
        RunVerdict::CorrectnessMismatch { .. }
    ));
}

#[test]
fn elb_interval_requires_monotonic_consistent_unsigned_gc_boundaries() {
    for (key, wrong) in [
        ("gcs_done_delta", json!(2)),
        ("gcs_done_end", json!(11)),
        ("gc_elapsed_us_delta", json!(79)),
        ("gc_elapsed_us_end", json!(99)),
        ("gcs_done_start", json!(-1)),
        ("gcs_done_end", Value::Null),
        ("gc_elapsed_us_start", Value::Null),
        ("elapsed_wall_us", Value::Null),
    ] {
        let mut result = valid_result();
        result[key] = wrong;
        assert!(
            serde_json::from_value::<ElispBenchmarksResult>(result).is_err(),
            "{key}"
        );
    }
    let mut zero = valid_result();
    zero["gcs_done_end"] = json!(12);
    zero["gcs_done_delta"] = json!(0);
    zero["gc_elapsed_us_end"] = json!(100);
    zero["gc_elapsed_us_delta"] = json!(0);
    let result: ElispBenchmarksResult = serde_json::from_value(zero).unwrap();
    let measurements = valid_elisp_benchmarks_measurements(&result, 1);
    for name in [MetricName::GcsDoneDelta, MetricName::GcElapsedDelta] {
        assert!(
            measurements
                .iter()
                .any(|m| m.name == name && m.value == 0.0)
        );
    }
    let mut maximum = valid_result();
    maximum["gcs_done_start"] = json!(u64::MAX - 3);
    maximum["gcs_done_end"] = json!(u64::MAX);
    assert!(serde_json::from_value::<ElispBenchmarksResult>(maximum.clone()).is_ok());
    maximum["gcs_done_end"] = json!(0);
    assert!(serde_json::from_value::<ElispBenchmarksResult>(maximum).is_err());
}

#[test]
fn elb_interval_rejects_contradictory_status_and_unknown_fields() {
    let mut contradictory = valid_result();
    contradictory["error"] = json!("failed despite ok");
    assert!(serde_json::from_value::<ElispBenchmarksResult>(contradictory).is_err());
    let mut unknown = valid_result();
    unknown["elapsed_wall_us_typo"] = json!(240);
    assert!(serde_json::from_value::<ElispBenchmarksResult>(unknown).is_err());
}

/// The lane runs this nextest invocation inside sandbox-run; the GNU child
/// inherits that sandbox. A missing explicitly requested oracle is a failure,
/// never a skip that could make eight unexecuted Lisp regressions look green.
#[test]
fn elb_interval_runs_all_eight_fixture_protocol_regressions_with_forced_gnu() {
    use neomacs_melpa_test_support::{CommandError, output_with_timeout};
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::time::Duration;

    let requested = std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
        .filter(|path| !path.is_empty())
        .expect(
            "ELB protocol regressions require NEOVM_FORCE_ORACLE_PATH; do not skip the GNU oracle",
        );
    let requested = PathBuf::from(requested);
    // Resolve a relative forced path against nextest's original working
    // directory before setting the child's fixture working directory.
    let oracle = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()
            .expect("nextest working directory")
            .join(requested)
    };
    assert!(
        oracle.is_file(),
        "forced GNU oracle does not exist: {}",
        oracle.display()
    );
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("fixtures/elisp-benchmarks-tests.el");
    assert!(
        fixture.is_file(),
        "ELB protocol fixture is missing: {}",
        fixture.display()
    );
    let mut command = Command::new(&oracle);
    command.current_dir(&manifest).env("LC_ALL", "C");
    command.args(["-Q", "--batch", "--load"]).arg(&fixture);
    command.arg("--eval").arg(
        r#"
(let* ((stats (ert-run-tests-batch "^neomacs-perf-elb-"))
       (total (ert-stats-total stats))
       (completed (ert-stats-completed stats))
       (expected (ert-stats-completed-expected stats))
       (unexpected (ert-stats-completed-unexpected stats))
       (skipped (ert-stats-skipped stats)))
  (princ (format "NEOMACS_PERF_ELB_ERT total=%d completed=%d expected=%d unexpected=%d skipped=%d\n"
                 total completed expected unexpected skipped))
  (kill-emacs (if (and (= total 8) (= completed 8) (= expected 8)
                      (= unexpected 0) (= skipped 0)) 0 1)))
"#,
    );
    let timeout = Duration::from_secs(60);
    let diagnostics = |output: &Output| {
        format!(
            "forced GNU={} fixture={} status={}\nstdout:\n{}\nstderr:\n{}",
            oracle.display(),
            fixture.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    };
    let output = match output_with_timeout(&mut command, timeout) {
        Ok(output) => output,
        Err(CommandError::TimedOut(output)) => panic!(
            "ELB protocol GNU child timed out after {timeout:?}\n{}",
            diagnostics(&output)
        ),
        Err(CommandError::Launch(error)) => panic!(
            "could not launch forced GNU oracle {} for {}: {error}",
            oracle.display(),
            fixture.display()
        ),
        Err(CommandError::Capture(error)) => panic!(
            "could not capture ELB protocol GNU diagnostics for {}: {error}",
            oracle.display()
        ),
    };
    assert!(
        output.status.success(),
        "ELB protocol regressions failed\n{}",
        diagnostics(&output)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let counts: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("NEOMACS_PERF_ELB_ERT "))
        .collect();
    assert_eq!(
        counts,
        ["NEOMACS_PERF_ELB_ERT total=8 completed=8 expected=8 unexpected=0 skipped=0"],
        "eight complete, expected, unskipped ERT regressions must be proved\n{}",
        diagnostics(&output)
    );
}
