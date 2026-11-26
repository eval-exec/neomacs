use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifact_store::{unix_time_ms, write_json_atomically};
use crate::host::{collect_host_provenance, validate_cohort_load_policy};
use crate::{
    BaselineManifest, ComparisonMetricSummary, ComparisonRequest, ComparisonSampleCount,
    ComparisonVerdict, CounterScope, MachinePolicy, MeasurementProvenance, MetricName, PerfError,
    PerfHarness, ResolvedGuardTools, ScenarioId, VerifiedBaselineManifest, resolve_guard_tools,
    scenario,
};

static SUITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) const SUITE_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteId {
    Standard,
    OrgEditingGuard,
    /// Instruction-gated regression guard for Issue #702's long-line truncated
    /// scrolling band (`scrolling-long-lines`, mean 179.3 chars, 55% > 200
    /// cols, 30-col right margin, `hl-line-mode`, 200x50 TUI).
    ScrollingLongLinesGuard,
}

impl fmt::Display for SuiteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Standard => "standard",
            Self::OrgEditingGuard => "org-editing-guard",
            Self::ScrollingLongLinesGuard => "scrolling-long-lines-guard",
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
            "org-editing-guard" => Ok(Self::OrgEditingGuard),
            "scrolling-long-lines-guard" => Ok(Self::ScrollingLongLinesGuard),
            value => Err(UnknownSuiteId(value.to_string())),
        }
    }
}

/// Why a suite row cannot be gated on instruction count alone.
///
/// Only rows whose contract is an actual elapsed-time property (input-to-present
/// latency, vsync cadence, or stall-avoidance deadline) remain time-gated; every
/// compute/editing/rendering-work row converts to `SuiteGateMetric::Instructions`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonConvertibleTimingReason {
    /// Per-keystroke command-dispatch-to-redisplay `p99` tail-latency
    /// distribution across 1,000 `self-insert-command` + `font-lock-ensure` +
    /// `(redisplay t)` steps under headless Weston (capturing GC, redisplay,
    /// and GPU-submission tail stalls that an aggregate instruction count
    /// cannot express).
    InputToRedisplayLatency,
}

impl fmt::Display for NonConvertibleTimingReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputToRedisplayLatency => f.write_str("input-to-redisplay-latency"),
        }
    }
}

/// Which metric (and gating mode) a suite row applies.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteGateMetric {
    /// Gate on the scenario's default primary metric (legacy wall/CPU mode).
    #[default]
    ScenarioPrimary,
    /// Gate on retired user-space instructions (`MetricName::Instructions`),
    /// requiring `--hardware-counters` and rejecting any run that omits it.
    /// Wall/CPU time and cycles remain recorded in `context_metrics` with full
    /// spread (median + MAD) for human interpretation, never as a pass gate.
    Instructions,
    /// Non-gating report row: executes the full scenario and still rejects the
    /// suite if any correctness or infrastructure invariant fails (state
    /// assertion), while reporting the primary timing metric (with median, MAD,
    /// and full sample span) and hardware-counter context without failing the
    /// suite on timing regressions or drift.
    NonGatingReport,
}

impl SuiteGateMetric {
    pub const fn requires_hardware_counters(self) -> bool {
        matches!(self, Self::Instructions)
    }

    pub const fn is_gating(self) -> bool {
        matches!(self, Self::ScenarioPrimary | Self::Instructions)
    }

    pub fn metric_for(self, scenario_id: ScenarioId) -> MetricName {
        match self {
            Self::ScenarioPrimary | Self::NonGatingReport => scenario(scenario_id).primary_metric,
            Self::Instructions => MetricName::Instructions,
        }
    }
}

/// Measured multi-run A/A calibration inputs for an instruction-gated row.
///
/// Budgets are computed directly in code from these recorded measurements rather
/// than hand-typed as unverified literals:
/// - `budget = ceil_to_0.5%(max(tier_floor, 10 * max_rel_mad, 2 * max_span))`
/// - `drift  = ceil_to_0.5%(max(0.5, 2 * max_aa_drift, max_span))` when strictly
///   less than `budget`, else `None` (a drift ceiling >= per-run budget is
///   redundant).
///
/// The `tier_floor_percent` (`1.0%` for deterministic rows) is an explicit
/// noise-independent engineering policy tolerance for routine compiler/codegen
/// churn and instruction-for-cycle trade-offs, shared identically between
/// `SuiteId::Standard` and `SuiteId::OrgEditingGuard`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstructionCalibration {
    /// Independent clean A/A runs backing these maxima (must be >= 2).
    pub runs: u32,
    /// Maximum samples per side across the calibration runs (must be >= 5).
    pub samples_per_side: u32,
    /// Maximum relative MAD (% of median) across baseline/candidate arms and runs.
    pub max_rel_mad_percent: f64,
    /// Maximum within-run sample span `(max - min) / min * 100` across runs.
    pub max_span_percent: f64,
    /// Maximum `|A/A percent_change|` across runs.
    pub max_aa_delta_percent: f64,
    /// Maximum `|drift|` between clean A/A runs.
    pub max_aa_drift_percent: f64,
    /// Noise-independent engineering floor (`%`).
    pub tier_floor_percent: f64,
}

