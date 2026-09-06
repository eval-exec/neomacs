//! The shared editor-workload scenario family: one elisp fixture
//! (`fixtures/editor-workloads.el`) driving the nine catalogued workloads
//! over deterministic sources, with per-scenario phase invariants.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use neomacs_melpa_test_support::{
    EmacsRuntime, MelpaSandbox, PreparedPackageSet, locked_melpa_sources,
};
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, INPUT_LATENCY_BUDGET_US, Measurement,
    MetricName, MetricUnit, PackageProvenance, PreparedScenario, PreparedWorkload, RunRequest,
    SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId, ScenarioOutcome, ScenarioStatus,
    benchmark_passthrough_environment, collect_editor_provenance, collect_host_provenance,
    deserialize_optional_error, mismatch, nearest_rank, prepare_gui_runtime_directory,
    require_positive_phase, scenario_outcome, sha256_file,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/editor-workloads.el");
    let source_fixture = workspace_root.join("lisp/emacs-lisp/bytecomp.el");
    for required in [&fixture_source, &source_fixture] {
        if !required.is_file() {
            return Err(format!(
                "missing committed performance fixture {}",
                required.display()
            ));
        }
    }
    let fixture = run_directory.join("editor-workloads.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let source = run_directory.join("workload-source.el");
    if request.scenario == ScenarioId::LargeFileEditing {
        let seed = fs::read_to_string(&source_fixture)
            .map_err(|error| format!("failed to read {}: {error}", source_fixture.display()))?;
        let mut large = String::with_capacity(seed.len() * 8);
        for _ in 0..8 {
            large.push_str(&seed);
        }
        fs::write(&source, large).map_err(|error| {
            format!(
                "failed to write large-file fixture {}: {error}",
                source.display()
            )
        })?;
    } else {
        fs::copy(&source_fixture, &source).map_err(|error| {
            format!(
                "failed to copy source fixture {} to {}: {error}",
                source_fixture.display(),
                source.display()
            )
        })?;
    }

    let mut package_provenance = None;
    let mut startup = None;
    let mut packages = None;
    let repository = if request.scenario == ScenarioId::MagitStatus {
        let magit_source = locked_melpa_sources()?
            .into_iter()
            .find(|source| source.package().0 == "magit")
            .ok_or_else(|| "the MELPA source lock does not contain magit".to_string())?;
        let package = magit_source.package();
        let prepared =
            PreparedPackageSet::from_locked_melpa(&EmacsRuntime::gnu_emacs(), package, "magit.el")?;
        startup = Some(prepared.write_startup_file(run_directory)?);
        package_provenance = Some(PackageProvenance {
            name: package.0,
            version: package.1,
            repository: magit_source.repository(),
            revision: magit_source.revision(),
            upstream_repository: magit_source.upstream_repository(),
            upstream_revision: magit_source.upstream_revision(),
        });
        packages = Some(Box::new(prepared));
        Some(prepare_magit_repository(run_directory)?)
    } else {
        None
    };

    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = EditorWorkloadInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        scenario: request.scenario,
        workload_fixture_sha256: sha256_file(&fixture_source)?,
        source_fixture_sha256: sha256_file(&source)?,
        package: package_provenance,
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
        workload: PreparedWorkload::EditorWorkload {
            source,
            repository,
            startup,
            packages,
        },
    })
}

