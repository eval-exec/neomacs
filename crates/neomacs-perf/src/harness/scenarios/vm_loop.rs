//! Rarely called, long-running bytecode loops under the editor's normal tier policy.
//! The paired scenarios differ by a live special binding across the hot loop.
//! First-entry diagnostics share preparation but validate every fresh function.

pub(crate) mod first_entry;

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
    let workload_source = if request.scenario.first_hot_loop_iterations().is_some() {
        "crates/neomacs-perf/fixtures/first-hot-loop.el"
    } else {
        "crates/neomacs-perf/fixtures/vm-loop.el"
    };
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
    let provenance_manifest = VmLoopInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source,
        first_call_inner_iterations: request.scenario.first_hot_loop_iterations(),
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
        workload: PreparedWorkload::VmLoop,
    })
}

#[derive(Serialize)]
struct VmLoopInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_call_inner_iterations: Option<u32>,
    workload_source_sha256: String,
    execution_policy: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "VmLoopResultWire")]
pub(crate) struct VmLoopResult {
    wire: VmLoopResultWire,
    outcome: ScenarioOutcome,
}

#[cfg(test)]
impl VmLoopResult {
    pub(crate) const fn elapsed_us(&self) -> u64 {
        self.wire.elapsed_us
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmLoopResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    completed_operations: u32,
    result_sum: i64,
    held_value: i64,
    outer_value_before: i64,
    outer_value_after: i64,
    warmup_result: [i64; 3],
    warmup_outer_value: i64,
    dynamic_binding: bool,
    bytecode_compiled: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<VmLoopResultWire> for VmLoopResult {
    type Error = String;

    fn try_from(mut wire: VmLoopResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error.take())?;
        Ok(Self { wire, outcome })
    }
}

fn expected_sum(iterations: u32) -> i64 {
    let n = u64::from(iterations);
    // Even u32::MAX * (u32::MAX - 1) fits u64; the halved result fits i64.
    (n * n.saturating_sub(1) / 2) as i64
}

pub(crate) fn validate_vm_loop_result(
    request: &RunRequest,
    result: &VmLoopResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    let r = &result.wire;
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
    mismatch(
        &mut mismatches,
        "sum",
        expected_sum(request.iterations.get()),
        r.result_sum,
    );
    mismatch(&mut mismatches, "held-value", 7, r.held_value);
    mismatch(
        &mut mismatches,
        "outer-value-before",
        17,
        r.outer_value_before,
    );
    mismatch(
        &mut mismatches,
        "outer-value-after",
        17,
        r.outer_value_after,
    );
    mismatch(
        &mut mismatches,
        "warmup-operations",
        100,
        r.warmup_result[0],
    );
    mismatch(
        &mut mismatches,
        "warmup-sum",
        expected_sum(100),
        r.warmup_result[1],
    );
    mismatch(&mut mismatches, "warmup-held-value", 7, r.warmup_result[2]);
    mismatch(
        &mut mismatches,
        "warmup-outer-value",
        17,
        r.warmup_outer_value,
    );
    mismatch(
        &mut mismatches,
        "dynamic-binding",
        request.scenario == ScenarioId::DynamicBindingLoop,
        r.dynamic_binding,
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

pub(crate) fn valid_vm_loop_measurements(
    result: &VmLoopResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let r = &result.wire;
    loop_measurements(
        r.iterations,
        r.completed_operations,
        r.elapsed_us,
        r.elapsed_wall_us,
        wall_elapsed_us,
    )
}

fn loop_measurements(
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
#[path = "vm_loop_test.rs"]
mod tests;
