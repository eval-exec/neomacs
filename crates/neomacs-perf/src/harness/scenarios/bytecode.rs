//! The bytecode-call-loop scenario: tier-0 bytecode-to-bytecode calls
//! with the Neomacs JIT disabled.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit,
    PreparedScenario, PreparedWorkload, RunRequest, SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId,
    ScenarioOutcome, ScenarioStatus, benchmark_passthrough_environment, collect_editor_provenance,
    collect_host_provenance, deserialize_optional_error, mismatch, prepare_gui_runtime_directory,
    scenario_outcome, sha256_file,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/bytecode-call-loop.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("bytecode-call-loop.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = BytecodeCallInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/bytecode-call-loop.el",
        workload_source_sha256: sha256_file(&fixture_source)?,
        execution_tier: "tier-0-interpreter",
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
        workload: PreparedWorkload::BytecodeCallLoop,
    })
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "BytecodeCallLoopResultWire")]
pub(crate) struct BytecodeCallLoopResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    bytecode_calls: u64,
    result: i64,
    expected_result: i64,
    bytecode_functions_compiled: bool,
    interpreter_requested: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BytecodeCallLoopResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    bytecode_calls: u64,
    result: i64,
    expected_result: i64,
    bytecode_functions_compiled: bool,
    interpreter_requested: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<BytecodeCallLoopResultWire> for BytecodeCallLoopResult {
    type Error = String;

    fn try_from(wire: BytecodeCallLoopResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            bytecode_calls: wire.bytecode_calls,
            result: wire.result,
            expected_result: wire.expected_result,
            bytecode_functions_compiled: wire.bytecode_functions_compiled,
            interpreter_requested: wire.interpreter_requested,
        })
    }
}

#[derive(Serialize)]
struct BytecodeCallInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    execution_tier: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_bytecode_call_loop_result(
    request: &RunRequest,
    result: &BytecodeCallLoopResult,
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
        "bytecode-call-count",
        u64::from(request.iterations.get()),
        result.bytecode_calls,
    );
    mismatch(
        &mut mismatches,
        "bytecode-result",
        result.expected_result,
        result.result,
    );
    mismatch(
        &mut mismatches,
        "expected-bytecode-result",
        1,
        result.expected_result,
    );
    mismatch(
        &mut mismatches,
        "bytecode-functions-compiled",
        true,
        result.bytecode_functions_compiled,
    );
    mismatch(
        &mut mismatches,
        "tier-0-interpreter-requested",
        true,
        result.interpreter_requested,
    );
    mismatches
}

pub(crate) fn valid_bytecode_call_loop_measurements(
    result: &BytecodeCallLoopResult,
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
            name: MetricName::PerBytecodeCallCpuTime,
            value: result.elapsed_us as f64 / result.bytecode_calls.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerBytecodeCall,
        },
        Measurement {
            name: MetricName::BytecodeCalls,
            value: result.bytecode_calls as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ]
}
