use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    EditorProvenance, Frontend, MetricName, MetricUnit, NativeVideoComparisonIdentity,
    NativeVideoExecutionIdentity, PerfError, PerfHarness, PortableDumpProvenance, RunRequest,
    RunVerdict, ScenarioId,
    artifact_store::{unix_time_ms, write_json_atomically},
    scenario,
};

pub(crate) const COMPARISON_ARTIFACT_SCHEMA_VERSION: u32 = 8;
static COMPARISON_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Immutable parameters shared by every run in one comparison.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonInput {
    pub scenario: ScenarioId,
    pub frontend: Frontend,
    pub iterations: NonZeroU32,
    pub samples_per_side: ComparisonSampleCount,
    pub primary_metric: MetricName,
    pub baseline_editor: PathBuf,
    pub candidate_editor: PathBuf,
    pub machine: crate::MachinePolicy,
    pub counters: Option<crate::CounterScope>,
    pub video_file: Option<PathBuf>,
    pub journal_file: Option<PathBuf>,
    pub baseline_execution_overrides: crate::ExecutionOverrides,
    pub candidate_execution_overrides: crate::ExecutionOverrides,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_baseline: Option<crate::VerifiedBaselineManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard_tools: Option<crate::ResolvedGuardTools>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonRunRole {
    Baseline,
    Candidate,
}

/// One underlying workload run retained by a comparison artifact.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonRun {
    pub role: ComparisonRunRole,
    pub sample_index: u32,
    pub run_id: String,
    pub artifact_path: PathBuf,
    pub scenario: ScenarioId,
    pub frontend: Frontend,
    pub editor: PathBuf,
    pub iterations: u32,
    pub editor_provenance: Option<EditorProvenance>,
    pub execution_environment: Option<BTreeMap<String, String>>,
    /// Full recorded passthrough environment from child `input-provenance.json`.
    /// Set-compared against the closed constructed guard environment when
    /// `ComparisonInput.expected_baseline` is `Some`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passthrough_environment: Option<BTreeMap<String, String>>,
    pub native_video_input: Option<NativeVideoComparisonIdentity>,
    pub native_video_execution: Option<NativeVideoExecutionIdentity>,
    pub outcome: ComparisonRunOutcome,
}

/// Classification only; detailed child results remain in the linked run artifact.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonRunOutcome {
    Valid,
    CorrectnessMismatch,
    InfrastructureFailure,
}

impl From<&RunVerdict> for ComparisonRunOutcome {
    fn from(verdict: &RunVerdict) -> Self {
        match verdict {
            RunVerdict::Valid { .. } => Self::Valid,
            RunVerdict::CorrectnessMismatch { .. } => Self::CorrectnessMismatch,
            RunVerdict::InfrastructureFailure { .. } => Self::InfrastructureFailure,
        }
    }
}

/// Evaluation-only data. It is deliberately not serializable so rejected
/// comparisons cannot accidentally embed valid child measurements.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComparisonObservation {
    pub(crate) run: ComparisonRun,
    pub(crate) verdict: RunVerdict,
}

/// Descriptive primary-metric values paired by the scheduled sample index.
/// These retain pairing when the marginal sample arrays below are sorted.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonPairedSample {
    pub sample_index: u32,
    pub baseline: f64,
    pub candidate: f64,
    pub candidate_to_baseline_ratio: f64,
    pub percent_change: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonMetricSummary {
    pub metric: MetricName,
    pub unit: MetricUnit,
    pub baseline_samples: Vec<f64>,
    pub candidate_samples: Vec<f64>,
    pub paired_samples: Vec<ComparisonPairedSample>,
    pub baseline_median: f64,
    pub candidate_median: f64,
    pub baseline_median_absolute_deviation: f64,
    pub candidate_median_absolute_deviation: f64,
    pub candidate_to_baseline_ratio: f64,
    pub percent_change: f64,
}

/// The minimum useful descriptive repetition count per editor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct ComparisonSampleCount(NonZeroU32);

impl ComparisonSampleCount {
    pub const MINIMUM: u32 = 3;

    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) if value.get() >= Self::MINIMUM => Some(Self(value)),
            Some(_) | None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for ComparisonSampleCount {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
            .ok_or_else(|| format!("comparison sample count must be at least {}", Self::MINIMUM))
    }
}

impl From<ComparisonSampleCount> for u32 {
    fn from(value: ComparisonSampleCount) -> Self {
        value.get()
    }
}

impl fmt::Display for ComparisonSampleCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

impl FromStr for ComparisonSampleCount {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .parse::<u32>()
            .map_err(|_| {
                format!("comparison sample count must be an unsigned integer, got `{value}`")
            })?
            .try_into()
    }
}