fn prepare_magit_repository(run_directory: &Path) -> Result<PathBuf, String> {
    let repository = run_directory.join("magit-repository");
    fs::create_dir_all(&repository).map_err(|error| {
        format!(
            "failed to create Magit fixture repository {}: {error}",
            repository.display()
        )
    })?;
    fs::write(repository.join("README.md"), "# neomacs-perf\n")
        .map_err(|error| format!("failed to write Magit fixture: {error}"))?;
    for arguments in [
        vec!["init", "--quiet"],
        vec!["add", "README.md"],
        vec![
            "-c",
            "user.name=neomacs-perf",
            "-c",
            "user.email=perf@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&repository)
            .output()
            .map_err(|error| format!("failed to launch git for Magit fixture: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "failed to prepare Magit fixture repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    fs::write(repository.join("README.md"), "# neomacs-perf\nmodified\n")
        .map_err(|error| format!("failed to modify Magit fixture: {error}"))?;
    Ok(repository)
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "EditorWorkloadResultWire")]
pub(crate) struct EditorWorkloadResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
    type_phase_us: u64,
    comment_phase_us: u64,
    kill_yank_phase_us: u64,
    indent_phase_us: u64,
    regex_phase_us: u64,
    latency_samples_us: Vec<u64>,
    mode_phase_us: u64,
    fontify_phase_us: u64,
    replace_phase_us: u64,
    undo_redo_phase_us: u64,
    isearch_phase_us: u64,
    buffer_switch_phase_us: u64,
    how_many_phase_us: u64,
    motion_phase_us: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorWorkloadResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
    type_phase_us: u64,
    comment_phase_us: u64,
    kill_yank_phase_us: u64,
    indent_phase_us: u64,
    regex_phase_us: u64,
    latency_samples_us: Vec<u64>,
    mode_phase_us: u64,
    fontify_phase_us: u64,
    replace_phase_us: u64,
    undo_redo_phase_us: u64,
    isearch_phase_us: u64,
    buffer_switch_phase_us: u64,
    how_many_phase_us: u64,
    motion_phase_us: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<EditorWorkloadResultWire> for EditorWorkloadResult {
    type Error = String;

    fn try_from(wire: EditorWorkloadResultWire) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            operation_count: wire.operation_count,
            initial_checksum: wire.initial_checksum,
            final_checksum: wire.final_checksum,
            point_restored: wire.point_restored,
            expected_major_mode: wire.expected_major_mode,
            actual_major_mode: wire.actual_major_mode,
            type_phase_us: wire.type_phase_us,
            comment_phase_us: wire.comment_phase_us,
            kill_yank_phase_us: wire.kill_yank_phase_us,
            indent_phase_us: wire.indent_phase_us,
            regex_phase_us: wire.regex_phase_us,
            latency_samples_us: wire.latency_samples_us,
            mode_phase_us: wire.mode_phase_us,
            fontify_phase_us: wire.fontify_phase_us,
            replace_phase_us: wire.replace_phase_us,
            undo_redo_phase_us: wire.undo_redo_phase_us,
            isearch_phase_us: wire.isearch_phase_us,
            buffer_switch_phase_us: wire.buffer_switch_phase_us,
            how_many_phase_us: wire.how_many_phase_us,
            motion_phase_us: wire.motion_phase_us,
        })
    }
}

