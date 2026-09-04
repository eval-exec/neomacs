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
    CommandError, EmacsRuntime, MelpaSandbox, PreparedPackageSet, group_output_with_timeout,
    locked_melpa_sources, output_with_timeout, prepare_cached_tree_sitter_grammar,
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
    native_video::{
        NativeVideoBuildProfile, NativeVideoComparisonIdentity, NativeVideoDecoderKind,
        NativeVideoExecutionIdentity, NativeVideoFrameFormat, NativeVideoGpuTimingStatus,
        NativeVideoGraphicsBackend, NativeVideoPresentationTarget, discover_media_metadata,
    },
    profile_gate::ProfileGate,
    scenario,
};

pub(crate) const ARTIFACT_SCHEMA_VERSION: u32 = 5;
const SCENARIO_RESULT_SCHEMA_VERSION: u32 = 1;
const NATIVE_VIDEO_RESULT_SCHEMA_VERSION: u32 = 4;
const MX_TAB_COMPLETION_CANDIDATE_COUNT: u64 = 1024;
const RUST_LSP_TYPING_OVERLAY_COUNT: u64 = 4;
const RUST_LSP_TYPING_DIAGNOSTIC_COUNT: u64 = 4;
const RUST_GRAMMAR_REPOSITORY: &str = "https://github.com/tree-sitter/tree-sitter-rust";
const RUST_GRAMMAR_REVISION: &str = "18b0515fca567f5a10aee9978c6d2640e878671a";
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
            .with_video_file(request.video_file.clone());
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
            ScenarioId::RustLspTyping => self.prepare_rust_lsp_typing(request, run_directory),
            ScenarioId::MxTabCompletion => self.prepare_mx_tab_completion(request, run_directory),
            ScenarioId::BytecodeCallLoop => self.prepare_bytecode_call_loop(request, run_directory),
            ScenarioId::EditingSimulation
            | ScenarioId::Startup
            | ScenarioId::SustainedEditing
            | ScenarioId::GuiInputLatency
            | ScenarioId::OrgEditing
            | ScenarioId::MagitStatus
            | ScenarioId::LargeFileEditing
            | ScenarioId::Indentation
            | ScenarioId::RegexSearch => self.prepare_editor_workload(request, run_directory),
            ScenarioId::SustainedNativeVideo => {
                self.prepare_sustained_native_video(request, run_directory)
            }
        }
    }

    fn prepare_editor_workload(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
        let editor = collect_editor_provenance(request.editor(), &sandbox)?;
        let fixture_source = self
            .workspace_root
            .join("crates/neomacs-perf/fixtures/editor-workloads.el");
        let source_fixture = self.workspace_root.join("lisp/emacs-lisp/bytecomp.el");
        for required in [&fixture_source, &source_fixture] {
            if !required.is_file() {
                return Err(format!(
                    "missing committed performance fixture {}",
                    required.display()
                ));
            }
        }
        let fixture = run_directory.join("editor-workloads.el");
        fs::copy(&fixture_source, &fixture).map_err(|error| {
            format!(
                "failed to copy performance fixture {} to {}: {error}",
                fixture_source.display(),
                fixture.display()
            )
        })?;
        let source = run_directory.join("workload-source.el");
        if request.scenario == ScenarioId::LargeFileEditing {
            let seed = fs::read_to_string(&source_fixture)
                .map_err(|error| format!("failed to read {}: {error}", source_fixture.display()))?;
            let mut large = String::with_capacity(seed.len() * 8);
            for _ in 0..8 {
                large.push_str(&seed);
            }
            fs::write(&source, large).map_err(|error| {
                format!(
                    "failed to write large-file fixture {}: {error}",
                    source.display()
                )
            })?;
        } else {
            fs::copy(&source_fixture, &source).map_err(|error| {
                format!(
                    "failed to copy source fixture {} to {}: {error}",
                    source_fixture.display(),
                    source.display()
                )
            })?;
        }

        let mut package_provenance = None;
        let mut startup = None;
        let mut packages = None;
        let repository = if request.scenario == ScenarioId::MagitStatus {
            let magit_source = locked_melpa_sources()?
                .into_iter()
                .find(|source| source.package().0 == "magit")
                .ok_or_else(|| "the MELPA source lock does not contain magit".to_string())?;
            let package = magit_source.package();
            let prepared = PreparedPackageSet::from_locked_melpa(
                &EmacsRuntime::gnu_emacs(),
                package,
                "magit.el",
            )?;
            startup = Some(prepared.write_startup_file(run_directory)?);
            package_provenance = Some(PackageProvenance {
                name: package.0,
                version: package.1,
                repository: magit_source.repository(),
                revision: magit_source.revision(),
                upstream_repository: magit_source.upstream_repository(),
                upstream_revision: magit_source.upstream_revision(),
            });
            packages = Some(Box::new(prepared));
            Some(prepare_magit_repository(run_directory)?)
        } else {
            None
        };

        let provenance = run_directory.join("input-provenance.json");
        let provenance_manifest = EditorWorkloadInputProvenanceManifest {
            editor,
            host: collect_host_provenance(request.machine_policy()),
            scenario: request.scenario,
            workload_fixture_sha256: sha256_file(&fixture_source)?,
            source_fixture_sha256: sha256_file(&source)?,
            package: package_provenance,
            environment_policy: "closed-v1",
            passthrough_environment: benchmark_passthrough_environment()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
                .collect(),
        };
        let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
            .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
        fs::write(&provenance, provenance_json).map_err(|error| {
            format!(
                "failed to write input provenance {}: {error}",
                provenance.display()
            )
        })?;

        Ok(PreparedScenario {
            fixture,
            provenance,
            result: run_directory.join("scenario-result.json"),
            sentinel: run_directory.join("completed"),
            terminal_bytes: run_directory.join("terminal.ansi"),
            gui_app_log: run_directory.join("gui-app.log"),
            gui_weston_log: run_directory.join("weston.log"),
            gui_runtime_directory: prepare_gui_runtime_directory(&self.workspace_root)?,
            sandbox,
            workload: PreparedWorkload::EditorWorkload {
                source,
                repository,
                startup,
                packages,
            },
        })
    }

    fn prepare_sustained_native_video(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        if !cfg!(target_os = "linux") {
            return Err(
                "sustained native-video performance is currently a Linux acceptance scenario"
                    .to_string(),
            );
        }
        let presentation_target = NativeVideoPresentationTarget::from_frontend(request.frontend())
            .map_err(|error| error.to_string())?;
        let video_file = request.video_file().ok_or_else(|| {
            "sustained-native-video requires --video-file pointing to a readable video".to_string()
        })?;
        if !video_file.is_file() {
            return Err(format!(
                "native-video input is not a file: {}",
                video_file.display()
            ));
        }
        let video_file = fs::canonicalize(video_file).map_err(|error| {
            format!(
                "failed to resolve native-video input {}: {error}",
                video_file.display()
            )
        })?;
        let media = discover_media_metadata(&video_file)?;
        media.validate_4k60()?;
        let display_environment: BTreeMap<String, String> = [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "XAUTHORITY",
        ]
        .into_iter()
        .filter_map(|name| {
            std::env::var_os(name)
                .map(|value| (name.to_string(), value.to_string_lossy().into_owned()))
        })
        .collect();
        let gstreamer_environment: BTreeMap<String, String> =
            ["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SCANNER_1_0"]
                .into_iter()
                .filter_map(|name| {
                    std::env::var_os(name)
                        .map(|value| (name.to_string(), value.to_string_lossy().into_owned()))
                })
                .collect();
        if !display_environment.contains_key("DISPLAY")
            && !display_environment.contains_key("WAYLAND_DISPLAY")
        {
            return Err(
                "sustained native-video performance requires the caller's graphical session"
                    .to_string(),
            );
        }

        let sandbox = MelpaSandbox::new("perf-sustained-native-video")?;
        let editor = collect_editor_provenance(request.editor(), &sandbox)?;
        if editor.kind != EditorKind::Neomacs {
            return Err("sustained native-video performance requires a Neomacs executable".into());
        }
        if !editor.capabilities.video_playback {
            return Err("Neomacs was built without native video playback support".into());
        }
        let build_profile = NativeVideoBuildProfile::from_version(&editor.version)?;
        let fixture_source = self
            .workspace_root
            .join("crates/neomacs-perf/fixtures/sustained-native-video.el");
        if !fixture_source.is_file() {
            return Err(format!(
                "missing committed performance fixture {}",
                fixture_source.display()
            ));
        }
        let fixture = run_directory.join("sustained-native-video.el");
        fs::copy(&fixture_source, &fixture).map_err(|error| {
            format!(
                "failed to copy performance fixture {} to {}: {error}",
                fixture_source.display(),
                fixture.display()
            )
        })?;
        let metadata = fs::metadata(&video_file).map_err(|error| {
            format!(
                "failed to inspect native-video input {}: {error}",
                video_file.display()
            )
        })?;
        let video_file_sha256 = sha256_file(&video_file)?;
        let harness = collect_harness_provenance(&self.workspace_root)?;
        if harness.source_tree_dirty {
            return Err(
                "sustained native-video acceptance requires a clean tracked source tree".to_owned(),
            );
        }
        let comparison_identity = NativeVideoComparisonIdentity {
            workload_fixture_sha256: sha256_file(&fixture_source)?,
            video_file_sha256: video_file_sha256.clone(),
            video_file_size_bytes: metadata.len(),
            media,
            presentation_width_pixels: presentation_target.width(),
            presentation_height_pixels: presentation_target.height(),
            display_environment: display_environment.clone(),
            gstreamer_environment,
            gpu_frame_timing: "requested".to_owned(),
        };
        let provenance = run_directory.join("input-provenance.json");
        let provenance_manifest = NativeVideoInputProvenanceManifest {
            editor,
            editor_build_profile: build_profile,
            host: collect_host_provenance(request.machine_policy()),
            harness,
            video_file: video_file.to_string_lossy().into_owned(),
            comparison_identity,
        };
        let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
            .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
        fs::write(&provenance, provenance_json).map_err(|error| {
            format!(
                "failed to write input provenance {}: {error}",
                provenance.display()
            )
        })?;

        Ok(PreparedScenario {
            fixture,
            provenance,
            result: run_directory.join("scenario-result.json"),
            sentinel: run_directory.join("completed"),
            terminal_bytes: run_directory.join("terminal.ansi"),
            gui_app_log: run_directory.join("gui-app.log"),
            gui_weston_log: run_directory.join("weston.log"),
            gui_runtime_directory: prepare_gui_runtime_directory(&self.workspace_root)?,
            sandbox,
            workload: PreparedWorkload::NativeVideo {
                video_file,
                video_file_sha256,
                video_file_size_bytes: metadata.len(),
                display_environment,
                presentation_target,
            },
        })
    }

    fn prepare_rust_lsp_typing(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        let lsp_mode_source = locked_melpa_sources()?
            .into_iter()
            .find(|source| source.package().0 == "lsp-mode")
            .ok_or_else(|| "the MELPA source lock does not contain lsp-mode".to_string())?;
        let lsp_mode = lsp_mode_source.package();
        let packages = PreparedPackageSet::from_locked_melpa(
            &EmacsRuntime::gnu_emacs(),
            lsp_mode,
            "lsp-mode.el",
        )?;
        let cached_grammar = prepare_cached_tree_sitter_grammar(
            &EmacsRuntime::gnu_emacs(),
            "rust",
            RUST_GRAMMAR_REPOSITORY,
            RUST_GRAMMAR_REVISION,
        )?;
        let grammar_directory = run_directory.join("tree-sitter");
        let grammar_libraries =
            copy_grammar_libraries(&cached_grammar, &grammar_directory, "tree-sitter-rust")?;
        let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
        let editor = collect_editor_provenance(request.editor(), &sandbox)?;
        let startup = packages.write_startup_file(run_directory)?;
        let fixture_root = self.workspace_root.join("crates/neomacs-perf/fixtures");
        let fixture_source = fixture_root.join("rust-lsp-typing.el");
        let source_source = fixture_root.join("rust-lsp-typing.rs");
        let replay_source = fixture_root.join("rust-lsp-diagnostics.json");
        for required in [&fixture_source, &source_source, &replay_source] {
            if !required.is_file() {
                return Err(format!(
                    "missing committed performance fixture {}",
                    required.display()
                ));
            }
        }
        let fixture = run_directory.join("rust-lsp-typing.el");
        let source = run_directory.join("rust-lsp-typing.rs");
        let replay = run_directory.join("rust-lsp-diagnostics.json");
        for (input, output) in [
            (&fixture_source, &fixture),
            (&source_source, &source),
            (&replay_source, &replay),
        ] {
            fs::copy(input, output).map_err(|error| {
                format!(
                    "failed to copy performance fixture {} to {}: {error}",
                    input.display(),
                    output.display()
                )
            })?;
        }
        let provenance = run_directory.join("input-provenance.json");
        let provenance_manifest = RustLspInputProvenanceManifest {
            lsp_mode: PackageProvenance {
                name: lsp_mode.0,
                version: lsp_mode.1,
                repository: lsp_mode_source.repository(),
                revision: lsp_mode_source.revision(),
                upstream_repository: lsp_mode_source.upstream_repository(),
                upstream_revision: lsp_mode_source.upstream_revision(),
            },
            tree_sitter_grammar: GrammarProvenance {
                language: "rust",
                repository: RUST_GRAMMAR_REPOSITORY,
                revision: RUST_GRAMMAR_REVISION,
            },
            editor,
            host: collect_host_provenance(request.machine_policy()),
            workload_source: "crates/neomacs-perf/fixtures/rust-lsp-typing.rs",
            workload_source_sha256: sha256_file(&source_source)?,
            environment_policy: "closed-v1",
            passthrough_environment: benchmark_passthrough_environment()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
                .collect(),
        };
        let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
            .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
        fs::write(&provenance, provenance_json).map_err(|error| {
            format!(
                "failed to write input provenance {}: {error}",
                provenance.display()
            )
        })?;
        let gui_runtime_directory = prepare_gui_runtime_directory(&self.workspace_root)?;
        Ok(PreparedScenario {
            fixture,
            provenance,
            result: run_directory.join("scenario-result.json"),
            sentinel: run_directory.join("completed"),
            terminal_bytes: run_directory.join("terminal.ansi"),
            gui_app_log: run_directory.join("gui-app.log"),
            gui_weston_log: run_directory.join("weston.log"),
            gui_runtime_directory,
            sandbox,
            workload: PreparedWorkload::RustLspTyping {
                startup,
                source,
                replay,
                grammar_directory,
                grammar_libraries,
                packages: Box::new(packages),
            },
        })
    }

    fn prepare_mx_tab_completion(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
        let editor = collect_editor_provenance(request.editor(), &sandbox)?;
        let fixture_source = self
            .workspace_root
            .join("crates/neomacs-perf/fixtures/mx-tab-completion.el");
        if !fixture_source.is_file() {
            return Err(format!(
                "missing committed performance fixture {}",
                fixture_source.display()
            ));
        }
        let fixture = run_directory.join("mx-tab-completion.el");
        fs::copy(&fixture_source, &fixture).map_err(|error| {
            format!(
                "failed to copy performance fixture {} to {}: {error}",
                fixture_source.display(),
                fixture.display()
            )
        })?;
        let provenance = run_directory.join("input-provenance.json");
        let provenance_manifest = MxTabInputProvenanceManifest {
            editor,
            host: collect_host_provenance(request.machine_policy()),
            workload_source: "crates/neomacs-perf/fixtures/mx-tab-completion.el",
            workload_source_sha256: sha256_file(&fixture_source)?,
            environment_policy: "closed-v1",
            passthrough_environment: benchmark_passthrough_environment()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
                .collect(),
        };
        let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
            .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
        fs::write(&provenance, provenance_json).map_err(|error| {
            format!(
                "failed to write input provenance {}: {error}",
                provenance.display()
            )
        })?;
        Ok(PreparedScenario {
            fixture,
            provenance,
            result: run_directory.join("scenario-result.json"),
            sentinel: run_directory.join("completed"),
            terminal_bytes: run_directory.join("terminal.ansi"),
            gui_app_log: run_directory.join("gui-app.log"),
            gui_weston_log: run_directory.join("weston.log"),
            gui_runtime_directory: prepare_gui_runtime_directory(&self.workspace_root)?,
            sandbox,
            workload: PreparedWorkload::MxTabCompletion,
        })
    }

    fn prepare_bytecode_call_loop(
        &self,
        request: &RunRequest,
        run_directory: &Path,
    ) -> Result<PreparedScenario, String> {
        let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
        let editor = collect_editor_provenance(request.editor(), &sandbox)?;
        let fixture_source = self
            .workspace_root
            .join("crates/neomacs-perf/fixtures/bytecode-call-loop.el");
        if !fixture_source.is_file() {
            return Err(format!(
                "missing committed performance fixture {}",
                fixture_source.display()
            ));
        }
        let fixture = run_directory.join("bytecode-call-loop.el");
        fs::copy(&fixture_source, &fixture).map_err(|error| {
            format!(
                "failed to copy performance fixture {} to {}: {error}",
                fixture_source.display(),
                fixture.display()
            )
        })?;
        let provenance = run_directory.join("input-provenance.json");
        let provenance_manifest = BytecodeCallInputProvenanceManifest {
            editor,
            host: collect_host_provenance(request.machine_policy()),
            workload_source: "crates/neomacs-perf/fixtures/bytecode-call-loop.el",
            workload_source_sha256: sha256_file(&fixture_source)?,
            execution_tier: "tier-0-interpreter",
            environment_policy: "closed-v1",
            passthrough_environment: benchmark_passthrough_environment()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
                .collect(),
        };
        let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
            .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
        fs::write(&provenance, provenance_json).map_err(|error| {
            format!(
                "failed to write input provenance {}: {error}",
                provenance.display()
            )
        })?;
        Ok(PreparedScenario {
            fixture,
            provenance,
            result: run_directory.join("scenario-result.json"),
            sentinel: run_directory.join("completed"),
            terminal_bytes: run_directory.join("terminal.ansi"),
            gui_app_log: run_directory.join("gui-app.log"),
            gui_weston_log: run_directory.join("weston.log"),
            gui_runtime_directory: prepare_gui_runtime_directory(&self.workspace_root)?,
            sandbox,
            workload: PreparedWorkload::BytecodeCallLoop,
        })
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

struct PreparedScenario {
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

fn benchmark_passthrough_environment() -> Vec<(&'static str, std::ffi::OsString)> {
    BENCHMARK_PASSTHROUGH_ENVIRONMENT
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (*name, value)))
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

fn copy_grammar_libraries(
    cached_directory: &Path,
    run_directory: &Path,
    library_stem: &str,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(run_directory).map_err(|error| {
        format!(
            "failed to create run-local Tree-sitter directory {}: {error}",
            run_directory.display()
        )
    })?;
    let entries = fs::read_dir(cached_directory).map_err(|error| {
        format!(
            "failed to enumerate cached Tree-sitter directory {}: {error}",
            cached_directory.display()
        )
    })?;
    let mut copied = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read cached Tree-sitter entry below {}: {error}",
                cached_directory.display()
            )
        })?;
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file())
            || !entry.file_name().to_string_lossy().contains(library_stem)
        {
            continue;
        }
        let destination = run_directory.join(entry.file_name());
        fs::copy(entry.path(), &destination).map_err(|error| {
            format!(
                "failed to copy Tree-sitter grammar {} to {}: {error}",
                entry.path().display(),
                destination.display()
            )
        })?;
        copied.push(destination);
    }
    if copied.is_empty() {
        return Err(format!(
            "cached Tree-sitter directory {} contains no `{library_stem}` library",
            cached_directory.display()
        ));
    }
    copied.sort();
    Ok(copied)
}