const fn max_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a >= b {
        a
    } else {
        b
    }
}

const fn ceil_to_half_percent(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return f64::NAN;
    }
    let doubled = value * 2.0;
    let truncated = doubled as u64;
    let ceiled = if doubled - (truncated as f64) > 1e-12 {
        truncated + 1
    } else {
        truncated
    };
    (ceiled as f64) / 2.0
}

impl InstructionCalibration {
    pub const fn budget_percent(&self) -> f64 {
        let raw = max_f64(
            self.tier_floor_percent,
            max_f64(10.0 * self.max_rel_mad_percent, 2.0 * self.max_span_percent),
        );
        ceil_to_half_percent(raw)
    }

    pub const fn drift_budget_percent(&self) -> Option<f64> {
        let budget = self.budget_percent();
        let raw = max_f64(
            0.5,
            max_f64(2.0 * self.max_aa_drift_percent, self.max_span_percent),
        );
        let drift = ceil_to_half_percent(raw);
        if drift.is_finite() && budget.is_finite() && drift < budget {
            Some(drift)
        } else {
            None
        }
    }

    /// Deny-by-default validation: every calibrated instruction row must come
    /// from >= `min_runs` independent runs with >= 5 samples per side, finite
    /// non-negative measurements, a positive floor, and a computed budget
    /// strictly wider than its observed A/A span.
    pub fn validate(&self, min_runs: u32) -> Result<(), String> {
        if self.runs < min_runs {
            return Err(format!(
                "calibration requires at least {min_runs} independent clean A/A runs, got {}",
                self.runs
            ));
        }
        if self.samples_per_side < 5 {
            return Err(format!(
                "calibration requires at least 5 samples per side, got {}",
                self.samples_per_side
            ));
        }
        for (name, value) in [
            ("max_rel_mad_percent", self.max_rel_mad_percent),
            ("max_span_percent", self.max_span_percent),
            ("max_aa_delta_percent", self.max_aa_delta_percent),
            ("max_aa_drift_percent", self.max_aa_drift_percent),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "{name} must be finite and non-negative, got {value}"
                ));
            }
        }
        if !self.tier_floor_percent.is_finite() || self.tier_floor_percent <= 0.0 {
            return Err(format!(
                "tier_floor_percent must be finite and positive, got {}",
                self.tier_floor_percent
            ));
        }
        let budget = self.budget_percent();
        if !budget.is_finite() || budget <= 0.0 {
            return Err(format!(
                "computed budget_percent must be finite and positive, got {budget}"
            ));
        }
        if budget <= self.max_span_percent {
            return Err(format!(
                "computed budget ({budget:.2}%) must strictly exceed observed max_span_percent ({:.3}%)",
                self.max_span_percent
            ));
        }
        if let Some(drift) = self.drift_budget_percent() {
            if !drift.is_finite() || drift <= 0.0 || drift <= self.max_aa_drift_percent {
                return Err(format!(
                    "computed drift_budget_percent ({drift:.2}%) must exceed observed max_aa_drift_percent ({:.2}%)",
                    self.max_aa_drift_percent
                ));
            }
        }
        Ok(())
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
    /// run, so a row that regresses a little every time never trips it. This is
    /// the ratchet that catches the accumulation, sized from multi-run clean
    /// A/A drift + span so clean A/A runs never false-trip it.
    pub maximum_drift_percent: Option<f64>,
    pub gate_metric: SuiteGateMetric,
    /// Per-row counter scope override when `EditLoop` does not cover the
    /// workload's measured phase (for example, `Startup` must count
    /// `WholeProcess` instructions because `editor-workloads.el` only runs a
    /// single `(redisplay t)` inside the edit-loop gate after startup ends).
    pub counter_scope_override: Option<CounterScope>,
    /// Required explanation whenever `gate_metric` is not `Instructions`.
    pub non_convertible_reason: Option<NonConvertibleTimingReason>,
    /// Required multi-run calibration descriptor for every `Instructions` row.
    pub calibration: Option<InstructionCalibration>,
}

impl SuiteScenario {
    /// Resolve the hardware counter scope for this row, preserving main-thread
    /// selection when a whole-process override applies.
    pub const fn effective_counter_scope(
        &self,
        requested: Option<CounterScope>,
    ) -> Option<CounterScope> {
        match (requested, self.counter_scope_override) {
            (Some(req), Some(CounterScope::WholeProcess)) if req.main_thread_only() => {
                Some(CounterScope::WholeProcessMainThread)
            }
            (Some(_), Some(override_scope)) => Some(override_scope),
            (counters, _) => counters,
        }
    }
}