/// Machine-readable record for one interleaved two-editor comparison.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonArtifact {
    pub schema_version: u32,
    pub comparison_id: String,
    pub input: ComparisonInput,
    pub started_unix_ms: u128,
    pub total_elapsed_us: u128,
    pub verdict: ComparisonVerdict,
    pub runs: Vec<ComparisonRun>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonRequest {
    scenario: ScenarioId,
    baseline_editor: PathBuf,
    candidate_editor: PathBuf,
    samples_per_side: ComparisonSampleCount,
    iterations: NonZeroU32,
    frontend: Option<Frontend>,
    timeout: Duration,
    machine: crate::MachinePolicy,
    counters: Option<crate::CounterScope>,
    primary_metric: Option<MetricName>,
    expected_baseline: Option<crate::VerifiedBaselineManifest>,
    guard_tools: Option<crate::ResolvedGuardTools>,
    video_file: Option<PathBuf>,
    journal_file: Option<PathBuf>,
    baseline_execution_overrides: crate::ExecutionOverrides,
    candidate_execution_overrides: crate::ExecutionOverrides,
    inherited_environment: BTreeMap<String, std::ffi::OsString>,
}

impl ComparisonRequest {
    pub fn new(
        scenario: ScenarioId,
        baseline_editor: impl Into<PathBuf>,
        candidate_editor: impl Into<PathBuf>,
        samples_per_side: ComparisonSampleCount,
        iterations: NonZeroU32,
    ) -> Self {
        Self {
            scenario,
            baseline_editor: baseline_editor.into(),
            candidate_editor: candidate_editor.into(),
            samples_per_side,
            iterations,
            frontend: None,
            timeout: Duration::from_secs(300),
            machine: crate::MachinePolicy::default(),
            counters: None,
            primary_metric: None,
            expected_baseline: None,
            guard_tools: None,
            video_file: None,
            journal_file: None,
            baseline_execution_overrides: crate::ExecutionOverrides::default(),
            candidate_execution_overrides: crate::ExecutionOverrides::default(),
            inherited_environment: crate::harness::benchmark_passthrough_environment()
                .into_iter()
                .collect(),
        }
    }