fn prepare_magit_repository(run_directory: &Path) -> Result<PathBuf, String> {
    let repository = run_directory.join("magit-repository");
    fs::create_dir_all(&repository).map_err(|error| {
        format!(
            "failed to create Magit fixture repository {}: {error}",
            repository.display()
        )
    })?;
    fs::write(repository.join("README.md"), "# neomacs-perf\n")
        .map_err(|error| format!("failed to write Magit fixture: {error}"))?;
    for arguments in [
        vec!["init", "--quiet"],
        vec!["add", "README.md"],
        vec![
            "-c",
            "user.name=neomacs-perf",
            "-c",
            "user.email=perf@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&repository)
            .output()
            .map_err(|error| format!("failed to launch git for Magit fixture: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "failed to prepare Magit fixture repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    fs::write(repository.join("README.md"), "# neomacs-perf\nmodified\n")
        .map_err(|error| format!("failed to modify Magit fixture: {error}"))?;
    Ok(repository)
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

#[derive(Debug, Deserialize)]
#[serde(try_from = "RustLspTypingResultWire")]
struct RustLspTypingResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    elapsed_us: u64,
    major_mode: String,
    lsp_mode_loaded: bool,
    treesit_parser_language: String,
    text_unchanged: bool,
    point_unchanged: bool,
    overlay_count: u64,
    lsp_diagnostic_count: u64,
}

enum ScenarioResult {
    RustLspTyping(RustLspTypingResult),
    MxTabCompletion(MxTabCompletionResult),
    BytecodeCallLoop(BytecodeCallLoopResult),
    EditorWorkload(EditorWorkloadResult),
    SustainedNativeVideo(SustainedNativeVideoResult),
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
        ScenarioId::SustainedNativeVideo => {
            serde_json::from_str(raw).map(ScenarioResult::SustainedNativeVideo)
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkBackend {
    Gstreamer,
}

impl std::fmt::Display for VideoBenchmarkBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Gstreamer => "gstreamer",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkImport {
    BorrowedNativeSurface,
    GpuBlit,
    CpuUpload,
}

impl std::fmt::Display for VideoBenchmarkImport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BorrowedNativeSurface => "borrowed-native-surface",
            Self::GpuBlit => "gpu-blit",
            Self::CpuUpload => "cpu-upload",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkPresentation {
    WgpuComposited,
}

impl std::fmt::Display for VideoBenchmarkPresentation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("wgpu-composited")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum VideoBenchmarkDecodeResidency {
    HardwareDecoderReportsRendererDevice,
    HardwareUnverified,
    Software,
    Unknown,
}

impl std::fmt::Display for VideoBenchmarkDecodeResidency {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::HardwareDecoderReportsRendererDevice => {
                "hardware-decoder-reports-renderer-device"
            }
            Self::HardwareUnverified => "hardware-unverified",
            Self::Software => "software",
            Self::Unknown => "unknown",
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "SustainedNativeVideoResultWire")]
struct SustainedNativeVideoResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    elapsed_cpu_us: u64,
    elapsed_wall_us: u64,
    presentation_target: NativeVideoPresentationTarget,
    viewport_width_pixels: u32,
    viewport_height_pixels: u32,
    backend: VideoBenchmarkBackend,
    decode_residency: VideoBenchmarkDecodeResidency,
    decoder_factory: String,
    decoder_plugin: String,
    decoder_kind: NativeVideoDecoderKind,
    gpu_adapter_name: String,
    gpu_vendor: u32,
    gpu_device: u32,
    gpu_device_type: String,
    graphics_backend: NativeVideoGraphicsBackend,
    gpu_driver: String,
    gpu_driver_info: String,
    drm_render_node: Option<String>,
    display_refresh_hz: Option<u16>,
    frame_format: NativeVideoFrameFormat,
    compositor_import: VideoBenchmarkImport,
    presentation: VideoBenchmarkPresentation,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    borrowed_native_frames: u64,
    gpu_blit_frames: u64,
    cpu_upload_frames: u64,
    submitted_frames: u64,
    presented_frames: u64,
    interval_samples: u64,
    interval_p50_us: u64,
    interval_p95_us: u64,
    interval_p99_us: u64,
    interval_max_us: u64,
    gpu_timing_status: NativeVideoGpuTimingStatus,
    gpu_pass_samples: u64,
    gpu_pass_total_us: u64,
    gpu_pass_min_us: Option<u64>,
    gpu_pass_max_us: Option<u64>,
    gpu_memory_bytes: u64,
    pool_capacity: u64,
    pool_allocations: u64,
    pool_reuses: u64,
    pool_backpressured_acquires: u64,
    pool_in_flight_high_water: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SustainedNativeVideoResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_cpu_us: u64,
    elapsed_wall_us: u64,
    presentation_width_pixels: u32,
    presentation_height_pixels: u32,
    viewport_width_pixels: u32,
    viewport_height_pixels: u32,
    backend: VideoBenchmarkBackend,
    decode_residency: VideoBenchmarkDecodeResidency,
    decoder_factory: String,
    decoder_plugin: String,
    decoder_kind: NativeVideoDecoderKind,
    gpu_adapter_name: String,
    gpu_vendor: u32,
    gpu_device: u32,
    gpu_device_type: String,
    graphics_backend: NativeVideoGraphicsBackend,
    gpu_driver: String,
    gpu_driver_info: String,
    drm_render_node: Option<String>,
    display_refresh_hz: Option<u16>,
    frame_format: NativeVideoFrameFormat,
    compositor_import: VideoBenchmarkImport,
    presentation: VideoBenchmarkPresentation,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    borrowed_native_frames: u64,
    gpu_blit_frames: u64,
    cpu_upload_frames: u64,
    submitted_frames: u64,
    presented_frames: u64,
    interval_samples: u64,
    interval_p50_us: u64,
    interval_p95_us: u64,
    interval_p99_us: u64,
    interval_max_us: u64,
    gpu_timing_status: NativeVideoGpuTimingStatus,
    gpu_pass_samples: u64,
    gpu_pass_total_us: u64,
    gpu_pass_min_us: Option<u64>,
    gpu_pass_max_us: Option<u64>,
    gpu_memory_bytes: u64,
    pool_capacity: u64,
    pool_allocations: u64,
    pool_reuses: u64,
    pool_backpressured_acquires: u64,
    pool_in_flight_high_water: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<SustainedNativeVideoResultWire> for SustainedNativeVideoResult {
    type Error = String;

    fn try_from(wire: SustainedNativeVideoResultWire) -> Result<Self, Self::Error> {
        let presentation_target = NativeVideoPresentationTarget::from_frontend(Frontend::Gui {
            width: wire.presentation_width_pixels,
            height: wire.presentation_height_pixels,
        })
        .map_err(|error| error.to_string())?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_cpu_us: wire.elapsed_cpu_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            presentation_target,
            viewport_width_pixels: wire.viewport_width_pixels,
            viewport_height_pixels: wire.viewport_height_pixels,
            backend: wire.backend,
            decode_residency: wire.decode_residency,
            decoder_factory: wire.decoder_factory,
            decoder_plugin: wire.decoder_plugin,
            decoder_kind: wire.decoder_kind,
            gpu_adapter_name: wire.gpu_adapter_name,
            gpu_vendor: wire.gpu_vendor,
            gpu_device: wire.gpu_device,
            gpu_device_type: wire.gpu_device_type,
            graphics_backend: wire.graphics_backend,
            gpu_driver: wire.gpu_driver,
            gpu_driver_info: wire.gpu_driver_info,
            drm_render_node: wire.drm_render_node,
            display_refresh_hz: wire.display_refresh_hz,
            frame_format: wire.frame_format,
            compositor_import: wire.compositor_import,
            presentation: wire.presentation,
            decoded_frames: wire.decoded_frames,
            replaced_frames: wire.replaced_frames,
            late_dropped_frames: wire.late_dropped_frames,
            imported_frames: wire.imported_frames,
            backpressured_frames: wire.backpressured_frames,
            borrowed_native_frames: wire.borrowed_native_frames,
            gpu_blit_frames: wire.gpu_blit_frames,
            cpu_upload_frames: wire.cpu_upload_frames,
            submitted_frames: wire.submitted_frames,
            presented_frames: wire.presented_frames,
            interval_samples: wire.interval_samples,
            interval_p50_us: wire.interval_p50_us,
            interval_p95_us: wire.interval_p95_us,
            interval_p99_us: wire.interval_p99_us,
            interval_max_us: wire.interval_max_us,
            gpu_timing_status: wire.gpu_timing_status,
            gpu_pass_samples: wire.gpu_pass_samples,
            gpu_pass_total_us: wire.gpu_pass_total_us,
            gpu_pass_min_us: wire.gpu_pass_min_us,
            gpu_pass_max_us: wire.gpu_pass_max_us,
            gpu_memory_bytes: wire.gpu_memory_bytes,
            pool_capacity: wire.pool_capacity,
            pool_allocations: wire.pool_allocations,
            pool_reuses: wire.pool_reuses,
            pool_backpressured_acquires: wire.pool_backpressured_acquires,
            pool_in_flight_high_water: wire.pool_in_flight_high_water,
        })
    }
}

impl SustainedNativeVideoResult {
    fn execution_identity(&self) -> NativeVideoExecutionIdentity {
        NativeVideoExecutionIdentity {
            decoder_factory: self.decoder_factory.clone(),
            decoder_plugin: self.decoder_plugin.clone(),
            decoder_kind: self.decoder_kind,
            gpu_adapter_name: self.gpu_adapter_name.clone(),
            gpu_vendor: self.gpu_vendor,
            gpu_device: self.gpu_device,
            gpu_device_type: self.gpu_device_type.clone(),
            graphics_backend: self.graphics_backend,
            gpu_driver: self.gpu_driver.clone(),
            gpu_driver_info: self.gpu_driver_info.clone(),
            drm_render_node: self.drm_render_node.clone(),
            display_refresh_hz: self.display_refresh_hz,
            frame_format: self.frame_format,
            gpu_timing_status: self.gpu_timing_status,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RustLspTypingResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    major_mode: String,
    lsp_mode_loaded: bool,
    treesit_parser_language: String,
    text_unchanged: bool,
    point_unchanged: bool,
    overlay_count: u64,
    lsp_diagnostic_count: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
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

impl TryFrom<RustLspTypingResultWire> for RustLspTypingResult {
    type Error = String;

    fn try_from(wire: RustLspTypingResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            major_mode: wire.major_mode,
            lsp_mode_loaded: wire.lsp_mode_loaded,
            treesit_parser_language: wire.treesit_parser_language,
            text_unchanged: wire.text_unchanged,
            point_unchanged: wire.point_unchanged,
            overlay_count: wire.overlay_count,
            lsp_diagnostic_count: wire.lsp_diagnostic_count,
        })
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

#[derive(Debug, Deserialize)]
#[serde(try_from = "MxTabCompletionResultWire")]
struct MxTabCompletionResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    elapsed_us: u64,
    completion_help_calls: u32,
    completion_visible: bool,
    completion_mode_correct: bool,
    known_commands_present: bool,
    completion_candidate_count: u64,
    candidate_count_stable: bool,
    completion_hidden_after_exit: bool,
    minibuffer_depth_restored: bool,
    selected_buffer_restored: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MxTabCompletionResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    completion_help_calls: u32,
    completion_visible: bool,
    completion_mode_correct: bool,
    known_commands_present: bool,
    completion_candidate_count: u64,
    candidate_count_stable: bool,
    completion_hidden_after_exit: bool,
    minibuffer_depth_restored: bool,
    selected_buffer_restored: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<MxTabCompletionResultWire> for MxTabCompletionResult {
    type Error = String;

    fn try_from(wire: MxTabCompletionResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            completion_help_calls: wire.completion_help_calls,
            completion_visible: wire.completion_visible,
            completion_mode_correct: wire.completion_mode_correct,
            known_commands_present: wire.known_commands_present,
            completion_candidate_count: wire.completion_candidate_count,
            candidate_count_stable: wire.candidate_count_stable,
            completion_hidden_after_exit: wire.completion_hidden_after_exit,
            minibuffer_depth_restored: wire.minibuffer_depth_restored,
            selected_buffer_restored: wire.selected_buffer_restored,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "BytecodeCallLoopResultWire")]
struct BytecodeCallLoopResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    elapsed_us: u64,
    bytecode_calls: u64,
    result: i64,
    expected_result: i64,
    bytecode_functions_compiled: bool,
    interpreter_requested: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BytecodeCallLoopResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    bytecode_calls: u64,
    result: i64,
    expected_result: i64,
    bytecode_functions_compiled: bool,
    interpreter_requested: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<BytecodeCallLoopResultWire> for BytecodeCallLoopResult {
    type Error = String;

    fn try_from(wire: BytecodeCallLoopResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            bytecode_calls: wire.bytecode_calls,
            result: wire.result,
            expected_result: wire.expected_result,
            bytecode_functions_compiled: wire.bytecode_functions_compiled,
            interpreter_requested: wire.interpreter_requested,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "EditorWorkloadResultWire")]
struct EditorWorkloadResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
    type_phase_us: u64,
    comment_phase_us: u64,
    kill_yank_phase_us: u64,
    indent_phase_us: u64,
    regex_phase_us: u64,
    latency_samples_us: Vec<u64>,
    mode_phase_us: u64,
    fontify_phase_us: u64,
    replace_phase_us: u64,
    undo_redo_phase_us: u64,
    isearch_phase_us: u64,
    buffer_switch_phase_us: u64,
    how_many_phase_us: u64,
    motion_phase_us: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditorWorkloadResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    initial_checksum: String,
    final_checksum: String,
    point_restored: bool,
    expected_major_mode: String,
    actual_major_mode: String,
    type_phase_us: u64,
    comment_phase_us: u64,
    kill_yank_phase_us: u64,
    indent_phase_us: u64,
    regex_phase_us: u64,
    latency_samples_us: Vec<u64>,
    mode_phase_us: u64,
    fontify_phase_us: u64,
    replace_phase_us: u64,
    undo_redo_phase_us: u64,
    isearch_phase_us: u64,
    buffer_switch_phase_us: u64,
    how_many_phase_us: u64,
    motion_phase_us: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<EditorWorkloadResultWire> for EditorWorkloadResult {
    type Error = String;

    fn try_from(wire: EditorWorkloadResultWire) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            operation_count: wire.operation_count,
            initial_checksum: wire.initial_checksum,
            final_checksum: wire.final_checksum,
            point_restored: wire.point_restored,
            expected_major_mode: wire.expected_major_mode,
            actual_major_mode: wire.actual_major_mode,
            type_phase_us: wire.type_phase_us,
            comment_phase_us: wire.comment_phase_us,
            kill_yank_phase_us: wire.kill_yank_phase_us,
            indent_phase_us: wire.indent_phase_us,
            regex_phase_us: wire.regex_phase_us,
            latency_samples_us: wire.latency_samples_us,
            mode_phase_us: wire.mode_phase_us,
            fontify_phase_us: wire.fontify_phase_us,
            replace_phase_us: wire.replace_phase_us,
            undo_redo_phase_us: wire.undo_redo_phase_us,
            isearch_phase_us: wire.isearch_phase_us,
            buffer_switch_phase_us: wire.buffer_switch_phase_us,
            how_many_phase_us: wire.how_many_phase_us,
            motion_phase_us: wire.motion_phase_us,
        })
    }
}

#[derive(Serialize)]
struct RustLspInputProvenanceManifest<'a> {
    lsp_mode: PackageProvenance<'a>,
    tree_sitter_grammar: GrammarProvenance<'a>,
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct MxTabInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct BytecodeCallInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    execution_tier: &'a str,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct EditorWorkloadInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    scenario: ScenarioId,
    workload_fixture_sha256: String,
    source_fixture_sha256: String,
    package: Option<PackageProvenance<'a>>,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct NativeVideoInputProvenanceManifest {
    editor: EditorProvenance,
    editor_build_profile: NativeVideoBuildProfile,
    host: HostProvenance,
    harness: HarnessProvenance,
    video_file: String,
    comparison_identity: NativeVideoComparisonIdentity,
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

fn validate_rust_lsp_typing_result(
    request: &RunRequest,
    result: &RustLspTypingResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "major-mode",
        "rust-ts-mode",
        result.major_mode.as_str(),
    );
    mismatch(
        &mut mismatches,
        "lsp-mode-loaded",
        true,
        result.lsp_mode_loaded,
    );
    mismatch(
        &mut mismatches,
        "treesit-parser-language",
        "rust",
        result.treesit_parser_language.as_str(),
    );
    mismatch(
        &mut mismatches,
        "final-buffer-text",
        true,
        result.text_unchanged,
    );
    mismatch(&mut mismatches, "final-point", true, result.point_unchanged);
    mismatch(
        &mut mismatches,
        "overlay-count",
        RUST_LSP_TYPING_OVERLAY_COUNT,
        result.overlay_count,
    );
    mismatch(
        &mut mismatches,
        "lsp-diagnostic-count",
        RUST_LSP_TYPING_DIAGNOSTIC_COUNT,
        result.lsp_diagnostic_count,
    );
    mismatches
}

fn validate_mx_tab_completion_result(
    request: &RunRequest,
    result: &MxTabCompletionResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "completion-help-calls",
        request.iterations.get(),
        result.completion_help_calls,
    );
    mismatch(
        &mut mismatches,
        "completion-window-visible",
        true,
        result.completion_visible,
    );
    mismatch(
        &mut mismatches,
        "completion-buffer-mode",
        true,
        result.completion_mode_correct,
    );
    mismatch(
        &mut mismatches,
        "known-command-candidates",
        true,
        result.known_commands_present,
    );
    mismatch(
        &mut mismatches,
        "completion-candidate-count-stable",
        true,
        result.candidate_count_stable,
    );
    mismatch(
        &mut mismatches,
        "completion-candidate-count",
        MX_TAB_COMPLETION_CANDIDATE_COUNT,
        result.completion_candidate_count,
    );
    mismatch(
        &mut mismatches,
        "completion-window-hidden-after-exit",
        true,
        result.completion_hidden_after_exit,
    );
    mismatch(
        &mut mismatches,
        "minibuffer-depth-restored",
        true,
        result.minibuffer_depth_restored,
    );
    mismatch(
        &mut mismatches,
        "selected-buffer-restored",
        true,
        result.selected_buffer_restored,
    );
    mismatches
}

fn validate_bytecode_call_loop_result(
    request: &RunRequest,
    result: &BytecodeCallLoopResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "bytecode-call-count",
        u64::from(request.iterations.get()),
        result.bytecode_calls,
    );
    mismatch(
        &mut mismatches,
        "bytecode-result",
        result.expected_result,
        result.result,
    );
    mismatch(
        &mut mismatches,
        "expected-bytecode-result",
        1,
        result.expected_result,
    );
    mismatch(
        &mut mismatches,
        "bytecode-functions-compiled",
        true,
        result.bytecode_functions_compiled,
    );
    mismatch(
        &mut mismatches,
        "tier-0-interpreter-requested",
        true,
        result.interpreter_requested,
    );
    mismatches
}

