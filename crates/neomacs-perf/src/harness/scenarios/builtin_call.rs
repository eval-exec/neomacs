//! Warmed bytecode loops calling builtins through aliases. Compilation, warmup
//! and validation are outside sampling; each operation includes loop overhead.

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
    let workload_source = "crates/neomacs-perf/fixtures/builtin-call.el";
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
    let provenance_manifest = BuiltinCallInputProvenanceManifest {
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
        workload: PreparedWorkload::BuiltinCall,
    })
}

#[derive(Serialize)]
struct BuiltinCallInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    execution_policy: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "BuiltinCallResultWire")]
pub(crate) struct BuiltinCallResult {
    wire: BuiltinCallResultWire,
    outcome: ScenarioOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltinCallResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    completed_operations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    result_value: String,
    call_alias: String,
    warmup_iterations: u32,
    warmup_results: Vec<String>,
    bytecode_compiled: bool,
    alias_retained: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<BuiltinCallResultWire> for BuiltinCallResult {
    type Error = String;

    fn try_from(mut wire: BuiltinCallResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error.take())?;
        Ok(Self { wire, outcome })
    }
}

#[cfg(test)]
impl BuiltinCallResult {
    pub(crate) const fn elapsed_us(&self) -> u64 {
        self.wire.elapsed_us
    }
}

fn expected(scenario: ScenarioId) -> (&'static str, &'static str) {
    match scenario {
        ScenarioId::BuiltinCallPoint => ("neomacs-perf-builtin-call-zero", "1"),
        ScenarioId::BuiltinCallStringBytes => ("neomacs-perf-builtin-call-one", "3"),
        ScenarioId::BuiltinCallStringLessp => ("neomacs-perf-builtin-call-two", "t"),
        ScenarioId::BuiltinCallGetTextProperty => ("neomacs-perf-builtin-call-three", "nil"),
        ScenarioId::BuiltinCallMultibyteStringP => ("neomacs-perf-builtin-call-multibyte", "t"),
        ScenarioId::BuiltinCallCharOrStringP => ("neomacs-perf-builtin-call-character", "t"),
        ScenarioId::BuiltinCallMaxChar => ("neomacs-perf-builtin-call-vector-control", "1114111"),
        _ => unreachable!("builtin call adapter requires a builtin call scenario"),
    }
}

pub(crate) fn validate(
    request: &RunRequest,
    result: &BuiltinCallResult,
) -> Vec<CorrectnessMismatch> {
    let r = &result.wire;
    let (alias, value) = expected(request.scenario);
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
    mismatch(&mut mismatches, "call-alias", alias, r.call_alias.as_str());
    mismatch(
        &mut mismatches,
        "result-value",
        value,
        r.result_value.as_str(),
    );
    mismatch(
        &mut mismatches,
        "warmup-iterations",
        2000,
        r.warmup_iterations,
    );
    mismatch(&mut mismatches, "warmup-count", 40, r.warmup_results.len());
    for (index, actual) in r.warmup_results.iter().enumerate() {
        mismatch(
            &mut mismatches,
            &format!("warmup-result-{index}"),
            value,
            actual.as_str(),
        );
    }
    mismatch(
        &mut mismatches,
        "bytecode-compiled",
        true,
        r.bytecode_compiled,
    );
    mismatch(&mut mismatches, "alias-retained", true, r.alias_retained);
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-cpu-time", r.elapsed_us);
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-wall-time", r.elapsed_wall_us);
    mismatches
}

pub(crate) fn measurements(result: &BuiltinCallResult, process_us: u128) -> Vec<Measurement> {
    let r = &result.wire;
    call_measurements(
        r.iterations,
        r.completed_operations,
        r.elapsed_us,
        r.elapsed_wall_us,
        process_us,
    )
}

fn call_measurements(
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
mod tests;