    pub fn with_frontend(mut self, frontend: Frontend) -> Self {
        self.frontend = Some(frontend);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_machine_policy(mut self, machine: crate::MachinePolicy) -> Self {
        self.machine = machine;
        self
    }

    pub fn with_counters(mut self, counters: Option<crate::CounterScope>) -> Self {
        self.counters = counters;
        self
    }

    pub fn with_primary_metric(mut self, metric: MetricName) -> Self {
        self.primary_metric = Some(metric);
        self
    }

    pub fn with_expected_baseline(
        mut self,
        expected_baseline: Option<crate::VerifiedBaselineManifest>,
    ) -> Self {
        self.expected_baseline = expected_baseline;
        self
    }

    pub fn with_guard_tools(mut self, guard_tools: Option<crate::ResolvedGuardTools>) -> Self {
        self.guard_tools = guard_tools;
        self
    }

    pub fn with_video_file(mut self, video_file: Option<PathBuf>) -> Self {
        self.video_file = video_file;
        self
    }

    pub fn with_journal_file(mut self, journal_file: Option<PathBuf>) -> Self {
        self.journal_file = journal_file;
        self
    }

    pub fn with_execution_overrides(
        mut self,
        baseline: crate::ExecutionOverrides,
        candidate: crate::ExecutionOverrides,
    ) -> Self {
        self.baseline_execution_overrides = baseline;
        self.candidate_execution_overrides = candidate;
        self
    }

    pub fn frontend(&self) -> Frontend {
        self.frontend
            .unwrap_or_else(|| scenario(self.scenario).default_frontend)
    }

    #[cfg(test)]
    pub(crate) fn with_inherited_environment(
        mut self,
        inherited_environment: BTreeMap<String, std::ffi::OsString>,
    ) -> Self {
        self.inherited_environment = inherited_environment;
        self
    }

    /// Construct the closed inherited environment for a manifest-pinned guard
    /// comparison.
    ///
    /// Built explicitly from scratch rather than inheriting-and-subtracting from
    /// `BENCHMARK_PASSTHROUGH_ENVIRONMENT` / `NEOVM_JIT_*`. Empty by design:
    /// `command.env_clear()` plus `MelpaSandbox::process_environment()` already
    /// pin `HOME`, `TMPDIR`, `LANG`, `LC_ALL`, `TZ`, and `TERM` deterministically,
    /// while `perf` and `taskset` are resolved once to canonical absolute paths
    /// outside the child environment. Neither arm inherits `PATH`,
    /// `LD_LIBRARY_PATH` / `DYLD_*`, `NEOMACS_RUNTIME_ROOT`, `NEOVM_JIT*`,
    /// `NEOVM_GC_*`, or any other tuning knob.
    pub(crate) fn guard_constructed_environment() -> BTreeMap<String, std::ffi::OsString> {
        BTreeMap::new()
    }

    /// Resolve the inherited environment for one comparison arm.
    ///
    /// Manifest-pinned guard comparisons construct the closed guard environment
    /// identically for BOTH `Baseline` and `Candidate` arms; non-manifest
    /// comparisons (`SuiteId::Standard`, standalone `perf compare`) keep the
    /// existing inherited passthrough environment untouched.
    pub(crate) fn run_inherited_environment(
        &self,
        _role: ComparisonRunRole,
    ) -> BTreeMap<String, std::ffi::OsString> {
        if self.expected_baseline.is_some() {
            Self::guard_constructed_environment()
        } else {
            self.inherited_environment.clone()
        }
    }

    pub(crate) fn run_request_for(&self, role: ComparisonRunRole) -> RunRequest {
        let editor = match role {
            ComparisonRunRole::Baseline => &self.baseline_editor,
            ComparisonRunRole::Candidate => &self.candidate_editor,
        };
        let overrides = match role {
            ComparisonRunRole::Baseline => &self.baseline_execution_overrides,
            ComparisonRunRole::Candidate => &self.candidate_execution_overrides,
        };
        let taskset_path = self
            .guard_tools
            .as_ref()
            .and_then(|tools| tools.taskset_path.clone());
        let perf_path = self
            .guard_tools
            .as_ref()
            .map(|tools| tools.perf_path.clone());
        RunRequest::new(self.scenario, editor, self.iterations)
            .with_inherited_environment(self.run_inherited_environment(role))
            .with_resolved_tools(taskset_path, perf_path)
            .with_execution_overrides(overrides.clone())
            .with_frontend(self.frontend())
            .with_timeout(self.timeout)
            .with_machine_policy(self.machine.clone())
            .with_counters(self.counters)
            .with_video_file(self.video_file.clone())
            .with_journal_file(self.journal_file.clone())
    }

    pub(crate) fn build_input(&self) -> ComparisonInput {
        ComparisonInput {
            scenario: self.scenario,
            frontend: self.frontend(),
            iterations: self.iterations,
            samples_per_side: self.samples_per_side,
            primary_metric: self
                .primary_metric
                .unwrap_or_else(|| scenario(self.scenario).primary_metric),
            baseline_editor: self.baseline_editor.clone(),
            candidate_editor: self.candidate_editor.clone(),
            machine: self.machine.clone(),
            counters: self.counters,
            video_file: self.video_file.clone(),
            journal_file: self.journal_file.clone(),
            baseline_execution_overrides: self.baseline_execution_overrides.clone(),
            candidate_execution_overrides: self.candidate_execution_overrides.clone(),
            expected_baseline: self.expected_baseline.clone(),
            guard_tools: self.guard_tools.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonReport {
    pub artifact: ComparisonArtifact,
    pub artifact_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComparisonRejection {
    WrongSampleCount {
        role: ComparisonRunRole,
        expected: u32,
        actual: u32,
    },
    WrongSampleIndexes {
        role: ComparisonRunRole,
        expected: Vec<u32>,
        actual: Vec<u32>,
    },
    DuplicateRunId {
        run_id: String,
    },
    DuplicateArtifactPath {
        artifact_path: PathBuf,
    },
    ScenarioMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: ScenarioId,
        actual: ScenarioId,
    },
    FrontendMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: Frontend,
        actual: Frontend,
    },
    EditorMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: PathBuf,
        actual: PathBuf,
    },
    IterationsMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: u32,
        actual: u32,
    },
    ExecutionEnvironmentMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        reason: String,
    },
    MissingEditorProvenance {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
    },
    EditorProvenanceMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: EditorProvenance,
        actual: EditorProvenance,
    },
    BaselineManifestProvenanceMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        reason: String,
    },
    MissingPortableDumpProvenance {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
    },
    PortableDumpStateMismatch {
        baseline: PortableDumpProvenance,
        candidate: PortableDumpProvenance,
    },
    MissingNativeVideoInput {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
    },
    NativeVideoInputMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: NativeVideoComparisonIdentity,
        actual: NativeVideoComparisonIdentity,
    },
    MissingNativeVideoExecution {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
    },
    NativeVideoExecutionMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: NativeVideoExecutionIdentity,
        actual: NativeVideoExecutionIdentity,
    },
    InvalidRun {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
    },
    MissingMetric {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        metric: MetricName,
    },
    DuplicateMetric {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        metric: MetricName,
    },
    InvalidMetricValue {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        metric: MetricName,
    },
    MetricUnitMismatch {
        role: ComparisonRunRole,
        sample_index: u32,
        run_id: String,
        expected: MetricUnit,
        actual: MetricUnit,
    },
    CrossEditorParityMismatch {
        metric: MetricName,
        baseline_values: Vec<String>,
        candidate_values: Vec<String>,
    },
}

