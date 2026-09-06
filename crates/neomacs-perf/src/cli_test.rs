use std::ffi::OsString;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use super::{
    ComparisonSampleCount, CounterScope, Frontend, MachinePolicy, NativeProfiler, PerfCommand,
    ProfileScope, ScenarioId, SuiteId, parse_perf_command,
};

fn parse(args: &[&str]) -> Result<PerfCommand, super::PerfCliError> {
    parse_perf_command(args.iter().map(OsString::from))
}

#[test]
fn run_command_parses_into_a_typed_workload_request() {
    assert_eq!(
        parse(&[
            "run",
            "rust-lsp-typing",
            "--editor",
            "target/profiling/neomacs",
            "--iterations",
            "25",
            "--frontend",
            "gui",
            "--timeout-secs",
            "90",
            "--cpu",
            "3",
            "--require-governor",
            "performance",
            "--hardware-counters",
            "--counter-scope",
            "whole-process",
        ])
        .expect("parse valid run command"),
        PerfCommand::Run {
            scenario: ScenarioId::RustLspTyping,
            editor: Some(PathBuf::from("target/profiling/neomacs")),
            iterations: NonZeroU32::new(25).expect("non-zero literal"),
            frontend: Some(Frontend::Gui {
                width: 1200,
                height: 800,
            }),
            timeout: Duration::from_secs(90),
            machine: MachinePolicy {
                cpu: Some(3),
                required_governor: Some("performance".to_string()),
            },
            counters: Some(CounterScope::WholeProcess),
            video_file: None,
            journal_file: None,
        }
    );
}

#[test]
fn run_command_rejects_zero_iterations_before_launch() {
    let error = parse(&["run", "rust-lsp-typing", "--iterations", "0"])
        .expect_err("zero iterations must fail");
    assert!(error.to_string().contains("non-zero type"));
}

#[test]
fn workload_defaults_come_from_the_typed_scenario_spec() {
    let PerfCommand::Run {
        scenario,
        iterations,
        frontend,
        ..
    } = parse(&["run", "mx-tab-completion"]).expect("parse M-x TAB workload")
    else {
        panic!("run command must remain typed")
    };
    assert_eq!(scenario, ScenarioId::MxTabCompletion);
    assert_eq!(iterations, NonZeroU32::new(5).expect("non-zero literal"));
    assert_eq!(frontend, None);
}

#[test]
fn sustained_native_video_parses_its_required_input_asset() {
    let PerfCommand::Run {
        scenario,
        iterations,
        frontend,
        video_file,
        ..
    } = parse(&[
        "run",
        "sustained-native-video",
        "--video-file",
        "target/perf-inputs/4k60.mp4",
    ])
    .expect("parse native-video workload")
    else {
        panic!("run command must remain typed")
    };
    assert_eq!(scenario, ScenarioId::SustainedNativeVideo);
    assert_eq!(iterations, NonZeroU32::new(300).expect("non-zero literal"));
    assert_eq!(frontend, None);
    assert_eq!(
        video_file,
        Some(PathBuf::from("target/perf-inputs/4k60.mp4"))
    );
}

#[test]
fn compare_command_requires_two_editors_and_parses_repetition_controls() {
    assert_eq!(
        parse(&[
            "compare",
            "rust-lsp-typing",
            "--baseline-editor",
            "target/release/neomacs",
            "--candidate-editor",
            "target/release-pgo/neomacs",
            "--samples",
            "7",
            "--iterations",
            "20",
            "--frontend",
            "tui",
            "--timeout-secs",
            "180",
        ])
        .expect("parse valid compare command"),
        PerfCommand::Compare {
            scenario: ScenarioId::RustLspTyping,
            baseline_editor: PathBuf::from("target/release/neomacs"),
            candidate_editor: PathBuf::from("target/release-pgo/neomacs"),
            samples: ComparisonSampleCount::new(7).expect("valid sample count"),
            iterations: NonZeroU32::new(20).expect("non-zero literal"),
            frontend: Some(Frontend::Tui {
                rows: 40,
                columns: 120,
            }),
            timeout: Duration::from_secs(180),
            machine: MachinePolicy::default(),
            counters: None,
            video_file: None,
            journal_file: None,
        }
    );
}

