use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifact_store::{unix_time_ms, write_json_atomically};
use crate::{
    ComparisonRequest, ComparisonSampleCount, ComparisonVerdict, CounterScope, MachinePolicy,
    PerfError, PerfHarness, ScenarioId, scenario,
};

static SUITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) const SUITE_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteId {
    Standard,
}

impl fmt::Display for SuiteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Standard => "standard",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownSuiteId(String);

impl fmt::Display for UnknownSuiteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown performance suite `{}`", self.0)
    }
}

impl std::error::Error for UnknownSuiteId {}

impl FromStr for SuiteId {
    type Err = UnknownSuiteId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "standard" => Ok(Self::Standard),
            value => Err(UnknownSuiteId(value.to_string())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuiteScenario {
    pub scenario: ScenarioId,
    pub maximum_regression_percent: f64,
    /// How far this row may move against the PREVIOUS suite run before the
    /// suite fails, or `None` to record the drift without gating on it.
    ///
    /// The budget above compares the candidate against the baseline within one
    /// run, so a row that regresses a little every time never trips it: five
    /// 2% regressions against a 10% budget all pass individually. This is the
    /// ratchet that catches the accumulation.
    ///
    /// It is armed per row rather than globally because a gate that fires on
    /// noise gets switched off. `rust-lsp-typing` spans 6.6% between runs of
    /// the same binary and `sustained-editing` about 0.8%, so arming those
    /// would produce false failures; the batch rows reproduce to ~0.03%.
    pub maximum_drift_percent: Option<f64>,
}

const STANDARD_SCENARIOS: &[SuiteScenario] = &[
    suite_scenario(ScenarioId::RustLspTyping, 8.0),
    suite_scenario(ScenarioId::RustLspTypingHeavy, 8.0),
    suite_scenario(ScenarioId::MxTabCompletion, 8.0),
    suite_scenario(ScenarioId::BytecodeCallLoop, 5.0),
    suite_scenario(ScenarioId::EditingSimulation, 8.0),
    suite_scenario(ScenarioId::Startup, 12.0),
    suite_scenario(ScenarioId::SustainedEditing, 8.0),
    suite_scenario(ScenarioId::GuiInputLatency, 15.0),
    ratcheted(ScenarioId::OrgEditing, 8.0, 2.0),
    ratcheted(ScenarioId::OrgEditingHeavy, 8.0, 2.0),
    ratcheted(ScenarioId::MagitStatus, 10.0, 2.0),
    ratcheted(ScenarioId::OrgJournalOpen, 10.0, 2.0),
    ratcheted(ScenarioId::LargeFileEditing, 8.0, 2.0),
    ratcheted(ScenarioId::Indentation, 8.0, 2.0),
    ratcheted(ScenarioId::RegexSearch, 8.0, 2.0),
    // Byte-code rows carry the same budgets as the source rows they mirror.
    ratcheted(ScenarioId::MagitStatusCompiled, 10.0, 2.0),
    ratcheted(ScenarioId::MagitStatusHeavy, 10.0, 2.0),
    ratcheted(ScenarioId::FileOpen, 8.0, 2.0),
    ratcheted(ScenarioId::OrgJournalOpenCompiled, 10.0, 2.0),
    // A pure compute round trip in batch, so it reproduces as tightly as the
    // other batch rows and can carry the same drift ratchet.
    ratcheted(ScenarioId::LspJsonRpc, 8.0, 2.0),
];

const fn suite_scenario(scenario: ScenarioId, maximum_regression_percent: f64) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent,
        maximum_drift_percent: None,
    }
}

/// A row whose run-to-run variance is small enough to ratchet against history.
const fn ratcheted(
    scenario: ScenarioId,
    maximum_regression_percent: f64,
    maximum_drift_percent: f64,
) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent,
        maximum_drift_percent: Some(maximum_drift_percent),
    }
}