const ORG_EDITING_CALIBRATION: InstructionCalibration = InstructionCalibration {
    runs: 2,
    samples_per_side: 5,
    max_rel_mad_percent: 0.0073,
    max_span_percent: 0.068,
    max_aa_delta_percent: 0.00,
    max_aa_drift_percent: 0.00,
    tier_floor_percent: 1.0,
};

const ORG_EDITING_HEAVY_CALIBRATION: InstructionCalibration = InstructionCalibration {
    runs: 2,
    samples_per_side: 5,
    max_rel_mad_percent: 0.0259,
    max_span_percent: 0.075,
    max_aa_delta_percent: 0.01,
    max_aa_drift_percent: 0.00,
    tier_floor_percent: 1.0,
};

// All 22 instruction-gated rows compute their budgets directly in Rust via
// `InstructionCalibration::budget_percent()` and `drift_budget_percent()` from
// multi-run clean A/A maxima on Matrix (Intel i9-14900K, Linux 6.18.64, CPU 0,
// governor=performance):
// - 2 independent full-suite n=5 clean A/A runs (loadavg 53.9->25.7 and
//   27.6->9.4), plus
// - an additional n=10 clean A/A run for all 5 bimodal/higher-spread rows
//   (`startup`, `mx-tab-completion`, `magit-status-heavy`,
//   `rust-lsp-typing-heavy`, `sustained-editing`).
//
// `GuiInputLatency` stays in the suite as `SuiteGateMetric::NonGatingReport`:
// it still executes all 1,000 edits + redisplays under Weston (failing the
// suite on any correctness/infra rejection) while reporting its `p99` latency
// and full span non-gating because clean A/A spanned 73.0%-411.6%
// (`(max - min) / min * 100`).
const STANDARD_SCENARIOS: &[SuiteScenario] = &[
    instruction_calibrated(
        ScenarioId::RustLspTyping,
        InstructionCalibration {
            runs: 4,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0567,
            max_span_percent: 0.620,
            max_aa_delta_percent: 0.07,
            max_aa_drift_percent: 0.07,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::RustLspTypingHeavy,
        InstructionCalibration {
            runs: 3,
            samples_per_side: 10,
            max_rel_mad_percent: 0.3960,
            max_span_percent: 1.710,
            max_aa_delta_percent: 0.06,
            max_aa_drift_percent: 0.04,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::MxTabCompletion,
        InstructionCalibration {
            runs: 3,
            samples_per_side: 10,
            max_rel_mad_percent: 1.2655,
            max_span_percent: 4.865,
            max_aa_delta_percent: 1.78,
            max_aa_drift_percent: 3.45,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::MxTabCompletionSteady,
        InstructionCalibration {
            runs: 4,
            samples_per_side: 5,
            max_rel_mad_percent: 0.2065,
            max_span_percent: 0.470,
            max_aa_delta_percent: 0.21,
            max_aa_drift_percent: 0.27,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::Scrolling,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.1706,
            max_span_percent: 0.649,
            max_aa_delta_percent: 0.17,
            max_aa_drift_percent: 0.22,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::BytecodeCallLoop,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0001,
            max_span_percent: 0.000,
            max_aa_delta_percent: 0.00,
            max_aa_drift_percent: 0.00,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated(
        ScenarioId::EditingSimulation,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0059,
            max_span_percent: 0.036,
            max_aa_delta_percent: 0.00,
            max_aa_drift_percent: 0.00,
            tier_floor_percent: 1.0,
        },
    ),
    // Startup work happens before the edit-loop gate opens; counting
    // WholeProcess instructions gates real startup instead of an empty redisplay.
    instruction_calibrated_with_scope(
        ScenarioId::Startup,
        InstructionCalibration {
            runs: 5,
            samples_per_side: 10,
            max_rel_mad_percent: 2.9247,
            max_span_percent: 16.134,
            max_aa_delta_percent: 13.68,
            max_aa_drift_percent: 16.69,
            tier_floor_percent: 1.0,
        },
        CounterScope::WholeProcess,
    ),
    instruction_calibrated(
        ScenarioId::SustainedEditing,
        InstructionCalibration {
            runs: 5,
            samples_per_side: 10,
            max_rel_mad_percent: 0.1554,
            max_span_percent: 5.285,
            max_aa_delta_percent: 0.07,
            max_aa_drift_percent: 0.12,
            tier_floor_percent: 1.0,
        },
    ),
    // Non-convertible & non-gating report: p99 command-dispatch-to-redisplay
    // tail latency under Weston spanned 73.0%-411.6% (`(max - min) / min * 100`)
    // on clean A/A. Keep the row in the suite as a correctness/state assertion
    // (rejecting failed redisplays) while reporting p99 latency + span non-gating.
    non_gating_report(
        ScenarioId::GuiInputLatency,
        NonConvertibleTimingReason::InputToRedisplayLatency,
    ),
    instruction_calibrated_ratcheted(ScenarioId::OrgEditing, ORG_EDITING_CALIBRATION),
    instruction_calibrated_ratcheted(ScenarioId::OrgEditingHeavy, ORG_EDITING_HEAVY_CALIBRATION),
    instruction_calibrated_ratcheted(
        ScenarioId::MagitStatus,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0640,
            max_span_percent: 0.191,
            max_aa_delta_percent: 0.04,
            max_aa_drift_percent: 0.02,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::OrgJournalOpen,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0101,
            max_span_percent: 0.116,
            max_aa_delta_percent: 0.02,
            max_aa_drift_percent: 0.02,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::LargeFileEditing,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0085,
            max_span_percent: 0.034,
            max_aa_delta_percent: 0.01,
            max_aa_drift_percent: 0.01,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::Indentation,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0278,
            max_span_percent: 0.074,
            max_aa_delta_percent: 0.01,
            max_aa_drift_percent: 0.01,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::RegexSearch,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0443,
            max_span_percent: 0.188,
            max_aa_delta_percent: 0.00,
            max_aa_drift_percent: 0.00,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::MagitStatusCompiled,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.1442,
            max_span_percent: 0.335,
            max_aa_delta_percent: 0.07,
            max_aa_drift_percent: 0.08,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::MagitStatusHeavy,
        InstructionCalibration {
            runs: 3,
            samples_per_side: 10,
            max_rel_mad_percent: 0.5535,
            max_span_percent: 2.427,
            max_aa_delta_percent: 1.67,
            max_aa_drift_percent: 1.86,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::FileOpen,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0145,
            max_span_percent: 0.037,
            max_aa_delta_percent: 0.00,
            max_aa_drift_percent: 0.00,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::ProcessOutput,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0498,
            max_span_percent: 0.148,
            max_aa_delta_percent: 0.02,
            max_aa_drift_percent: 0.01,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::OrgJournalOpenCompiled,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.0514,
            max_span_percent: 0.157,
            max_aa_delta_percent: 0.03,
            max_aa_drift_percent: 0.05,
            tier_floor_percent: 1.0,
        },
    ),
    instruction_calibrated_ratcheted(
        ScenarioId::LspJsonRpc,
        InstructionCalibration {
            runs: 2,
            samples_per_side: 5,
            max_rel_mad_percent: 0.1364,
            max_span_percent: 0.375,
            max_aa_delta_percent: 0.15,
            max_aa_drift_percent: 0.14,
            tier_floor_percent: 1.0,
        },
    ),
];

const ORG_EDITING_GUARD_SCENARIOS: &[SuiteScenario] = &[
    instruction_calibrated(ScenarioId::OrgEditing, ORG_EDITING_CALIBRATION),
    instruction_calibrated(ScenarioId::OrgEditingHeavy, ORG_EDITING_HEAVY_CALIBRATION),
];

/// Issue #702 long-line truncated scroll regression guard.
///
/// Gates strictly on retired user-space instructions (`MetricName::Instructions`,
/// `CounterScope::EditLoop`) with a +2.0% regression budget: retired
/// instructions on this deterministic loop are load-insensitive (< 0.1%
/// run-to-run variance) unlike TUI wall time (5-17% under ambient host load),
/// so +2.0% catches real layout/horizon/right-edge/overlay regressions without
/// flaking on allocator/GC boundary drift.
///
/// Scope boundary: `scrolling-long-lines` runs in `fundamental-mode` with
/// pre-propertized lines, so this guard covers the layout/truncation/horizon/
/// right-edge-marker/overlay-route path (84.6% of instructions / 52.1% of
/// cycles in Issue #702). It does NOT exercise `jit-lock` lazy fontification
/// or guard `RowsFontificationDemand::ensure` CoW/realloc (`58.99%` of cache
/// misses on 71k-line Magit logs).
const SCROLLING_LONG_LINES_GUARD_SCENARIOS: &[SuiteScenario] =
    &[instruction_gated(ScenarioId::ScrollingLongLines, 2.0)];

/// A standalone guard row gated directly on retired user-space instructions
/// with an explicit fixed regression budget.
const fn instruction_gated(scenario: ScenarioId, maximum_regression_percent: f64) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent,
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::Instructions,
        counter_scope_override: None,
        non_convertible_reason: None,
        calibration: None,
    }
}

/// A row whose contract is a genuine elapsed-time / tail-latency property that
/// instruction counts cannot express and whose relative A/A noise cannot gate
/// CI reliably: kept in the suite as a correctness/state assertion with
/// non-gating latency + span reporting.
const fn non_gating_report(
    scenario: ScenarioId,
    reason: NonConvertibleTimingReason,
) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent: 0.0,
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::NonGatingReport,
        counter_scope_override: None,
        non_convertible_reason: Some(reason),
        calibration: None,
    }
}

