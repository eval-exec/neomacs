use std::ffi::OsString;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Frontend, MachinePolicy, RunReport, ScenarioId, scenario};

/// Native sampling backend used for a diagnostic workload run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeProfiler {
    Perf,
}

/// Portion of a scenario whose native stacks are sampled.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileScope {
    /// Sample only the scenario's repeated editing operation.
    EditLoop,
    /// Sample editor startup, fixture loading, and the editing operation.
    WholeProcess,
}

/// How to render sampled data. Both styles retain the same raw stacks.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileReportStyle {
    /// Unwind stacks and render call graphs.
    #[default]
    CallGraph,
    /// Attribute samples to their current symbol without unwinding stacks.
    SelfTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRequest {
    pub(crate) scenario: ScenarioId,
    pub(crate) editor: PathBuf,
    pub(crate) iterations: NonZeroU32,
    pub(crate) profiler: NativeProfiler,
    pub(crate) scope: ProfileScope,
    pub(crate) report_style: ProfileReportStyle,
    pub(crate) configuration: PerfCaptureConfiguration,
    pub(crate) frontend: Option<Frontend>,
    pub(crate) timeout: Duration,
    pub(crate) machine: MachinePolicy,
    pub(crate) video_file: Option<PathBuf>,
    pub(crate) journal_file: Option<PathBuf>,
    pub(crate) execution_overrides: crate::ExecutionOverrides,
}

impl ProfileRequest {
    pub fn new(
        scenario: ScenarioId,
        editor: impl Into<PathBuf>,
        iterations: NonZeroU32,
        profiler: NativeProfiler,
    ) -> Self {
        Self {
            scenario,
            editor: editor.into(),
            iterations,
            profiler,
            scope: ProfileScope::EditLoop,
            report_style: ProfileReportStyle::default(),
            configuration: profiler.capture_configuration(),
            frontend: None,
            timeout: Duration::from_secs(300),
            machine: MachinePolicy::default(),
            video_file: None,
            journal_file: None,
            execution_overrides: crate::ExecutionOverrides::default(),
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

    pub fn with_scope(mut self, scope: ProfileScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_report_style(mut self, report_style: ProfileReportStyle) -> Self {
        self.report_style = report_style;
        self
    }

    pub fn with_capture_configuration(mut self, configuration: PerfCaptureConfiguration) -> Self {
        self.configuration = configuration;
        self
    }

    pub fn with_machine_policy(mut self, machine: MachinePolicy) -> Self {
        self.machine = machine;
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

    pub fn with_execution_overrides(mut self, overrides: crate::ExecutionOverrides) -> Self {
        self.execution_overrides = overrides;
        self
    }

    pub fn frontend(&self) -> Frontend {
        self.frontend
            .unwrap_or_else(|| scenario(self.scenario).default_frontend)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerfSamplingEvent {
    UserCpuClock,
    /// Retired user-space instructions on any supported hardware PMU.
    UserInstructions,
    /// Retired user-space instructions on Linux's explicit P-core PMU.
    UserCoreInstructions,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PerfCallGraph {
    Dwarf { stack_size_bytes: NonZeroU32 },
    Lbr,
}

/// A frequency requests adaptive sampling; a period counts events per sample.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PerfSamplingRate {
    Frequency { frequency_hz: NonZeroU32 },
    Period { sample_period: NonZeroU64 },
}

/// Exact Linux perf recording settings persisted with every profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PerfCaptureConfiguration {
    pub event: PerfSamplingEvent,
    pub sampling: PerfSamplingRate,
    pub call_graph: PerfCallGraph,
}

// Schemas 3 and 4 recorded only frequency_hz. Read that representation without
// allowing an ambiguous artifact to claim both a period and a frequency.
impl<'de> Deserialize<'de> for PerfCaptureConfiguration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Configuration {
            event: PerfSamplingEvent,
            sampling: Option<PerfSamplingRate>,
            frequency_hz: Option<NonZeroU32>,
            call_graph: PerfCallGraph,
        }
        let configuration = Configuration::deserialize(deserializer)?;
        let sampling = match (configuration.sampling, configuration.frequency_hz) {
            (Some(sampling), None) => sampling,
            (None, Some(frequency_hz)) => PerfSamplingRate::Frequency { frequency_hz },
            _ => {
                return Err(serde::de::Error::custom(
                    "expected exactly one sampling or legacy frequency_hz setting",
                ));
            }
        };
        Ok(Self {
            event: configuration.event,
            sampling,
            call_graph: configuration.call_graph,
        })
    }
}

impl PerfCaptureConfiguration {
    pub const fn standard() -> Self {
        Self {
            event: PerfSamplingEvent::UserCpuClock,
            sampling: PerfSamplingRate::Frequency {
                frequency_hz: NonZeroU32::new(999).expect("999 is non-zero"),
            },
            call_graph: PerfCallGraph::Dwarf {
                stack_size_bytes: NonZeroU32::new(16_384).expect("16384 is non-zero"),
            },
        }
    }

    pub(crate) fn record_arguments(
        self,
        output: &Path,
        control: Option<(&Path, &Path)>,
    ) -> Vec<OsString> {
        let (rate_flag, rate_value) = self.sampling.record_argument();
        let mut arguments = vec![
            OsString::from("record"),
            OsString::from("--quiet"),
            OsString::from("--no-buildid-cache"),
            OsString::from("--event"),
            OsString::from(self.event.perf_name()),
            OsString::from(rate_flag),
            OsString::from(rate_value),
            OsString::from("--call-graph"),
            OsString::from(self.call_graph.perf_name()),
        ];
        if let Some((command, acknowledgement)) = control {
            arguments.push(OsString::from("--delay=-1"));
            arguments.push(OsString::from(format!(
                "--control=fifo:{},{}",
                command.display(),
                acknowledgement.display()
            )));
        }
        arguments.extend([
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            OsString::from("--"),
        ]);
        arguments
    }

    pub(crate) fn adapter_record_environment(
        self,
        prefix: &'static str,
    ) -> [(String, Option<OsString>); 4] {
        let (frequency, period) = match self.sampling {
            PerfSamplingRate::Frequency { frequency_hz } => {
                (Some(OsString::from(frequency_hz.get().to_string())), None)
            }
            PerfSamplingRate::Period { sample_period } => {
                (None, Some(OsString::from(sample_period.get().to_string())))
            }
        };
        [
            (
                format!("{prefix}_PERF_EVENT"),
                Some(OsString::from(self.event.perf_name())),
            ),
            (format!("{prefix}_PERF_FREQUENCY"), frequency),
            (format!("{prefix}_PERF_PERIOD"), period),
            (
                format!("{prefix}_PERF_CALL_GRAPH"),
                Some(OsString::from(self.call_graph.perf_name())),
            ),
        ]
    }
}

impl PerfSamplingRate {
    fn record_argument(self) -> (&'static str, String) {
        match self {
            Self::Frequency { frequency_hz } => ("--freq", frequency_hz.get().to_string()),
            Self::Period { sample_period } => ("--count", sample_period.get().to_string()),
        }
    }
}

impl PerfCallGraph {
    fn perf_name(self) -> String {
        match self {
            Self::Dwarf { stack_size_bytes } => format!("dwarf,{}", stack_size_bytes.get()),
            Self::Lbr => "lbr".to_string(),
        }
    }
}

impl ProfileReportStyle {
    pub(crate) fn report_arguments(self, input: &Path) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from("report"),
            OsString::from("--stdio"),
            OsString::from("--input"),
            input.as_os_str().to_os_string(),
            OsString::from("--no-children"),
            OsString::from("--percent-limit"),
            OsString::from("0.5"),
            OsString::from("--call-graph"),
            OsString::from(match self {
                Self::CallGraph => "fractal,0.5",
                Self::SelfTime => "none",
            }),
        ];
        if self == Self::SelfTime {
            // Inline expansion invokes DWARF lookup even without call graphs,
            // which can dominate reporting for a large optimized editor binary.
            arguments.extend([
                OsString::from("--no-inline"),
                OsString::from("--sort"),
                OsString::from("symbol"),
            ]);
        }
        arguments
    }
}

impl PerfSamplingEvent {
    const fn perf_name(self) -> &'static str {
        match self {
            Self::UserCpuClock => "cpu-clock:u",
            Self::UserInstructions => "instructions:u",
            Self::UserCoreInstructions => "cpu_core/instructions/u",
        }
    }
}