fn validate_editor_workload_result(
    request: &RunRequest,
    result: &EditorWorkloadResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "operation-count",
        u64::from(request.iterations.get()),
        result.operation_count,
    );
    mismatch(
        &mut mismatches,
        "final-buffer-checksum",
        result.initial_checksum.as_str(),
        result.final_checksum.as_str(),
    );
    mismatch(&mut mismatches, "final-point", true, result.point_restored);
    mismatch(
        &mut mismatches,
        "major-mode",
        result.expected_major_mode.as_str(),
        result.actual_major_mode.as_str(),
    );
    if result.initial_checksum.is_empty() {
        mismatches.push(CorrectnessMismatch {
            invariant: "initial-buffer-checksum".to_string(),
            expected: "non-empty".to_string(),
            actual: "empty".to_string(),
        });
    }
    if result.expected_major_mode.is_empty() {
        mismatches.push(CorrectnessMismatch {
            invariant: "expected-major-mode".to_string(),
            expected: "non-empty".to_string(),
            actual: "empty".to_string(),
        });
    }
    if result.elapsed_us == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "elapsed-time".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    if result.elapsed_wall_us == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "elapsed-wall-time".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    match request.scenario {
        ScenarioId::EditingSimulation => {
            for (name, value) in [
                ("mode-phase-time", result.mode_phase_us),
                ("fontify-phase-time", result.fontify_phase_us),
                ("regex-phase-time", result.regex_phase_us),
                ("type-phase-time", result.type_phase_us),
                ("replace-phase-time", result.replace_phase_us),
                ("indent-phase-time", result.indent_phase_us),
                ("kill-yank-phase-time", result.kill_yank_phase_us),
                ("undo-redo-phase-time", result.undo_redo_phase_us),
                ("isearch-phase-time", result.isearch_phase_us),
                ("buffer-switch-phase-time", result.buffer_switch_phase_us),
                ("comment-phase-time", result.comment_phase_us),
                ("how-many-phase-time", result.how_many_phase_us),
                ("motion-phase-time", result.motion_phase_us),
            ] {
                require_positive_phase(&mut mismatches, name, value);
            }
        }
        ScenarioId::SustainedEditing | ScenarioId::OrgEditing => {
            require_positive_phase(&mut mismatches, "type-phase-time", result.type_phase_us);
        }
        ScenarioId::MagitStatus | ScenarioId::RegexSearch => {
            require_positive_phase(&mut mismatches, "regex-phase-time", result.regex_phase_us);
        }
        ScenarioId::LargeFileEditing => {
            require_positive_phase(&mut mismatches, "type-phase-time", result.type_phase_us);
            require_positive_phase(&mut mismatches, "regex-phase-time", result.regex_phase_us);
            require_positive_phase(&mut mismatches, "motion-phase-time", result.motion_phase_us);
        }
        ScenarioId::Indentation => {
            require_positive_phase(&mut mismatches, "indent-phase-time", result.indent_phase_us);
        }
        ScenarioId::Startup | ScenarioId::GuiInputLatency => {}
        ScenarioId::SustainedNativeVideo => {
            unreachable!("native video has a dedicated result validator")
        }
        ScenarioId::RustLspTyping | ScenarioId::MxTabCompletion | ScenarioId::BytecodeCallLoop => {
            unreachable!("dedicated scenario results do not use the editor workload validator")
        }
    }
    let expected_latency_samples = if request.scenario == ScenarioId::GuiInputLatency {
        request.iterations.get() as usize
    } else {
        0
    };
    mismatch(
        &mut mismatches,
        "latency-sample-count",
        expected_latency_samples,
        result.latency_samples_us.len(),
    );
    if result.latency_samples_us.contains(&0) {
        mismatches.push(CorrectnessMismatch {
            invariant: "latency-samples".to_string(),
            expected: "all positive".to_string(),
            actual: "contains zero".to_string(),
        });
    }
    mismatches
}

