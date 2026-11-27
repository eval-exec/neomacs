mod artifact;
mod artifact_store;
mod baseline_manifest;
mod capture;
mod catalog;
mod cli;
mod comparison;
mod counters;
mod execution_policy;
mod harness;
mod host;
mod input_latency;
mod native_video;
mod portable_dump;
mod profile;
mod profile_gate;
mod suite;

pub use artifact::{
    ArtifactFile, ArtifactKind, CorrectnessMismatch, EditorCapabilities, EditorKind,
    EditorProvenance, Measurement, MetricName, MetricUnit, PortableDumpProvenance, RunArtifact,
    RunVerdict,
};
pub use baseline_manifest::{
    BaselineAssetEntry, BaselineBinaryEntry, BaselineManifest, BaselinePdumpEntry,
    ResolvedGuardTools, VerifiedBaselineManifest, resolve_guard_tools, resolve_pinned_tool,
};
pub(crate) use capture::CaptureRoute;
pub use catalog::{
    CrossEditorParityMetric, Frontend, ScenarioId, ScenarioSpec, scenario, scenarios,
};
pub use cli::{PerfCliError, PerfCommand, parse_perf_command, run_cli};
#[cfg(test)]
pub(crate) use comparison::{
    COMPARISON_ARTIFACT_SCHEMA_VERSION, ComparisonObservation, comparison_run_from_child,
    evaluate_comparison,
};
pub use comparison::{
    ComparisonArtifact, ComparisonInput, ComparisonMetricSummary, ComparisonPairedSample,
    ComparisonRejection, ComparisonReport, ComparisonRequest, ComparisonRun, ComparisonRunOutcome,
    ComparisonRunRole, ComparisonSampleCount, ComparisonVerdict, comparison_schedule,
};
pub use counters::{CounterScope, parse_perf_stat_csv};
pub use execution_policy::{ExecutionOverride, ExecutionOverrides};
#[cfg(test)]
pub(crate) use harness::{
    ARTIFACT_SCHEMA_VERSION, PerfCapture, collect_editor_provenance,
    configure_benchmark_environment, validate_harness_build, validate_harness_revision,
};
pub use harness::{PerfError, PerfHarness, RunReport, RunRequest};
#[cfg(test)]
pub(crate) use harness::{perf_data_sample_count, profile_verdict};
pub use host::{HostProvenance, LOCAL_HIGH_LOAD_THRESHOLD, MachinePolicy, MeasurementProvenance};
#[cfg(test)]
pub(crate) use host::{
    collect_host_provenance_with_loadavg, cpu_list_contains, parse_loadavg_1m,
    validate_cohort_load_policy, validate_machine_policy,
};
pub use native_video::{
    NativeVideoComparisonIdentity, NativeVideoDecoderKind, NativeVideoExecutionIdentity,
    NativeVideoFrameFormat, NativeVideoFrameRate, NativeVideoGpuTimingStatus,
    NativeVideoGraphicsBackend, NativeVideoMediaMetadata,
};
pub use profile::{
    NativeProfiler, PerfCallGraph, PerfCaptureConfiguration, PerfSamplingEvent, PerfSamplingRate,
    ProfileArtifact, ProfileRejection, ProfileReport, ProfileReportStyle, ProfileRequest,
    ProfileScope, ProfileVerdict,
};
#[cfg(test)]
pub(crate) use profile_gate::ProfileGate;
pub use suite::{
    InstructionCalibration, NonConvertibleTimingReason, SuiteArtifact, SuiteDrift, SuiteDriftSkip,
    SuiteDriftSkipReason, SuiteGateMetric, SuiteHistoryLink, SuiteId, SuiteRegression, SuiteReport,
    SuiteRequest, SuiteScenario, SuiteScenarioResult, SuiteVerdict,
};
#[cfg(test)]
pub(crate) use suite::{
    SUITE_ARTIFACT_SCHEMA_VERSION, SuiteEvaluationContext, evaluate_suite,
    evaluate_suite_with_context, read_history_link,
};

/// The workspace root of the machine running this code: nextest's runtime
/// `NEXTEST_WORKSPACE_ROOT` when present, the baked compile-time constant
/// otherwise (see neomacs-infra::workspace_root).
pub fn workspace_root() -> std::path::PathBuf {
    std::env::var_os("NEXTEST_WORKSPACE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_WORKSPACE_DIR")))
}

#[cfg(test)]
mod tests;