/// A deterministic row gated on retired user-space instructions with both
/// per-run budget and drift ratchet derived in code from multi-run calibration.
const fn instruction_calibrated_ratcheted(
    scenario: ScenarioId,
    calibration: InstructionCalibration,
) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent: calibration.budget_percent(),
        maximum_drift_percent: calibration.drift_budget_percent(),
        gate_metric: SuiteGateMetric::Instructions,
        counter_scope_override: None,
        non_convertible_reason: None,
        calibration: Some(calibration),
    }
}

/// A deterministic row gated directly on retired user-space instructions using
/// a budget derived in code from multi-run calibration.
const fn instruction_calibrated(
    scenario: ScenarioId,
    calibration: InstructionCalibration,
) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent: calibration.budget_percent(),
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::Instructions,
        counter_scope_override: None,
        non_convertible_reason: None,
        calibration: Some(calibration),
    }
}

/// An instruction-gated row that requires a specific hardware counter scope.
const fn instruction_calibrated_with_scope(
    scenario: ScenarioId,
    calibration: InstructionCalibration,
    counter_scope: CounterScope,
) -> SuiteScenario {
    SuiteScenario {
        scenario,
        maximum_regression_percent: calibration.budget_percent(),
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::Instructions,
        counter_scope_override: Some(counter_scope),
        non_convertible_reason: None,
        calibration: Some(calibration),
    }
}

