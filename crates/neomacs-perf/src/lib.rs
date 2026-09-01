mod artifact;
mod artifact_store;
mod catalog;
mod cli;
mod comparison;
mod harness;
mod profile;
mod profile_gate;

pub use artifact::{
    ArtifactFile, ArtifactKind, CorrectnessMismatch, EditorProvenance, Measurement, MetricName,
    MetricUnit, RunArtifact, RunVerdict,
};
pub use catalog::{
    CrossEditorParityMetric, Frontend, ScenarioId, ScenarioSpec, scenario, scenarios,
};
pub use cli::{PerfCliError, PerfCommand, parse_perf_command, run_cli};
pub use comparison::{
    ComparisonArtifact, ComparisonInput, ComparisonMetricSummary, ComparisonRejection,
    ComparisonReport, ComparisonRequest, ComparisonRun, ComparisonRunOutcome, ComparisonRunRole,
    ComparisonSampleCount, ComparisonVerdict, comparison_schedule,
};
#[cfg(test)]
pub(crate) use comparison::{ComparisonObservation, evaluate_comparison};
#[cfg(test)]
pub(crate) use harness::{PerfCapture, collect_editor_provenance, configure_benchmark_environment};
pub use harness::{PerfError, PerfHarness, RunReport, RunRequest};
#[cfg(test)]
pub(crate) use harness::{perf_data_sample_count, profile_verdict};
pub use profile::{
    NativeProfiler, PerfCallGraph, PerfCaptureConfiguration, PerfSamplingEvent, ProfileArtifact,
    ProfileRejection, ProfileReport, ProfileRequest, ProfileScope, ProfileVerdict,
};
#[cfg(test)]
pub(crate) use profile_gate::ProfileGate;

#[cfg(test)]
mod artifact_test;
#[cfg(test)]
mod catalog_test;
#[cfg(test)]
mod cli_test;
#[cfg(test)]
mod comparison_test;
#[cfg(test)]
mod harness_test;
#[cfg(test)]
mod profile_test;