/// Aggregate outcome. Statistics exist only when every underlying run is valid.
///
/// **`Valid` means the runs completed and the workload checksums matched. It
/// does NOT mean the number is trustworthy.** A wall- or CPU-time metric is
/// hostage to whatever else the machine is doing; only instruction counts and
/// cycles (`--hardware-counters`) are deterministic. Gate on those and read
/// time as a secondary signal.
///
/// The comparison interleaves baseline and candidate samples, which makes the
/// BASELINE a control worth reading before believing any delta: if both sides
/// inflated and both MADs blew out, the machine was loaded and the run should
/// be discarded; if the baseline sat in its usual band and only the candidate
/// moved, the effect is real. See the crate README, "Which number decides".
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComparisonVerdict {
    Valid { summary: ComparisonMetricSummary },
    Rejected { reasons: Vec<ComparisonRejection> },
}

impl ComparisonVerdict {
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }
}

/// Interleave pairs and reverse odd pairs to reduce time-order bias.
pub fn comparison_schedule(
    samples_per_side: ComparisonSampleCount,
) -> Vec<(ComparisonRunRole, u32)> {
    let mut schedule = Vec::with_capacity(samples_per_side.get() as usize * 2);
    for sample_index in 0..samples_per_side.get() {
        if sample_index.is_multiple_of(2) {
            schedule.push((ComparisonRunRole::Baseline, sample_index));
            schedule.push((ComparisonRunRole::Candidate, sample_index));
        } else {
            schedule.push((ComparisonRunRole::Candidate, sample_index));
            schedule.push((ComparisonRunRole::Baseline, sample_index));
        }
    }
    schedule
}

