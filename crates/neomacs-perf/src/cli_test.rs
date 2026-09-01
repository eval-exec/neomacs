use std::ffi::OsString;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use super::{
    ComparisonSampleCount, Frontend, NativeProfiler, PerfCommand, ProfileScope, ScenarioId,
    parse_perf_command,
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
