use std::ffi::OsString;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};
use thiserror::Error;

use crate::{
    ComparisonRequest, ComparisonSampleCount, ComparisonVerdict, CounterScope, Frontend,
    MachinePolicy, NativeProfiler, PerfCallGraph, PerfCaptureConfiguration, PerfError, PerfHarness,
    PerfSamplingEvent, PerfSamplingRate, ProfileReportStyle, ProfileRequest, ProfileScope,
    ProfileVerdict, RunRequest, ScenarioId, SuiteId, SuiteRequest, SuiteVerdict, scenarios,
};

const DEFAULT_SAMPLES: ComparisonSampleCount =
    ComparisonSampleCount::new(5).expect("5 meets the minimum sample count");
const DEFAULT_TIMEOUT_SECS: NonZeroU64 = NonZeroU64::new(300).expect("300 is non-zero");

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PerfCommand {
    List,
    InputLatency {
        path: PathBuf,
        budget_us: NonZeroU64,
    },
    Run {
        scenario: ScenarioId,
        editor: Option<PathBuf>,
        editor_arguments: Vec<String>,
        iterations: NonZeroU32,
        frontend: Option<Frontend>,
        timeout: Duration,
        machine: MachinePolicy,
        counters: Option<CounterScope>,
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
        execution_overrides: crate::ExecutionOverrides,
    },
    Compare {
        scenario: ScenarioId,
        baseline_editor: PathBuf,
        candidate_editor: PathBuf,
        samples: ComparisonSampleCount,
        iterations: NonZeroU32,
        frontend: Option<Frontend>,
        timeout: Duration,
        machine: MachinePolicy,
        counters: Option<CounterScope>,
        gate_metric: crate::SuiteGateMetric,
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
        baseline_execution_overrides: crate::ExecutionOverrides,
        candidate_execution_overrides: crate::ExecutionOverrides,
    },
    Profile {
        scenario: ScenarioId,
        profiler: NativeProfiler,
        scope: ProfileScope,
        report_style: ProfileReportStyle,
        configuration: PerfCaptureConfiguration,
        editor: Option<PathBuf>,
        iterations: NonZeroU32,
        frontend: Option<Frontend>,
        timeout: Duration,
        machine: MachinePolicy,
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
        execution_overrides: crate::ExecutionOverrides,
    },
    Suite {
        suite: SuiteId,
        baseline_editor: PathBuf,
        candidate_editor: PathBuf,
        samples: ComparisonSampleCount,
        timeout: Duration,
        machine: MachinePolicy,
        counters: Option<CounterScope>,
        previous_suite: Option<PathBuf>,
        baseline_manifest: Option<PathBuf>,
    },
    Help {
        rendered: String,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "cargo xtask perf",
    about = "Run correctness-gated Neomacs performance workloads",
    after_long_help = "Every attempt writes a structured artifact below ./tmp. Only a run whose\nfixture invariants all pass receives a valid verdict and performance samples.\nA comparison is valid only when every baseline and candidate run is valid.\nProfile runs are diagnostic and never contribute samples to a comparison."
)]
struct PerfCli {
    #[command(subcommand)]
    command: PerfSubcommand,
}