pub(crate) fn evaluate_comparison(
    input: &ComparisonInput,
    observations: &[ComparisonObservation],
) -> ComparisonVerdict {
    let mut reasons = Vec::new();
    for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
        let mut actual_indexes = observations
            .iter()
            .filter_map(|observation| {
                (observation.run.role == role).then_some(observation.run.sample_index)
            })
            .collect::<Vec<_>>();
        actual_indexes.sort_unstable();
        let actual_count = actual_indexes.len() as u32;
        if actual_count != input.samples_per_side.get() {
            reasons.push(ComparisonRejection::WrongSampleCount {
                role,
                expected: input.samples_per_side.get(),
                actual: actual_count,
            });
        }
        let expected_indexes = (0..input.samples_per_side.get()).collect::<Vec<_>>();
        if actual_indexes != expected_indexes {
            reasons.push(ComparisonRejection::WrongSampleIndexes {
                role,
                expected: expected_indexes,
                actual: actual_indexes,
            });
        }
    }

    let mut run_ids = BTreeSet::new();
    let mut artifact_paths = BTreeSet::new();
    let mut baseline_provenance: Option<EditorProvenance> = None;
    let mut candidate_provenance: Option<EditorProvenance> = None;
    let mut native_video_input: Option<NativeVideoComparisonIdentity> = None;
    let mut native_video_execution: Option<NativeVideoExecutionIdentity> = None;
    let mut execution_environments = BTreeMap::new();
    let mut samples = Vec::with_capacity(observations.len());
    let expected_unit = input.primary_metric.canonical_unit();
    for observation in observations {
        let run = &observation.run;
        if !run_ids.insert(run.run_id.clone()) {
            reasons.push(ComparisonRejection::DuplicateRunId {
                run_id: run.run_id.clone(),
            });
        }
        if !artifact_paths.insert(run.artifact_path.clone()) {
            reasons.push(ComparisonRejection::DuplicateArtifactPath {
                artifact_path: run.artifact_path.clone(),
            });
        }
        if run.scenario != input.scenario {
            reasons.push(ComparisonRejection::ScenarioMismatch {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
                expected: input.scenario,
                actual: run.scenario,
            });
        }
        if run.frontend != input.frontend {
            reasons.push(ComparisonRejection::FrontendMismatch {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
                expected: input.frontend,
                actual: run.frontend,
            });
        }
        let expected_editor = match run.role {
            ComparisonRunRole::Baseline => &input.baseline_editor,
            ComparisonRunRole::Candidate => &input.candidate_editor,
        };
        if run.editor != *expected_editor {
            reasons.push(ComparisonRejection::EditorMismatch {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
                expected: expected_editor.clone(),
                actual: run.editor.clone(),
            });
        }
        if run.iterations != input.iterations.get() {
            reasons.push(ComparisonRejection::IterationsMismatch {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
                expected: input.iterations.get(),
                actual: run.iterations,
            });
        }
        let overrides = match run.role {
            ComparisonRunRole::Baseline => &input.baseline_execution_overrides,
            ComparisonRunRole::Candidate => &input.candidate_execution_overrides,
        };
        let environment_check = match &run.execution_environment {
            None => Err("missing execution environment provenance".to_owned()),
            Some(actual) => overrides.validate_recorded(actual).and_then(|()| {
                match execution_environments.get(&run.role) {
                    Some(expected) if expected != actual => {
                        Err("execution settings changed between samples of the same arm".to_owned())
                    }
                    Some(_) => Ok(()),
                    None => {
                        execution_environments.insert(run.role, actual.clone());
                        Ok(())
                    }
                }
            }),
        };
        if let Err(reason) = environment_check {
            reasons.push(ComparisonRejection::ExecutionEnvironmentMismatch {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
                reason,
            });
        }
        if let Some(expected_baseline) = &input.expected_baseline {
            if let Err(reason) = validate_guard_run_environment(expected_baseline, input, run) {
                reasons.push(ComparisonRejection::BaselineManifestProvenanceMismatch {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                    reason,
                });
            }
        }
        match &run.editor_provenance {
            None => reasons.push(ComparisonRejection::MissingEditorProvenance {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
            }),
            Some(actual) => {
                if actual.portable_dump == PortableDumpProvenance::Unknown {
                    reasons.push(ComparisonRejection::MissingPortableDumpProvenance {
                        role: run.role,
                        sample_index: run.sample_index,
                        run_id: run.run_id.clone(),
                    });
                }
                if let Some(expected_baseline) = &input.expected_baseline {
                    match run.role {
                        ComparisonRunRole::Baseline => {
                            if let Err(reason) = validate_baseline_manifest_provenance(
                                expected_baseline,
                                &input.baseline_editor,
                                actual,
                            ) {
                                reasons.push(
                                    ComparisonRejection::BaselineManifestProvenanceMismatch {
                                        role: run.role,
                                        sample_index: run.sample_index,
                                        run_id: run.run_id.clone(),
                                        reason,
                                    },
                                );
                            }
                        }
                        ComparisonRunRole::Candidate => {
                            if let Err(reason) = validate_candidate_guard_provenance(
                                expected_baseline,
                                &input.candidate_editor,
                                actual,
                            ) {
                                reasons.push(
                                    ComparisonRejection::BaselineManifestProvenanceMismatch {
                                        role: run.role,
                                        sample_index: run.sample_index,
                                        run_id: run.run_id.clone(),
                                        reason,
                                    },
                                );
                            }
                        }
                    }
                }
                let expected = match run.role {
                    ComparisonRunRole::Baseline => &mut baseline_provenance,
                    ComparisonRunRole::Candidate => &mut candidate_provenance,
                };
                match expected {
                    Some(expected) if expected != actual => {
                        reasons.push(ComparisonRejection::EditorProvenanceMismatch {
                            role: run.role,
                            sample_index: run.sample_index,
                            run_id: run.run_id.clone(),
                            expected: expected.clone(),
                            actual: actual.clone(),
                        });
                    }
                    Some(_) => {}
                    None => *expected = Some(actual.clone()),
                }
            }
        }
        if input.scenario == ScenarioId::SustainedNativeVideo {
            match &run.native_video_input {
                None => reasons.push(ComparisonRejection::MissingNativeVideoInput {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                }),
                Some(actual) => match &native_video_input {
                    Some(expected) if expected != actual => {
                        reasons.push(ComparisonRejection::NativeVideoInputMismatch {
                            role: run.role,
                            sample_index: run.sample_index,
                            run_id: run.run_id.clone(),
                            expected: expected.clone(),
                            actual: actual.clone(),
                        });
                    }
                    Some(_) => {}
                    None => native_video_input = Some(actual.clone()),
                },
            }
            match &run.native_video_execution {
                None => reasons.push(ComparisonRejection::MissingNativeVideoExecution {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                }),
                Some(actual) => match &native_video_execution {
                    Some(expected) if expected != actual => {
                        reasons.push(ComparisonRejection::NativeVideoExecutionMismatch {
                            role: run.role,
                            sample_index: run.sample_index,
                            run_id: run.run_id.clone(),
                            expected: expected.clone(),
                            actual: actual.clone(),
                        });
                    }
                    Some(_) => {}
                    None => native_video_execution = Some(actual.clone()),
                },
            }
        }

        let Some(measurements) = observation.verdict.measurements() else {
            reasons.push(ComparisonRejection::InvalidRun {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
            });
            continue;
        };
        let mut seen_metrics = BTreeSet::new();
        let mut duplicate_metrics = BTreeSet::new();
        for measurement in measurements {
            if !seen_metrics.insert(measurement.name) && duplicate_metrics.insert(measurement.name)
            {
                reasons.push(ComparisonRejection::DuplicateMetric {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                    metric: measurement.name,
                });
            }
            let value_is_invalid = !measurement.value.is_finite()
                || measurement.value < 0.0
                || (measurement.name == input.primary_metric && measurement.value == 0.0)
                || (measurement.name.canonical_unit() == MetricUnit::Count
                    && measurement.value.fract() != 0.0);
            if value_is_invalid {
                reasons.push(ComparisonRejection::InvalidMetricValue {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                    metric: measurement.name,
                });
            }
            let canonical_unit = measurement.name.canonical_unit();
            if measurement.unit != canonical_unit {
                reasons.push(ComparisonRejection::MetricUnitMismatch {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                    expected: canonical_unit,
                    actual: measurement.unit,
                });
            }
        }
        let matching = measurements
            .iter()
            .filter(|measurement| measurement.name == input.primary_metric)
            .collect::<Vec<_>>();
        let measurement = match matching.as_slice() {
            [] => {
                reasons.push(ComparisonRejection::MissingMetric {
                    role: run.role,
                    sample_index: run.sample_index,
                    run_id: run.run_id.clone(),
                    metric: input.primary_metric,
                });
                continue;
            }
            [measurement] => *measurement,
            _ => continue,
        };
        if !measurement.value.is_finite() || measurement.value <= 0.0 {
            continue;
        }
        if measurement.unit != expected_unit {
            continue;
        }
        samples.push((run.role, run.sample_index, measurement.value));
    }

    if let (Some(baseline), Some(candidate)) = (&baseline_provenance, &candidate_provenance) {
        if baseline.portable_dump != PortableDumpProvenance::Unknown
            && candidate.portable_dump != PortableDumpProvenance::Unknown
            && !baseline
                .portable_dump
                .same_load_state(&candidate.portable_dump)
        {
            reasons.push(ComparisonRejection::PortableDumpStateMismatch {
                baseline: baseline.portable_dump.clone(),
                candidate: candidate.portable_dump.clone(),
            });
        }
    }

    for parity_metric in scenario(input.scenario).cross_editor_parity_metrics {
        let metric = parity_metric.metric_name();
        let mut baseline_values =
            parity_metric_values(observations, ComparisonRunRole::Baseline, metric);
        let mut candidate_values =
            parity_metric_values(observations, ComparisonRunRole::Candidate, metric);
        baseline_values.sort_by(f64::total_cmp);
        candidate_values.sort_by(f64::total_cmp);
        if baseline_values != candidate_values {
            reasons.push(ComparisonRejection::CrossEditorParityMismatch {
                metric,
                baseline_values: render_metric_values(&baseline_values),
                candidate_values: render_metric_values(&candidate_values),
            });
        }
    }

    if !reasons.is_empty() {
        return ComparisonVerdict::Rejected { reasons };
    }

    let mut paired_values = BTreeMap::new();
    for &(role, sample_index, value) in &samples {
        let pair = paired_values.entry(sample_index).or_insert([None, None]);
        pair[match role {
            ComparisonRunRole::Baseline => 0,
            ComparisonRunRole::Candidate => 1,
        }] = Some(value);
    }
    let paired_samples = paired_values
        .into_iter()
        .map(|(sample_index, pair)| {
            let [Some(baseline), Some(candidate)] = pair else {
                unreachable!("validated comparison has both roles at every sample index")
            };
            let ratio = candidate / baseline;
            ComparisonPairedSample {
                sample_index,
                baseline,
                candidate,
                candidate_to_baseline_ratio: ratio,
                percent_change: (ratio - 1.0) * 100.0,
            }
        })
        .collect();

    let mut baseline_samples = samples
        .iter()
        .filter_map(|(role, _, value)| (*role == ComparisonRunRole::Baseline).then_some(*value))
        .collect::<Vec<_>>();
    let mut candidate_samples = samples
        .iter()
        .filter_map(|(role, _, value)| (*role == ComparisonRunRole::Candidate).then_some(*value))
        .collect::<Vec<_>>();
    baseline_samples.sort_by(f64::total_cmp);
    candidate_samples.sort_by(f64::total_cmp);
    let baseline_median = median(baseline_samples.iter().copied());
    let candidate_median = median(candidate_samples.iter().copied());
    let baseline_mad = median_absolute_deviation(&baseline_samples, baseline_median);
    let candidate_mad = median_absolute_deviation(&candidate_samples, candidate_median);
    let ratio = candidate_median / baseline_median;
    ComparisonVerdict::Valid {
        summary: ComparisonMetricSummary {
            metric: input.primary_metric,
            unit: expected_unit,
            baseline_samples,
            candidate_samples,
            paired_samples,
            baseline_median,
            candidate_median,
            baseline_median_absolute_deviation: baseline_mad,
            candidate_median_absolute_deviation: candidate_mad,
            candidate_to_baseline_ratio: ratio,
            percent_change: (ratio - 1.0) * 100.0,
        },
    }
}

