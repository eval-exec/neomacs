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
    EditingSimulation,
    Startup,
    SustainedEditing,
    GuiInputLatency,
    OrgEditing,
    MagitStatus,
    OrgJournalOpen,
    LargeFileEditing,
    Indentation,
    RegexSearch,
    SustainedNativeVideo,
}

impl ScenarioId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RustLspTyping => "rust-lsp-typing",
            Self::MxTabCompletion => "mx-tab-completion",
            Self::BytecodeCallLoop => "bytecode-call-loop",
            Self::EditingSimulation => "editing-simulation",
            Self::Startup => "startup",
            Self::SustainedEditing => "sustained-editing",
            Self::GuiInputLatency => "gui-input-latency",
            Self::OrgEditing => "org-editing",
            Self::MagitStatus => "magit-status",
            Self::OrgJournalOpen => "org-journal-open",
            Self::LargeFileEditing => "large-file-editing",
            Self::Indentation => "indentation",
            Self::RegexSearch => "regex-search",
            Self::SustainedNativeVideo => "sustained-native-video",
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
            "editing-simulation" => Ok(Self::EditingSimulation),
            "startup" => Ok(Self::Startup),
            "sustained-editing" => Ok(Self::SustainedEditing),
            "gui-input-latency" => Ok(Self::GuiInputLatency),
            "org-editing" => Ok(Self::OrgEditing),
            "magit-status" => Ok(Self::MagitStatus),
            "org-journal-open" => Ok(Self::OrgJournalOpen),
            "large-file-editing" => Ok(Self::LargeFileEditing),
            "indentation" => Ok(Self::Indentation),
            "regex-search" => Ok(Self::RegexSearch),
            "sustained-native-video" => Ok(Self::SustainedNativeVideo),
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
        description: "M-x TAB over 1,024 controlled commands through a real minibuffer and completion window",
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
    ScenarioSpec {
        id: ScenarioId::EditingSimulation,
        description: "Composite editing simulation with typed phase timings",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::Startup,
        description: "Clean editor startup through the complete process lifecycle",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(1).expect("non-zero scenario default"),
        primary_metric: MetricName::ProcessWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SustainedEditing,
        description: "Long-running insert, fontification, redisplay, and deletion cycle",
        default_frontend: Frontend::Tui {
            rows: 40,
            columns: 120,
        },
        default_iterations: NonZeroU32::new(100).expect("non-zero scenario default"),
        primary_metric: MetricName::PerEditWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::GuiInputLatency,
        description: "GUI self-insert-command-to-forced-redisplay latency distribution",
        default_frontend: Frontend::Gui {
            width: 1200,
            height: 800,
        },
        // The ranked metric is a p99: over 100 samples that is the 2nd-largest
        // value, an extreme-value statistic decided by whether a handful of
        // scheduling or GC events land inside the timed window. Over 1000 it
        // is the 10th-largest. The workload costs ~2-5 ms per keystroke, so
        // this adds a few seconds per run, not the minutes a percentile of
        // 100 would need to become trustworthy by repetition.
        default_iterations: NonZeroU32::new(1000).expect("non-zero scenario default"),
        primary_metric: MetricName::P99InputToRedisplayLatency,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgEditing,
        description: "Org headings, TODO state, tables, fontification, and edits",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::MagitStatus,
        description: "Revision-pinned Magit status refresh in a deterministic Git repository",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(10).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::OrgJournalOpen,
        description: "Revision-pinned org-journal yearly file open with org-superstar and git-gutter overlays",
        default_frontend: Frontend::Batch,
        // One operation is a full journal-open cycle (kill the buffer, let
        // org-journal find-file, fontify, and lay out the yearly file). At the
        // real workload's scale that is seconds, not milliseconds, so five
        // iterations keep the run bounded while still giving the median
        // something to work with.
        default_iterations: NonZeroU32::new(5).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::LargeFileEditing,
        description: "Editing, fontification, and navigation in a deterministic large file",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(20).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::Indentation,
        description: "Repeated Emacs Lisp region indentation with state restoration",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(50).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::RegexSearch,
        description: "Repeated regular-expression searches over realistic Emacs Lisp",
        default_frontend: Frontend::Batch,
        default_iterations: NonZeroU32::new(50).expect("non-zero scenario default"),
        primary_metric: MetricName::PerOperationWallTime,
        cross_editor_parity_metrics: &[],
    },
    ScenarioSpec {
        id: ScenarioId::SustainedNativeVideo,
        description: "Sustained native video decode, zero-copy import, GPU composition, pacing, and pool reuse on the caller's physical Linux display",
        default_frontend: Frontend::Gui {
            width: 1920,
            height: 1080,
        },
        // One operation is a 100 ms observation tick: 300 gives a 30 second
        // measurement window after decoder and renderer warmup.
        default_iterations: NonZeroU32::new(300).expect("non-zero scenario default"),
        primary_metric: MetricName::P99VideoPresentationInterval,
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
        ScenarioId::EditingSimulation => &SCENARIOS[3],
        ScenarioId::Startup => &SCENARIOS[4],
        ScenarioId::SustainedEditing => &SCENARIOS[5],
        ScenarioId::GuiInputLatency => &SCENARIOS[6],
        ScenarioId::OrgEditing => &SCENARIOS[7],
        ScenarioId::MagitStatus => &SCENARIOS[8],
        ScenarioId::OrgJournalOpen => &SCENARIOS[9],
        ScenarioId::LargeFileEditing => &SCENARIOS[10],
        ScenarioId::Indentation => &SCENARIOS[11],
        ScenarioId::RegexSearch => &SCENARIOS[12],
        ScenarioId::SustainedNativeVideo => &SCENARIOS[13],
    }
}
