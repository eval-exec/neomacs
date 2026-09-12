//! The `elisp-benchmarks` scenario: GNU ELPA's own Elisp benchmark suite.
//!
//! Every other fixture in this crate was written here, and auditing them found
//! that each either flattered this engine or hid a defect. A suite authored
//! upstream cannot be shaped to our strengths, which is the entire reason this
//! row exists.
//!
//! It is NOT an editor benchmark: twelve of its eighteen members are
//! arithmetic and list compute. It is deliberately kept out of
//! `STANDARD_SCENARIOS`, so it can never enter the board's geometric mean.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::{EmacsRuntime, MelpaSandbox, prepare_cached_gnu_elpa_package};
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit,
    PreparedScenario, PreparedWorkload, RunRequest, SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId,
    ScenarioOutcome, ScenarioStatus, benchmark_passthrough_environment, collect_editor_provenance,
    collect_host_provenance, deserialize_optional_error, mismatch, prepare_gui_runtime_directory,
    scenario_outcome, sha256_file,
};

/// The pinned release. Note that GNU ELPA serves only an UNVERSIONED
/// `elisp-benchmarks.tar`; the version is what `package-archive-contents`
/// reports, which is what the cache validates against, so the pin is real even
/// though the URL does not carry it.
const ELISP_BENCHMARKS_PIN: (&str, &str) = ("elisp-benchmarks", "1.16");

/// Benchmark SOURCE FILES in release 1.16. Pinned so a suite that silently
/// shrank -- an upstream file rename, a half-extracted archive -- is a
/// correctness failure rather than a quiet speed-up.
///
/// This counts files, not reported tests: `fibn.el` alone contributes four
/// rows to the suite's table (`fibn`, `fibn-named-let`, `fibn-rec`,
/// `fibn-tc`), so 17 files produce 22 measured benchmarks.
const EXPECTED_BENCHMARK_COUNT: u32 = 17;

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/elisp-benchmarks.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("elisp-benchmarks.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;

    // Installed with GNU Emacs, like every other pinned package here, so the
    // measured engine never participates in preparing its own input.
    let package_dir =
        prepare_cached_gnu_elpa_package(&EmacsRuntime::gnu_emacs(), ELISP_BENCHMARKS_PIN)?;

    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = ElispBenchmarksInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/elisp-benchmarks.el",
        workload_source_sha256: sha256_file(&fixture_source)?,
        package_name: ELISP_BENCHMARKS_PIN.0,
        package_version: ELISP_BENCHMARKS_PIN.1,
        package_archive: "https://elpa.gnu.org/packages/",
        environment_policy: "closed-v1",
        passthrough_environment: benchmark_passthrough_environment()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
            .collect(),
    };
    let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
        .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
    fs::write(&provenance, provenance_json).map_err(|error| {
        format!(
            "failed to write input provenance {}: {error}",
            provenance.display()
        )
    })?;

    Ok(PreparedScenario {
        fixture,
        provenance,
        result: run_directory.join("scenario-result.json"),
        sentinel: run_directory.join("completed"),
        terminal_bytes: run_directory.join("terminal.ansi"),
        gui_app_log: run_directory.join("gui-app.log"),
        gui_weston_log: run_directory.join("weston.log"),
        gui_runtime_directory: prepare_gui_runtime_directory(workspace_root)?,
        sandbox,
        workload: PreparedWorkload::ElispBenchmarks {
            package_dir,
            report: run_directory.join("elisp-benchmarks-report.txt"),
        },
    })
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "ElispBenchmarksResultWire")]
pub(crate) struct ElispBenchmarksResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    benchmark_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ElispBenchmarksResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    benchmark_count: u32,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<ElispBenchmarksResultWire> for ElispBenchmarksResult {
    type Error = String;

    fn try_from(wire: ElispBenchmarksResultWire) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            benchmark_count: wire.benchmark_count,
        })
    }
}

#[derive(Serialize)]
struct ElispBenchmarksInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    package_name: &'a str,
    package_version: &'a str,
    package_archive: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_elisp_benchmarks_result(
    request: &RunRequest,
    result: &ElispBenchmarksResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario.workload_str(),
        result.scenario.as_str(),
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    // A suite that quietly lost members would look like a speed-up.
    mismatch(
        &mut mismatches,
        "benchmark-count",
        EXPECTED_BENCHMARK_COUNT,
        result.benchmark_count,
    );
    if result.elapsed_us == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "elapsed-time".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    mismatches
}

pub(crate) fn valid_elisp_benchmarks_measurements(
    result: &ElispBenchmarksResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerOperationCpuTime,
            value: result.elapsed_us as f64 / u64::from(result.iterations).max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::OperationCount,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ]
}