fn validate_guard_run_environment(
    expected: &crate::VerifiedBaselineManifest,
    input: &ComparisonInput,
    run: &ComparisonRun,
) -> Result<(), String> {
    let overrides = match run.role {
        ComparisonRunRole::Baseline => &input.baseline_execution_overrides,
        ComparisonRunRole::Candidate => &input.candidate_execution_overrides,
    };
    if *overrides != crate::ExecutionOverrides::default() {
        return Err(format!(
            "manifest-pinned guard {:?} run forbids execution overrides (manifest={}): {:?}",
            run.role,
            expected.manifest_path.display(),
            overrides
        ));
    }
    if input.primary_metric == MetricName::Instructions
        && input.counters != Some(crate::CounterScope::EditLoop)
    {
        return Err(format!(
            "manifest-pinned instruction guard requires counter scope EditLoop (manifest={}), got {:?}",
            expected.manifest_path.display(),
            input.counters
        ));
    }
    if !input.candidate_editor.is_absolute() {
        return Err(format!(
            "manifest-pinned guard requires canonical absolute candidate executable path (manifest={}), got {}",
            expected.manifest_path.display(),
            input.candidate_editor.display()
        ));
    }
    match &input.guard_tools {
        None => {
            return Err(format!(
                "manifest-pinned guard requires resolved canonical host tools (`perf`, and `taskset` when --cpu is set) (manifest={})",
                expected.manifest_path.display()
            ));
        }
        Some(tools) => {
            if !tools.perf_path.is_absolute() {
                return Err(format!(
                    "manifest-pinned guard requires canonical absolute `perf` path, got {} (manifest={})",
                    tools.perf_path.display(),
                    expected.manifest_path.display()
                ));
            }
            if input.machine.cpu.is_some() {
                match &tools.taskset_path {
                    Some(path) if path.is_absolute() => {}
                    Some(path) => {
                        return Err(format!(
                            "manifest-pinned guard requires canonical absolute `taskset` path when --cpu is set, got {} (manifest={})",
                            path.display(),
                            expected.manifest_path.display()
                        ));
                    }
                    None => {
                        return Err(format!(
                            "manifest-pinned guard requires resolved `taskset` path when --cpu is set (manifest={})",
                            expected.manifest_path.display()
                        ));
                    }
                }
            }
        }
    }
    let Some(actual_env) = &run.passthrough_environment else {
        return Err(format!(
            "manifest-pinned guard {:?} run is missing full passthrough_environment provenance (manifest={})",
            run.role,
            expected.manifest_path.display()
        ));
    };
    let expected_env: BTreeMap<String, String> = ComparisonRequest::guard_constructed_environment()
        .into_iter()
        .map(|(k, v)| (k, v.to_string_lossy().into_owned()))
        .collect();
    if *actual_env != expected_env {
        let unexpected = actual_env
            .iter()
            .filter(|(k, v)| expected_env.get(*k) != Some(*v))
            .map(|(k, v)| format!("{k}={v:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "manifest-pinned guard {:?} run environment differs from closed guard contract (manifest={}): unexpected [{}]",
            run.role,
            expected.manifest_path.display(),
            unexpected
        ));
    }
    Ok(())
}