fn validate_sustained_native_video_result(
    request: &RunRequest,
    result: &SustainedNativeVideoResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        NATIVE_VIDEO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    let requested_target = NativeVideoPresentationTarget::from_frontend(request.frontend())
        .expect("native-video preparation rejects a non-GUI or zero-sized frontend");
    mismatch(
        &mut mismatches,
        "presentation-width",
        requested_target.width(),
        result.presentation_target.width(),
    );
    mismatch(
        &mut mismatches,
        "presentation-height",
        requested_target.height(),
        result.presentation_target.height(),
    );
    if result.viewport_width_pixels < result.presentation_target.width()
        || result.viewport_height_pixels < result.presentation_target.height()
    {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-fits-viewport".to_string(),
            expected: format!(
                "viewport at least {}x{}",
                result.presentation_target.width(),
                result.presentation_target.height()
            ),
            actual: format!(
                "{}x{}",
                result.viewport_width_pixels, result.viewport_height_pixels
            ),
        });
    }
    mismatch(
        &mut mismatches,
        "backend",
        VideoBenchmarkBackend::Gstreamer,
        result.backend,
    );
    mismatch(
        &mut mismatches,
        "decode-residency",
        VideoBenchmarkDecodeResidency::HardwareDecoderReportsRendererDevice,
        result.decode_residency,
    );
    mismatch(
        &mut mismatches,
        "decoder-kind",
        NativeVideoDecoderKind::Hardware,
        result.decoder_kind,
    );
    for (name, value) in [
        ("decoder-factory", result.decoder_factory.as_str()),
        ("decoder-plugin", result.decoder_plugin.as_str()),
        ("gpu-adapter-name", result.gpu_adapter_name.as_str()),
        ("gpu-device-type", result.gpu_device_type.as_str()),
        ("gpu-driver", result.gpu_driver.as_str()),
    ] {
        if value.is_empty() {
            mismatches.push(CorrectnessMismatch {
                invariant: name.to_owned(),
                expected: "non-empty".to_owned(),
                actual: "empty".to_owned(),
            });
        }
    }
    if result.decoder_plugin == "unknown" {
        mismatches.push(CorrectnessMismatch {
            invariant: "decoder-plugin".to_owned(),
            expected: "identified GStreamer plugin".to_owned(),
            actual: "unknown".to_owned(),
        });
    }
    mismatch(
        &mut mismatches,
        "graphics-backend",
        NativeVideoGraphicsBackend::Vulkan,
        result.graphics_backend,
    );
    require_positive_phase(&mut mismatches, "gpu-vendor", u64::from(result.gpu_vendor));
    require_positive_phase(&mut mismatches, "gpu-device", u64::from(result.gpu_device));
    if result.drm_render_node.is_none() {
        mismatches.push(CorrectnessMismatch {
            invariant: "drm-render-node".to_owned(),
            expected: "identified Linux render node".to_owned(),
            actual: "unknown".to_owned(),
        });
    }
    if result.display_refresh_hz.is_none_or(|rate| rate == 0) {
        mismatches.push(CorrectnessMismatch {
            invariant: "display-refresh-rate".to_owned(),
            expected: "positive reported or bounded fallback rate".to_owned(),
            actual: format!("{:?}", result.display_refresh_hz),
        });
    }
    // Some wgpu backends expose no supplementary driver-info string; the
    // field remains recorded for adapters that do.
    let _gpu_driver_info = &result.gpu_driver_info;
    if !matches!(
        result.frame_format,
        NativeVideoFrameFormat::Nv12 | NativeVideoFrameFormat::P010
    ) {
        mismatches.push(CorrectnessMismatch {
            invariant: "frame-format".to_string(),
            expected: "nv12 or p010".to_string(),
            actual: result.frame_format.to_string(),
        });
    }
    mismatch(
        &mut mismatches,
        "compositor-import",
        VideoBenchmarkImport::BorrowedNativeSurface,
        result.compositor_import,
    );
    mismatch(
        &mut mismatches,
        "presentation",
        VideoBenchmarkPresentation::WgpuComposited,
        result.presentation,
    );
    mismatch(
        &mut mismatches,
        "borrowed-import-count",
        result.imported_frames,
        result.borrowed_native_frames,
    );
    for (name, value) in [
        ("elapsed-cpu-time", result.elapsed_cpu_us),
        ("elapsed-wall-time", result.elapsed_wall_us),
        ("decoded-frames", result.decoded_frames),
        ("imported-frames", result.imported_frames),
        ("submitted-frames", result.submitted_frames),
        ("presented-frames", result.presented_frames),
        ("presentation-interval-samples", result.interval_samples),
        ("presentation-p50", result.interval_p50_us),
        ("presentation-p95", result.interval_p95_us),
        ("presentation-p99", result.interval_p99_us),
        ("presentation-max", result.interval_max_us),
        ("gpu-memory", result.gpu_memory_bytes),
        ("surface-pool-capacity", result.pool_capacity),
        ("surface-pool-reuses", result.pool_reuses),
        (
            "surface-pool-in-flight-high-water",
            result.pool_in_flight_high_water,
        ),
    ] {
        require_positive_phase(&mut mismatches, name, value);
    }
    mismatch(
        &mut mismatches,
        "gpu-blit-frames",
        0,
        result.gpu_blit_frames,
    );
    mismatch(
        &mut mismatches,
        "cpu-upload-frames",
        0,
        result.cpu_upload_frames,
    );
    if result.presented_frames > result.submitted_frames {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-count-order".to_string(),
            expected: "presented <= submitted".to_string(),
            actual: format!("{} > {}", result.presented_frames, result.submitted_frames),
        });
    }
    if result.interval_samples > result.presented_frames {
        mismatches.push(CorrectnessMismatch {
            invariant: "presentation-interval-count".to_string(),
            expected: "interval samples <= presented frames".to_string(),
            actual: format!("{} > {}", result.interval_samples, result.presented_frames),
        });
    }
    if result.pool_in_flight_high_water > result.pool_capacity {
        mismatches.push(CorrectnessMismatch {
            invariant: "surface-pool-high-water".to_string(),
            expected: format!("<= {}", result.pool_capacity),
            actual: result.pool_in_flight_high_water.to_string(),
        });
    }
    match result.gpu_timing_status {
        NativeVideoGpuTimingStatus::Enabled => {
            for (name, value) in [
                ("gpu-pass-samples", result.gpu_pass_samples),
                ("gpu-pass-total-time", result.gpu_pass_total_us),
                ("gpu-pass-min-time", result.gpu_pass_min_us.unwrap_or(0)),
                ("gpu-pass-max-time", result.gpu_pass_max_us.unwrap_or(0)),
            ] {
                require_positive_phase(&mut mismatches, name, value);
            }
        }
        NativeVideoGpuTimingStatus::Disabled => mismatches.push(CorrectnessMismatch {
            invariant: "gpu-timing-status".to_string(),
            expected: "enabled or unsupported".to_string(),
            actual: "disabled".to_string(),
        }),
        NativeVideoGpuTimingStatus::Unsupported => {}
    }
    mismatches
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
        ScenarioResult::RustLspTyping(result) => validate_rust_lsp_typing_result(request, result),
        ScenarioResult::MxTabCompletion(result) => {
            validate_mx_tab_completion_result(request, result)
        }
        ScenarioResult::BytecodeCallLoop(result) => {
            validate_bytecode_call_loop_result(request, result)
        }
        ScenarioResult::EditorWorkload(result) => validate_editor_workload_result(request, result),
        ScenarioResult::SustainedNativeVideo(result) => {
            validate_sustained_native_video_result(request, result)
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
            valid_rust_lsp_typing_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::MxTabCompletion(result) => {
            valid_mx_tab_completion_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::BytecodeCallLoop(result) => {
            valid_bytecode_call_loop_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::EditorWorkload(result) => {
            valid_editor_workload_measurements(result, wall_elapsed_us)
        }
        ScenarioResult::SustainedNativeVideo(result) => {
            valid_sustained_native_video_measurements(result, wall_elapsed_us)
        }
    }
}

fn valid_sustained_native_video_measurements(
    result: &SustainedNativeVideoResult,
    process_wall_us: u128,
) -> Vec<Measurement> {
    let elapsed_seconds = result.elapsed_wall_us.max(1) as f64 / 1_000_000.0;
    let mut measurements = vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: process_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_cpu_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadWallTime,
            value: result.elapsed_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::VideoPresentationFramesPerSecond,
            value: result.presented_frames as f64 / elapsed_seconds,
            unit: MetricUnit::FramesPerSecond,
        },
        Measurement {
            name: MetricName::VideoDecodeFramesPerSecond,
            value: result.decoded_frames as f64 / elapsed_seconds,
            unit: MetricUnit::FramesPerSecond,
        },
        Measurement {
            name: MetricName::P50VideoPresentationInterval,
            value: result.interval_p50_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::P95VideoPresentationInterval,
            value: result.interval_p95_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::P99VideoPresentationInterval,
            value: result.interval_p99_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::MaxVideoPresentationInterval,
            value: result.interval_max_us as f64,
            unit: MetricUnit::Microseconds,
        },
    ];
    for (name, value) in [
        (MetricName::VideoDecodedFrames, result.decoded_frames),
        (MetricName::VideoPresentedFrames, result.presented_frames),
        (MetricName::VideoReplacedFrames, result.replaced_frames),
        (
            MetricName::VideoLateDroppedFrames,
            result.late_dropped_frames,
        ),
        (
            MetricName::VideoBackpressuredFrames,
            result.backpressured_frames,
        ),
        (MetricName::VideoGpuPassSamples, result.gpu_pass_samples),
        (
            MetricName::VideoSurfacePoolAllocations,
            result.pool_allocations,
        ),
        (MetricName::VideoSurfacePoolReuses, result.pool_reuses),
        (
            MetricName::VideoSurfacePoolBackpressuredAcquires,
            result.pool_backpressured_acquires,
        ),
        (
            MetricName::VideoSurfacePoolInFlightHighWater,
            result.pool_in_flight_high_water,
        ),
    ] {
        measurements.push(Measurement {
            name,
            value: value as f64,
            unit: MetricUnit::Count,
        });
    }
    measurements.push(Measurement {
        name: MetricName::VideoGpuMemoryBytes,
        value: result.gpu_memory_bytes as f64,
        unit: MetricUnit::Bytes,
    });
    if result.gpu_pass_samples > 0 {
        measurements.push(Measurement {
            name: MetricName::AverageVideoGpuPassTime,
            value: result.gpu_pass_total_us as f64 / result.gpu_pass_samples as f64,
            unit: MetricUnit::MicrosecondsPerFrame,
        });
    }
    measurements
}

