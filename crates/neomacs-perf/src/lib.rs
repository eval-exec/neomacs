mod artifact;
mod artifact_store;
mod capture;
mod catalog;
mod cli;
mod comparison;
mod counters;
mod harness;
mod host;
mod native_video;
mod profile;
mod profile_gate;
mod suite;

pub use artifact::{
    ArtifactFile, ArtifactKind, CorrectnessMismatch, EditorCapabilities, EditorKind,
    EditorProvenance, Measurement, MetricName, MetricUnit, RunArtifact, RunVerdict,
};
pub(crate) use capture::CaptureRoute;
pub use catalog::{
    CrossEditorParityMetric, Frontend, ScenarioId, ScenarioSpec, scenario, scenarios,
};
pub use cli::{PerfCliError, PerfCommand, parse_perf_command, run_cli};
#[cfg(test)]
pub(crate) use comparison::{
    COMPARISON_ARTIFACT_SCHEMA_VERSION, ComparisonObservation, evaluate_comparison,
};
pub use comparison::{
    ComparisonArtifact, ComparisonInput, ComparisonMetricSummary, ComparisonRejection,
    ComparisonReport, ComparisonRequest, ComparisonRun, ComparisonRunOutcome, ComparisonRunRole,
    ComparisonSampleCount, ComparisonVerdict, comparison_schedule,
};
pub use counters::{CounterScope, parse_perf_stat_csv};
#[cfg(test)]
pub(crate) use harness::{
    ARTIFACT_SCHEMA_VERSION, PerfCapture, collect_editor_provenance,
    configure_benchmark_environment, validate_harness_build, validate_harness_revision,
};
pub use harness::{PerfError, PerfHarness, RunReport, RunRequest};
#[cfg(test)]
pub(crate) use harness::{perf_data_sample_count, profile_verdict};
pub use host::{HostProvenance, MachinePolicy};
#[cfg(test)]
pub(crate) use host::{cpu_list_contains, validate_machine_policy};
pub use native_video::{
    NativeVideoComparisonIdentity, NativeVideoDecoderKind, NativeVideoExecutionIdentity,
    NativeVideoFrameFormat, NativeVideoFrameRate, NativeVideoGpuTimingStatus,
    NativeVideoGraphicsBackend, NativeVideoMediaMetadata,
};
pub use profile::{
    NativeProfiler, PerfCallGraph, PerfCaptureConfiguration, PerfSamplingEvent, ProfileArtifact,
    ProfileRejection, ProfileReport, ProfileRequest, ProfileScope, ProfileVerdict,
};
#[cfg(test)]
pub(crate) use profile_gate::ProfileGate;
#[cfg(test)]
pub(crate) use suite::{SUITE_ARTIFACT_SCHEMA_VERSION, evaluate_suite, read_history_link};
pub use suite::{
    SuiteArtifact, SuiteHistoryLink, SuiteId, SuiteRegression, SuiteReport, SuiteRequest,
    SuiteScenario, SuiteScenarioResult, SuiteVerdict,
};

#[cfg(test)]
mod architecture_test;
#[cfg(test)]
mod artifact_test;
#[cfg(test)]
mod build_provenance_test;
#[cfg(test)]
mod catalog_test;
#[cfg(test)]
mod cli_test;
#[cfg(test)]
mod comparison_test;
#[cfg(test)]
mod counters_test;
#[cfg(test)]
mod harness_test;
#[cfg(test)]
mod host_test;
#[cfg(test)]
mod profile_test;
#[cfg(test)]
mod suite_test;
