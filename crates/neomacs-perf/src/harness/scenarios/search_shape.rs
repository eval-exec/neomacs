//! Six bounded literal/regexp/POSIX search loops over the same multibyte text.
//! Compilation, warmup and validation are outside sampling; each operation
//! includes repositioning the point and the bytecode loop bookkeeping.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName,
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
    let workload_source = "crates/neomacs-perf/fixtures/search-shape.el";
    let fixture_source = workspace_root.join(workload_source);
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join(fixture_source.file_name().expect("fixture filename"));
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = SearchShapeInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source,
        workload_source_sha256: sha256_file(&fixture_source)?,
        execution_policy: "editor-default-with-recorded-overrides",
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
        workload: PreparedWorkload::SearchShape,
    })
}

#[derive(Serialize)]
struct SearchShapeInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    execution_policy: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "SearchShapeResultWire")]
pub(crate) struct SearchShapeResult {
    wire: SearchShapeResultWire,
    outcome: ScenarioOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchShapeResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    completed_operations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    result_value: u32,
    point: u32,
    match_data: String,
    buffer_contents: String,
    buffer_size: u32,
    buffer_bytes: u32,
    multibyte: bool,
    warmup_single_calls: u32,
    warmup_batch_calls: u32,
    warmup_batch_iterations: u32,
    warmup_validations: u32,
    bytecode_compiled: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<SearchShapeResultWire> for SearchShapeResult {
    type Error = String;

    fn try_from(mut wire: SearchShapeResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error.take())?;
        Ok(Self { wire, outcome })
    }
}

#[cfg(test)]
impl SearchShapeResult {
    pub(crate) const fn elapsed_us(&self) -> u64 {
        self.wire.elapsed_us
    }
}

fn expected(scenario: ScenarioId) -> (u32, &'static str) {
    match scenario {
        ScenarioId::SearchLiteralForward => (4, "(2 4 t)"),
        ScenarioId::SearchLiteralBackward => (5, "(5 7 t)"),
        ScenarioId::SearchRegexpForward | ScenarioId::SearchPosixForward => (4, "(2 4 2 4 t)"),
        ScenarioId::SearchRegexpBackward | ScenarioId::SearchPosixBackward => (5, "(5 7 5 7 t)"),
        _ => unreachable!("search shape adapter requires a search shape scenario"),
    }
}

pub(crate) fn validate(
    request: &RunRequest,
    result: &SearchShapeResult,
) -> Vec<CorrectnessMismatch> {
    let r = &result.wire;
    let (value, match_data) = expected(request.scenario);
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        r.schema_version,
    );
    mismatch(&mut mismatches, "scenario-id", request.scenario, r.scenario);
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
        r.iterations,
    );
    mismatch(
        &mut mismatches,
        "completed-operations",
        request.iterations.get(),
        r.completed_operations,
    );
    mismatch(&mut mismatches, "result-value", value, r.result_value);
    mismatch(&mut mismatches, "point", value, r.point);
    mismatch(
        &mut mismatches,
        "match-data",
        match_data,
        r.match_data.as_str(),
    );
    mismatch(
        &mut mismatches,
        "buffer-contents",
        "éab abc",
        r.buffer_contents.as_str(),
    );
    mismatch(&mut mismatches, "buffer-size", 7, r.buffer_size);
    mismatch(&mut mismatches, "buffer-bytes", 8, r.buffer_bytes);
    mismatch(&mut mismatches, "multibyte", true, r.multibyte);
    mismatch(
        &mut mismatches,
        "warmup-single-calls",
        2000,
        r.warmup_single_calls,
    );
    mismatch(
        &mut mismatches,
        "warmup-batch-calls",
        20,
        r.warmup_batch_calls,
    );
    mismatch(
        &mut mismatches,
        "warmup-batch-iterations",
        100,
        r.warmup_batch_iterations,
    );
    mismatch(
        &mut mismatches,
        "warmup-validations",
        2020,
        r.warmup_validations,
    );
    mismatch(
        &mut mismatches,
        "bytecode-compiled",
        true,
        r.bytecode_compiled,
    );
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-cpu-time", r.elapsed_us);
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-wall-time", r.elapsed_wall_us);
    mismatches
}

pub(crate) fn measurements(result: &SearchShapeResult, process_us: u128) -> Vec<Measurement> {
    let r = &result.wire;
    search_measurements(
        r.iterations,
        r.completed_operations,
        r.elapsed_us,
        r.elapsed_wall_us,
        process_us,
    )
}

fn search_measurements(
    iterations: u32,
    completed: u32,
    cpu_us: u64,
    wall_us: u64,
    process_us: u128,
) -> Vec<Measurement> {
    [
        (MetricName::ProcessWallTime, process_us as f64),
        (MetricName::WorkloadCpuTime, cpu_us as f64),
        (MetricName::WorkloadWallTime, wall_us as f64),
        (
            MetricName::PerOperationCpuTime,
            cpu_us as f64 / f64::from(iterations),
        ),
        (
            MetricName::PerOperationWallTime,
            wall_us as f64 / f64::from(iterations),
        ),
        (MetricName::OperationCount, f64::from(completed)),
        (MetricName::Iterations, f64::from(iterations)),
    ]
    .into_iter()
    .map(|(name, value)| Measurement {
        name,
        value,
        unit: name.canonical_unit(),
    })
    .collect()
}

#[cfg(test)]
#[path = "search_shape_test.rs"]
mod tests;
