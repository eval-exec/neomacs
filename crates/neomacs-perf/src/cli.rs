use std::ffi::OsString;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};
use thiserror::Error;

use crate::{
    ComparisonRequest, ComparisonSampleCount, ComparisonVerdict, CounterScope, Frontend,
    MachinePolicy, NativeProfiler, PerfError, PerfHarness, ProfileRequest, ProfileScope,
    ProfileVerdict, RunRequest, ScenarioId, SuiteId, SuiteRequest, SuiteVerdict, scenarios,
};

const DEFAULT_SAMPLES: ComparisonSampleCount =
    ComparisonSampleCount::new(5).expect("5 meets the minimum sample count");
const DEFAULT_TIMEOUT_SECS: NonZeroU64 = NonZeroU64::new(300).expect("300 is non-zero");

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PerfCommand {
    List,
    Run {
        scenario: ScenarioId,
        editor: Option<PathBuf>,
        iterations: NonZeroU32,
        frontend: Option<Frontend>,
        timeout: Duration,
        machine: MachinePolicy,
        counters: Option<CounterScope>,
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
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
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
    },
    Profile {
        scenario: ScenarioId,
        profiler: NativeProfiler,
        scope: ProfileScope,
        editor: Option<PathBuf>,
        iterations: NonZeroU32,
        frontend: Option<Frontend>,
        timeout: Duration,
        machine: MachinePolicy,
        video_file: Option<PathBuf>,
        journal_file: Option<PathBuf>,
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
    #[command(flatten)]
    workload: WorkloadArgs,
    #[command(flatten)]
    counters: CounterArgs,
}

#[derive(Debug, Args)]
struct ProfileArgs {
    /// Registered scenario to execute.
    scenario: ScenarioId,
    /// Native sampling backend.
    #[arg(long, value_enum, default_value_t = NativeProfilerArg::Perf)]
    profiler: NativeProfilerArg,
    /// Portion of the scenario sampled by the native profiler.
    #[arg(long, value_enum, default_value_t = ProfileScopeArg::EditLoop)]
    scope: ProfileScopeArg,
    /// Editor executable (defaults to target/profiling/neomacs).
    #[arg(long)]
    editor: Option<PathBuf>,
    #[command(flatten)]
    workload: WorkloadArgs,
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
    /// Link this artifact to an earlier immutable suite artifact.
    #[arg(long)]
    previous_suite: Option<PathBuf>,
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
    /// Portion of the scenario observed by hardware counters.
    #[arg(long, value_enum, default_value_t = CounterScopeArg::EditLoop)]
    counter_scope: CounterScopeArg,
}

impl CounterArgs {
    fn scope(&self) -> Option<CounterScope> {
        self.hardware_counters.then(|| self.counter_scope.into())
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CounterScopeArg {
    EditLoop,
    WholeProcess,
}

impl From<CounterScopeArg> for CounterScope {
    fn from(scope: CounterScopeArg) -> Self {
        match scope {
            CounterScopeArg::EditLoop => Self::EditLoop,
            CounterScopeArg::WholeProcess => Self::WholeProcess,
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
        Ok(cli) => Ok(cli.command.into()),
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

impl From<PerfSubcommand> for PerfCommand {
    fn from(command: PerfSubcommand) -> Self {
        match command {
            PerfSubcommand::List => Self::List,
            PerfSubcommand::Run(arguments) => {
                let counters = arguments.counters.scope();
                let (iterations, frontend, timeout, machine, video_file, journal_file) =
                    arguments.workload.into_semantic(arguments.scenario);
                Self::Run {
                    scenario: arguments.scenario,
                    editor: arguments.editor,
                    iterations,
                    frontend,
                    timeout,
                    machine,
                    counters,
                    video_file,
                    journal_file,
                }
            }
            PerfSubcommand::Compare(arguments) => {
                let counters = arguments.counters.scope();
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
                    video_file,
                    journal_file,
                }
            }
            PerfSubcommand::Profile(arguments) => {
                let (iterations, frontend, timeout, machine, video_file, journal_file) =
                    arguments.workload.into_semantic(arguments.scenario);
                Self::Profile {
                    scenario: arguments.scenario,
                    profiler: arguments.profiler.into(),
                    scope: arguments.scope.into(),
                    editor: arguments.editor,
                    iterations,
                    frontend,
                    timeout,
                    machine,
                    video_file,
                    journal_file,
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
                },
                counters: arguments.counters.scope(),
                previous_suite: arguments.previous_suite,
            },
        }
    }
}

pub fn run_cli(
    workspace_root: impl AsRef<Path>,
    args: impl IntoIterator<Item = OsString>,
) -> Result<(), PerfCliError> {
    let workspace_root = workspace_root.as_ref();
    match parse_perf_command(args)? {
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
            iterations,
            frontend,
            timeout,
            machine,
            counters,
            video_file,
            journal_file,
        } => {
            let editor = editor.unwrap_or_else(|| workspace_root.join("target/release/neomacs"));
            let mut request = RunRequest::new(scenario, editor, iterations)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_counters(counters)
                .with_video_file(video_file)
                .with_journal_file(journal_file);
            if let Some(frontend) = frontend {
                request = request.with_frontend(frontend);
            }
            let report = PerfHarness::new(workspace_root).run(&request)?;
            println!("artifact = {}", report.artifact_path.display());
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
            video_file,
            journal_file,
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
            .with_video_file(video_file)
            .with_journal_file(journal_file);
            if let Some(frontend) = frontend {
                request = request.with_frontend(frontend);
            }
            let report = PerfHarness::new(workspace_root).compare(&request)?;
            println!("comparison = {}", report.artifact_path.display());
            match &report.artifact.verdict {
                ComparisonVerdict::Valid { summary } => {
                    println!("verdict    = valid");
                    println!(
                        "baseline   = {:.3} ± {:.3} MAD {:?}",
                        summary.baseline_median,
                        summary.baseline_median_absolute_deviation,
                        summary.unit
                    );
                    println!(
                        "candidate  = {:.3} ± {:.3} MAD {:?}",
                        summary.candidate_median,
                        summary.candidate_median_absolute_deviation,
                        summary.unit
                    );
                    println!("change     = {:+.2}%", summary.percent_change);
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
            editor,
            iterations,
            frontend,
            timeout,
            machine,
            video_file,
            journal_file,
        } => {
            let editor = editor.unwrap_or_else(|| workspace_root.join("target/profiling/neomacs"));
            let mut request = ProfileRequest::new(scenario, editor, iterations, profiler)
                .with_scope(scope)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_video_file(video_file)
                .with_journal_file(journal_file);
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
        } => {
            let request = SuiteRequest::new(suite, baseline_editor, candidate_editor, samples)
                .with_timeout(timeout)
                .with_machine_policy(machine)
                .with_counters(counters)
                .with_previous_suite(previous_suite);
            let report = PerfHarness::new(workspace_root).suite(&request)?;
            println!("suite   = {}", report.artifact_path.display());
            match &report.artifact.verdict {
                SuiteVerdict::Passed => {
                    println!("verdict = passed");
                    Ok(())
                }
                SuiteVerdict::Regressed { regressions } => Err(PerfCliError::SuiteRejected {
                    artifact: report.artifact_path,
                    reason: format!("regressions: {regressions:?}"),
                }),
                SuiteVerdict::Rejected { scenarios } => Err(PerfCliError::SuiteRejected {
                    artifact: report.artifact_path,
                    reason: format!("rejected scenarios: {scenarios:?}"),
                }),
            }
        }
    }
}
