use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Read};
use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use linux_perf_data::{PerfFileReader, PerfFileRecord, linux_perf_event_reader::RecordType};
use neomacs_melpa_test_support::{
    CommandError, MelpaSandbox, PreparedPackageSet, group_output_with_timeout, output_with_timeout,
    prepare_cached_tree_sitter_grammar,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    ArtifactFile, ArtifactKind, CorrectnessMismatch, CounterScope, EditorCapabilities, EditorKind,
    EditorProvenance, Frontend, HostProvenance, MachinePolicy, Measurement, MetricName, MetricUnit,
    PerfCaptureConfiguration, ProfileArtifact, ProfileRejection, ProfileReport, ProfileRequest,
    ProfileScope, ProfileVerdict, RunArtifact, RunVerdict, ScenarioId,
    artifact_store::{unix_time_ms, write_json_atomically},
    counters::PerfStatCapture,
    host::{collect_host_provenance, validate_machine_policy},
    native_video::{NativeVideoExecutionIdentity, NativeVideoPresentationTarget},
    profile_gate::ProfileGate,
    scenario,
};

pub(crate) mod scenarios;
use scenarios::org_journal_open::{ExternalJournalInput, journal_file_in_directory};

pub(crate) const ARTIFACT_SCHEMA_VERSION: u32 = 5;
const SCENARIO_RESULT_SCHEMA_VERSION: u32 = 1;
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRequest {
    scenario: ScenarioId,
    editor: PathBuf,
    iterations: NonZeroU32,
    frontend: Option<Frontend>,
    timeout: Duration,
    machine: MachinePolicy,
    counters: Option<CounterScope>,
    video_file: Option<PathBuf>,
    journal_file: Option<PathBuf>,
}