fn valid_rust_lsp_typing_measurements(
    result: &RustLspTypingResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let edits = u64::from(result.iterations) * 2;
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerEditCpuTime,
            value: result.elapsed_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        },
        Measurement {
            name: MetricName::Edits,
            value: edits as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Redisplays,
            value: edits as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::OverlayCount,
            value: result.overlay_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::LspDiagnosticCount,
            value: result.lsp_diagnostic_count as f64,
            unit: MetricUnit::Count,
        },
    ]
}

fn valid_mx_tab_completion_measurements(
    result: &MxTabCompletionResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerCompletionCpuTime,
            value: result.elapsed_us as f64 / f64::from(result.completion_help_calls.max(1)),
            unit: MetricUnit::MicrosecondsPerCompletion,
        },
        Measurement {
            name: MetricName::CompletionHelpCalls,
            value: f64::from(result.completion_help_calls),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::CompletionCandidateCount,
            value: result.completion_candidate_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ]
}

fn valid_bytecode_call_loop_measurements(
    result: &BytecodeCallLoopResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerBytecodeCallCpuTime,
            value: result.elapsed_us as f64 / result.bytecode_calls.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerBytecodeCall,
        },
        Measurement {
            name: MetricName::BytecodeCalls,
            value: result.bytecode_calls as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ]
}

