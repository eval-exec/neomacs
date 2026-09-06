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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRequest {
    pub(crate) scenario: ScenarioId,
    pub(crate) editor: PathBuf,
    pub(crate) iterations: NonZeroU32,
    pub(crate) profiler: NativeProfiler,
    pub(crate) scope: ProfileScope,
    pub(crate) frontend: Option<Frontend>,
    pub(crate) timeout: Duration,
    pub(crate) machine: MachinePolicy,
    pub(crate) video_file: Option<PathBuf>,
    pub(crate) journal_file: Option<PathBuf>,
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
            frontend: None,
            timeout: Duration::from_secs(300),
            machine: MachinePolicy::default(),
            video_file: None,
            journal_file: None,
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

    pub fn frontend(&self) -> Frontend {
        self.frontend
            .unwrap_or_else(|| scenario(self.scenario).default_frontend)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerfSamplingEvent {
    UserCpuClock,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PerfCallGraph {
    Dwarf { stack_size_bytes: NonZeroU32 },
}

/// Exact Linux perf recording settings persisted with every profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerfCaptureConfiguration {
    pub event: PerfSamplingEvent,
    pub frequency_hz: NonZeroU32,
    pub call_graph: PerfCallGraph,
}

impl PerfCaptureConfiguration {
    pub const fn standard() -> Self {
        Self {
            event: PerfSamplingEvent::UserCpuClock,
            frequency_hz: NonZeroU32::new(999).expect("999 is non-zero"),
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
        let PerfCallGraph::Dwarf { stack_size_bytes } = self.call_graph;
        let mut arguments = vec![
            OsString::from("record"),
            OsString::from("--quiet"),
            OsString::from("--no-buildid-cache"),
            OsString::from("--event"),
            OsString::from(self.event.perf_name()),
            OsString::from("--freq"),
            OsString::from(self.frequency_hz.get().to_string()),
            OsString::from("--call-graph"),
            OsString::from(format!("dwarf,{}", stack_size_bytes.get())),
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
    ) -> [(String, OsString); 3] {
        let PerfCallGraph::Dwarf { stack_size_bytes } = self.call_graph;
        [
            (
                format!("{prefix}_PERF_EVENT"),
                OsString::from(self.event.perf_name()),
            ),
            (
                format!("{prefix}_PERF_FREQUENCY"),
                OsString::from(self.frequency_hz.get().to_string()),
            ),
            (
                format!("{prefix}_PERF_CALL_GRAPH"),
                OsString::from(format!("dwarf,{}", stack_size_bytes.get())),
            ),
        ]
    }

    pub(crate) fn report_arguments(input: &Path) -> Vec<OsString> {
        vec![
            OsString::from("report"),
            OsString::from("--stdio"),
            OsString::from("--input"),
            input.as_os_str().to_os_string(),
            OsString::from("--no-children"),
            OsString::from("--percent-limit"),
            OsString::from("0.5"),
            OsString::from("--call-graph"),
            OsString::from("fractal,0.5"),
        ]
    }
}

impl PerfSamplingEvent {
    const fn perf_name(self) -> &'static str {
        match self {
            Self::UserCpuClock => "cpu-clock:u",
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
    pub configuration: PerfCaptureConfiguration,
    pub run_artifact_path: PathBuf,
    pub verdict: ProfileVerdict,
}

impl ProfileArtifact {
    pub const SCHEMA_VERSION: u32 = 3;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileReport {
    pub artifact: ProfileArtifact,
    pub artifact_path: PathBuf,
    pub run: RunReport,
}
