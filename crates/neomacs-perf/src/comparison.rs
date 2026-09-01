use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    EditorProvenance, Frontend, MetricName, MetricUnit, PerfError, PerfHarness, RunRequest,
    RunVerdict, ScenarioId,
    artifact_store::{unix_time_ms, write_json_atomically},
    scenario,
};

const COMPARISON_ARTIFACT_SCHEMA_VERSION: u32 = 1;
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonMetricSummary {
    pub metric: MetricName,
    pub unit: MetricUnit,
    pub baseline_samples: Vec<f64>,
    pub candidate_samples: Vec<f64>,
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

    pub fn frontend(&self) -> Frontend {
        self.frontend
            .unwrap_or_else(|| scenario(self.scenario).default_frontend)
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
        match &run.editor_provenance {
            None => reasons.push(ComparisonRejection::MissingEditorProvenance {
                role: run.role,
                sample_index: run.sample_index,
                run_id: run.run_id.clone(),
            }),
            Some(actual) => {
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
        samples.push((run.role, measurement.value));
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

    let mut baseline_samples = samples
        .iter()
        .filter_map(|(role, value)| (*role == ComparisonRunRole::Baseline).then_some(*value))
        .collect::<Vec<_>>();
    let mut candidate_samples = samples
        .iter()
        .filter_map(|(role, value)| (*role == ComparisonRunRole::Candidate).then_some(*value))
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
            baseline_median,
            candidate_median,
            baseline_median_absolute_deviation: baseline_mad,
            candidate_median_absolute_deviation: candidate_mad,
            candidate_to_baseline_ratio: ratio,
            percent_change: (ratio - 1.0) * 100.0,
        },
    }
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

        let frontend = request.frontend();
        let input = ComparisonInput {
            scenario: request.scenario,
            frontend,
            iterations: request.iterations,
            samples_per_side: request.samples_per_side,
            primary_metric: scenario(request.scenario).primary_metric,
            baseline_editor: request.baseline_editor.clone(),
            candidate_editor: request.candidate_editor.clone(),
        };
        let mut observations = Vec::with_capacity(request.samples_per_side.get() as usize * 2);
        for (role, sample_index) in comparison_schedule(request.samples_per_side) {
            let editor = match role {
                ComparisonRunRole::Baseline => &request.baseline_editor,
                ComparisonRunRole::Candidate => &request.candidate_editor,
            };
            let run_request = RunRequest::new(request.scenario, editor, request.iterations)
                .with_frontend(frontend)
                .with_timeout(request.timeout);
            let report = self.run(&run_request)?;
            let editor_provenance = read_child_editor_provenance(&report.artifact_path);
            let child = report.artifact;
            let outcome = ComparisonRunOutcome::from(&child.verdict);
            observations.push(ComparisonObservation {
                run: ComparisonRun {
                    role,
                    sample_index,
                    run_id: child.run_id,
                    artifact_path: report.artifact_path,
                    scenario: child.scenario,
                    frontend: child.frontend,
                    editor: child.editor,
                    iterations: child.iterations,
                    editor_provenance,
                    outcome,
                },
                verdict: child.verdict,
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

#[derive(Deserialize)]
struct ComparisonInputProvenance {
    editor: EditorProvenance,
}

fn read_child_editor_provenance(artifact_path: &Path) -> Option<EditorProvenance> {
    let path = artifact_path.parent()?.join("input-provenance.json");
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<ComparisonInputProvenance>(&bytes)
        .ok()
        .map(|provenance| provenance.editor)
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
