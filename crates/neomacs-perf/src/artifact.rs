use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Frontend, HostProvenance, ScenarioId};

/// Why a completed workload cannot contribute a performance sample.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectnessMismatch {
    pub invariant: String,
    pub expected: String,
    pub actual: String,
}

/// Semantic outcome of a run, kept separate from process execution success.
///
/// Only `Valid` may be aggregated or compared. A completed process with the
/// wrong editor state is a correctness failure, not a fast benchmark result.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RunVerdict {
    Valid {
        measurements: Vec<Measurement>,
    },
    CorrectnessMismatch {
        mismatches: Vec<CorrectnessMismatch>,
    },
    InfrastructureFailure {
        message: String,
    },
}

impl RunVerdict {
    pub const fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    pub fn measurements(&self) -> Option<&[Measurement]> {
        match self {
            Self::Valid { measurements } => Some(measurements),
            Self::CorrectnessMismatch { .. } | Self::InfrastructureFailure { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricName {
    WorkloadCpuTime,
    WorkloadWallTime,
    ProcessWallTime,
    PerEditCpuTime,
    PerEditWallTime,
    PerCompletionCpuTime,
    PerBytecodeCallCpuTime,
    Iterations,
    Edits,
    Redisplays,
    CompletionHelpCalls,
    CompletionCandidateCount,
    OverlayCount,
    LspDiagnosticCount,
    BytecodeCalls,
    CpuCycles,
    Instructions,
    PageFaults,
    BranchMisses,
    CacheMisses,
    L1DataCacheLoadMisses,
    DataTlbLoadMisses,
    PerOperationCpuTime,
    PerOperationWallTime,
    OperationCount,
    TypePhaseCpuTime,
    CommentPhaseCpuTime,
    KillYankPhaseCpuTime,
    IndentPhaseCpuTime,
    RegexPhaseCpuTime,
    ModePhaseCpuTime,
    FontifyPhaseCpuTime,
    ReplacePhaseCpuTime,
    UndoRedoPhaseCpuTime,
    IsearchPhaseCpuTime,
    BufferSwitchPhaseCpuTime,
    HowManyPhaseCpuTime,
    MotionPhaseCpuTime,
    P50InputToRedisplayLatency,
    P95InputToRedisplayLatency,
    P99InputToRedisplayLatency,
    /// Keystrokes whose input-to-redisplay latency exceeded the 1 ms budget,
    /// as a count over the run. A Poisson count of over-budget events has far
    /// lower variance than any single order statistic of the same samples,
    /// and it is the quantity pause-placement work actually moves; read it
    /// beside the percentiles, never instead of them.
    InputLatencyOverBudgetCount,
    VideoPresentationFramesPerSecond,
    VideoDecodeFramesPerSecond,
    P50VideoPresentationInterval,
    P95VideoPresentationInterval,
    P99VideoPresentationInterval,
    MaxVideoPresentationInterval,
    AverageVideoGpuPassTime,
    VideoDecodedFrames,
    VideoPresentedFrames,
    VideoReplacedFrames,
    VideoLateDroppedFrames,
    VideoBackpressuredFrames,
    VideoGpuPassSamples,
    VideoGpuMemoryBytes,
    VideoSurfacePoolAllocations,
    VideoSurfacePoolReuses,
    VideoSurfacePoolBackpressuredAcquires,
    VideoSurfacePoolInFlightHighWater,
}

impl MetricName {
    /// The only unit in which this metric is valid in persisted artifacts.
    pub const fn canonical_unit(self) -> MetricUnit {
        match self {
            Self::WorkloadCpuTime
            | Self::WorkloadWallTime
            | Self::ProcessWallTime
            | Self::TypePhaseCpuTime
            | Self::CommentPhaseCpuTime
            | Self::KillYankPhaseCpuTime
            | Self::IndentPhaseCpuTime
            | Self::RegexPhaseCpuTime
            | Self::ModePhaseCpuTime
            | Self::FontifyPhaseCpuTime
            | Self::ReplacePhaseCpuTime
            | Self::UndoRedoPhaseCpuTime
            | Self::IsearchPhaseCpuTime
            | Self::BufferSwitchPhaseCpuTime
            | Self::HowManyPhaseCpuTime
            | Self::MotionPhaseCpuTime
            | Self::P50InputToRedisplayLatency
            | Self::P95InputToRedisplayLatency
            | Self::P99InputToRedisplayLatency
            | Self::P50VideoPresentationInterval
            | Self::P95VideoPresentationInterval
            | Self::P99VideoPresentationInterval
            | Self::MaxVideoPresentationInterval => MetricUnit::Microseconds,
            Self::PerEditCpuTime | Self::PerEditWallTime => MetricUnit::MicrosecondsPerEdit,
            Self::PerCompletionCpuTime => MetricUnit::MicrosecondsPerCompletion,
            Self::PerBytecodeCallCpuTime => MetricUnit::MicrosecondsPerBytecodeCall,
            Self::PerOperationCpuTime | Self::PerOperationWallTime => {
                MetricUnit::MicrosecondsPerOperation
            }
            Self::AverageVideoGpuPassTime => MetricUnit::MicrosecondsPerFrame,
            Self::VideoPresentationFramesPerSecond | Self::VideoDecodeFramesPerSecond => {
                MetricUnit::FramesPerSecond
            }
            Self::Iterations
            | Self::Edits
            | Self::Redisplays
            | Self::CompletionHelpCalls
            | Self::CompletionCandidateCount
            | Self::OverlayCount
            | Self::LspDiagnosticCount
            | Self::BytecodeCalls
            | Self::CpuCycles
            | Self::Instructions
            | Self::PageFaults
            | Self::BranchMisses
            | Self::CacheMisses
            | Self::L1DataCacheLoadMisses
            | Self::DataTlbLoadMisses => MetricUnit::Count,
            Self::OperationCount
            | Self::InputLatencyOverBudgetCount
            | Self::VideoDecodedFrames
            | Self::VideoPresentedFrames
            | Self::VideoReplacedFrames
            | Self::VideoLateDroppedFrames
            | Self::VideoBackpressuredFrames
            | Self::VideoGpuPassSamples
            | Self::VideoSurfacePoolAllocations
            | Self::VideoSurfacePoolReuses
            | Self::VideoSurfacePoolBackpressuredAcquires
            | Self::VideoSurfacePoolInFlightHighWater => MetricUnit::Count,
            Self::VideoGpuMemoryBytes => MetricUnit::Bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricUnit {
    Microseconds,
    MicrosecondsPerEdit,
    MicrosecondsPerCompletion,
    MicrosecondsPerBytecodeCall,
    MicrosecondsPerOperation,
    MicrosecondsPerFrame,
    FramesPerSecond,
    Bytes,
    Count,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub name: MetricName,
    pub value: f64,
    pub unit: MetricUnit,
}

/// Immutable identity of the executable and matching portable dump used by a run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditorProvenance {
    pub path: String,
    pub executable_sha256: String,
    pub executable_size_bytes: u64,
    pub pdump_fingerprint: String,
    pub version: String,
    pub kind: EditorKind,
    pub capabilities: EditorCapabilities,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorKind {
    Neomacs,
    GnuEmacs,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditorCapabilities {
    pub native_compilation: bool,
    pub tree_sitter: bool,
    pub dynamic_modules: bool,
    pub video_playback: bool,
    pub webview: bool,
    pub embedded_terminal: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    ScenarioResult,
    ScenarioFixture,
    SourceFixture,
    LspReplay,
    PackageStartup,
    TreeSitterGrammar,
    TerminalByteStream,
    Stdout,
    Stderr,
    FrontendLog,
    CompositorLog,
    InputProvenance,
    NativeProfileData,
    NativeProfileReport,
    HardwareCounters,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFile {
    pub kind: ArtifactKind,
    pub path: PathBuf,
}

/// Machine-readable record persisted for every attempted workload run.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunArtifact {
    pub schema_version: u32,
    pub run_id: String,
    pub scenario: ScenarioId,
    pub frontend: Frontend,
    pub editor: PathBuf,
    pub host: HostProvenance,
    pub iterations: u32,
    pub started_unix_ms: u128,
    pub total_elapsed_us: u128,
    /// Runtime pipeline identity for a native-video run. `None` for every
    /// other scenario and for failures before the pipeline reported itself.
    pub native_video_execution: Option<crate::NativeVideoExecutionIdentity>,
    pub verdict: RunVerdict,
    pub files: Vec<ArtifactFile>,
}