impl SuiteId {
    pub const fn scenarios(self) -> &'static [SuiteScenario] {
        match self {
            Self::Standard => STANDARD_SCENARIOS,
            Self::OrgEditingGuard => ORG_EDITING_GUARD_SCENARIOS,
            Self::ScrollingLongLinesGuard => SCROLLING_LONG_LINES_GUARD_SCENARIOS,
        }
    }

    pub const fn requires_baseline_manifest(self) -> bool {
        match self {
            Self::Standard | Self::ScrollingLongLinesGuard => false,
            Self::OrgEditingGuard => true,
        }
    }

    /// Whether this suite enables guard provenance hardening: canonical
    /// candidate executable resolution, pinned absolute `perf`/`taskset`
    /// resolution with `PATH` shadowing rejection, and the closed empty guard
    /// environment.
    pub const fn uses_guard_hardening(self) -> bool {
        match self {
            Self::Standard => false,
            Self::OrgEditingGuard | Self::ScrollingLongLinesGuard => true,
        }
    }

    pub const fn required_counter_scope(self) -> Option<CounterScope> {
        match self {
            Self::Standard | Self::OrgEditingGuard | Self::ScrollingLongLinesGuard => {
                Some(CounterScope::EditLoop)
            }
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
    pub(crate) baseline_manifest: Option<PathBuf>,
    #[cfg(test)]
    pub(crate) guard_path_override: Option<std::ffi::OsString>,
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
            baseline_manifest: None,
            #[cfg(test)]
            guard_path_override: None,
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

    pub fn with_baseline_manifest(mut self, baseline_manifest: Option<PathBuf>) -> Self {
        self.baseline_manifest = baseline_manifest;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_guard_path_override(mut self, path: Option<std::ffi::OsString>) -> Self {
        self.guard_path_override = path;
        self
    }

    fn guard_path_env(&self) -> Option<std::ffi::OsString> {
        #[cfg(test)]
        if let Some(override_path) = &self.guard_path_override {
            return Some(override_path.clone());
        }
        std::env::var_os("PATH")
    }

    /// Reject invalid suite configurations before any artifact directory or
    /// editor process is created.
    pub fn validate(&self) -> Result<(), PerfError> {
        let instruction_scenarios: Vec<ScenarioId> = self
            .suite
            .scenarios()
            .iter()
            .filter(|row| row.gate_metric.requires_hardware_counters())
            .map(|row| row.scenario)
            .collect();
        if !instruction_scenarios.is_empty() && self.counters.is_none() {
            return Err(PerfError::HardwareCountersRequired {
                suite: self.suite,
                scenarios: instruction_scenarios,
            });
        }
        if let (Some(expected), Some(actual)) = (self.suite.required_counter_scope(), self.counters)
        {
            let allowed = match self.suite {
                SuiteId::Standard => {
                    matches!(
                        actual,
                        CounterScope::EditLoop | CounterScope::EditLoopMainThread
                    )
                }
                SuiteId::OrgEditingGuard | SuiteId::ScrollingLongLinesGuard => actual == expected,
            };
            if !allowed {
                return Err(PerfError::CounterScopeMismatch {
                    suite: self.suite,
                    expected,
                    actual,
                });
            }
        }
        for row in self.suite.scenarios() {
            if row.gate_metric == SuiteGateMetric::Instructions {
                match row.calibration {
                    Some(calibration) => {
                        if let Err(reason) = calibration.validate(2) {
                            return Err(PerfError::SuiteCalibrationInvalid {
                                suite: self.suite,
                                scenario: row.scenario,
                                reason,
                            });
                        }
                    }
                    None if matches!(self.suite, SuiteId::Standard | SuiteId::OrgEditingGuard) => {
                        return Err(PerfError::SuiteCalibrationInvalid {
                            suite: self.suite,
                            scenario: row.scenario,
                            reason: "instruction-gated row is missing InstructionCalibration"
                                .to_string(),
                        });
                    }
                    None => {}
                }
                if !row.maximum_regression_percent.is_finite()
                    || row.maximum_regression_percent <= 0.0
                {
                    return Err(PerfError::SuiteCalibrationInvalid {
                        suite: self.suite,
                        scenario: row.scenario,
                        reason: format!(
                            "maximum_regression_percent must be finite and positive, got {}",
                            row.maximum_regression_percent
                        ),
                    });
                }
            }
        }
        if self.suite.requires_baseline_manifest() && self.baseline_manifest.is_none() {
            return Err(PerfError::BaselineManifestRequired { suite: self.suite });
        }
        Ok(())
    }

    pub(crate) fn comparison_request_for(
        &self,
        suite_scenario: &SuiteScenario,
        expected_baseline: Option<VerifiedBaselineManifest>,
        guard_tools: Option<ResolvedGuardTools>,
        canonical_candidate_editor: Option<&Path>,
    ) -> ComparisonRequest {
        let spec = scenario(suite_scenario.scenario);
        // Thread canonical absolute editor paths for manifest-pinned runs into
        // every downstream provenance check and execution command so verify and
        // exec cannot resolve relative paths against different directories
        // (process CWD vs workspace_root) on either arm.
        let baseline_editor = expected_baseline
            .as_ref()
            .map_or(self.baseline_editor.as_path(), |verified| {
                verified.binary_path.as_path()
            });
        let candidate_editor =
            canonical_candidate_editor.unwrap_or(self.candidate_editor.as_path());
        ComparisonRequest::new(
            suite_scenario.scenario,
            baseline_editor,
            candidate_editor,
            self.samples,
            spec.default_iterations,
        )
        .with_frontend(spec.default_frontend)
        .with_timeout(self.timeout)
        .with_machine_policy(self.machine.clone())
        .with_counters(suite_scenario.effective_counter_scope(self.counters))
        .with_primary_metric(
            suite_scenario
                .gate_metric
                .metric_for(suite_scenario.scenario),
        )
        .with_expected_baseline(expected_baseline)
        .with_guard_tools(guard_tools)
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
    #[serde(default)]
    pub gate_metric: SuiteGateMetric,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_convertible_reason: Option<NonConvertibleTimingReason>,
    /// Iteration count executed per run so readers can compute per-operation /
    /// per-key instruction rates directly from `gate_summary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u32>,
    pub percent_change: Option<f64>,
    /// Full median + MAD spread summary for the gating metric.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<ComparisonMetricSummary>,
    /// Non-gating context summaries (wall/CPU time and cycles with full median
    /// + MAD spread). Reported for human inspection; never evaluated by
    /// `evaluate_suite`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_metrics: Vec<ComparisonMetricSummary>,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteDriftSkipReason {
    SuiteChanged,
    BaselineEditorChanged,
    CounterScopeChanged,
    PreviousRowMissing,
    GateMetricChanged,
    IterationsChanged,
    PreviousValueInvalid,
}

impl fmt::Display for SuiteDriftSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::SuiteChanged => "previous suite id differs from current suite",
            Self::BaselineEditorChanged => "previous baseline editor differs from current baseline",
            Self::CounterScopeChanged => "previous hardware counter scope differs from current",
            Self::PreviousRowMissing => "scenario missing from previous suite artifact",
            Self::GateMetricChanged => "gate_metric changed between previous and current suite",
            Self::IterationsChanged => "scenario iteration count changed since previous suite",
            Self::PreviousValueInvalid => "previous scenario measurement was missing or invalid",
        };
        f.write_str(text)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteDriftSkip {
    pub scenario: ScenarioId,
    pub reason: SuiteDriftSkipReason,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_loadavg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<MeasurementProvenance>,
    pub started_unix_ms: u128,
    pub total_elapsed_us: u128,
    pub previous_suite: Option<SuiteHistoryLink>,
    #[serde(default)]
    pub baseline_manifest: Option<VerifiedBaselineManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard_tools: Option<ResolvedGuardTools>,
    pub scenarios: Vec<SuiteScenarioResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_drifts: Vec<SuiteDriftSkip>,
    pub verdict: SuiteVerdict,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuiteReport {
    pub artifact: SuiteArtifact,
    pub artifact_path: PathBuf,
}

impl PerfHarness {
    pub fn suite(&self, request: &SuiteRequest) -> Result<SuiteReport, PerfError> {
        request.validate()?;
        let preflight_host = collect_host_provenance(&request.machine);
        for suite_scenario in request.suite.scenarios() {
            validate_cohort_load_policy(
                suite_scenario.scenario,
                &request.machine,
                &preflight_host,
            )?;
        }
        let baseline_manifest = match request.baseline_manifest.as_deref() {
            Some(path) => {
                let (manifest, manifest_sha256) = BaselineManifest::load(path)?;
                Some(manifest.verify_editor(path, manifest_sha256, &request.baseline_editor)?)
            }
            None => None,
        };
        let use_guard_hardening =
            baseline_manifest.is_some() || request.suite.uses_guard_hardening();
        let canonical_candidate = if use_guard_hardening {
            Some(resolve_canonical_candidate_editor(
                &request.candidate_editor,
            )?)
        } else {
            None
        };
        let guard_tools = if use_guard_hardening {
            Some(resolve_guard_tools(
                request.guard_path_env().as_deref(),
                request.machine.cpu.is_some(),
            )?)
        } else {
            None
        };
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
            let comparison = request.comparison_request_for(
                suite_scenario,
                baseline_manifest.clone(),
                guard_tools.clone(),
                canonical_candidate.as_deref(),
            );
            let report = self.compare(&comparison)?;
            let (percent_change, gate_summary, context_metrics) = match &report.artifact.verdict {
                ComparisonVerdict::Valid {
                    summary,
                    context_metrics,
                } => (
                    Some(summary.percent_change),
                    Some(summary.clone()),
                    context_metrics.clone(),
                ),
                ComparisonVerdict::Rejected { .. } => (None, None, Vec::new()),
            };
            scenario_results.push(SuiteScenarioResult {
                scenario: suite_scenario.scenario,
                maximum_regression_percent: suite_scenario.maximum_regression_percent,
                maximum_drift_percent: suite_scenario.maximum_drift_percent,
                gate_metric: suite_scenario.gate_metric,
                non_convertible_reason: suite_scenario.non_convertible_reason,
                iterations: Some(report.artifact.input.iterations.get()),
                percent_change,
                gate_summary,
                context_metrics,
                comparison_artifact: report.artifact_path,
            });
        }
        let baseline_editor = baseline_manifest.as_ref().map_or_else(
            || request.baseline_editor.clone(),
            |v| v.binary_path.clone(),
        );
        let (verdict, skipped_drifts) = evaluate_suite_with_context(
            &scenario_results,
            previous_artifact.as_ref(),
            Some(SuiteEvaluationContext {
                suite: request.suite,
                baseline_editor: &baseline_editor,
                counters: request.counters,
            }),
        );
        let artifact = SuiteArtifact {
            schema_version: SUITE_ARTIFACT_SCHEMA_VERSION,
            suite_id,
            suite: request.suite,
            baseline_editor,
            candidate_editor: canonical_candidate
                .unwrap_or_else(|| request.candidate_editor.clone()),
            samples_per_side: request.samples,
            machine: request.machine.clone(),
            counters: request.counters,
            host_loadavg: preflight_host.loadavg,
            provenance: preflight_host.provenance,
            started_unix_ms,
            total_elapsed_us: started.elapsed().as_micros(),
            previous_suite,
            baseline_manifest,
            guard_tools,
            scenarios: scenario_results,
            skipped_drifts,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct SuiteEvaluationContext<'a> {
    pub(crate) suite: SuiteId,
    pub(crate) baseline_editor: &'a Path,
    pub(crate) counters: Option<CounterScope>,
}

impl SuiteScenarioResult {
    /// Percentage change evaluated by the suite gate: prefers `gate_summary`
    /// when present (verifying its metric matches `gate_metric.metric_for`),
    /// falling back to `percent_change` for legacy or minimal fixtures.
    /// Rejects non-finite values so `NaN` / `Infinity` never silently passes.
    pub(crate) fn effective_percent_change(&self) -> Option<f64> {
        let value = match &self.gate_summary {
            Some(summary) => {
                if summary.metric != self.gate_metric.metric_for(self.scenario) {
                    return None;
                }
                summary.percent_change
            }
            None => self.percent_change?,
        };
        value.is_finite().then_some(value)
    }

    fn has_valid_gate_budgets(&self) -> bool {
        if !self.gate_metric.is_gating() {
            return true;
        }
        if !self.maximum_regression_percent.is_finite() || self.maximum_regression_percent <= 0.0 {
            return false;
        }
        if let Some(drift) = self.maximum_drift_percent
            && (!drift.is_finite() || drift <= 0.0)
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
pub(crate) fn evaluate_suite(
    scenarios: &[SuiteScenarioResult],
    previous: Option<&SuiteArtifact>,
) -> SuiteVerdict {
    evaluate_suite_with_context(scenarios, previous, None).0
}

pub(crate) fn evaluate_suite_with_context(
    scenarios: &[SuiteScenarioResult],
    previous: Option<&SuiteArtifact>,
    context: Option<SuiteEvaluationContext<'_>>,
) -> (SuiteVerdict, Vec<SuiteDriftSkip>) {
    // Fail closed on any rejected/missing/non-finite observation OR any
    // non-finite/non-positive gating budget (including NaN/Infinity).
    // NonGatingReport rows still fail here on correctness/infra rejection.
    let rejected = scenarios
        .iter()
        .filter(|scenario| {
            scenario.effective_percent_change().is_none() || !scenario.has_valid_gate_budgets()
        })
        .map(|scenario| scenario.scenario)
        .collect::<Vec<_>>();
    if !rejected.is_empty() {
        return (
            SuiteVerdict::Rejected {
                scenarios: rejected,
            },
            Vec::new(),
        );
    }
    let regressions = scenarios
        .iter()
        .filter(|scenario| scenario.gate_metric.is_gating())
        .filter_map(|scenario| {
            let percent_change = scenario.effective_percent_change()?;
            (percent_change > scenario.maximum_regression_percent).then_some(SuiteRegression {
                scenario: scenario.scenario,
                maximum_regression_percent: scenario.maximum_regression_percent,
                actual_regression_percent: percent_change,
            })
        })
        .collect::<Vec<_>>();
    if !regressions.is_empty() {
        return (SuiteVerdict::Regressed { regressions }, Vec::new());
    }
    // Only now consider drift: a row already over its own budget is reported as
    // a regression rather than as history. Verify full lineage compatibility
    // (suite, baseline_editor, counters, gate_metric, iterations, valid value)
    // and record every skipped drift check explicitly so stale/mismatched
    // history can never silently disarm the ratchet.
    let mut drifts = Vec::new();
    let mut skipped = Vec::new();
    if let Some(previous) = previous {
        for scenario in scenarios {
            if !scenario.gate_metric.is_gating() {
                continue;
            }
            let Some(maximum_drift_percent) = scenario.maximum_drift_percent else {
                continue;
            };
            let Some(current) = scenario.effective_percent_change() else {
                continue;
            };
            if let Some(ctx) = context {
                if previous.suite != ctx.suite {
                    skipped.push(SuiteDriftSkip {
                        scenario: scenario.scenario,
                        reason: SuiteDriftSkipReason::SuiteChanged,
                    });
                    continue;
                }
                if previous.baseline_editor != ctx.baseline_editor {
                    skipped.push(SuiteDriftSkip {
                        scenario: scenario.scenario,
                        reason: SuiteDriftSkipReason::BaselineEditorChanged,
                    });
                    continue;
                }
                if previous.counters != ctx.counters {
                    skipped.push(SuiteDriftSkip {
                        scenario: scenario.scenario,
                        reason: SuiteDriftSkipReason::CounterScopeChanged,
                    });
                    continue;
                }
            }
            let Some(earlier) = previous
                .scenarios
                .iter()
                .find(|earlier| earlier.scenario == scenario.scenario)
            else {
                skipped.push(SuiteDriftSkip {
                    scenario: scenario.scenario,
                    reason: SuiteDriftSkipReason::PreviousRowMissing,
                });
                continue;
            };
            if earlier.gate_metric != scenario.gate_metric {
                skipped.push(SuiteDriftSkip {
                    scenario: scenario.scenario,
                    reason: SuiteDriftSkipReason::GateMetricChanged,
                });
                continue;
            }
            if earlier.iterations != scenario.iterations {
                skipped.push(SuiteDriftSkip {
                    scenario: scenario.scenario,
                    reason: SuiteDriftSkipReason::IterationsChanged,
                });
                continue;
            }
            let Some(before) = earlier.effective_percent_change() else {
                skipped.push(SuiteDriftSkip {
                    scenario: scenario.scenario,
                    reason: SuiteDriftSkipReason::PreviousValueInvalid,
                });
                continue;
            };
            let drift_percent = current - before;
            if drift_percent > maximum_drift_percent {
                drifts.push(SuiteDrift {
                    scenario: scenario.scenario,
                    previous_percent_change: before,
                    current_percent_change: current,
                    drift_percent,
                    maximum_drift_percent,
                });
            }
        }
    }
    let verdict = if drifts.is_empty() {
        SuiteVerdict::Passed
    } else {
        SuiteVerdict::Drifted { drifts }
    };
    (verdict, skipped)
}

fn resolve_canonical_candidate_editor(path: &Path) -> Result<PathBuf, PerfError> {
    let canonical = fs::canonicalize(path).map_err(|error| PerfError::CandidateEditorInvalid {
        path: path.to_path_buf(),
        reason: format!("cannot resolve candidate binary: {error}"),
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| PerfError::CandidateEditorInvalid {
        path: canonical.clone(),
        reason: format!("cannot stat candidate binary: {error}"),
    })?;
    if !metadata.is_file() {
        return Err(PerfError::CandidateEditorInvalid {
            path: canonical,
            reason: "candidate editor path is not a regular file".to_owned(),
        });
    }
    Ok(canonical)
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

#[cfg(test)]
#[path = "suite/tests/suite_test.rs"]
mod tests;