fn validate_candidate_guard_provenance(
    expected: &crate::VerifiedBaselineManifest,
    configured_candidate_editor: &Path,
    actual: &EditorProvenance,
) -> Result<(), String> {
    if !configured_candidate_editor.is_absolute() {
        return Err(format!(
            "manifest-pinned guard candidate executable must use a canonical absolute path (manifest={}, configured={})",
            expected.manifest_path.display(),
            configured_candidate_editor.display(),
        ));
    }
    let expected_candidate_path = configured_candidate_editor.to_string_lossy();
    if actual.path != expected_candidate_path {
        return Err(format!(
            "candidate observed canonical executable path mismatch (manifest={}): expected {}, observed {}",
            expected.manifest_path.display(),
            expected_candidate_path,
            actual.path,
        ));
    }
    Ok(())
}

fn validate_baseline_manifest_provenance(
    expected: &crate::VerifiedBaselineManifest,
    configured_baseline_editor: &Path,
    actual: &EditorProvenance,
) -> Result<(), String> {
    if !expected.binary_path.is_absolute() || !configured_baseline_editor.is_absolute() {
        return Err(format!(
            "manifest-pinned baseline executable must use a canonical absolute path (manifest={}, configured={}, verified={})",
            expected.manifest_path.display(),
            configured_baseline_editor.display(),
            expected.binary_path.display(),
        ));
    }
    if configured_baseline_editor != expected.binary_path {
        return Err(format!(
            "baseline editor path mismatch against manifest {}: expected {}, configured {}",
            expected.manifest_path.display(),
            expected.binary_path.display(),
            configured_baseline_editor.display(),
        ));
    }
    let expected_binary_path = expected.binary_path.to_string_lossy();
    if actual.path != expected_binary_path {
        return Err(format!(
            "baseline observed canonical executable path mismatch against manifest {}: expected {}, observed {}",
            expected.manifest_path.display(),
            expected_binary_path,
            actual.path,
        ));
    }
    if actual.executable_sha256 != expected.binary_sha256
        || actual.executable_size_bytes != expected.binary_size_bytes
    {
        return Err(format!(
            "baseline executable provenance mismatch against manifest {}: expected sha256={} ({} B), observed sha256={} ({} B)",
            expected.manifest_path.display(),
            expected.binary_sha256,
            expected.binary_size_bytes,
            actual.executable_sha256,
            actual.executable_size_bytes,
        ));
    }
    match &actual.portable_dump {
        PortableDumpProvenance::Loaded {
            path,
            sha256,
            size_bytes,
        } => {
            if *sha256 != expected.pdump_sha256 || *size_bytes != expected.pdump_size_bytes {
                return Err(format!(
                    "baseline loaded portable dump `{path}` does not match manifest {}: expected sha256={} ({} B), observed sha256={} ({} B)",
                    expected.manifest_path.display(),
                    expected.pdump_sha256,
                    expected.pdump_size_bytes,
                    sha256,
                    size_bytes,
                ));
            }
            Ok(())
        }
        other => Err(format!(
            "baseline run did not load verified portable dump from manifest {}: observed portable_dump={other:?}",
            expected.manifest_path.display(),
        )),
    }
}