impl NativeProfiler {
    pub(crate) const fn capture_configuration(self) -> PerfCaptureConfiguration {
        match self {
            Self::Perf => PerfCaptureConfiguration::standard(),
        }
    }

    pub(crate) fn platform_rejection(self) -> Option<ProfileRejection> {
        match self {
            Self::Perf if cfg!(target_os = "linux") => None,
            Self::Perf => Some(ProfileRejection::UnsupportedPlatform {
                operating_system: std::env::consts::OS.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProfileVerdict {
    Captured {
        perf_data_path: PathBuf,
        hotspot_report_path: PathBuf,
        sample_count: NonZeroU64,
    },
    Rejected {
        reason: ProfileRejection,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProfileRejection {
    CorrectnessMismatch,
    InfrastructureFailure { message: String },
    UnsupportedPlatform { operating_system: String },
}

/// Diagnostic profile metadata. Timings stay in the linked instrumented run
/// and are deliberately absent here so this artifact cannot be compared.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileArtifact {
    pub schema_version: u32,
    pub profile_id: String,
    pub scenario: ScenarioId,
    pub frontend: Frontend,
    pub editor: PathBuf,
    pub video_file: Option<PathBuf>,
    pub iterations: NonZeroU32,
    pub profiler: NativeProfiler,
    pub scope: ProfileScope,
    /// Absent in schema 3, which always rendered a call graph.
    #[serde(default)]
    pub report_style: ProfileReportStyle,
    pub configuration: PerfCaptureConfiguration,
    pub run_artifact_path: PathBuf,
    pub verdict: ProfileVerdict,
}

impl ProfileArtifact {
    pub const SCHEMA_VERSION: u32 = 5;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileReport {
    pub artifact: ProfileArtifact,
    pub artifact_path: PathBuf,
    pub run: RunReport,
}

#[cfg(test)]
#[path = "profile/tests/profile_test.rs"]
mod tests;
