//! The mx-tab-completion scenario: a real M-x TAB completion-window
//! lifecycle over 1,024 controlled commands.

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
const MX_TAB_COMPLETION_CANDIDATE_COUNT: u64 = 1024;

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/mx-tab-completion.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("mx-tab-completion.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = MxTabInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/mx-tab-completion.el",
        workload_source_sha256: sha256_file(&fixture_source)?,
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
        workload: PreparedWorkload::MxTabCompletion,
    })
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "MxTabCompletionResultWire")]
pub(crate) struct MxTabCompletionResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    completion_help_calls: u32,
    completion_visible: bool,
    completion_mode_correct: bool,
    known_commands_present: bool,
    completion_candidate_count: u64,
    candidate_count_stable: bool,
    completion_hidden_after_exit: bool,
    minibuffer_depth_restored: bool,
    selected_buffer_restored: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MxTabCompletionResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    completion_help_calls: u32,
    completion_visible: bool,
    completion_mode_correct: bool,
    known_commands_present: bool,
    completion_candidate_count: u64,
    candidate_count_stable: bool,
    completion_hidden_after_exit: bool,
    minibuffer_depth_restored: bool,
    selected_buffer_restored: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<MxTabCompletionResultWire> for MxTabCompletionResult {
    type Error = String;

    fn try_from(wire: MxTabCompletionResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            completion_help_calls: wire.completion_help_calls,
            completion_visible: wire.completion_visible,
            completion_mode_correct: wire.completion_mode_correct,
            known_commands_present: wire.known_commands_present,
            completion_candidate_count: wire.completion_candidate_count,
            candidate_count_stable: wire.candidate_count_stable,
            completion_hidden_after_exit: wire.completion_hidden_after_exit,
            minibuffer_depth_restored: wire.minibuffer_depth_restored,
            selected_buffer_restored: wire.selected_buffer_restored,
        })
    }
}

#[derive(Serialize)]
struct MxTabInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_mx_tab_completion_result(
    request: &RunRequest,
    result: &MxTabCompletionResult,
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
        "completion-help-calls",
        request.iterations.get(),
        result.completion_help_calls,
    );
    mismatch(
        &mut mismatches,
        "completion-window-visible",
        true,
        result.completion_visible,
    );
    mismatch(
        &mut mismatches,
        "completion-buffer-mode",
        true,
        result.completion_mode_correct,
    );
    mismatch(
        &mut mismatches,
        "known-command-candidates",
        true,
        result.known_commands_present,
    );
    mismatch(
        &mut mismatches,
        "completion-candidate-count-stable",
        true,
        result.candidate_count_stable,
    );
    mismatch(
        &mut mismatches,
        "completion-candidate-count",
        MX_TAB_COMPLETION_CANDIDATE_COUNT,
        result.completion_candidate_count,
    );
    mismatch(
        &mut mismatches,
        "completion-window-hidden-after-exit",
        true,
        result.completion_hidden_after_exit,
    );
    mismatch(
        &mut mismatches,
        "minibuffer-depth-restored",
        true,
        result.minibuffer_depth_restored,
    );
    mismatch(
        &mut mismatches,
        "selected-buffer-restored",
        true,
        result.selected_buffer_restored,
    );
    mismatches
}

pub(crate) fn valid_mx_tab_completion_measurements(
    result: &MxTabCompletionResult,
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
            name: MetricName::PerCompletionCpuTime,
            value: result.elapsed_us as f64 / f64::from(result.completion_help_calls.max(1)),
            unit: MetricUnit::MicrosecondsPerCompletion,
        },
        Measurement {
            name: MetricName::CompletionHelpCalls,
            value: f64::from(result.completion_help_calls),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::CompletionCandidateCount,
            value: result.completion_candidate_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ]
}
