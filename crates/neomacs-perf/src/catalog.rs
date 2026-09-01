use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::MetricName;

/// A semantic count that must agree exactly between editors before their
/// timings may be compared.
///
/// This is deliberately a closed enum rather than an arbitrary `MetricName`:
/// duration metrics cannot accidentally be declared correctness invariants.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CrossEditorParityMetric {
    CompletionCandidateCount,
}

impl CrossEditorParityMetric {
    pub const fn metric_name(self) -> MetricName {
        match self {
            Self::CompletionCandidateCount => MetricName::CompletionCandidateCount,
        }
    }
}

/// Stable identity of a committed performance workload.
///
/// A closed enum prevents a typo from selecting a different fixture or
/// silently creating a new time series.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioId {
    RustLspTyping,
    MxTabCompletion,
    BytecodeCallLoop,
}

impl ScenarioId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RustLspTyping => "rust-lsp-typing",
            Self::MxTabCompletion => "mx-tab-completion",
            Self::BytecodeCallLoop => "bytecode-call-loop",
        }
    }
}

impl fmt::Display for ScenarioId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownScenarioId(String);

impl fmt::Display for UnknownScenarioId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown performance scenario `{}`", self.0)
    }
}

impl std::error::Error for UnknownScenarioId {}

impl FromStr for ScenarioId {
    type Err = UnknownScenarioId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rust-lsp-typing" => Ok(Self::RustLspTyping),
            "mx-tab-completion" => Ok(Self::MxTabCompletion),
            "bytecode-call-loop" => Ok(Self::BytecodeCallLoop),
            unknown => Err(UnknownScenarioId(unknown.to_string())),
        }
    }
}

/// Display adapter selected for a workload run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frontend {
    Batch,
    Tui { rows: u16, columns: u16 },
    Gui { width: u32, height: u32 },
}

/// Immutable definition of one committed performance workload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioSpec {
    pub id: ScenarioId,
    pub description: &'static str,
    pub default_frontend: Frontend,
    pub default_iterations: NonZeroU32,
    pub primary_metric: MetricName,
    pub cross_editor_parity_metrics: &'static [CrossEditorParityMetric],
}

const SCENARIOS: &[ScenarioSpec] = &[
    ScenarioSpec {
        id: ScenarioId::RustLspTyping,
        description: "Rust Tree-sitter typing with revision-pinned LSP Mode and deterministic diagnostic replay",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerEditCpuTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::MxTabCompletion,
        description: "Empty M-x TAB command completion through a real minibuffer and completion window",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(5).expect("non-zero scenario default"),
        primary_metric: MetricName::PerCompletionCpuTime,
        cross_editor_parity_metrics: &[CrossEditorParityMetric::CompletionCandidateCount],
    },
    ScenarioSpec {
        id: ScenarioId::BytecodeCallLoop,
        description: "Tier-0 bytecode-to-bytecode call and return loop with the Neomacs JIT disabled",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20_000_000).expect("non-zero scenario default"),
        primary_metric: MetricName::PerBytecodeCallCpuTime,
        cross_editor_parity_metrics: &[],
    },
];

pub fn scenarios() -> &'static [ScenarioSpec] {
    SCENARIOS
}

/// Return the definition for a typed scenario identity.
///
/// `ScenarioId` is closed, so absence is not a representable state. Keeping
/// this match exhaustive makes adding an enum variant fail to compile until
/// its workload definition is also registered.
pub const fn scenario(id: ScenarioId) -> &'static ScenarioSpec {
    match id {
        ScenarioId::RustLspTyping => &SCENARIOS[0],
        ScenarioId::MxTabCompletion => &SCENARIOS[1],
        ScenarioId::BytecodeCallLoop => &SCENARIOS[2],
    }
}
