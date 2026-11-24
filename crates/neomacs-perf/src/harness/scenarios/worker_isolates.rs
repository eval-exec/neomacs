//! Correctness-gated workloads for the opt-in worker isolate pool.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, PreparedScenario, PreparedWorkload, RunRequest,
    SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId, ScenarioOutcome, ScenarioStatus,
    collect_editor_provenance, collect_host_provenance, mismatch, prepare_gui_runtime_directory,
    scenario_outcome, sha256_file,
};
use crate::{EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let source = workspace_root.join("crates/neomacs-perf/fixtures/worker-isolates.el");
    if !source.is_file() {
        return Err(format!("missing performance fixture {}", source.display()));
    }
    let fixture = run_directory.join("worker-isolates.el");
    fs::copy(&source, &fixture)
        .map_err(|error| format!("failed to copy fixture {}: {error}", source.display()))?;
    let provenance = run_directory.join("input-provenance.json");
    let manifest = WorkerIsolatesInputProvenance {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/worker-isolates.el",
        workload_source_sha256: sha256_file(&source)?,
        execution_tier: execution_tier(request),
        passthrough_environment: request
            .benchmark_environment()
            .into_iter()
            .map(|(name, value)| (name, value.to_string_lossy().into_owned()))
            .collect(),
    };
    fs::write(
        &provenance,
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("failed to serialize provenance: {error}"))?,
    )
    .map_err(|error| format!("failed to write provenance: {error}"))?;
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
        workload: PreparedWorkload::WorkerIsolates,
    })
}

fn execution_tier(request: &RunRequest) -> &'static str {
    match request.benchmark_environment().get("NEOVM_JIT") {
        None => "editor-default",
        Some(value) if value.to_string_lossy() == "0" => "jit-disabled",
        Some(_) => "jit-enabled",
    }
}

#[derive(Serialize)]
struct WorkerIsolatesInputProvenance<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    execution_tier: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "WorkerIsolatesResultWire")]
pub(crate) struct WorkerIsolatesResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    pub(crate) elapsed_us: u64,
    num_processors: u32,
    pool_size: u32,
    first_task_latency_us: u64,
    boot_max_us: u64,
    boot_mean_us: u64,
    samples: Vec<WorkerIsolatesSample>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerIsolatesResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    elapsed_us: u64,
    num_processors: u32,
    pool_size: u32,
    first_task_latency_us: u64,
    boot_max_us: u64,
    boot_mean_us: u64,
    samples: Vec<WorkerIsolatesSample>,
    #[serde(
        deserialize_with = "crate::harness::deserialize_optional_error",
        rename = "error"
    )]
    error: Option<String>,
}

impl TryFrom<WorkerIsolatesResultWire> for WorkerIsolatesResult {
    type Error = String;

    fn try_from(wire: WorkerIsolatesResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            elapsed_us: wire.elapsed_us,
            num_processors: wire.num_processors,
            pool_size: wire.pool_size,
            first_task_latency_us: wire.first_task_latency_us,
            boot_max_us: wire.boot_max_us,
            boot_mean_us: wire.boot_mean_us,
            samples: wire.samples,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerIsolatesSample {
    tier: String,
    tasks: u32,
    serial_seconds: f64,
    parallel_first_seconds: f64,
    parallel_seconds: f64,
    serial_parallel_ratio: f64,
    results_equal: bool,
}

pub(crate) fn validate(
    request: &RunRequest,
    result: &WorkerIsolatesResult,
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
    if result.num_processors == 0 {
        mismatch(&mut mismatches, "num-processors-positive", true, false);
    }
    if result.pool_size == 0 {
        mismatch(&mut mismatches, "pool-size-positive", true, false);
    }
    if result.elapsed_us == 0 {
        mismatch(&mut mismatches, "elapsed-wall-time-positive", true, false);
    }
    if result.samples.is_empty() {
        mismatch(&mut mismatches, "worker-samples-present", true, false);
    }
    let expected_samples = match result.scenario {
        ScenarioId::WorkerIsolatesLoop => 4,
        ScenarioId::WorkerIsolatesJson | ScenarioId::WorkerIsolatesBoot => 1,
        _ => 0,
    };
    mismatch(
        &mut mismatches,
        "worker-sample-count",
        expected_samples,
        result.samples.len(),
    );
    for (index, sample) in result.samples.iter().enumerate() {
        if sample.tier.trim().is_empty() || sample.tasks == 0 {
            mismatch(
                &mut mismatches,
                &format!("sample-{index}-identity"),
                true,
                false,
            );
        }
        if !sample.results_equal {
            mismatch(
                &mut mismatches,
                &format!("sample-{index}-results-equal"),
                true,
                false,
            );
        }
        let serial_is_valid =
            result.scenario == ScenarioId::WorkerIsolatesBoot || sample.serial_seconds > 0.0;
        if !sample.serial_seconds.is_finite()
            || !sample.parallel_first_seconds.is_finite()
            || sample.parallel_first_seconds <= 0.0
            || !sample.parallel_seconds.is_finite()
            || !serial_is_valid
            || sample.parallel_seconds <= 0.0
            || !sample.serial_parallel_ratio.is_finite()
            || sample.serial_parallel_ratio <= 0.0
        {
            mismatch(
                &mut mismatches,
                &format!("sample-{index}-measurements-positive"),
                true,
                false,
            );
        }
        let expected_tasks = match (result.scenario, index) {
            (ScenarioId::WorkerIsolatesLoop, 0 | 2) => result.pool_size,
            (ScenarioId::WorkerIsolatesLoop, 1 | 3) => result.pool_size.saturating_mul(4),
            (ScenarioId::WorkerIsolatesJson, 0) => result.pool_size,
            (ScenarioId::WorkerIsolatesBoot, 0) => 1,
            _ => 0,
        };
        mismatch(
            &mut mismatches,
            &format!("sample-{index}-task-count"),
            expected_tasks,
            sample.tasks,
        );
        if result.scenario != ScenarioId::WorkerIsolatesBoot {
            let expected_ratio = sample.serial_seconds / sample.parallel_seconds;
            let ratio_error = (sample.serial_parallel_ratio - expected_ratio).abs();
            if ratio_error > expected_ratio.abs().max(1.0) * 1e-9 {
                mismatch(
                    &mut mismatches,
                    &format!("sample-{index}-ratio-consistency"),
                    format!("{expected_ratio:.9}"),
                    format!("{:.9}", sample.serial_parallel_ratio),
                );
            }
        }
    }
    if result.scenario == ScenarioId::WorkerIsolatesBoot
        && (result.first_task_latency_us == 0
            || result.boot_max_us == 0
            || result.boot_mean_us == 0)
    {
        mismatch(&mut mismatches, "boot-measurements-positive", true, false);
    }
    mismatches
}

pub(crate) fn measurements(
    result: &WorkerIsolatesResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let primary = result
        .samples
        .first()
        .map_or(wall_elapsed_us as f64, |sample| {
            sample.parallel_seconds * 1_000_000.0
        });
    vec![
        Measurement {
            name: MetricName::PerOperationWallTime,
            value: primary,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::OperationCount,
            value: result.samples.len() as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::WorkloadWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
    ]
}

#[cfg(test)]
#[path = "worker_isolates/tests/worker_isolates_test.rs"]
mod tests;
