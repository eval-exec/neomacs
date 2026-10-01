//! The scrolling scenario: page up/down (`scroll-up`/`scroll-down`) over a
//! deterministic face-rich buffer, with the cold (first display of every
//! line) and warm (re-display of laid-out rows) phases timed apart.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit,
    PreparedScenario, PreparedWorkload, RunRequest, SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId,
    ScenarioOutcome, ScenarioStatus, collect_editor_provenance, collect_host_provenance,
    deserialize_optional_error, mismatch, prepare_gui_runtime_directory, scenario_outcome,
    sha256_file,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/scrolling.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("scrolling.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let content_source = workspace_root.join("crates/neomacs-perf/fixtures/scrolling-content.el");
    fs::copy(&content_source, run_directory.join("scrolling-content.el"))
        .map_err(|error| format!("failed to copy scrolling content fixture: {error}"))?;
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = ScrollingInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/scrolling.el",
        workload_source_sha256: sha256_file(&fixture_source)?,
        content_source_sha256: sha256_file(&content_source)?,
        environment_policy: "closed-v1",
        passthrough_environment: request
            .benchmark_environment()
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
        workload: PreparedWorkload::Scrolling,
    })
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "ScrollingResultWire")]
pub(crate) struct ScrollingResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    cold_scroll_us: u64,
    warm_scroll_us: u64,
    cold_scroll_commands: u64,
    warm_scroll_commands: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    window_start_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrollingResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    cold_scroll_us: u64,
    warm_scroll_us: u64,
    cold_scroll_commands: u64,
    warm_scroll_commands: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    window_start_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<ScrollingResultWire> for ScrollingResult {
    type Error = String;

    fn try_from(wire: ScrollingResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            operation_count: wire.operation_count,
            cold_scroll_us: wire.cold_scroll_us,
            warm_scroll_us: wire.warm_scroll_us,
            cold_scroll_commands: wire.cold_scroll_commands,
            warm_scroll_commands: wire.warm_scroll_commands,
            initial_checksum: wire.initial_checksum,
            final_checksum: wire.final_checksum,
            point_restored: wire.point_restored,
            window_start_restored: wire.window_start_restored,
            expected_major_mode: wire.expected_major_mode,
            actual_major_mode: wire.actual_major_mode,
        })
    }
}

#[derive(Serialize)]
struct ScrollingInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    content_source_sha256: String,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_scrolling_result(
    request: &RunRequest,
    result: &ScrollingResult,
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
        request.scenario,
        result.scenario,
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
    mismatch(
        &mut mismatches,
        "operation-count",
        result.cold_scroll_commands + result.warm_scroll_commands,
        result.operation_count,
    );
    mismatch(
        &mut mismatches,
        "cold-scroll-commands",
        true,
        result.cold_scroll_commands > 0,
    );
    mismatch(
        &mut mismatches,
        "warm-scroll-commands",
        true,
        result.warm_scroll_commands > 0,
    );
    mismatch(
        &mut mismatches,
        "cold-phase-time",
        true,
        result.cold_scroll_us > 0,
    );
    mismatch(
        &mut mismatches,
        "warm-phase-time",
        true,
        result.warm_scroll_us > 0,
    );
    mismatch(
        &mut mismatches,
        "final-buffer-checksum",
        result.initial_checksum.as_str(),
        result.final_checksum.as_str(),
    );
    mismatch(
        &mut mismatches,
        "point-restored",
        true,
        result.point_restored,
    );
    mismatch(
        &mut mismatches,
        "window-start-restored",
        true,
        result.window_start_restored,
    );
    mismatch(
        &mut mismatches,
        "major-mode",
        result.expected_major_mode.as_str(),
        result.actual_major_mode.as_str(),
    );
    if result.initial_checksum.is_empty() {
        mismatches.push(CorrectnessMismatch {
            invariant: "initial-buffer-checksum".to_string(),
            expected: "non-empty".to_string(),
            actual: "empty".to_string(),
        });
    }
    if result.elapsed_us == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "elapsed-time".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    mismatches
}

pub(crate) fn valid_scrolling_measurements(
    result: &ScrollingResult,
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
            name: MetricName::WorkloadWallTime,
            value: result.elapsed_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerOperationCpuTime,
            value: result.elapsed_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::PerOperationWallTime,
            value: result.elapsed_wall_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::OperationCount,
            value: result.operation_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::ColdScrollPhaseCpuTime,
            value: result.cold_scroll_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WarmScrollPhaseCpuTime,
            value: result.warm_scroll_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::ScrollCommandCount,
            value: result.operation_count as f64,
            unit: MetricUnit::Count,
        },
    ]
}