fn valid_editor_workload_measurements(
    result: &EditorWorkloadResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let mut measurements = vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadWallTime,
            value: result.elapsed_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerOperationCpuTime,
            value: result.elapsed_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::PerOperationWallTime,
            value: result.elapsed_wall_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::OperationCount,
            value: result.operation_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
    ];
    if result.scenario == ScenarioId::SustainedEditing {
        let edits = result.operation_count.saturating_mul(2);
        measurements.push(Measurement {
            name: MetricName::PerEditCpuTime,
            value: result.elapsed_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        });
        measurements.push(Measurement {
            name: MetricName::PerEditWallTime,
            value: result.elapsed_wall_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        });
    }
    if matches!(
        result.scenario,
        ScenarioId::SustainedEditing | ScenarioId::GuiInputLatency
    ) {
        let edits = result.operation_count.saturating_mul(2);
        measurements.extend([
            Measurement {
                name: MetricName::Edits,
                value: edits as f64,
                unit: MetricUnit::Count,
            },
            Measurement {
                name: MetricName::Redisplays,
                value: edits as f64,
                unit: MetricUnit::Count,
            },
        ]);
    }
    for (name, value) in [
        (MetricName::TypePhaseCpuTime, result.type_phase_us),
        (MetricName::CommentPhaseCpuTime, result.comment_phase_us),
        (MetricName::KillYankPhaseCpuTime, result.kill_yank_phase_us),
        (MetricName::IndentPhaseCpuTime, result.indent_phase_us),
        (MetricName::RegexPhaseCpuTime, result.regex_phase_us),
        (MetricName::ModePhaseCpuTime, result.mode_phase_us),
        (MetricName::FontifyPhaseCpuTime, result.fontify_phase_us),
        (MetricName::ReplacePhaseCpuTime, result.replace_phase_us),
        (MetricName::UndoRedoPhaseCpuTime, result.undo_redo_phase_us),
        (MetricName::IsearchPhaseCpuTime, result.isearch_phase_us),
        (
            MetricName::BufferSwitchPhaseCpuTime,
            result.buffer_switch_phase_us,
        ),
        (MetricName::HowManyPhaseCpuTime, result.how_many_phase_us),
        (MetricName::MotionPhaseCpuTime, result.motion_phase_us),
    ] {
        if value > 0 {
            measurements.push(Measurement {
                name,
                value: value as f64,
                unit: MetricUnit::Microseconds,
            });
        }
    }
    if !result.latency_samples_us.is_empty() {
        let mut samples = result.latency_samples_us.clone();
        samples.sort_unstable();
        for (name, percentile) in [
            (MetricName::P50InputToRedisplayLatency, 0.50),
            (MetricName::P95InputToRedisplayLatency, 0.95),
            (MetricName::P99InputToRedisplayLatency, 0.99),
        ] {
            measurements.push(Measurement {
                name,
                value: nearest_rank(&samples, percentile) as f64,
                unit: MetricUnit::Microseconds,
            });
        }
        let over_budget = samples
            .iter()
            .filter(|&&sample| sample > INPUT_LATENCY_BUDGET_US)
            .count();
        measurements.push(Measurement {
            name: MetricName::InputLatencyOverBudgetCount,
            value: over_budget as f64,
            unit: MetricUnit::Count,
        });
    }
    measurements
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