#[derive(Debug, Subcommand)]
enum PerfSubcommand {
    /// Summarize one NEOMACS_INPUT_LATENCY_FILE native GUI session.
    InputLatency {
        path: PathBuf,
        /// Input-to-confirmed-presentation budget, in microseconds.
        #[arg(long, default_value = "16667")]
        budget_us: NonZeroU64,
    },
    /// List the registered performance scenarios.
    List,
    /// Execute one correctness-gated workload run.
    Run(RunArgs),
    /// Compare repeated, interleaved runs from two editor binaries.
    Compare(CompareArgs),
    /// Capture native sampled stacks for one workload run.
    Profile(ProfileArgs),
    /// Run a thresholded collection of comparisons.
    Suite(SuiteArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Runtime setting: NAME=VALUE sets; NAME alone removes an inherited setting. Repeatable.
    #[arg(long = "env", value_name = "NAME[=VALUE]")]
    execution_overrides: Vec<crate::ExecutionOverride>,
    /// Extra editor argv, preserved verbatim and recorded; repeat as --editor-arg=ARG.
    #[arg(
        long = "editor-arg",
        value_name = "ARG",
        action = clap::ArgAction::Append,
        allow_hyphen_values = true,
        require_equals = true
    )]
    editor_arguments: Vec<String>,
    /// Registered scenario to execute.
    scenario: ScenarioId,
    /// Editor executable (defaults to target/release/neomacs).
    #[arg(long)]
    editor: Option<PathBuf>,
    #[command(flatten)]
    workload: WorkloadArgs,
    #[command(flatten)]
    counters: CounterArgs,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Runtime setting: NAME=VALUE sets; NAME alone removes an inherited setting. Repeatable.
    #[arg(long = "baseline-env", value_name = "NAME[=VALUE]")]
    baseline_execution_overrides: Vec<crate::ExecutionOverride>,
    /// Runtime setting: NAME=VALUE sets; NAME alone removes an inherited setting. Repeatable.
    #[arg(long = "candidate-env", value_name = "NAME[=VALUE]")]
    candidate_execution_overrides: Vec<crate::ExecutionOverride>,
    /// Registered scenario to execute.
    scenario: ScenarioId,
    /// Baseline editor executable.
    #[arg(long)]
    baseline_editor: PathBuf,
    /// Candidate editor executable.
    #[arg(long)]
    candidate_editor: PathBuf,
    /// Repetitions per editor; must be at least three.
    #[arg(long, default_value_t = DEFAULT_SAMPLES)]
    samples: ComparisonSampleCount,
    /// Metric to summarize as the primary gate (`instructions` requires --hardware-counters).
    #[arg(long, value_enum, default_value_t = CompareGateMetricArg::ScenarioPrimary)]
    gate_metric: CompareGateMetricArg,
    #[command(flatten)]
    workload: WorkloadArgs,
    #[command(flatten)]
    counters: CounterArgs,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompareGateMetricArg {
    ScenarioPrimary,
    Instructions,
}

impl From<CompareGateMetricArg> for crate::SuiteGateMetric {
    fn from(value: CompareGateMetricArg) -> Self {
        match value {
            CompareGateMetricArg::ScenarioPrimary => Self::ScenarioPrimary,
            CompareGateMetricArg::Instructions => Self::Instructions,
        }
    }
}

#[derive(Debug, Args)]
struct ProfileArgs {
    /// Runtime setting: NAME=VALUE sets; NAME alone removes an inherited setting. Repeatable.
    #[arg(long = "env", value_name = "NAME[=VALUE]")]
    execution_overrides: Vec<crate::ExecutionOverride>,
    /// Registered scenario to execute.
    scenario: ScenarioId,
    /// Native sampling backend.
    #[arg(long, value_enum, default_value_t = NativeProfilerArg::Perf)]
    profiler: NativeProfilerArg,
    /// Portion of the scenario sampled by the native profiler.
    #[arg(long, value_enum, default_value_t = ProfileScopeArg::EditLoop)]
    scope: ProfileScopeArg,
    /// Self-time skips stack unwinding for a faster report; raw stacks are retained.
    #[arg(long, value_enum, default_value_t = ProfileReportStyleArg::CallGraph)]
    report_style: ProfileReportStyleArg,
    /// Event to sample; unavailable hardware events fail without a clock fallback.
    #[arg(long, value_enum, default_value_t = PerfSamplingEventArg::CpuClock)]
    sampling_event: PerfSamplingEventArg,
    /// Events per sample (defaults to 4000000 for instruction sampling).
    #[arg(long, conflicts_with = "sample_frequency")]
    sample_period: Option<NonZeroU64>,
    /// Adaptive samples per second (defaults to 999 for CPU clock sampling).
    #[arg(long, conflicts_with = "sample_period")]
    sample_frequency: Option<NonZeroU32>,
    /// Stack capture method; LBR requires hardware branch recording support.
    #[arg(long, value_enum, default_value_t = PerfCallGraphArg::Dwarf)]
    call_graph: PerfCallGraphArg,
    /// Editor executable (defaults to target/profiling/neomacs).
    #[arg(long)]
    editor: Option<PathBuf>,
    #[command(flatten)]
    workload: WorkloadArgs,
}