impl RunRequest {
    pub fn new(scenario: ScenarioId, editor: impl Into<PathBuf>, iterations: NonZeroU32) -> Self {
        Self {
            scenario,
            editor: editor.into(),
            iterations,
            frontend: None,
            timeout: Duration::from_secs(300),
            machine: MachinePolicy::default(),
            counters: None,
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

    pub fn with_machine_policy(mut self, machine: MachinePolicy) -> Self {
        self.machine = machine;
        self
    }

    pub fn with_counters(mut self, counters: Option<CounterScope>) -> Self {
        self.counters = counters;
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

    pub const fn scenario(&self) -> ScenarioId {
        self.scenario
    }

    pub fn editor(&self) -> &Path {
        &self.editor
    }

    pub const fn iterations(&self) -> NonZeroU32 {
        self.iterations
    }

    pub fn frontend(&self) -> Frontend {
        self.frontend
            .unwrap_or_else(|| scenario(self.scenario).default_frontend)
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn machine_policy(&self) -> &MachinePolicy {
        &self.machine
    }

    pub const fn counters(&self) -> Option<CounterScope> {
        self.counters
    }

    pub fn video_file(&self) -> Option<&Path> {
        self.video_file.as_deref()
    }

    pub fn journal_file(&self) -> Option<&Path> {
        self.journal_file.as_deref()
    }

    fn validate_scenario_input(&self) -> Result<(), String> {
        match (self.scenario, self.video_file.as_ref()) {
            (ScenarioId::SustainedNativeVideo, None) => Err(
                "sustained-native-video requires --video-file pointing to a readable video"
                    .to_owned(),
            ),
            (ScenarioId::SustainedNativeVideo, Some(_)) | (_, None) => Ok(()),
            (scenario, Some(path)) => Err(format!(
                "scenario {scenario} does not accept native-video input {}",
                path.display()
            )),
        }?;
        match (self.scenario, self.journal_file.as_ref()) {
            (ScenarioId::OrgJournalOpen, Some(path)) if path.is_file() => Ok(()),
            (ScenarioId::OrgJournalOpen, Some(path)) => Err(format!(
                "org-journal-open requires an existing readable journal file at {}",
                path.display()
            )),
            (ScenarioId::OrgJournalOpen, None) | (_, None) => Ok(()),
            (scenario, Some(path)) => Err(format!(
                "scenario {scenario} does not accept journal input {}",
                path.display()
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    pub artifact: RunArtifact,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum PerfError {
    #[error("failed to create performance artifact directory {path}: {source}")]
    CreateArtifactDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write performance artifact {path}: {source}")]
    WriteArtifact {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("scenario `{scenario}` emitted an invalid result document: {source}")]
    InvalidScenarioResult {
        scenario: ScenarioId,
        source: serde_json::Error,
    },
    #[error("failed to serialize performance artifact: {0}")]
    SerializeArtifact(serde_json::Error),
    #[error("invalid previous performance suite {path}: {message}")]
    InvalidSuiteHistory { path: PathBuf, message: String },
}

#[derive(Clone, Debug)]
pub struct PerfHarness {
    pub(crate) workspace_root: PathBuf,
}

impl PerfHarness {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    pub fn run(&self, request: &RunRequest) -> Result<RunReport, PerfError> {
        let context = RunContext::create(&self.workspace_root, request)?;
        if let Err(message) = request.validate_scenario_input() {
            return context.infrastructure_failure(message, Vec::new());
        }
        if let Err(message) = validate_machine_policy(&request.machine, &context.host) {
            return context.infrastructure_failure(message, Vec::new());
        }
        if !request.editor.is_file() {
            return context.infrastructure_failure(
                format!("missing editor executable {}", request.editor.display()),
                Vec::new(),
            );
        }

        self.run_prepared_scenario(request, context, None)
    }

    pub fn profile(&self, request: &ProfileRequest) -> Result<ProfileReport, PerfError> {
        let run_request = RunRequest::new(request.scenario, &request.editor, request.iterations)
            .with_frontend(request.frontend())
            .with_timeout(request.timeout)
            .with_machine_policy(request.machine.clone())
            .with_video_file(request.video_file.clone())
            .with_journal_file(request.journal_file.clone());
        let context = RunContext::create_in(
            &self.workspace_root,
            &run_request,
            ArtifactNamespace::Profiles,
        )?;
        let platform_rejection = request.profiler.platform_rejection();
        let run = if let Err(message) = run_request.validate_scenario_input() {
            context.infrastructure_failure(message, Vec::new())?
        } else if let Some(reason) = &platform_rejection {
            context.infrastructure_failure(
                format!("native profiler is unavailable: {reason:?}"),
                Vec::new(),
            )?
        } else if !request.editor.is_file() {
            context.infrastructure_failure(
                format!("missing editor executable {}", request.editor.display()),
                Vec::new(),
            )?
        } else {
            let mut capture = PerfCapture::new(
                &context.directory,
                request.profiler.capture_configuration(),
                request.scope,
                request.timeout,
            );
            self.run_prepared_scenario(&run_request, context, Some(&mut capture))?
        };
        self.publish_profile(request, run, platform_rejection)
    }

    fn publish_profile(
        &self,
        request: &ProfileRequest,
        run: RunReport,
        forced_rejection: Option<ProfileRejection>,
    ) -> Result<ProfileReport, PerfError> {
        let verdict = forced_rejection.map_or_else(
            || profile_verdict(&run),
            |reason| ProfileVerdict::Rejected { reason },
        );
        let artifact = ProfileArtifact {
            schema_version: ProfileArtifact::SCHEMA_VERSION,
            profile_id: run.artifact.run_id.clone(),
            scenario: request.scenario,
            frontend: request.frontend(),
            editor: request.editor.clone(),
            video_file: request.video_file.clone(),
            iterations: request.iterations,
            profiler: request.profiler,
            scope: request.scope,
            configuration: request.profiler.capture_configuration(),
            run_artifact_path: PathBuf::from("artifact.json"),
            verdict,
        };
        let artifact_path = run
            .artifact_path
            .parent()
            .expect("run artifact has a parent directory")
            .join("profile.json");
        write_json_atomically(&artifact_path, &artifact)?;
        Ok(ProfileReport {
            artifact,
            artifact_path,
            run,
        })
    }

    /// Validate and persist a result produced by a frontend adapter.
    ///
    /// Out-of-process adapters publish the result file before the harness sees
    /// it. Keeping validation and artifact publication together prevents an
    /// adapter from turning a mismatch into a valid sample.
    #[cfg(test)]
    pub(crate) fn record_fixture_result(
        &self,
        request: &RunRequest,
        raw_result: &str,
    ) -> Result<RunReport, PerfError> {
        let result = parse_scenario_result(request.scenario, raw_result).map_err(|source| {
            PerfError::InvalidScenarioResult {
                scenario: request.scenario,
                source,
            }
        })?;
        let context = RunContext::create(&self.workspace_root, request)?;

        let scenario_result_path = context.directory.join("scenario-result.json");
        fs::write(&scenario_result_path, raw_result).map_err(|source| {
            PerfError::WriteArtifact {
                path: scenario_result_path.clone(),
                source,
            }
        })?;

        let verdict = result_verdict(request, &result, u128::from(result.elapsed_us()));
        context.publish(
            u128::from(result.elapsed_us()),
            result.native_video_execution_identity(),
            verdict,
            vec![ArtifactFile {
                kind: ArtifactKind::ScenarioResult,
                path: PathBuf::from("scenario-result.json"),
            }],
        )
    }

    fn run_prepared_scenario(
        &self,
        request: &RunRequest,
        context: RunContext,
        mut profile: Option<&mut PerfCapture>,
    ) -> Result<RunReport, PerfError> {
        let mut files = Vec::new();
        let prepared = match self.prepare_scenario(request, &context.directory) {
            Ok(prepared) => prepared,
            Err(message) => {
                return context.infrastructure_failure(message, files);
            }
        };
        files.extend(prepared.input_artifacts());

        let frontend = frontend_command(request, &self.workspace_root, &prepared);
        let capture_route =
            crate::CaptureRoute::for_frontend(request.frontend(), prepared.uses_native_display());
        let mut counters = request
            .counters()
            .map(|scope| PerfStatCapture::new(&context.directory, scope, request.timeout()));
        let mut command = match profile.as_deref_mut() {
            Some(profile) => match profile.wrap(frontend, capture_route) {
                Ok(command) => command,
                Err(message) => return context.infrastructure_failure(message, files),
            },
            None => match counters.as_mut() {
                Some(counters) => match counters.wrap(frontend, capture_route) {
                    Ok(command) => command,
                    Err(message) => return context.infrastructure_failure(message, files),
                },
                None => frontend,
            },
        };
        let process_started = Instant::now();
        let execution = if profile.is_some() || counters.is_some() {
            group_output_with_timeout(&mut command, request.timeout)
        } else {
            output_with_timeout(&mut command, request.timeout)
        };
        let output = match execution {
            Ok(output) => output,
            Err(error) => {
                if let Some(profile) = profile.as_deref_mut() {
                    profile.cancel_gate();
                }
                if let Some(counters) = counters.as_mut() {
                    counters.cancel_gate();
                }
                let (message, output) = command_error_details(error, request.timeout);
                if let Some(output) = output {
                    files.extend(write_process_output(&context.directory, &output)?);
                }
                files.extend(frontend_artifacts_if_present(&prepared));
                return context.infrastructure_failure(message, files);
            }
        };
        let process_wall_us = process_started.elapsed().as_micros();
        files.extend(write_process_output(&context.directory, &output)?);
        files.extend(frontend_artifacts_if_present(&prepared));
        if let Some(profile) = profile {
            if let Err(message) = profile.finish_gate() {
                return context.infrastructure_failure(message, files);
            }
            match profile.collect() {
                Ok(profile_files) => files.extend(profile_files),
                Err(error) => {
                    files.extend(error.files);
                    return context.infrastructure_failure(error.message, files);
                }
            }
        }
        let counter_measurements = if let Some(counters) = counters.as_mut() {
            if let Err(message) = counters.finish_gate() {
                return context.infrastructure_failure(message, files);
            }
            match counters.collect() {
                Ok((measurements, artifact)) => {
                    files.push(artifact);
                    Some(measurements)
                }
                Err(message) => return context.infrastructure_failure(message, files),
            }
        } else {
            None
        };

        if !output.status.success() {
            return context.infrastructure_failure(
                format!(
                    "{} {} exited with status {}",
                    frontend_name(request.frontend()),
                    capture_route.process_role(),
                    output.status
                ),
                files,
            );
        }
        if let Err(message) = prepared.verify_inputs_unchanged() {
            return context.infrastructure_failure(message, files);
        }
        if let Err(message) = prepared.verify_external_journal_unchanged() {
            return context.infrastructure_failure(message, files);
        }
        if !prepared.sentinel.is_file() {
            return context.infrastructure_failure(
                "scenario process exited without publishing its completion sentinel".to_string(),
                files,
            );
        }
        let raw_result = match fs::read_to_string(&prepared.result) {
            Ok(result) => result,
            Err(error) => {
                return context.infrastructure_failure(
                    format!(
                        "completed scenario did not publish a readable result {}: {error}",
                        prepared.result.display()
                    ),
                    files,
                );
            }
        };
        files.push(ArtifactFile {
            kind: ArtifactKind::ScenarioResult,
            path: relative_artifact_path(&prepared.result),
        });
        let result = match parse_scenario_result(request.scenario, &raw_result) {
            Ok(result) => result,
            Err(error) => {
                return context.infrastructure_failure(
                    format!("scenario emitted invalid result JSON: {error}"),
                    files,
                );
            }
        };
        let native_video_execution = result.native_video_execution_identity();
        let mut verdict = result_verdict(request, &result, process_wall_us);
        if let (RunVerdict::Valid { measurements }, Some(counter_measurements)) =
            (&mut verdict, counter_measurements)
        {
            measurements.extend(counter_measurements);
        }
        context.publish(context.elapsed_us(), native_video_execution, verdict, files)
    }

    fn prepare_scenario(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        match request.scenario {
            ScenarioId::RustLspTyping => {
                scenarios::rust_lsp::prepare(&self.workspace_root, request, run_directory)
            }
            ScenarioId::MxTabCompletion => {
                scenarios::mx_tab::prepare(&self.workspace_root, request, run_directory)
            }
            ScenarioId::BytecodeCallLoop => {
                scenarios::bytecode::prepare(&self.workspace_root, request, run_directory)
            }
            ScenarioId::EditingSimulation
            | ScenarioId::Startup
            | ScenarioId::SustainedEditing
            | ScenarioId::GuiInputLatency
            | ScenarioId::OrgEditing
            | ScenarioId::MagitStatus
            | ScenarioId::LargeFileEditing
            | ScenarioId::Indentation
            | ScenarioId::RegexSearch => {
                scenarios::editor_workload::prepare(&self.workspace_root, request, run_directory)
            }
            ScenarioId::OrgJournalOpen => {
                scenarios::org_journal_open::prepare(&self.workspace_root, request, run_directory)
            }
            ScenarioId::SustainedNativeVideo => scenarios::sustained_native_video::prepare(
                &self.workspace_root,
                request,
                run_directory,
            ),
        }
    }
}

struct RunContext {
    request: RunRequest,
    run_id: String,
    started_unix_ms: u128,
    started: Instant,
    directory: PathBuf,
    host: HostProvenance,
}

impl RunContext {
    fn create(workspace_root: &Path, request: &RunRequest) -> Result<Self, PerfError> {
        Self::create_in(workspace_root, request, ArtifactNamespace::Benchmarks)
    }

    fn create_in(
        workspace_root: &Path,
        request: &RunRequest,
        namespace: ArtifactNamespace,
    ) -> Result<Self, PerfError> {
        let started = Instant::now();
        let started_unix_ms = unix_time_ms();
        let run_id = next_run_id(request.scenario, started_unix_ms);
        let directory = workspace_root.join(namespace.relative_root()).join(&run_id);
        fs::create_dir_all(&directory).map_err(|source| PerfError::CreateArtifactDirectory {
            path: directory.clone(),
            source,
        })?;
        Ok(Self {
            request: request.clone(),
            run_id,
            started_unix_ms,
            started,
            directory,
            host: collect_host_provenance(request.machine_policy()),
        })
    }

    fn elapsed_us(&self) -> u128 {
        self.started.elapsed().as_micros()
    }

    fn infrastructure_failure(
        &self,
        message: String,
        files: Vec<ArtifactFile>,
    ) -> Result<RunReport, PerfError> {
        self.publish(
            self.elapsed_us(),
            None,
            RunVerdict::InfrastructureFailure { message },
            files,
        )
    }

    fn publish(
        &self,
        total_elapsed_us: u128,
        native_video_execution: Option<NativeVideoExecutionIdentity>,
        verdict: RunVerdict,
        files: Vec<ArtifactFile>,
    ) -> Result<RunReport, PerfError> {
        let artifact = RunArtifact {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            scenario: self.request.scenario,
            frontend: self.request.frontend(),
            editor: self.request.editor.clone(),
            host: self.host.clone(),
            iterations: self.request.iterations.get(),
            started_unix_ms: self.started_unix_ms,
            total_elapsed_us,
            native_video_execution,
            verdict,
            files,
        };
        let artifact_path = self.directory.join("artifact.json");
        write_json_atomically(&artifact_path, &artifact)?;
        Ok(RunReport {
            artifact,
            artifact_path,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactNamespace {
    Benchmarks,
    Profiles,
}

impl ArtifactNamespace {
    const fn relative_root(self) -> &'static str {
        match self {
            Self::Benchmarks => "tmp/perf",
            Self::Profiles => "tmp/perf-profiles",
        }
    }
}

pub(crate) fn profile_verdict(run: &RunReport) -> ProfileVerdict {
    match &run.artifact.verdict {
        RunVerdict::CorrectnessMismatch { .. } => ProfileVerdict::Rejected {
            reason: ProfileRejection::CorrectnessMismatch,
        },
        RunVerdict::InfrastructureFailure { message } => ProfileVerdict::Rejected {
            reason: ProfileRejection::InfrastructureFailure {
                message: message.clone(),
            },
        },
        RunVerdict::Valid { .. } => {
            let Some(data) =
                artifact_path_of_kind(&run.artifact.files, ArtifactKind::NativeProfileData)
            else {
                return ProfileVerdict::Rejected {
                    reason: ProfileRejection::InfrastructureFailure {
                        message: "profile run did not retain perf.data".to_string(),
                    },
                };
            };
            let Some(report) =
                artifact_path_of_kind(&run.artifact.files, ArtifactKind::NativeProfileReport)
            else {
                return ProfileVerdict::Rejected {
                    reason: ProfileRejection::InfrastructureFailure {
                        message: "profile run did not retain its hotspot report".to_string(),
                    },
                };
            };
            let data_on_disk = run
                .artifact_path
                .parent()
                .expect("run artifact has a parent directory")
                .join(data);
            match perf_data_sample_count(&data_on_disk) {
                Ok(sample_count) => ProfileVerdict::Captured {
                    perf_data_path: data.to_path_buf(),
                    hotspot_report_path: report.to_path_buf(),
                    sample_count,
                },
                Err(message) => ProfileVerdict::Rejected {
                    reason: ProfileRejection::InfrastructureFailure { message },
                },
            }
        }
    }
}

fn artifact_path_of_kind(files: &[ArtifactFile], kind: ArtifactKind) -> Option<&Path> {
    files
        .iter()
        .find(|file| file.kind == kind)
        .map(|file| file.path.as_path())
}

pub(crate) fn perf_data_sample_count(path: &Path) -> Result<NonZeroU64, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("failed to open native profile {}: {error}", path.display()))?;
    let PerfFileReader {
        mut perf_file,
        mut record_iter,
    } = PerfFileReader::parse_file(BufReader::new(file))
        .map_err(|error| format!("failed to parse native profile {}: {error}", path.display()))?;
    let mut sample_count = 0_u64;
    while let Some(record) = record_iter
        .next_record(&mut perf_file)
        .map_err(|error| format!("failed to read native profile {}: {error}", path.display()))?
    {
        if matches!(
            record,
            PerfFileRecord::EventRecord { record, .. } if record.record_type == RecordType::SAMPLE
        ) {
            sample_count = sample_count
                .checked_add(1)
                .ok_or_else(|| "native profile sample count overflowed u64".to_string())?;
        }
    }
    NonZeroU64::new(sample_count)
        .ok_or_else(|| format!("native profile {} contained no samples", path.display()))
}

pub(crate) struct PerfCapture {
    configuration: PerfCaptureConfiguration,
    data: PathBuf,
    report: PathBuf,
    scope: ProfileScope,
    timeout: Duration,
    gate: Option<ProfileGate>,
}

impl PerfCapture {
    pub(crate) fn new(
        directory: &Path,
        configuration: PerfCaptureConfiguration,
        scope: ProfileScope,
        timeout: Duration,
    ) -> Self {
        Self {
            configuration,
            data: directory.join("perf.data"),
            report: directory.join("perf-report.txt"),
            scope,
            timeout,
            gate: None,
        }
    }

    pub(crate) fn wrap(
        &mut self,
        mut command: Command,
        route: crate::CaptureRoute,
    ) -> Result<Command, String> {
        if self.scope == ProfileScope::EditLoop {
            self.gate = Some(ProfileGate::start(
                self.data
                    .parent()
                    .expect("profile data has a parent directory"),
                self.timeout,
            )?);
        }
        if let crate::CaptureRoute::Adapter(prefix) = route {
            command.env(format!("{prefix}_PERF_RECORD"), &self.data);
            for (name, value) in self.configuration.adapter_record_environment(prefix) {
                command.env(name, value);
            }
            self.configure_gate_environment(&mut command, Some(prefix));
            return Ok(command);
        }

        let mut profiled = Command::new("perf");
        let control = self.gate.as_ref().map(|gate| {
            let paths = gate.control_paths();
            (paths.command.as_path(), paths.acknowledgement.as_path())
        });
        profiled.args(self.configuration.record_arguments(&self.data, control));
        profiled.arg(command.get_program());
        profiled.args(command.get_args());
        if let Some(directory) = command.get_current_dir() {
            profiled.current_dir(directory);
        }
        profiled.env_clear();
        for (name, value) in command.get_envs() {
            match value {
                Some(value) => {
                    profiled.env(name, value);
                }
                None => {
                    profiled.env_remove(name);
                }
            }
        }
        self.configure_gate_environment(&mut profiled, None);
        Ok(profiled)
    }

    fn configure_gate_environment(&self, command: &mut Command, adapter_prefix: Option<&str>) {
        let Some(gate) = &self.gate else {
            return;
        };
        command.env("NEOMACS_PERF_GATE_PORT", gate.endpoint().port().to_string());
        if let Some(prefix) = adapter_prefix {
            let paths = gate.control_paths();
            command.env(
                format!("{prefix}_PERF_CONTROL"),
                format!(
                    "fifo:{},{}",
                    paths.command.display(),
                    paths.acknowledgement.display()
                ),
            );
        }
    }

    fn finish_gate(&mut self) -> Result<(), String> {
        match &mut self.gate {
            Some(gate) => gate.finish(),
            None => Ok(()),
        }
    }

    fn cancel_gate(&mut self) {
        self.gate.take();
    }

    fn collect(&self) -> Result<Vec<ArtifactFile>, ProfileCaptureError> {
        if !self.data.is_file() {
            return Err(ProfileCaptureError {
                message: format!(
                    "perf record did not produce profile data {}",
                    self.data.display()
                ),
                files: Vec::new(),
            });
        }
        let mut files = vec![ArtifactFile {
            kind: ArtifactKind::NativeProfileData,
            path: relative_artifact_path(&self.data),
        }];
        let output = Command::new("perf")
            .args(PerfCaptureConfiguration::report_arguments(&self.data))
            .env("LC_ALL", "C")
            .env("PERF_PAGER", "cat")
            .output()
            .map_err(|error| ProfileCaptureError {
                message: format!("failed to launch perf report: {error}"),
                files: files.clone(),
            })?;
        let mut report = output.stdout;
        if !output.stderr.is_empty() {
            report.extend_from_slice(b"\n# perf report stderr\n");
            report.extend_from_slice(&output.stderr);
        }
        fs::write(&self.report, &report).map_err(|error| ProfileCaptureError {
            message: format!(
                "failed to write native hotspot report {}: {error}",
                self.report.display()
            ),
            files: files.clone(),
        })?;
        files.push(ArtifactFile {
            kind: ArtifactKind::NativeProfileReport,
            path: relative_artifact_path(&self.report),
        });
        if !output.status.success() {
            return Err(ProfileCaptureError {
                message: format!("perf report exited with status {}", output.status),
                files,
            });
        }
        perf_data_sample_count(&self.data).map_err(|message| ProfileCaptureError {
            message,
            files: files.clone(),
        })?;
        Ok(files)
    }
}

struct ProfileCaptureError {
    message: String,
    files: Vec<ArtifactFile>,
}

pub(crate) struct PreparedScenario {
    fixture: PathBuf,
    provenance: PathBuf,
    result: PathBuf,
    sentinel: PathBuf,
    terminal_bytes: PathBuf,
    gui_app_log: PathBuf,
    gui_weston_log: PathBuf,
    gui_runtime_directory: PathBuf,
    sandbox: MelpaSandbox,
    workload: PreparedWorkload,
}

enum PreparedWorkload {
    RustLspTyping {
        startup: PathBuf,
        source: PathBuf,
        replay: PathBuf,
        grammar_directory: PathBuf,
        grammar_libraries: Vec<PathBuf>,
        packages: Box<PreparedPackageSet>,
    },
    MxTabCompletion,
    BytecodeCallLoop,
    EditorWorkload {
        source: PathBuf,
        repository: Option<PathBuf>,
        startup: Option<PathBuf>,
        packages: Option<Box<PreparedPackageSet>>,
    },
    OrgJournalOpen {
        startup: PathBuf,
        journal_directory: PathBuf,
        external_journal: Option<ExternalJournalInput>,
        packages: Box<PreparedPackageSet>,
    },
    NativeVideo {
        video_file: PathBuf,
        video_file_sha256: String,
        video_file_size_bytes: u64,
        display_environment: BTreeMap<String, String>,
        presentation_target: NativeVideoPresentationTarget,
    },
}

impl PreparedScenario {
    fn verify_inputs_unchanged(&self) -> Result<(), String> {
        let PreparedWorkload::NativeVideo {
            video_file,
            video_file_sha256,
            video_file_size_bytes,
            ..
        } = &self.workload
        else {
            return Ok(());
        };
        let metadata = fs::metadata(video_file).map_err(|error| {
            format!(
                "failed to re-inspect native-video input {}: {error}",
                video_file.display()
            )
        })?;
        let observed_hash = sha256_file(video_file)?;
        if metadata.len() != *video_file_size_bytes || observed_hash != *video_file_sha256 {
            return Err(format!(
                "native-video input changed while the benchmark was running: {}",
                video_file.display()
            ));
        }
        Ok(())
    }

    /// Re-hash an external journal source after the run.  The scenario
    /// works on a copy inside the run directory; this proves the source
    /// file the user pointed at was never written.
    fn verify_external_journal_unchanged(&self) -> Result<(), String> {
        let PreparedWorkload::OrgJournalOpen {
            external_journal: Some(journal),
            ..
        } = &self.workload
        else {
            return Ok(());
        };
        let metadata = fs::metadata(&journal.path).map_err(|error| {
            format!(
                "failed to re-inspect journal input {}: {error}",
                journal.path.display()
            )
        })?;
        let observed_hash = sha256_file(&journal.path)?;
        if metadata.len() != journal.size_bytes || observed_hash != journal.sha256 {
            return Err(format!(
                "journal input changed while the benchmark was running: {}",
                journal.path.display()
            ));
        }
        Ok(())
    }

    fn input_artifacts(&self) -> Vec<ArtifactFile> {
        let mut artifacts = vec![
            ArtifactFile {
                kind: ArtifactKind::ScenarioFixture,
                path: relative_artifact_path(&self.fixture),
            },
            ArtifactFile {
                kind: ArtifactKind::InputProvenance,
                path: relative_artifact_path(&self.provenance),
            },
        ];
        if let PreparedWorkload::EditorWorkload {
            source, startup, ..
        } = &self.workload
        {
            artifacts.push(ArtifactFile {
                kind: ArtifactKind::SourceFixture,
                path: relative_artifact_path(source),
            });
            if let Some(startup) = startup {
                artifacts.push(ArtifactFile {
                    kind: ArtifactKind::PackageStartup,
                    path: relative_artifact_path(startup),
                });
            }
            return artifacts;
        }
        let PreparedWorkload::RustLspTyping {
            startup,
            source,
            replay,
            grammar_libraries,
            ..
        } = &self.workload
        else {
            if let PreparedWorkload::OrgJournalOpen {
                startup,
                journal_directory,
                ..
            } = &self.workload
            {
                artifacts.push(ArtifactFile {
                    kind: ArtifactKind::PackageStartup,
                    path: relative_artifact_path(startup),
                });
                if let Some(journal) = journal_file_in_directory(journal_directory) {
                    artifacts.push(ArtifactFile {
                        kind: ArtifactKind::SourceFixture,
                        path: relative_artifact_path(&journal),
                    });
                }
            }
            return artifacts;
        };
        artifacts.extend(
            [
                (ArtifactKind::PackageStartup, startup),
                (ArtifactKind::SourceFixture, source),
                (ArtifactKind::LspReplay, replay),
            ]
            .into_iter()
            .map(|(kind, path)| ArtifactFile {
                kind,
                path: relative_artifact_path(path),
            }),
        );
        artifacts.extend(grammar_libraries.iter().map(|path| {
            ArtifactFile {
                kind: ArtifactKind::TreeSitterGrammar,
                path: PathBuf::from("tree-sitter").join(
                    path.file_name()
                        .expect("copied grammar library has a file name"),
                ),
            }
        }));
        artifacts
    }

    fn add_workload_arguments(&self, command: &mut Command) {
        if let PreparedWorkload::RustLspTyping { startup, .. } = &self.workload {
            command.arg("--load").arg(startup);
        }
        if let PreparedWorkload::EditorWorkload {
            startup: Some(startup),
            ..
        } = &self.workload
        {
            command.arg("--load").arg(startup);
        }
        if let PreparedWorkload::OrgJournalOpen { startup, .. } = &self.workload {
            command.arg("--load").arg(startup);
        }
        command.arg("--load").arg(&self.fixture);
    }

    fn add_workload_environment(&self, command: &mut Command) {
        if matches!(&self.workload, PreparedWorkload::BytecodeCallLoop) {
            command.env("NEOVM_JIT", "0");
        }
        if let PreparedWorkload::RustLspTyping {
            source,
            replay,
            grammar_directory,
            packages,
            ..
        } = &self.workload
        {
            command
                .envs(packages.process_environment())
                .env("NEOMACS_PERF_SOURCE", source)
                .env("NEOMACS_PERF_LSP_REPLAY", replay)
                .env("NEOMACS_PERF_TREE_SITTER_DIR", grammar_directory);
        }
        if let PreparedWorkload::EditorWorkload {
            source,
            repository,
            packages,
            ..
        } = &self.workload
        {
            command.env("NEOMACS_PERF_SOURCE", source);
            if let Some(repository) = repository {
                command.env("NEOMACS_PERF_REPOSITORY", repository);
            }
            if let Some(packages) = packages {
                command.envs(packages.process_environment());
            }
        }
        if let PreparedWorkload::OrgJournalOpen {
            journal_directory,
            packages,
            ..
        } = &self.workload
        {
            command
                .envs(packages.process_environment())
                .env("NEOMACS_PERF_JOURNAL_DIR", journal_directory);
        }
        if let PreparedWorkload::NativeVideo {
            video_file,
            display_environment,
            presentation_target,
            ..
        } = &self.workload
        {
            command
                .envs(display_environment)
                .env("NEOMACS_PERF_VIDEO_FILE", video_file)
                .env(
                    "NEOMACS_PERF_VIDEO_WIDTH",
                    presentation_target.width().to_string(),
                )
                .env(
                    "NEOMACS_PERF_VIDEO_HEIGHT",
                    presentation_target.height().to_string(),
                )
                .env("NEOMACS_GPU_FRAME_TIMING", "1");
        }
    }

    const fn uses_native_display(&self) -> bool {
        matches!(&self.workload, PreparedWorkload::NativeVideo { .. })
    }
}

fn prepare_gui_runtime_directory(workspace_root: &Path) -> Result<PathBuf, String> {
    let directory = workspace_root
        .join("tmp/gui-runtime")
        .join(std::process::id().to_string());
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "failed to create short GUI runtime directory {}: {error}",
            directory.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!(
                "failed to secure GUI runtime directory {}: {error}",
                directory.display()
            )
        })?;
    }
    Ok(directory)
}

fn frontend_command(
    request: &RunRequest,
    workspace_root: &Path,
    prepared: &PreparedScenario,
) -> Command {
    let frontend = request.frontend();
    let mut command = match frontend {
        Frontend::Batch => {
            let mut command = if let Some(cpu) = request.machine_policy().cpu {
                let mut command = Command::new("taskset");
                command.arg("-c").arg(cpu.to_string()).arg(request.editor());
                command
            } else {
                Command::new(request.editor())
            };
            command.arg("--batch").arg("-Q");
            command
        }
        Frontend::Tui { .. } => {
            let mut command = Command::new("python3");
            command
                .arg(workspace_root.join("tools/bench/pty-run.py"))
                .arg(request.editor())
                .arg("-nw")
                .arg("-Q");
            command
        }
        Frontend::Gui { .. } if prepared.uses_native_display() => {
            let mut command = if let Some(cpu) = request.machine_policy().cpu {
                let mut command = Command::new("taskset");
                command.arg("-c").arg(cpu.to_string()).arg(request.editor());
                command
            } else {
                Command::new(request.editor())
            };
            // Maximization is asynchronous on real window managers.  The
            // fixture waits for and validates the resulting body dimensions
            // before inserting the requested video presentation.
            command.arg("-Q").arg("--maximized");
            command
        }
        Frontend::Gui { .. } => {
            let mut command = Command::new(workspace_root.join("tools/bench/gui-run.sh"));
            command.arg(request.editor()).arg("-Q");
            command
        }
    };
    configure_benchmark_environment(&mut command, &prepared.sandbox);
    match frontend {
        Frontend::Batch => {}
        Frontend::Tui { rows, columns } => {
            command
                .env("PTY_ROWS", rows.to_string())
                .env("PTY_COLS", columns.to_string())
                .env("PTY_TIMEOUT", adapter_timeout(request.timeout()))
                .env("PTY_OUTPUT", &prepared.terminal_bytes)
                // The package sandbox deliberately defaults to TERM=dumb for
                // batch tests. A real PTY owns its display capabilities.
                .env("TERM", "screen-256color");
            if let Some(cpu) = request.machine_policy().cpu {
                command.env("PTY_CPU", cpu.to_string());
            }
        }
        Frontend::Gui { .. } if prepared.uses_native_display() => {}
        Frontend::Gui { width, height } => {
            command
                .env("GUI_WIDTH", width.to_string())
                .env("GUI_HEIGHT", height.to_string())
                .env("GUI_TIMEOUT", adapter_timeout(request.timeout()))
                .env("GUI_APP_LOG", &prepared.gui_app_log)
                .env("GUI_WESTON_LOG", &prepared.gui_weston_log)
                .env("XDG_RUNTIME_DIR", &prepared.gui_runtime_directory);
            if let Some(cpu) = request.machine_policy().cpu {
                command.env("GUI_CPU", cpu.to_string());
            }
        }
    }
    prepared.add_workload_arguments(&mut command);
    command.current_dir(workspace_root);
    command
        .env_remove("EMACSLOADPATH")
        .env("SENTINEL", &prepared.sentinel)
        .env("NEOMACS_PERF_RESULT", &prepared.result)
        .env("NEOMACS_PERF_WORKLOAD", request.scenario.as_str())
        .env(
            "NEOMACS_PERF_ITERATIONS",
            request.iterations().get().to_string(),
        );
    prepared.add_workload_environment(&mut command);
    command
}

fn adapter_timeout(outer_timeout: Duration) -> String {
    let grace = Duration::from_millis(500).min(outer_timeout / 10);
    format!("{:.3}", outer_timeout.saturating_sub(grace).as_secs_f64())
}

const BENCHMARK_PASSTHROUGH_ENVIRONMENT: &[&str] = &[
    "PATH",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "GST_PLUGIN_SCANNER_1_0",
    "SYSTEMROOT",
    "WINDIR",
];

pub(crate) fn configure_benchmark_environment(command: &mut Command, sandbox: &MelpaSandbox) {
    command.env_clear();
    command.envs(benchmark_passthrough_environment());
    command.envs(sandbox.process_environment());
}

/// Operator-set JIT diagnostic knobs (`NEOVM_JIT_PROFILE`, `NEOVM_JIT_THRESHOLD`,
/// `NEOVM_JIT_COMPILE_STATS`, ...) reach the editor too: a census of what the
/// JIT compiles or rejects under a real scenario needs them, and an unset knob
/// forwards nothing, so a plain benchmark run is unchanged.
const BENCHMARK_PASSTHROUGH_PREFIX: &str = "NEOVM_JIT_";

pub(crate) fn benchmark_passthrough_environment() -> Vec<(String, std::ffi::OsString)> {
    passthrough_from(std::env::vars_os())
}

/// [`benchmark_passthrough_environment`] over an explicit environment (testable).
pub(crate) fn passthrough_from(
    vars: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(String, std::ffi::OsString)> {
    vars.into_iter()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let forwarded = BENCHMARK_PASSTHROUGH_ENVIRONMENT.contains(&name.as_str())
                || name.starts_with(BENCHMARK_PASSTHROUGH_PREFIX);
            forwarded.then_some((name, value))
        })
        .collect()
}

pub(crate) fn collect_editor_provenance(
    editor: &Path,
    sandbox: &MelpaSandbox,
) -> Result<EditorProvenance, String> {
    let metadata = fs::metadata(editor)
        .map_err(|error| format!("failed to inspect editor {}: {error}", editor.display()))?;
    let canonical_path = fs::canonicalize(editor).map_err(|error| {
        format!(
            "failed to resolve editor executable {}: {error}",
            editor.display()
        )
    })?;
    let version = editor_identity_value(editor, "--version", sandbox)?;
    let capabilities = editor_capabilities(editor, sandbox)?;
    let lowercase_version = version.to_ascii_lowercase();
    let kind = if lowercase_version.contains("neomacs") {
        EditorKind::Neomacs
    } else if lowercase_version.contains("gnu emacs") {
        EditorKind::GnuEmacs
    } else {
        EditorKind::Unknown
    };
    Ok(EditorProvenance {
        path: canonical_path.to_string_lossy().into_owned(),
        executable_sha256: sha256_file(editor)?,
        executable_size_bytes: metadata.len(),
        pdump_fingerprint: editor_identity_value(editor, "--fingerprint", sandbox)?,
        version,
        kind,
        capabilities,
    })
}

fn editor_capabilities(
    editor: &Path,
    sandbox: &MelpaSandbox,
) -> Result<EditorCapabilities, String> {
    let mut command = Command::new(editor);
    configure_benchmark_environment(&mut command, sandbox);
    command.args([
        "--batch",
        "-Q",
        "--eval",
        r#"(princ (format "%d,%d,%d,%d,%d,%d" (if (and (fboundp 'native-comp-available-p) (native-comp-available-p)) 1 0) (if (and (fboundp 'treesit-available-p) (treesit-available-p)) 1 0) (if (fboundp 'module-load) 1 0) (if (fboundp 'neomacs-video-load) 1 0) (if (fboundp 'neomacs-webkit-create) 1 0) (if (fboundp 'neomacs-terminal-create) 1 0)))"#,
    ]);
    let output = command
        .output()
        .map_err(|error| format!("failed to query editor capabilities: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "editor capability query exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|error| format!("editor capability output was not UTF-8: {error}"))?;
    let flags = value.trim().split(',').collect::<Vec<_>>();
    let [
        native_compilation,
        tree_sitter,
        dynamic_modules,
        video_playback,
        webview,
        embedded_terminal,
    ] = flags.as_slice()
    else {
        return Err(format!(
            "unexpected editor capability output {:?}",
            value.trim()
        ));
    };
    Ok(EditorCapabilities {
        native_compilation: *native_compilation == "1",
        tree_sitter: *tree_sitter == "1",
        dynamic_modules: *dynamic_modules == "1",
        video_playback: *video_playback == "1",
        webview: *webview == "1",
        embedded_terminal: *embedded_terminal == "1",
    })
}

fn editor_identity_value(
    editor: &Path,
    argument: &str,
    sandbox: &MelpaSandbox,
) -> Result<String, String> {
    let mut command = Command::new(editor);
    configure_benchmark_environment(&mut command, sandbox);
    command.arg(argument);
    let output = command.output().map_err(|error| {
        format!(
            "failed to query editor {} with {argument}: {error}",
            editor.display()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "editor {} {argument} exited with {}: {}",
            editor.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|error| format!("editor {argument} output was not UTF-8: {error}"))?;
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "editor {} {argument} returned an empty identity",
            editor.display()
        ));
    }
    Ok(value.to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(hex)
}

fn command_error_details(error: CommandError, timeout: Duration) -> (String, Option<Output>) {
    match error {
        CommandError::Launch(error) => {
            (format!("failed to launch frontend adapter: {error}"), None)
        }
        CommandError::TimedOut(output) => (
            format!("frontend adapter timed out after {timeout:?}"),
            Some(output),
        ),
        CommandError::Capture(error) => (
            format!("failed to capture frontend adapter output: {error}"),
            None,
        ),
    }
}

fn write_process_output(
    run_directory: &Path,
    output: &Output,
) -> Result<Vec<ArtifactFile>, PerfError> {
    let outputs = [
        (ArtifactKind::Stdout, "stdout.log", output.stdout.as_slice()),
        (ArtifactKind::Stderr, "stderr.log", output.stderr.as_slice()),
    ];
    let mut files = Vec::with_capacity(outputs.len());
    for (kind, name, bytes) in outputs {
        let path = run_directory.join(name);
        fs::write(&path, bytes).map_err(|source| PerfError::WriteArtifact {
            path: path.clone(),
            source,
        })?;
        files.push(ArtifactFile {
            kind,
            path: PathBuf::from(name),
        });
    }
    Ok(files)
}

fn frontend_artifacts_if_present(prepared: &PreparedScenario) -> Vec<ArtifactFile> {
    [
        (ArtifactKind::TerminalByteStream, &prepared.terminal_bytes),
        (ArtifactKind::FrontendLog, &prepared.gui_app_log),
        (ArtifactKind::CompositorLog, &prepared.gui_weston_log),
    ]
    .into_iter()
    .filter(|(_, path)| path.is_file())
    .map(|(kind, path)| ArtifactFile {
        kind,
        path: relative_artifact_path(path),
    })
    .collect()
}

fn relative_artifact_path(path: &Path) -> PathBuf {
    path.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf())
}

fn frontend_name(frontend: Frontend) -> &'static str {
    match frontend {
        Frontend::Batch => "batch",
        Frontend::Tui { .. } => "TUI",
        Frontend::Gui { .. } => "GUI",
    }
}

enum ScenarioResult {
    RustLspTyping(scenarios::rust_lsp::RustLspTypingResult),
    MxTabCompletion(scenarios::mx_tab::MxTabCompletionResult),
    BytecodeCallLoop(scenarios::bytecode::BytecodeCallLoopResult),
    EditorWorkload(scenarios::editor_workload::EditorWorkloadResult),
    OrgJournalOpen(scenarios::org_journal_open::OrgJournalOpenResult),
    SustainedNativeVideo(scenarios::sustained_native_video::SustainedNativeVideoResult),
}

impl ScenarioResult {
    fn native_video_execution_identity(&self) -> Option<NativeVideoExecutionIdentity> {
        match self {
            Self::SustainedNativeVideo(result) => Some(result.execution_identity()),
            _ => None,
        }
    }

    #[cfg(test)]
    const fn elapsed_us(&self) -> u64 {
        match self {
            Self::RustLspTyping(result) => result.elapsed_us,
            Self::MxTabCompletion(result) => result.elapsed_us,
            Self::BytecodeCallLoop(result) => result.elapsed_us,
            Self::EditorWorkload(result) => result.elapsed_us,
            Self::OrgJournalOpen(result) => result.elapsed_us,
            Self::SustainedNativeVideo(result) => result.elapsed_cpu_us,
        }
    }
}

fn parse_scenario_result(
    scenario: ScenarioId,
    raw: &str,
) -> Result<ScenarioResult, serde_json::Error> {
    match scenario {
        ScenarioId::RustLspTyping => serde_json::from_str(raw).map(ScenarioResult::RustLspTyping),
        ScenarioId::MxTabCompletion => {
            serde_json::from_str(raw).map(ScenarioResult::MxTabCompletion)
        }
        ScenarioId::BytecodeCallLoop => {
            serde_json::from_str(raw).map(ScenarioResult::BytecodeCallLoop)
        }
        ScenarioId::EditingSimulation
        | ScenarioId::Startup
        | ScenarioId::SustainedEditing
        | ScenarioId::GuiInputLatency
        | ScenarioId::OrgEditing
        | ScenarioId::MagitStatus
        | ScenarioId::LargeFileEditing
        | ScenarioId::Indentation
        | ScenarioId::RegexSearch => serde_json::from_str(raw).map(ScenarioResult::EditorWorkload),
        ScenarioId::OrgJournalOpen => serde_json::from_str(raw).map(ScenarioResult::OrgJournalOpen),
        ScenarioId::SustainedNativeVideo => {
            serde_json::from_str(raw).map(ScenarioResult::SustainedNativeVideo)
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ScenarioStatus {
    Ok,
    Error,
}

#[derive(Debug, Eq, PartialEq)]
enum ScenarioOutcome {
    Ok,
    Error(String),
}

impl std::fmt::Display for ScenarioOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => formatter.write_str("ok"),
            Self::Error(message) => write!(formatter, "error: {message}"),
        }
    }
}

fn scenario_outcome(
    status: ScenarioStatus,
    error: Option<String>,
) -> Result<ScenarioOutcome, String> {
    match (status, error) {
        (ScenarioStatus::Ok, None) => Ok(ScenarioOutcome::Ok),
        (ScenarioStatus::Ok, Some(_)) => Err("status `ok` requires a null error".to_string()),
        (ScenarioStatus::Error, Some(message)) if !message.trim().is_empty() => {
            Ok(ScenarioOutcome::Error(message))
        }
        (ScenarioStatus::Error, Some(_)) => {
            Err("status `error` requires a non-empty error".to_string())
        }
        (ScenarioStatus::Error, None) => {
            Err("status `error` requires a non-null error".to_string())
        }
    }
}

#[derive(Serialize)]
struct HarnessProvenance {
    /// Revision embedded when this harness executable was compiled.
    revision: String,
    checkout_revision: String,
    source_tree_dirty: bool,
    harness_inputs_dirty_when_built: bool,
    executable_sha256: String,
    executable_size_bytes: u64,
    invocation: Vec<String>,
}

fn collect_harness_provenance(workspace_root: &Path) -> Result<HarnessProvenance, String> {
    fn git_stdout(workspace_root: &Path, arguments: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(workspace_root)
            .output()
            .map_err(|error| format!("failed to run git for benchmark provenance: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git {:?} failed while collecting benchmark provenance: {}",
                arguments,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_owned())
            .map_err(|error| format!("git benchmark provenance was not UTF-8: {error}"))
    }

    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to identify benchmark harness executable: {error}"))?;
    let executable_metadata = fs::metadata(&executable).map_err(|error| {
        format!(
            "failed to inspect benchmark harness executable {}: {error}",
            executable.display()
        )
    })?;

    let checkout_revision = git_stdout(workspace_root, &["rev-parse", "HEAD"])?;
    let embedded_revision = option_env!("NEOMACS_PERF_GIT_SHA")
        .filter(|revision| !revision.is_empty() && *revision != "unknown")
        .ok_or_else(|| {
            "sustained native-video acceptance requires a harness with embedded Git provenance"
                .to_owned()
        })?;
    let harness_inputs_dirty_when_built = match option_env!("NEOMACS_PERF_INPUTS_DIRTY") {
        Some("true") => true,
        Some("false") => false,
        Some(value) => {
            return Err(format!(
                "benchmark harness contains invalid build-time dirty marker {value:?}"
            ));
        }
        None => {
            return Err(
                "sustained native-video acceptance requires build-time source provenance"
                    .to_owned(),
            );
        }
    };
    validate_harness_build(
        embedded_revision,
        &checkout_revision,
        harness_inputs_dirty_when_built,
    )?;

    Ok(HarnessProvenance {
        revision: embedded_revision.to_owned(),
        checkout_revision,
        source_tree_dirty: !git_stdout(
            workspace_root,
            &["status", "--porcelain", "--untracked-files=no"],
        )?
        .is_empty(),
        harness_inputs_dirty_when_built,
        executable_sha256: sha256_file(&executable)?,
        executable_size_bytes: executable_metadata.len(),
        invocation: std::env::args_os()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
    })
}

pub(crate) fn validate_harness_revision(embedded: &str, checkout: &str) -> Result<(), String> {
    if embedded == checkout {
        return Ok(());
    }
    Err(format!(
        "benchmark harness was built from {embedded}, but the checkout is {checkout}; rebuild the harness before collecting acceptance evidence"
    ))
}

pub(crate) fn validate_harness_build(
    embedded: &str,
    checkout: &str,
    harness_inputs_dirty_when_built: bool,
) -> Result<(), String> {
    validate_harness_revision(embedded, checkout)?;
    if harness_inputs_dirty_when_built {
        return Err(
            "benchmark harness was built from dirty tracked harness inputs; restore clean inputs and rebuild the harness before collecting acceptance evidence"
                .to_owned(),
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct PackageProvenance<'a> {
    name: &'a str,
    version: &'a str,
    repository: &'a str,
    revision: &'a str,
    upstream_repository: &'a str,
    upstream_revision: &'a str,
}

#[derive(Serialize)]
struct GrammarProvenance<'a> {
    language: &'a str,
    repository: &'a str,
    revision: &'a str,
}

fn deserialize_optional_error<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

fn require_positive_phase(mismatches: &mut Vec<CorrectnessMismatch>, invariant: &str, actual: u64) {
    if actual == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: invariant.to_string(),
            expected: "positive".to_string(),
            actual: actual.to_string(),
        });
    }
}

fn result_verdict(
    request: &RunRequest,
    result: &ScenarioResult,
    process_wall_us: u128,
) -> RunVerdict {
    let mismatches = match result {
        ScenarioResult::RustLspTyping(result) => {
            scenarios::rust_lsp::validate_rust_lsp_typing_result(request, result)
        }
        ScenarioResult::MxTabCompletion(result) => {
            scenarios::mx_tab::validate_mx_tab_completion_result(request, result)
        }
        ScenarioResult::BytecodeCallLoop(result) => {
            scenarios::bytecode::validate_bytecode_call_loop_result(request, result)
        }
        ScenarioResult::EditorWorkload(result) => {
            scenarios::editor_workload::validate_editor_workload_result(request, result)
        }
        ScenarioResult::OrgJournalOpen(result) => {
            scenarios::org_journal_open::validate_org_journal_open_result(request, result)
        }
        ScenarioResult::SustainedNativeVideo(result) => {
            scenarios::sustained_native_video::validate_sustained_native_video_result(
                request, result,
            )
        }
    };
    if mismatches.is_empty() {
        RunVerdict::Valid {
            measurements: valid_measurements(result, process_wall_us),
        }
    } else {
        RunVerdict::CorrectnessMismatch { mismatches }
    }
}

fn mismatch<T>(mismatches: &mut Vec<CorrectnessMismatch>, invariant: &str, expected: T, actual: T)
where
    T: PartialEq + std::fmt::Display,
{
    if expected != actual {
        mismatches.push(CorrectnessMismatch {
            invariant: invariant.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }
}

fn valid_measurements(result: &ScenarioResult, wall_elapsed_us: u128) -> Vec<Measurement> {
    match result {
        ScenarioResult::RustLspTyping(result) => {
            scenarios::rust_lsp::valid_rust_lsp_typing_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::MxTabCompletion(result) => {
            scenarios::mx_tab::valid_mx_tab_completion_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::BytecodeCallLoop(result) => {
            scenarios::bytecode::valid_bytecode_call_loop_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::EditorWorkload(result) => {
            scenarios::editor_workload::valid_editor_workload_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::OrgJournalOpen(result) => {
            scenarios::org_journal_open::valid_org_journal_open_measurements(
                result,
                wall_elapsed_us,
            )
        }
        ScenarioResult::SustainedNativeVideo(result) => {
            scenarios::sustained_native_video::valid_sustained_native_video_measurements(
                result,
                wall_elapsed_us,
            )
        }
    }
}

/// The per-keystroke latency budget behind
/// [`MetricName::InputLatencyOverBudgetCount`]: one millisecond, the point at
/// which a keystroke stops feeling immediate and past which every GNU sample
/// in this scenario but a handful lies.
const INPUT_LATENCY_BUDGET_US: u64 = 1_000;

fn nearest_rank(sorted_samples: &[u64], percentile: f64) -> u64 {
    let rank = (percentile * sorted_samples.len() as f64).ceil() as usize;
    sorted_samples[rank.saturating_sub(1).min(sorted_samples.len() - 1)]
}

fn next_run_id(scenario: ScenarioId, unix_ms: u128) -> String {
    let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{unix_ms}-{}-{sequence}",
        scenario.as_str(),
        std::process::id()
    )
}