fn extract_execution_environment(
    passthrough: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    passthrough
        .iter()
        .filter(|(name, _)| {
            matches!(
                name.as_str(),
                "NEOVM_JIT" | "NEOVM_JIT_OSR" | "NEOVM_JIT_THRESHOLD" | "NEOVM_JIT_LOOP_HEAT"
            )
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn parity_metric_values(
    observations: &[ComparisonObservation],
    role: ComparisonRunRole,
    metric: MetricName,
) -> Vec<f64> {
    observations
        .iter()
        .filter(|observation| observation.run.role == role)
        .filter_map(|observation| {
            let measurements = observation.verdict.measurements()?;
            let mut matching = measurements
                .iter()
                .filter(|measurement| measurement.name == metric);
            let measurement = matching.next()?;
            matching.next().is_none().then_some(measurement.value)
        })
        .collect()
}

fn render_metric_values(values: &[f64]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

impl PerfHarness {
    pub fn compare(&self, request: &ComparisonRequest) -> Result<ComparisonReport, PerfError> {
        let started = Instant::now();
        let started_unix_ms = unix_time_ms();
        let sequence = COMPARISON_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let comparison_id = format!(
            "{}-compare-{started_unix_ms}-{}-{sequence}",
            request.scenario,
            std::process::id()
        );
        let directory = self
            .workspace_root
            .join("tmp/perf-comparisons")
            .join(&comparison_id);
        fs::create_dir_all(&directory).map_err(|source| PerfError::CreateArtifactDirectory {
            path: directory.clone(),
            source,
        })?;

        let input = request.build_input();
        let mut observations = Vec::with_capacity(request.samples_per_side.get() as usize * 2);
        for (role, sample_index) in comparison_schedule(request.samples_per_side) {
            let run_request = request.run_request_for(role);
            let report = self.run(&run_request)?;
            let run = comparison_run_from_child(role, sample_index, &report);
            observations.push(ComparisonObservation {
                run,
                verdict: report.artifact.verdict,
            });
        }
        let verdict = evaluate_comparison(&input, &observations);
        let runs = observations
            .into_iter()
            .map(|observation| observation.run)
            .collect();
        let artifact = ComparisonArtifact {
            schema_version: COMPARISON_ARTIFACT_SCHEMA_VERSION,
            comparison_id,
            input,
            started_unix_ms,
            total_elapsed_us: started.elapsed().as_micros(),
            verdict,
            runs,
        };
        let artifact_path = directory.join("comparison.json");
        write_json_atomically(&artifact_path, &artifact)?;
        Ok(ComparisonReport {
            artifact,
            artifact_path,
        })
    }
}

pub(crate) fn comparison_run_from_child(
    role: ComparisonRunRole,
    sample_index: u32,
    report: &crate::RunReport,
) -> ComparisonRun {
    let child_provenance = read_child_provenance(&report.artifact_path);
    let child = &report.artifact;
    let outcome = ComparisonRunOutcome::from(&child.verdict);
    ComparisonRun {
        role,
        sample_index,
        run_id: child.run_id.clone(),
        artifact_path: report.artifact_path.clone(),
        scenario: child.scenario,
        frontend: child.frontend,
        editor: child.editor.clone(),
        iterations: child.iterations,
        editor_provenance: child_provenance
            .as_ref()
            .map(|provenance| provenance.editor.clone()),
        execution_environment: child_provenance
            .as_ref()
            .and_then(|p| p.passthrough_environment.as_ref())
            .map(extract_execution_environment),
        passthrough_environment: child_provenance
            .as_ref()
            .and_then(|p| p.passthrough_environment.clone()),
        native_video_input: child_provenance.and_then(|provenance| provenance.comparison_identity),
        native_video_execution: child.native_video_execution.clone(),
        outcome,
    }
}

#[derive(Deserialize)]
struct ComparisonInputProvenance {
    editor: EditorProvenance,
    #[serde(default)]
    passthrough_environment: Option<BTreeMap<String, String>>,
    #[serde(default)]
    comparison_identity: Option<NativeVideoComparisonIdentity>,
}

fn read_child_provenance(artifact_path: &Path) -> Option<ComparisonInputProvenance> {
    let path = artifact_path.parent()?.join("input-provenance.json");
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let midpoint = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[midpoint - 1] + values[midpoint]) / 2.0
    } else {
        values[midpoint]
    }
}

fn median_absolute_deviation(values: &[f64], center: f64) -> f64 {
    median(values.iter().map(|value| (value - center).abs()))
}

#[cfg(test)]
#[path = "comparison/tests/comparison_test.rs"]
mod tests;