impl SuiteId {
    pub const fn scenarios(self) -> &'static [SuiteScenario] {
        match self {
            Self::Standard => STANDARD_SCENARIOS,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuiteRequest {
    pub(crate) suite: SuiteId,
    pub(crate) baseline_editor: PathBuf,
    pub(crate) candidate_editor: PathBuf,
    pub(crate) samples: ComparisonSampleCount,
    pub(crate) timeout: Duration,
    pub(crate) machine: MachinePolicy,
    pub(crate) counters: Option<CounterScope>,
    pub(crate) previous_suite: Option<PathBuf>,
}

impl SuiteRequest {
    pub fn new(
        suite: SuiteId,
        baseline_editor: impl Into<PathBuf>,
        candidate_editor: impl Into<PathBuf>,
        samples: ComparisonSampleCount,
    ) -> Self {
        Self {
            suite,
            baseline_editor: baseline_editor.into(),
            candidate_editor: candidate_editor.into(),
            samples,
            timeout: Duration::from_secs(300),
            machine: MachinePolicy::default(),
            counters: None,
            previous_suite: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_machine_policy(mut self, machine: MachinePolicy) -> Self {
        self.machine = machine;
        self
    }

    pub fn with_counters(mut self, counters: Option<CounterScope>) -> Self {
        self.counters = counters;
        self
    }

    pub fn with_previous_suite(mut self, previous_suite: Option<PathBuf>) -> Self {
        self.previous_suite = previous_suite;
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteScenarioResult {
    pub scenario: ScenarioId,
    pub maximum_regression_percent: f64,
    /// Carried from the suite definition so the verdict can ratchet without
    /// re-deriving it, and so the artifact records what gate was in force.
    #[serde(default)]
    pub maximum_drift_percent: Option<f64>,
    pub percent_change: Option<f64>,
    pub comparison_artifact: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteHistoryLink {
    pub path: PathBuf,
    pub retained_path: PathBuf,
    pub suite_id: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteDrift {
    pub scenario: ScenarioId,
    pub previous_percent_change: f64,
    pub current_percent_change: f64,
    pub drift_percent: f64,
    pub maximum_drift_percent: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteRegression {
    pub scenario: ScenarioId,
    pub maximum_regression_percent: f64,
    pub actual_regression_percent: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SuiteVerdict {
    Passed,
    Regressed {
        regressions: Vec<SuiteRegression>,
    },
    /// The suite is inside every per-run budget but has moved against the
    /// previous run by more than an armed row allows. This is the failure the
    /// budgets cannot see: repeated small regressions that each pass.
    Drifted {
        drifts: Vec<SuiteDrift>,
    },
    Rejected {
        scenarios: Vec<ScenarioId>,
    },
}

impl SuiteVerdict {
    pub const fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteArtifact {
    pub schema_version: u32,
    pub suite_id: String,
    pub suite: SuiteId,
    pub baseline_editor: PathBuf,
    pub candidate_editor: PathBuf,
    pub samples_per_side: ComparisonSampleCount,
    pub machine: MachinePolicy,
    pub counters: Option<CounterScope>,
    pub started_unix_ms: u128,
    pub total_elapsed_us: u128,
    pub previous_suite: Option<SuiteHistoryLink>,
    pub scenarios: Vec<SuiteScenarioResult>,
    pub verdict: SuiteVerdict,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuiteReport {
    pub artifact: SuiteArtifact,
    pub artifact_path: PathBuf,
}

impl PerfHarness {
    pub fn suite(&self, request: &SuiteRequest) -> Result<SuiteReport, PerfError> {
        let started = Instant::now();
        let started_unix_ms = unix_time_ms();
        let sequence = SUITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suite_id = format!(
            "{}-{started_unix_ms}-{}-{sequence}",
            request.suite,
            std::process::id()
        );
        let directory = self.workspace_root.join("tmp/perf-suites").join(&suite_id);
        fs::create_dir_all(&directory).map_err(|source| PerfError::CreateArtifactDirectory {
            path: directory.clone(),
            source,
        })?;

        // The previous artifact was retained and linked but never read back,
        // so the suite had history and no ratchet. Keep both: the link for
        // provenance, the parsed artifact for the drift comparison.
        let (previous_suite, previous_artifact) = match request.previous_suite.as_deref() {
            Some(path) => {
                let (link, bytes) = read_history(path)?;
                let retained = directory.join(&link.retained_path);
                fs::write(&retained, &bytes).map_err(|source| PerfError::WriteArtifact {
                    path: retained,
                    source,
                })?;
                let parsed = serde_json::from_slice::<SuiteArtifact>(&bytes).map_err(|error| {
                    PerfError::InvalidSuiteHistory {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    }
                })?;
                (Some(link), Some(parsed))
            }
            None => (None, None),
        };
        let mut scenario_results = Vec::with_capacity(request.suite.scenarios().len());
        for suite_scenario in request.suite.scenarios() {
            let spec = scenario(suite_scenario.scenario);
            let comparison = ComparisonRequest::new(
                suite_scenario.scenario,
                &request.baseline_editor,
                &request.candidate_editor,
                request.samples,
                spec.default_iterations,
            )
            .with_frontend(spec.default_frontend)
            .with_timeout(request.timeout)
            .with_machine_policy(request.machine.clone())
            .with_counters(request.counters);
            let report = self.compare(&comparison)?;
            let percent_change = match &report.artifact.verdict {
                ComparisonVerdict::Valid { summary } => Some(summary.percent_change),
                ComparisonVerdict::Rejected { .. } => None,
            };
            scenario_results.push(SuiteScenarioResult {
                scenario: suite_scenario.scenario,
                maximum_regression_percent: suite_scenario.maximum_regression_percent,
                maximum_drift_percent: suite_scenario.maximum_drift_percent,
                percent_change,
                comparison_artifact: report.artifact_path,
            });
        }
        let verdict = evaluate_suite(&scenario_results, previous_artifact.as_ref());
        let artifact = SuiteArtifact {
            schema_version: SUITE_ARTIFACT_SCHEMA_VERSION,
            suite_id,
            suite: request.suite,
            baseline_editor: request.baseline_editor.clone(),
            candidate_editor: request.candidate_editor.clone(),
            samples_per_side: request.samples,
            machine: request.machine.clone(),
            counters: request.counters,
            started_unix_ms,
            total_elapsed_us: started.elapsed().as_micros(),
            previous_suite,
            scenarios: scenario_results,
            verdict,
        };
        let artifact_path = directory.join("suite.json");
        write_json_atomically(&artifact_path, &artifact)?;
        Ok(SuiteReport {
            artifact,
            artifact_path,
        })
    }
}

pub(crate) fn evaluate_suite(
    scenarios: &[SuiteScenarioResult],
    previous: Option<&SuiteArtifact>,
) -> SuiteVerdict {
    let rejected = scenarios
        .iter()
        .filter(|scenario| scenario.percent_change.is_none())
        .map(|scenario| scenario.scenario)
        .collect::<Vec<_>>();
    if !rejected.is_empty() {
        return SuiteVerdict::Rejected {
            scenarios: rejected,
        };
    }
    let regressions = scenarios
        .iter()
        .filter_map(|scenario| {
            let percent_change = scenario.percent_change?;
            (percent_change > scenario.maximum_regression_percent).then_some(SuiteRegression {
                scenario: scenario.scenario,
                maximum_regression_percent: scenario.maximum_regression_percent,
                actual_regression_percent: percent_change,
            })
        })
        .collect::<Vec<_>>();
    if !regressions.is_empty() {
        return SuiteVerdict::Regressed { regressions };
    }
    // Only now consider drift: a row already over its own budget is reported as
    // a regression rather than as history.
    let drifts = previous
        .map(|previous| {
            scenarios
                .iter()
                .filter_map(|scenario| {
                    let maximum_drift_percent = scenario.maximum_drift_percent?;
                    let current = scenario.percent_change?;
                    let before = previous
                        .scenarios
                        .iter()
                        .find(|earlier| earlier.scenario == scenario.scenario)?
                        .percent_change?;
                    let drift_percent = current - before;
                    (drift_percent > maximum_drift_percent).then_some(SuiteDrift {
                        scenario: scenario.scenario,
                        previous_percent_change: before,
                        current_percent_change: current,
                        drift_percent,
                        maximum_drift_percent,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if drifts.is_empty() {
        SuiteVerdict::Passed
    } else {
        SuiteVerdict::Drifted { drifts }
    }
}

#[cfg(test)]
pub(crate) fn read_history_link(path: &Path) -> Result<SuiteHistoryLink, PerfError> {
    read_history(path).map(|(link, _)| link)
}

fn read_history(path: &Path) -> Result<(SuiteHistoryLink, Vec<u8>), PerfError> {
    let bytes = fs::read(path).map_err(|error| PerfError::InvalidSuiteHistory {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let artifact = serde_json::from_slice::<SuiteArtifact>(&bytes).map_err(|error| {
        PerfError::InvalidSuiteHistory {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    if artifact.schema_version != SUITE_ARTIFACT_SCHEMA_VERSION {
        return Err(PerfError::InvalidSuiteHistory {
            path: path.to_path_buf(),
            message: format!(
                "unsupported suite artifact schema version {}; expected {}",
                artifact.schema_version, SUITE_ARTIFACT_SCHEMA_VERSION
            ),
        });
    }
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((
        SuiteHistoryLink {
            path: path.to_path_buf(),
            retained_path: PathBuf::from("previous-suite.json"),
            suite_id: artifact.suite_id,
            sha256,
        },
        bytes,
    ))
}