impl ProfileArgs {
    fn capture_configuration(&self) -> PerfCaptureConfiguration {
        let event = match self.sampling_event {
            PerfSamplingEventArg::CpuClock => PerfSamplingEvent::UserCpuClock,
            PerfSamplingEventArg::Instructions => PerfSamplingEvent::UserInstructions,
            PerfSamplingEventArg::CoreInstructions => PerfSamplingEvent::UserCoreInstructions,
        };
        let sampling = match (self.sample_period, self.sample_frequency) {
            (Some(sample_period), _) => PerfSamplingRate::Period { sample_period },
            (_, Some(frequency_hz)) => PerfSamplingRate::Frequency { frequency_hz },
            _ if event == PerfSamplingEvent::UserCpuClock => {
                PerfCaptureConfiguration::standard().sampling
            }
            _ => PerfSamplingRate::Period {
                sample_period: NonZeroU64::new(4_000_000).expect("4000000 is non-zero"),
            },
        };
        let call_graph = match self.call_graph {
            PerfCallGraphArg::Dwarf => PerfCaptureConfiguration::standard().call_graph,
            PerfCallGraphArg::Lbr => PerfCallGraph::Lbr,
        };
        PerfCaptureConfiguration {
            event,
            sampling,
            call_graph,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PerfSamplingEventArg {
    CpuClock,
    Instructions,
    /// Linux P-core PMU, for hosts with hybrid CPUs.
    CoreInstructions,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PerfCallGraphArg {
    Dwarf,
    Lbr,
}

#[derive(Debug, Args)]
struct SuiteArgs {
    /// Registered suite to execute.
    suite: SuiteId,
    /// Baseline editor executable.
    #[arg(long)]
    baseline_editor: PathBuf,
    /// Candidate editor executable.
    #[arg(long)]
    candidate_editor: PathBuf,
    /// Repetitions per editor and scenario; must be at least three.
    #[arg(long, default_value_t = DEFAULT_SAMPLES)]
    samples: ComparisonSampleCount,
    /// Hard deadline for each editor process.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout_secs: NonZeroU64,
    /// Pin each editor process to one Linux logical CPU.
    #[arg(long)]
    cpu: Option<u32>,
    /// Reject the suite unless the selected CPU uses this scaling governor.
    #[arg(long)]
    require_governor: Option<String>,
    /// Explicitly permit running a cohort-gated suite on a loaded local host,
    /// stamping all artifacts with `provenance=local-UNTRUSTED`.
    #[arg(long)]
    allow_local: bool,
    /// Link this artifact to an earlier immutable suite artifact.
    #[arg(long)]
    previous_suite: Option<PathBuf>,
    /// Verify the baseline executable and its sibling portable dump against a
    /// committed baseline manifest JSON before running any scenario.
    #[arg(long)]
    baseline_manifest: Option<PathBuf>,
    #[command(flatten)]
    counters: CounterArgs,
}

#[derive(Debug, Args)]
struct WorkloadArgs {
    /// Number of workload operations (uses the scenario-owned default).
    #[arg(long)]
    iterations: Option<NonZeroU32>,
    /// Editor frontend used for the workload.
    #[arg(long, value_enum)]
    frontend: Option<FrontendArg>,
    /// Hard deadline for the editor process.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout_secs: NonZeroU64,
    /// Pin the editor process to one Linux logical CPU.
    #[arg(long)]
    cpu: Option<u32>,
    /// Reject the run unless the selected CPU uses this scaling governor.
    #[arg(long)]
    require_governor: Option<String>,
    /// Explicitly permit running a cohort-gated scenario on a loaded local
    /// host, stamping all artifacts with `provenance=local-UNTRUSTED`.
    #[arg(long)]
    allow_local: bool,
    /// Video input for the sustained-native-video scenario.
    #[arg(long)]
    video_file: Option<PathBuf>,
    /// External yearly journal the org-journal-open scenario works from (by
    /// copying; the source file is never written).
    #[arg(long)]
    journal_file: Option<PathBuf>,
}

impl WorkloadArgs {
    fn into_semantic(
        self,
        scenario_id: ScenarioId,
    ) -> (
        NonZeroU32,
        Option<Frontend>,
        Duration,
        MachinePolicy,
        Option<PathBuf>,
        Option<PathBuf>,
    ) {
        let default_iterations = crate::scenario(scenario_id).default_iterations;
        (
            self.iterations.unwrap_or(default_iterations),
            self.frontend.map(Frontend::from),
            Duration::from_secs(self.timeout_secs.get()),
            MachinePolicy {
                cpu: self.cpu,
                required_governor: self.require_governor,
                allow_local: self.allow_local,
            },
            self.video_file,
            self.journal_file,
        )
    }
}

#[derive(Debug, Args)]
struct CounterArgs {
    /// Collect Linux perf hardware counters around the editor process.
    #[arg(long)]
    hardware_counters: bool,
    /// Portion of the scenario observed by hardware counters (defaults to
    /// `whole-process` for `startup` and `edit-loop` for all other scenarios).
    #[arg(long, value_enum)]
    counter_scope: Option<CounterScopeArg>,
}

impl CounterArgs {
    fn scope(&self) -> Option<CounterScope> {
        self.hardware_counters.then(|| {
            self.counter_scope
                .map(Into::into)
                .unwrap_or(CounterScope::EditLoop)
        })
    }

    fn scope_for_scenario(&self, scenario: ScenarioId) -> Option<CounterScope> {
        self.hardware_counters.then(|| {
            self.counter_scope.map(Into::into).unwrap_or_else(|| {
                if scenario == ScenarioId::Startup {
                    CounterScope::WholeProcess
                } else {
                    CounterScope::EditLoop
                }
            })
        })
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CounterScopeArg {
    EditLoop,
    WholeProcess,
    /// The edit loop, main (eval) thread only: `perf stat --no-inherit`.
    EditLoopMainThread,
    /// The whole process, main (eval) thread only.
    WholeProcessMainThread,
}

impl From<CounterScopeArg> for CounterScope {
    fn from(scope: CounterScopeArg) -> Self {
        match scope {
            CounterScopeArg::EditLoop => Self::EditLoop,
            CounterScopeArg::WholeProcess => Self::WholeProcess,
            CounterScopeArg::EditLoopMainThread => Self::EditLoopMainThread,
            CounterScopeArg::WholeProcessMainThread => Self::WholeProcessMainThread,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FrontendArg {
    Batch,
    Tui,
    Gui,
}

impl From<FrontendArg> for Frontend {
    fn from(frontend: FrontendArg) -> Self {
        match frontend {
            FrontendArg::Batch => Self::Batch,
            FrontendArg::Tui => Self::Tui {
                rows: 40,
                columns: 120,
            },
            FrontendArg::Gui => Self::Gui {
                width: 1200,
                height: 800,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum NativeProfilerArg {
    Perf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProfileScopeArg {
    EditLoop,
    WholeProcess,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProfileReportStyleArg {
    CallGraph,
    SelfTime,
}

impl From<ProfileReportStyleArg> for ProfileReportStyle {
    fn from(style: ProfileReportStyleArg) -> Self {
        match style {
            ProfileReportStyleArg::CallGraph => Self::CallGraph,
            ProfileReportStyleArg::SelfTime => Self::SelfTime,
        }
    }
}

impl From<ProfileScopeArg> for ProfileScope {
    fn from(scope: ProfileScopeArg) -> Self {
        match scope {
            ProfileScopeArg::EditLoop => Self::EditLoop,
            ProfileScopeArg::WholeProcess => Self::WholeProcess,
        }
    }
}

impl From<NativeProfilerArg> for NativeProfiler {
    fn from(profiler: NativeProfilerArg) -> Self {
        match profiler {
            NativeProfilerArg::Perf => Self::Perf,
        }
    }
}

#[derive(Debug, Error)]
pub enum PerfCliError {
    #[error("{message}")]
    Usage { message: String },
    #[error(transparent)]
    Harness(#[from] PerfError),
    #[error("performance run was rejected; inspect {artifact}: {reason}")]
    RunRejected { artifact: PathBuf, reason: String },
    #[error("performance comparison was rejected; inspect {artifact}: {reason}")]
    ComparisonRejected { artifact: PathBuf, reason: String },
    #[error("native performance profile was rejected; inspect {artifact}: {reason}")]
    ProfileRejected { artifact: PathBuf, reason: String },
    #[error("performance suite was rejected; inspect {artifact}: {reason}")]
    SuiteRejected { artifact: PathBuf, reason: String },
}

pub fn parse_perf_command(
    args: impl IntoIterator<Item = OsString>,
) -> Result<PerfCommand, PerfCliError> {
    let arguments = std::iter::once(OsString::from("cargo xtask perf")).chain(args);
    match PerfCli::try_parse_from(arguments) {
        Ok(cli) => cli.command.try_into(),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(PerfCommand::Help {
                rendered: error.to_string(),
            })
        }
        Err(error) => Err(PerfCliError::Usage {
            message: error.to_string(),
        }),
    }
}

impl TryFrom<PerfSubcommand> for PerfCommand {
    type Error = PerfCliError;
    fn try_from(command: PerfSubcommand) -> Result<Self, Self::Error> {
        Ok(match command {
            PerfSubcommand::List => Self::List,
            PerfSubcommand::InputLatency { path, budget_us } => {
                Self::InputLatency { path, budget_us }
            }
            PerfSubcommand::Run(arguments) => {
                let execution_overrides = arguments
                    .execution_overrides
                    .try_into()
                    .map_err(|message| PerfCliError::Usage { message })?;
                let counters = arguments.counters.scope_for_scenario(arguments.scenario);
                let (iterations, frontend, timeout, machine, video_file, journal_file) =
                    arguments.workload.into_semantic(arguments.scenario);
                Self::Run {
                    scenario: arguments.scenario,
                    editor: arguments.editor,
                    editor_arguments: arguments.editor_arguments,
                    iterations,
                    frontend,
                    timeout,
                    machine,
                    counters,
                    video_file,
                    journal_file,
                    execution_overrides,
                }
            }
            PerfSubcommand::Compare(arguments) => {
                let baseline_execution_overrides = arguments
                    .baseline_execution_overrides
                    .try_into()
                    .map_err(|message| PerfCliError::Usage { message })?;
                let candidate_execution_overrides = arguments
                    .candidate_execution_overrides
                    .try_into()
                    .map_err(|message| PerfCliError::Usage { message })?;
                let counters = arguments.counters.scope_for_scenario(arguments.scenario);
                let gate_metric: crate::SuiteGateMetric = arguments.gate_metric.into();
                if gate_metric.requires_hardware_counters() && counters.is_none() {
                    return Err(PerfCliError::Usage {
                        message: "--gate-metric instructions requires --hardware-counters"
                            .to_string(),
                    });
                }
                let (iterations, frontend, timeout, machine, video_file, journal_file) =
                    arguments.workload.into_semantic(arguments.scenario);
                Self::Compare {
                    scenario: arguments.scenario,
                    baseline_editor: arguments.baseline_editor,
                    candidate_editor: arguments.candidate_editor,
                    samples: arguments.samples,
                    iterations,
                    frontend,
                    timeout,
                    machine,
                    counters,
                    gate_metric,
                    video_file,
                    journal_file,
                    baseline_execution_overrides,
                    candidate_execution_overrides,
                }
            }
            PerfSubcommand::Profile(arguments) => {
                let configuration = arguments.capture_configuration();
                let execution_overrides = arguments
                    .execution_overrides
                    .try_into()
                    .map_err(|message| PerfCliError::Usage { message })?;
                let (iterations, frontend, timeout, machine, video_file, journal_file) =
                    arguments.workload.into_semantic(arguments.scenario);
                Self::Profile {
                    scenario: arguments.scenario,
                    profiler: arguments.profiler.into(),
                    scope: arguments.scope.into(),
                    report_style: arguments.report_style.into(),
                    configuration,
                    editor: arguments.editor,
                    iterations,
                    frontend,
                    timeout,
                    machine,
                    video_file,
                    journal_file,
                    execution_overrides,
                }
            }
            PerfSubcommand::Suite(arguments) => Self::Suite {
                suite: arguments.suite,
                baseline_editor: arguments.baseline_editor,
                candidate_editor: arguments.candidate_editor,
                samples: arguments.samples,
                timeout: Duration::from_secs(arguments.timeout_secs.get()),
                machine: MachinePolicy {
                    cpu: arguments.cpu,
                    required_governor: arguments.require_governor,
                    allow_local: arguments.allow_local,
                },
                counters: arguments.counters.scope(),
                previous_suite: arguments.previous_suite,
                baseline_manifest: arguments.baseline_manifest,
            },
        })
    }
}

pub fn run_cli(
    workspace_root: impl AsRef<Path>,
    args: impl IntoIterator<Item = OsString>,
) -> Result<(), PerfCliError> {
    let workspace_root = workspace_root.as_ref();
    match parse_perf_command(args)? {
        PerfCommand::InputLatency { path, budget_us } => {
            let text = std::fs::read_to_string(&path).map_err(|error| PerfCliError::Usage {
                message: format!("{}: {error}", path.display()),
            })?;
            let report = crate::input_latency::report(&text, budget_us.get())
                .map_err(|message| PerfCliError::Usage { message })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("valid report")
            );
            Ok(())
        }
        PerfCommand::List => {
            for scenario in scenarios() {
                println!("{}\t{}", scenario.id, scenario.description);
            }
            Ok(())
        }
        PerfCommand::Help { rendered } => {
            print!("{rendered}");
            Ok(())
        }
        PerfCommand::Run {
            scenario,
            editor,
            editor_arguments,
            iterations,
            frontend,
            timeout,
            machine,
            counters,
            video_file,
            journal_file,
            execution_overrides,
        } => {
            let editor = editor.unwrap_or_else(|| workspace_root.join("target/release/neomacs"));
            let mut request = RunRequest::new(scenario, editor, iterations)
                .with_editor_arguments(editor_arguments)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_counters(counters)
                .with_video_file(video_file)
                .with_journal_file(journal_file)
                .with_execution_overrides(execution_overrides);
            if let Some(frontend) = frontend {
                request = request.with_frontend(frontend);
            }
            let report = PerfHarness::new(workspace_root).run(&request)?;
            println!("artifact = {}", report.artifact_path.display());
            if let Some(provenance) = report.artifact.host.provenance {
                println!("provenance = {provenance}");
            }
            if let Some(loadavg) = &report.artifact.host.loadavg {
                println!("loadavg  = {loadavg}");
            }
            if report.artifact.verdict.is_valid() {
                println!("verdict  = valid");
                Ok(())
            } else {
                Err(PerfCliError::RunRejected {
                    artifact: report.artifact_path,
                    reason: format!("{:?}", report.artifact.verdict),
                })
            }
        }
        PerfCommand::Compare {
            scenario,
            baseline_editor,
            candidate_editor,
            samples,
            iterations,
            frontend,
            timeout,
            machine,
            counters,
            gate_metric,
            video_file,
            journal_file,
            baseline_execution_overrides,
            candidate_execution_overrides,
        } => {
            let mut request = ComparisonRequest::new(
                scenario,
                baseline_editor,
                candidate_editor,
                samples,
                iterations,
            )
            .with_timeout(timeout)
            .with_machine_policy(machine)
            .with_counters(counters)
            .with_primary_metric(gate_metric.metric_for(scenario))
            .with_video_file(video_file)
            .with_journal_file(journal_file)
            .with_execution_overrides(baseline_execution_overrides, candidate_execution_overrides);
            if let Some(frontend) = frontend {
                request = request.with_frontend(frontend);
            }
            let report = PerfHarness::new(workspace_root).compare(&request)?;
            println!("comparison = {}", report.artifact_path.display());
            if let Some(provenance) = report.artifact.provenance {
                println!("provenance = {provenance}");
            }
            if let Some(loadavg) = &report.artifact.host_loadavg {
                println!("loadavg    = {loadavg}");
            }
            match &report.artifact.verdict {
                ComparisonVerdict::Valid {
                    summary,
                    context_metrics,
                } => {
                    for line in format_comparison_summary_lines(
                        summary,
                        context_metrics,
                        report.artifact.input.iterations.get(),
                    ) {
                        println!("{line}");
                    }
                    Ok(())
                }
                ComparisonVerdict::Rejected { reasons } => Err(PerfCliError::ComparisonRejected {
                    artifact: report.artifact_path,
                    reason: format!("{reasons:?}"),
                }),
            }
        }
        PerfCommand::Profile {
            scenario,
            profiler,
            scope,
            report_style,
            configuration,
            editor,
            iterations,
            frontend,
            timeout,
            machine,
            video_file,
            journal_file,
            execution_overrides,
        } => {
            let editor = editor.unwrap_or_else(|| workspace_root.join("target/profiling/neomacs"));
            let mut request = ProfileRequest::new(scenario, editor, iterations, profiler)
                .with_scope(scope)
                .with_report_style(report_style)
                .with_capture_configuration(configuration)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_video_file(video_file)
                .with_journal_file(journal_file)
                .with_execution_overrides(execution_overrides);
            if let Some(frontend) = frontend {
                request = request.with_frontend(frontend);
            }
            let report = PerfHarness::new(workspace_root).profile(&request)?;
            println!("profile = {}", report.artifact_path.display());
            match &report.artifact.verdict {
                ProfileVerdict::Captured {
                    perf_data_path,
                    hotspot_report_path,
                    sample_count,
                } => {
                    let directory = report
                        .artifact_path
                        .parent()
                        .expect("profile artifact has a parent directory");
                    println!("verdict = captured ({sample_count} sampled stacks)");
                    println!("data    = {}", directory.join(perf_data_path).display());
                    println!(
                        "report  = {}",
                        directory.join(hotspot_report_path).display()
                    );
                    println!("note    = instrumented timings are diagnostic, not comparable");
                    Ok(())
                }
                ProfileVerdict::Rejected { reason } => Err(PerfCliError::ProfileRejected {
                    artifact: report.artifact_path,
                    reason: format!("{reason:?}"),
                }),
            }
        }
        PerfCommand::Suite {
            suite,
            baseline_editor,
            candidate_editor,
            samples,
            timeout,
            machine,
            counters,
            previous_suite,
            baseline_manifest,
        } => {
            let request = SuiteRequest::new(suite, baseline_editor, candidate_editor, samples)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_counters(counters)
                .with_previous_suite(previous_suite)
                .with_baseline_manifest(baseline_manifest);
            let report = PerfHarness::new(workspace_root).suite(&request)?;
            println!("suite   = {}", report.artifact_path.display());
            if let Some(provenance) = report.artifact.provenance {
                println!("provenance = {provenance}");
            }
            if let Some(loadavg) = &report.artifact.host_loadavg {
                println!("loadavg = {loadavg}");
            }
            for row in &report.artifact.scenarios {
                println!("{}", format_suite_row_summary(row));
            }
            for skip in &report.artifact.skipped_drifts {
                println!(
                    "warning = drift check skipped for {}: {}",
                    skip.scenario, skip.reason
                );
            }
            match &report.artifact.verdict {
                SuiteVerdict::Passed => {
                    println!("verdict = passed");
                    Ok(())
                }
                SuiteVerdict::Regressed { regressions } => Err(PerfCliError::SuiteRejected {
                    artifact: report.artifact_path,
                    reason: format!("regressions: {regressions:?}"),
                }),
                SuiteVerdict::Drifted { drifts } => Err(PerfCliError::SuiteRejected {
                    artifact: report.artifact_path,
                    reason: format!(
                        "drifted against the previous suite (each row is inside \
                         its own budget; the movement is the accumulation): {drifts:?}"
                    ),
                }),
                SuiteVerdict::Rejected { scenarios } => Err(PerfCliError::SuiteRejected {
                    artifact: report.artifact_path,
                    reason: format!("rejected scenarios: {scenarios:?}"),
                }),
            }
        }
    }
}

fn format_metric_value(value: f64, small_scale: bool) -> String {
    if small_scale {
        format!("{value:.6}")
    } else {
        format!("{value:.3}")
    }
}

fn format_metric_delta(
    summary: &crate::ComparisonMetricSummary,
    iterations: Option<u32>,
) -> String {
    let small_scale = summary.baseline_median.abs() < 1.0 || summary.candidate_median.abs() < 1.0;
    let per_op = match (summary.metric, iterations) {
        (crate::MetricName::Instructions, Some(iters)) if iters > 0 => format!(
            " ({:.1}/op -> {:.1}/op)",
            summary.baseline_median / f64::from(iters),
            summary.candidate_median / f64::from(iters)
        ),
        _ => String::new(),
    };
    format!(
        "{:?} baseline={} ± {} MAD candidate={} ± {} MAD {:?}{} ({:+.2}%)",
        summary.metric,
        format_metric_value(summary.baseline_median, small_scale),
        format_metric_value(summary.baseline_median_absolute_deviation, small_scale),
        format_metric_value(summary.candidate_median, small_scale),
        format_metric_value(summary.candidate_median_absolute_deviation, small_scale),
        summary.unit,
        per_op,
        summary.percent_change,
    )
}

pub(crate) fn format_comparison_summary_lines(
    summary: &crate::ComparisonMetricSummary,
    context_metrics: &[crate::ComparisonMetricSummary],
    iterations: u32,
) -> Vec<String> {
    let mut lines = vec![
        "verdict    = valid".to_string(),
        format!(
            "gate       = {}",
            format_metric_delta(summary, Some(iterations))
        ),
        format!(
            "baseline   = {:.3} ± {:.3} MAD {:?}",
            summary.baseline_median, summary.baseline_median_absolute_deviation, summary.unit
        ),
        format!(
            "candidate  = {:.3} ± {:.3} MAD {:?}",
            summary.candidate_median, summary.candidate_median_absolute_deviation, summary.unit
        ),
        format!("change     = {:+.2}%", summary.percent_change),
        format!(
            "pairs      = {}",
            summary
                .paired_samples
                .iter()
                .map(|pair| format!("{}:{:+.2}%", pair.sample_index, pair.percent_change))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ];
    for context in context_metrics {
        lines.push(format!(
            "context    = [non-gating] {}",
            format_metric_delta(context, Some(iterations))
        ));
    }
    lines
}

fn sample_span_percent(summary: &crate::ComparisonMetricSummary) -> Option<f64> {
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;
    for &v in summary
        .baseline_samples
        .iter()
        .chain(summary.candidate_samples.iter())
    {
        if !v.is_finite() || v <= 0.0 {
            return None;
        }
        if v < min_val {
            min_val = v;
        }
        if v > max_val {
            max_val = v;
        }
    }
    if min_val.is_finite() && max_val.is_finite() && min_val > 0.0 {
        Some((max_val - min_val) / min_val * 100.0)
    } else {
        None
    }
}

pub(crate) fn format_suite_row_summary(row: &crate::SuiteScenarioResult) -> String {
    let non_convertible = row
        .non_convertible_reason
        .map(|reason| format!(" [non-convertible: {reason}]"))
        .unwrap_or_default();
    let gate_part = if row.gate_metric == crate::SuiteGateMetric::NonGatingReport {
        let label = row
            .non_convertible_reason
            .map(|reason| format!("non-gating: {reason}"))
            .unwrap_or_else(|| "non-gating".to_string());
        match (&row.gate_summary, row.percent_change) {
            (Some(summary), _) => {
                let span = sample_span_percent(summary)
                    .map(|s| format!(" span={s:.1}%"))
                    .unwrap_or_default();
                format!(
                    "REPORT [{label}] [{}{span}]",
                    format_metric_delta(summary, row.iterations)
                )
            }
            (None, Some(change)) => format!(
                "REPORT [{label}] [{:?} {:+.2}%]",
                row.gate_metric.metric_for(row.scenario),
                change
            ),
            (None, None) => format!(
                "REPORT [{label}] [{:?} REJECTED]",
                row.gate_metric.metric_for(row.scenario)
            ),
        }
    } else {
        match (&row.gate_summary, row.percent_change) {
            (Some(summary), _) => format!(
                "GATE [{}] (budget {:+.2}%){}",
                format_metric_delta(summary, row.iterations),
                row.maximum_regression_percent,
                non_convertible
            ),
            (None, Some(change)) => format!(
                "GATE [{:?} {:+.2}%] (budget {:+.2}%){}",
                row.gate_metric.metric_for(row.scenario),
                change,
                row.maximum_regression_percent,
                non_convertible
            ),
            (None, None) => format!(
                "GATE [{:?} REJECTED] (budget {:+.2}%){}",
                row.gate_metric.metric_for(row.scenario),
                row.maximum_regression_percent,
                non_convertible
            ),
        }
    };
    if row.context_metrics.is_empty() {
        return format!("row {}: {}", row.scenario, gate_part);
    }
    let contexts = row
        .context_metrics
        .iter()
        .map(|summary| format_metric_delta(summary, row.iterations))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "row {}: {} | CONTEXT [non-gating]: {}",
        row.scenario, gate_part, contexts
    )
}

#[cfg(test)]
#[path = "cli/tests/cli_test.rs"]
mod tests;