#[derive(Serialize)]
struct EditorWorkloadInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    scenario: ScenarioId,
    workload_fixture_sha256: String,
    source_fixture_sha256: String,
    package: Option<PackageProvenance<'a>>,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_editor_workload_result(
    request: &RunRequest,
    result: &EditorWorkloadResult,
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
        u64::from(request.iterations.get()),
        result.operation_count,
    );
    mismatch(
        &mut mismatches,
        "final-buffer-checksum",
        result.initial_checksum.as_str(),
        result.final_checksum.as_str(),
    );
    mismatch(&mut mismatches, "final-point", true, result.point_restored);
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
    if result.expected_major_mode.is_empty() {
        mismatches.push(CorrectnessMismatch {
            invariant: "expected-major-mode".to_string(),
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
    if result.elapsed_wall_us == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "elapsed-wall-time".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    match request.scenario {
        ScenarioId::EditingSimulation => {
            for (name, value) in [
                ("mode-phase-time", result.mode_phase_us),
                ("fontify-phase-time", result.fontify_phase_us),
                ("regex-phase-time", result.regex_phase_us),
                ("type-phase-time", result.type_phase_us),
                ("replace-phase-time", result.replace_phase_us),
                ("indent-phase-time", result.indent_phase_us),
                ("kill-yank-phase-time", result.kill_yank_phase_us),
                ("undo-redo-phase-time", result.undo_redo_phase_us),
                ("isearch-phase-time", result.isearch_phase_us),
                ("buffer-switch-phase-time", result.buffer_switch_phase_us),
                ("comment-phase-time", result.comment_phase_us),
                ("how-many-phase-time", result.how_many_phase_us),
                ("motion-phase-time", result.motion_phase_us),
            ] {
                require_positive_phase(&mut mismatches, name, value);
            }
        }
        ScenarioId::SustainedEditing | ScenarioId::OrgEditing => {
            require_positive_phase(&mut mismatches, "type-phase-time", result.type_phase_us);
        }
        ScenarioId::MagitStatus | ScenarioId::RegexSearch => {
            require_positive_phase(&mut mismatches, "regex-phase-time", result.regex_phase_us);
        }
        ScenarioId::LargeFileEditing => {
            require_positive_phase(&mut mismatches, "type-phase-time", result.type_phase_us);
            require_positive_phase(&mut mismatches, "regex-phase-time", result.regex_phase_us);
            require_positive_phase(&mut mismatches, "motion-phase-time", result.motion_phase_us);
        }
        ScenarioId::Indentation => {
            require_positive_phase(&mut mismatches, "indent-phase-time", result.indent_phase_us);
        }
        ScenarioId::Startup | ScenarioId::GuiInputLatency => {}
        ScenarioId::OrgJournalOpen => {
            unreachable!("org-journal-open has a dedicated result validator")
        }
        ScenarioId::SustainedNativeVideo => {
            unreachable!("native video has a dedicated result validator")
        }
        ScenarioId::RustLspTyping | ScenarioId::MxTabCompletion | ScenarioId::BytecodeCallLoop => {
            unreachable!("dedicated scenario results do not use the editor workload validator")
        }
    }
    let expected_latency_samples = if request.scenario == ScenarioId::GuiInputLatency {
        request.iterations.get() as usize
    } else {
        0
    };
    mismatch(
        &mut mismatches,
        "latency-sample-count",
        expected_latency_samples,
        result.latency_samples_us.len(),
    );
    if result.latency_samples_us.contains(&0) {
        mismatches.push(CorrectnessMismatch {
            invariant: "latency-samples".to_string(),
            expected: "all positive".to_string(),
            actual: "contains zero".to_string(),
        });
    }
    mismatches
}

pub(crate) fn valid_editor_workload_measurements(
    result: &EditorWorkloadResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let mut measurements = vec![
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
    ];
    if result.scenario == ScenarioId::SustainedEditing {
        let edits = result.operation_count.saturating_mul(2);
        measurements.push(Measurement {
            name: MetricName::PerEditCpuTime,
            value: result.elapsed_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        });
        measurements.push(Measurement {
            name: MetricName::PerEditWallTime,
            value: result.elapsed_wall_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        });
    }
    if matches!(
        result.scenario,
        ScenarioId::SustainedEditing | ScenarioId::GuiInputLatency
    ) {
        let edits = result.operation_count.saturating_mul(2);
        measurements.extend([
            Measurement {
                name: MetricName::Edits,
                value: edits as f64,
                unit: MetricUnit::Count,
            },
            Measurement {
                name: MetricName::Redisplays,
                value: edits as f64,
                unit: MetricUnit::Count,
            },
        ]);
    }
    for (name, value) in [
        (MetricName::TypePhaseCpuTime, result.type_phase_us),
        (MetricName::CommentPhaseCpuTime, result.comment_phase_us),
        (MetricName::KillYankPhaseCpuTime, result.kill_yank_phase_us),
        (MetricName::IndentPhaseCpuTime, result.indent_phase_us),
        (MetricName::RegexPhaseCpuTime, result.regex_phase_us),
        (MetricName::ModePhaseCpuTime, result.mode_phase_us),
        (MetricName::FontifyPhaseCpuTime, result.fontify_phase_us),
        (MetricName::ReplacePhaseCpuTime, result.replace_phase_us),
        (MetricName::UndoRedoPhaseCpuTime, result.undo_redo_phase_us),
        (MetricName::IsearchPhaseCpuTime, result.isearch_phase_us),
        (
            MetricName::BufferSwitchPhaseCpuTime,
            result.buffer_switch_phase_us,
        ),
        (MetricName::HowManyPhaseCpuTime, result.how_many_phase_us),
        (MetricName::MotionPhaseCpuTime, result.motion_phase_us),
    ] {
        if value > 0 {
            measurements.push(Measurement {
                name,
                value: value as f64,
                unit: MetricUnit::Microseconds,
            });
        }
    }
    if !result.latency_samples_us.is_empty() {
        let mut samples = result.latency_samples_us.clone();
        samples.sort_unstable();
        for (name, percentile) in [
            (MetricName::P50InputToRedisplayLatency, 0.50),
            (MetricName::P95InputToRedisplayLatency, 0.95),
            (MetricName::P99InputToRedisplayLatency, 0.99),
        ] {
            measurements.push(Measurement {
                name,
                value: nearest_rank(&samples, percentile) as f64,
                unit: MetricUnit::Microseconds,
            });
        }
        let over_budget = samples
            .iter()
            .filter(|&&sample| sample > INPUT_LATENCY_BUDGET_US)
            .count();
        measurements.push(Measurement {
            name: MetricName::InputLatencyOverBudgetCount,
            value: over_budget as f64,
            unit: MetricUnit::Count,
        });
    }
    measurements
}