#[test]
fn compare_command_rejects_a_missing_candidate_before_launch() {
    let error = parse(&[
        "compare",
        "rust-lsp-typing",
        "--baseline-editor",
        "target/release/neomacs",
    ])
    .expect_err("both editor identities are required");
    assert!(error.to_string().contains("--candidate-editor"));
    assert!(error.to_string().contains("required arguments"));
}

#[test]
fn compare_command_rejects_fewer_than_three_samples_per_side() {
    let error = parse(&[
        "compare",
        "rust-lsp-typing",
        "--baseline-editor",
        "target/release/neomacs",
        "--candidate-editor",
        "target/release-pgo/neomacs",
        "--samples",
        "2",
    ])
    .expect_err("two samples cannot characterize run-to-run dispersion");
    assert!(
        error
            .to_string()
            .contains("comparison sample count must be at least 3")
    );
}

#[test]
fn profile_command_selects_native_sampling_without_becoming_a_comparison() {
    assert_eq!(
        parse(&[
            "profile",
            "rust-lsp-typing",
            "--profiler",
            "perf",
            "--editor",
            "target/profiling/neomacs",
            "--iterations",
            "40",
            "--frontend",
            "tui",
            "--timeout-secs",
            "180",
        ])
        .expect("parse native profile command"),
        PerfCommand::Profile {
            scenario: ScenarioId::RustLspTyping,
            profiler: NativeProfiler::Perf,
            scope: ProfileScope::EditLoop,
            editor: Some(PathBuf::from("target/profiling/neomacs")),
            iterations: NonZeroU32::new(40).expect("non-zero literal"),
            frontend: Some(Frontend::Tui {
                rows: 40,
                columns: 120,
            }),
            timeout: Duration::from_secs(180),
            machine: MachinePolicy::default(),
            video_file: None,
            journal_file: None,
        }
    );

    let PerfCommand::Profile { scope, .. } =
        parse(&["profile", "rust-lsp-typing", "--scope", "whole-process"])
            .expect("parse explicit whole-process profile")
    else {
        panic!("profile command must remain typed")
    };
    assert_eq!(scope, ProfileScope::WholeProcess);
}

#[test]
fn list_and_help_are_explicit_commands() {
    assert_eq!(parse(&["list"]).expect("parse list"), PerfCommand::List);
    let PerfCommand::Help { rendered } = parse(&["--help"]).expect("parse root help") else {
        panic!("--help must produce rendered help")
    };
    assert!(rendered.contains("Usage: cargo xtask perf <COMMAND>"));

    let PerfCommand::Help { rendered } = parse(&["profile", "--help"]).expect("parse profile help")
    else {
        panic!("profile --help must produce rendered help")
    };
    assert!(rendered.contains("Usage: cargo xtask perf profile [OPTIONS] <SCENARIO>"));
    assert!(rendered.contains("--profiler <PROFILER>"));
    assert!(rendered.contains("--scope <SCOPE>"));
}

#[test]
fn suite_command_parses_regression_and_history_controls() {
    assert_eq!(
        parse(&[
            "suite",
            "standard",
            "--baseline-editor",
            "reference/gnu-emacs",
            "--candidate-editor",
            "target/release/neomacs",
            "--samples",
            "9",
            "--cpu",
            "4",
            "--require-governor",
            "performance",
            "--hardware-counters",
            "--previous-suite",
            "tmp/perf-suites/previous/suite.json",
        ])
        .expect("parse standard suite"),
        PerfCommand::Suite {
            suite: SuiteId::Standard,
            baseline_editor: PathBuf::from("reference/gnu-emacs"),
            candidate_editor: PathBuf::from("target/release/neomacs"),
            samples: ComparisonSampleCount::new(9).expect("valid sample count"),
            timeout: Duration::from_secs(300),
            machine: MachinePolicy {
                cpu: Some(4),
                required_governor: Some("performance".to_string()),
            },
            counters: Some(CounterScope::EditLoop),
            previous_suite: Some(PathBuf::from("tmp/perf-suites/previous/suite.json")),
        }
    );
}
