use std::fs;
use std::path::PathBuf;

use super::{
    ComparisonSampleCount, MachinePolicy, PerfError, SUITE_ARTIFACT_SCHEMA_VERSION, ScenarioId,
    SuiteArtifact, SuiteId, SuiteScenarioResult, SuiteVerdict, evaluate_suite, read_history_link,
};

fn result(scenario: ScenarioId, threshold: f64, change: Option<f64>) -> SuiteScenarioResult {
    SuiteScenarioResult {
        scenario,
        maximum_regression_percent: threshold,
        maximum_drift_percent: None,
        percent_change: change,
        comparison_artifact: PathBuf::from(format!("{scenario}/comparison.json")),
    }
}

#[test]
fn suite_thresholds_distinguish_improvements_regressions_and_rejections() {
    assert_eq!(
        evaluate_suite_for_test(&[
            result(ScenarioId::Startup, 12.0, Some(11.9)),
            result(ScenarioId::RegexSearch, 8.0, Some(-20.0)),
        ]),
        SuiteVerdict::Passed
    );

    let SuiteVerdict::Regressed { regressions } = evaluate_suite_for_test(&[
        result(ScenarioId::Startup, 12.0, Some(12.1)),
        result(ScenarioId::RegexSearch, 8.0, Some(-20.0)),
    ]) else {
        panic!("a change beyond the scenario budget must regress the suite")
    };
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].scenario, ScenarioId::Startup);

    assert_eq!(
        evaluate_suite_for_test(&[result(ScenarioId::OrgEditing, 8.0, None)]),
        SuiteVerdict::Rejected {
            scenarios: vec![ScenarioId::OrgEditing],
        }
    );
}

#[test]
fn history_rejects_an_unknown_suite_artifact_schema() {
    let workspace_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace scratch root");
    let directory = tempfile::Builder::new()
        .prefix("neomacs-perf-suite-history-")
        .tempdir_in(workspace_tmp)
        .expect("create suite history scratch directory");
    let path = directory.path().join("suite.json");
    let mut artifact = SuiteArtifact {
        schema_version: 99,
        suite_id: "old-standard-suite".to_string(),
        suite: SuiteId::Standard,
        baseline_editor: PathBuf::from("baseline-emacs"),
        candidate_editor: PathBuf::from("candidate-emacs"),
        samples_per_side: ComparisonSampleCount::new(3).expect("valid sample count"),
        machine: MachinePolicy::default(),
        counters: None,
        started_unix_ms: 1,
        total_elapsed_us: 2,
        previous_suite: None,
        scenarios: Vec::new(),
        verdict: SuiteVerdict::Passed,
    };
    fs::write(
        &path,
        serde_json::to_vec_pretty(&artifact).expect("serialize old suite artifact"),
    )
    .expect("write old suite artifact");

    let error = read_history_link(&path).expect_err("unknown suite schemas must not form lineage");
    let PerfError::InvalidSuiteHistory { message, .. } = error else {
        panic!("suite history schema rejection must be typed")
    };
    assert!(message.contains("schema version 99"));

    artifact.schema_version = SUITE_ARTIFACT_SCHEMA_VERSION;
    fs::write(
        &path,
        serde_json::to_vec_pretty(&artifact).expect("serialize current suite artifact"),
    )
    .expect("write current suite artifact");
    let link = read_history_link(&path).expect("current suite schema forms valid lineage");
    assert_eq!(link.suite_id, "old-standard-suite");
    assert_eq!(link.retained_path, PathBuf::from("previous-suite.json"));
    assert_eq!(link.sha256.len(), 64);
    assert!(link.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

/// `evaluate_suite` with no history, which is what these cases exercise.
fn evaluate_suite_for_test(scenarios: &[SuiteScenarioResult]) -> SuiteVerdict {
    evaluate_suite(scenarios, None)
}

fn drifting_result(
    scenario: ScenarioId,
    threshold: f64,
    drift_allowance: Option<f64>,
    change: Option<f64>,
) -> SuiteScenarioResult {
    SuiteScenarioResult {
        scenario,
        maximum_regression_percent: threshold,
        maximum_drift_percent: drift_allowance,
        percent_change: change,
        comparison_artifact: PathBuf::from(format!("{scenario}/comparison.json")),
    }
}

fn suite_history(scenarios: Vec<SuiteScenarioResult>) -> SuiteArtifact {
    SuiteArtifact {
        schema_version: SUITE_ARTIFACT_SCHEMA_VERSION,
        suite_id: "previous-suite".to_string(),
        suite: SuiteId::Standard,
        baseline_editor: PathBuf::from("baseline-emacs"),
        candidate_editor: PathBuf::from("candidate-emacs"),
        samples_per_side: ComparisonSampleCount::new(3).expect("valid sample count"),
        machine: MachinePolicy::default(),
        counters: None,
        started_unix_ms: 1,
        total_elapsed_us: 2,
        previous_suite: None,
        scenarios,
        verdict: SuiteVerdict::Passed,
    }
}

/// The ratchet catches what the per-run budgets structurally cannot: a row that
/// regresses a little on every run and passes its own budget every time.
///
/// The budget compares candidate against baseline within one run, so a 10%
/// budget accepts five separate 2% regressions. Only comparing against the
/// previous run sees the accumulation.
#[test]
fn drift_against_the_previous_suite_fails_even_when_every_row_is_inside_its_budget() {
    let previous = suite_history(vec![drifting_result(
        ScenarioId::MagitStatus,
        10.0,
        Some(2.0),
        Some(1.0),
    )]);
    // 1.0% -> 4.5% is a 3.5-point move: comfortably inside the 10% budget,
    // and past the 2% the row is ratcheted at.
    let current = [drifting_result(
        ScenarioId::MagitStatus,
        10.0,
        Some(2.0),
        Some(4.5),
    )];

    let SuiteVerdict::Drifted { drifts } = evaluate_suite(&current, Some(&previous)) else {
        panic!("accumulated drift inside the budget must still fail the suite")
    };
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].scenario, ScenarioId::MagitStatus);
    assert!((drifts[0].drift_percent - 3.5).abs() < 1e-9);
    assert!((drifts[0].previous_percent_change - 1.0).abs() < 1e-9);

    // The same movement on an unarmed row is recorded and not gated: a gate
    // that fires on a scenario's own noise gets switched off.
    let unarmed = [drifting_result(
        ScenarioId::MagitStatus,
        10.0,
        None,
        Some(4.5),
    )];
    assert_eq!(
        evaluate_suite(&unarmed, Some(&previous)),
        SuiteVerdict::Passed
    );

    // With no history there is nothing to ratchet against.
    assert_eq!(evaluate_suite(&current, None), SuiteVerdict::Passed);

    // A row over its own budget is a regression, not drift: the more specific
    // failure wins so the report names the real problem.
    let over_budget = [drifting_result(
        ScenarioId::MagitStatus,
        10.0,
        Some(2.0),
        Some(12.0),
    )];
    assert!(matches!(
        evaluate_suite(&over_budget, Some(&previous)),
        SuiteVerdict::Regressed { .. }
    ));

    // Improving by more than the allowance is not drift.
    let improved = [drifting_result(
        ScenarioId::MagitStatus,
        10.0,
        Some(2.0),
        Some(-6.0),
    )];
    assert_eq!(
        evaluate_suite(&improved, Some(&previous)),
        SuiteVerdict::Passed
    );
}
