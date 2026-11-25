use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use crate::{
    ComparisonObservation, ComparisonRejection, ComparisonRun, ComparisonRunOutcome,
    ComparisonRunRole, ComparisonSampleCount, ComparisonVerdict, CounterScope, EditorCapabilities,
    EditorKind, EditorProvenance, Frontend, MachinePolicy, Measurement, MetricName, MetricUnit,
    PerfError, PerfHarness, PortableDumpProvenance, ResolvedGuardTools, RunVerdict,
    SUITE_ARTIFACT_SCHEMA_VERSION, ScenarioId, SuiteArtifact, SuiteGateMetric, SuiteId,
    SuiteRequest, SuiteScenarioResult, SuiteVerdict, VerifiedBaselineManifest, evaluate_comparison,
    evaluate_suite, read_history_link, resolve_guard_tools,
};

fn result(scenario: ScenarioId, threshold: f64, change: Option<f64>) -> SuiteScenarioResult {
    SuiteScenarioResult {
        scenario,
        maximum_regression_percent: threshold,
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::ScenarioPrimary,
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
    let workspace_tmp = crate::workspace_root().join("tmp");
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
        baseline_manifest: None,
        guard_tools: None,
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
        gate_metric: SuiteGateMetric::ScenarioPrimary,
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
        baseline_manifest: None,
        guard_tools: None,
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

fn verified_guard_baseline() -> VerifiedBaselineManifest {
    VerifiedBaselineManifest {
        manifest_path: PathBuf::from(".github/perf-baselines/v0.0.21.json"),
        manifest_sha256: "0".repeat(64),
        version: "v0.0.21".to_string(),
        git_commit: "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a".to_string(),
        binary_path: PathBuf::from("/opt/neomacs-perf/v0.0.21/bin/neomacs"),
        binary_sha256: "Baseline-exe-sha256".to_string(),
        binary_size_bytes: 100,
        pdump_path: PathBuf::from(
            "/opt/neomacs-perf/v0.0.21/libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        ),
        pdump_sha256: "Baseline-pdump-sha256".to_string(),
        pdump_size_bytes: 200,
    }
}

fn resolved_test_guard_tools() -> ResolvedGuardTools {
    ResolvedGuardTools {
        perf_path: PathBuf::from("/usr/bin/perf"),
        taskset_path: Some(PathBuf::from("/usr/bin/taskset")),
    }
}

fn guard_provenance(role: ComparisonRunRole) -> EditorProvenance {
    EditorProvenance {
        path: match role {
            ComparisonRunRole::Baseline => "/opt/neomacs-perf/v0.0.21/bin/neomacs".to_string(),
            ComparisonRunRole::Candidate => "/opt/neomacs-perf/Candidate/neomacs".to_string(),
        },
        executable_sha256: format!("{role:?}-exe-sha256"),
        executable_size_bytes: 100,
        pdump_fingerprint: format!("{role:?}-fp"),
        portable_dump: PortableDumpProvenance::Loaded {
            path: format!("/opt/neomacs-perf/{role:?}/neomacs.pdump"),
            sha256: format!("{role:?}-pdump-sha256"),
            size_bytes: 200,
        },
        version: "Neomacs guard-test".to_string(),
        kind: EditorKind::Neomacs,
        capabilities: EditorCapabilities {
            native_compilation: false,
            tree_sitter: true,
            dynamic_modules: true,
            video_playback: false,
            webview: false,
            embedded_terminal: true,
        },
    }
}

fn guard_observation(
    scenario: ScenarioId,
    iterations: u32,
    role: ComparisonRunRole,
    sample_index: u32,
    measurements: Vec<Measurement>,
) -> ComparisonObservation {
    ComparisonObservation {
        run: ComparisonRun {
            role,
            sample_index,
            run_id: format!("{scenario}-{role:?}-{sample_index}"),
            artifact_path: PathBuf::from(format!(
                "{scenario}-{role:?}-{sample_index}/artifact.json"
            )),
            scenario,
            frontend: Frontend::Batch,
            editor: PathBuf::from(match role {
                ComparisonRunRole::Baseline => "/opt/neomacs-perf/v0.0.21/bin/neomacs",
                ComparisonRunRole::Candidate => "target/release/neomacs",
            }),
            iterations,
            editor_provenance: Some(guard_provenance(role)),
            execution_environment: Some(Default::default()),
            passthrough_environment: Some(BTreeMap::new()),
            native_video_input: None,
            native_video_execution: None,
            outcome: ComparisonRunOutcome::Valid,
        },
        verdict: RunVerdict::Valid { measurements },
    }
}

fn guard_measurements(wall_us: f64, cycles: f64, instructions: Option<f64>) -> Vec<Measurement> {
    let mut measurements = vec![
        Measurement {
            name: MetricName::PerOperationWallTime,
            value: wall_us,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::CpuCycles,
            value: cycles,
            unit: MetricUnit::Count,
        },
    ];
    if let Some(instructions) = instructions {
        measurements.push(Measurement {
            name: MetricName::Instructions,
            value: instructions,
            unit: MetricUnit::Count,
        });
    }
    measurements
}

/// Ruling 3 (#14682 / #14858): a suite containing an instruction-gated row MUST
/// fail at the `SuiteRequest` level when `--hardware-counters` is omitted,
/// before any artifact directory or editor process is created.
#[test]
fn org_editing_guard_rejects_omitted_hardware_counters_at_suite_request_level() {
    let dir = tempfile::tempdir().expect("tempdir");
    let request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        "/nonexistent/v0.0.21/neomacs",
        "/nonexistent/candidate/neomacs",
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .with_baseline_manifest(Some(PathBuf::from(".github/perf-baselines/v0.0.21.json")));

    let err = request
        .validate()
        .expect_err("org-editing-guard without --hardware-counters must fail at SuiteRequest");
    assert!(
        matches!(
            &err,
            PerfError::HardwareCountersRequired {
                suite: SuiteId::OrgEditingGuard,
                scenarios,
            } if scenarios == &[ScenarioId::OrgEditing, ScenarioId::OrgEditingHeavy]
        ),
        "expected HardwareCountersRequired for both org rows, got {err:?}"
    );
    assert!(
        err.to_string().contains("--hardware-counters"),
        "error must direct caller to --hardware-counters: {err}"
    );

    // Executing through PerfHarness::suite must reject before creating any
    // suite artifact directory or launching an editor.
    let harness = PerfHarness::new(dir.path());
    let run_err = harness
        .suite(&request)
        .expect_err("PerfHarness::suite must enforce SuiteRequest::validate");
    assert!(matches!(
        run_err,
        PerfError::HardwareCountersRequired {
            suite: SuiteId::OrgEditingGuard,
            ..
        }
    ));
    assert!(
        !dir.path().join("tmp/perf-suites").exists(),
        "rejected SuiteRequest must not create suite artifacts"
    );

    // Providing both --hardware-counters and --baseline-manifest satisfies
    // SuiteRequest::validate.
    request
        .with_counters(Some(CounterScope::EditLoop))
        .validate()
        .expect("org-editing-guard with hardware counters and baseline manifest must validate");
}

/// Ruling 1 & 2 (#14682 / #14858 / #14901): `org-editing-guard` MUST fail at
/// the `SuiteRequest` level when `--baseline-manifest` is omitted, while
/// `SuiteId::Standard` continues to validate without one.
#[test]
fn org_editing_guard_rejects_missing_baseline_manifest_at_suite_request_level() {
    let dir = tempfile::tempdir().expect("tempdir");
    let request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        "/nonexistent/v0.0.21/neomacs",
        "/nonexistent/candidate/neomacs",
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .with_counters(Some(CounterScope::EditLoop));

    let err = request
        .validate()
        .expect_err("org-editing-guard without --baseline-manifest must fail at SuiteRequest");
    assert!(
        matches!(
            &err,
            PerfError::BaselineManifestRequired {
                suite: SuiteId::OrgEditingGuard,
            }
        ),
        "expected BaselineManifestRequired, got {err:?}"
    );
    assert!(
        err.to_string().contains("--baseline-manifest"),
        "error must direct caller to --baseline-manifest: {err}"
    );

    let run_err = PerfHarness::new(dir.path())
        .suite(&request)
        .expect_err("PerfHarness::suite must reject missing baseline manifest");
    assert!(matches!(
        run_err,
        PerfError::BaselineManifestRequired {
            suite: SuiteId::OrgEditingGuard,
        }
    ));
    assert!(
        !dir.path().join("tmp/perf-suites").exists(),
        "rejected SuiteRequest must not create suite artifacts"
    );

    // Ruling 4: SuiteId::Standard remains untouched and does not require a
    // baseline manifest.
    SuiteRequest::new(
        SuiteId::Standard,
        "reference/gnu-emacs",
        "target/release/neomacs",
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .validate()
    .expect("standard suite must still validate without --baseline-manifest");
}

/// Ruling 3 (#14682 / #14858 / #14901): exercises the production
/// `SuiteRequest::comparison_request_for -> ComparisonRequest::build_input ->
/// evaluate_comparison` pipeline so dropping `with_primary_metric` or
/// `primary_metric` propagation fails this test. When `MetricName::Instructions`
/// is missing from a run's measurements, the guard comparison and suite MUST be
/// rejected rather than silently falling back to wall time.
#[test]
fn org_editing_guard_rejects_missing_instructions_metric_without_wall_time_fallback() {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let suite_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        "/opt/neomacs-perf/v0.0.21/bin/neomacs",
        "target/release/neomacs",
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    suite_request
        .validate()
        .expect("configured guard request validates");

    let mut suite_results = Vec::new();
    for row in SuiteId::OrgEditingGuard.scenarios() {
        let input = suite_request
            .comparison_request_for(
                row,
                Some(verified.clone()),
                Some(resolved_test_guard_tools()),
            )
            .build_input();
        assert_eq!(input.primary_metric, MetricName::Instructions);
        assert_eq!(input.expected_baseline.as_ref(), Some(&verified));

        // Every sample has valid wall-time AND cpu-cycles measurements, yet
        // omits MetricName::Instructions on candidate sample 1.
        let mut observations = Vec::new();
        for sample_index in 0..samples.get() {
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Baseline,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            ));
            let candidate_instructions = (sample_index != 1).then_some(9_950_000.0);
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Candidate,
                sample_index,
                guard_measurements(950.0, 4_900_000.0, candidate_instructions),
            ));
        }

        let verdict = evaluate_comparison(&input, &observations);
        let ComparisonVerdict::Rejected { reasons } = verdict else {
            panic!(
                "missing MetricName::Instructions must reject comparison, never fall back to wall time"
            );
        };
        assert_eq!(
            reasons.len(),
            1,
            "only MissingMetric(Instructions) must fire, got {reasons:?}"
        );
        assert!(
            matches!(
                &reasons[0],
                ComparisonRejection::MissingMetric {
                    role: ComparisonRunRole::Candidate,
                    sample_index: 1,
                    metric: MetricName::Instructions,
                    ..
                }
            ),
            "expected MissingMetric(Instructions) for candidate sample 1, got {reasons:?}"
        );

        suite_results.push(SuiteScenarioResult {
            scenario: row.scenario,
            maximum_regression_percent: row.maximum_regression_percent,
            maximum_drift_percent: row.maximum_drift_percent,
            gate_metric: row.gate_metric,
            percent_change: None,
            comparison_artifact: PathBuf::from(format!("{}/comparison.json", row.scenario)),
        });
    }

    assert_eq!(
        evaluate_suite(&suite_results, None),
        SuiteVerdict::Rejected {
            scenarios: vec![ScenarioId::OrgEditing, ScenarioId::OrgEditingHeavy],
        }
    );
}

/// Ruling 2 (#14901 Finding 1): every baseline sample's observed runtime
/// `editor_provenance` (executable SHA-256/size AND actually loaded portable
/// dump SHA-256/size) MUST match the verified baseline manifest, rejecting
/// Rung 1–3 shadowed `bin/neomacs.pdump`, unloaded dumps, and TOCTOU swaps.
#[test]
fn org_editing_guard_rejects_shadowed_pdump_and_mismatched_baseline_provenance() {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let suite_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        "/opt/neomacs-perf/v0.0.21/bin/neomacs",
        "target/release/neomacs",
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));

    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let input = suite_request
        .comparison_request_for(
            row,
            Some(verified.clone()),
            Some(resolved_test_guard_tools()),
        )
        .build_input();

    let build_observations = || {
        let mut observations = Vec::new();
        for sample_index in 0..samples.get() {
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Baseline,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            ));
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Candidate,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_050_000.0)),
            ));
        }
        observations
    };

    // Matching baseline executable and loaded pdump provenance passes.
    assert!(
        evaluate_comparison(&input, &build_observations()).is_valid(),
        "matching baseline manifest provenance must pass"
    );

    // Case 1: Rung-1 shadowed `bin/neomacs.pdump` loaded on every baseline
    // sample (same across all samples so intra-arm provenance consistency passes,
    // proving the manifest binding itself is what rejects it).
    let mut shadowed = build_observations();
    for obs in &mut shadowed {
        if obs.run.role == ComparisonRunRole::Baseline {
            obs.run.editor_provenance.as_mut().unwrap().portable_dump =
                PortableDumpProvenance::Loaded {
                    path: "/opt/neomacs-perf/v0.0.21/bin/neomacs.pdump".to_string(),
                    sha256: "shadowed-rung1-pdump-sha256".to_string(),
                    size_bytes: 200,
                };
        }
    }
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &shadowed) else {
        panic!("Rung-1 shadowed baseline pdump must reject comparison");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                sample_index: 0,
                reason,
                ..
            } if reason.contains("bin/neomacs.pdump")
        )),
        "expected BaselineManifestProvenanceMismatch for shadowed pdump, got {reasons:?}"
    );

    // Case 2: baseline booted without a portable dump (e.g. --no-loadup).
    let mut not_loaded = build_observations();
    for obs in &mut not_loaded {
        if obs.run.role == ComparisonRunRole::Baseline {
            obs.run.editor_provenance.as_mut().unwrap().portable_dump =
                PortableDumpProvenance::NotLoaded;
        }
    }
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &not_loaded) else {
        panic!("baseline without loaded pdump must reject comparison");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                sample_index: 0,
                reason,
                ..
            } if reason.contains("NotLoaded")
        )),
        "expected BaselineManifestProvenanceMismatch for NotLoaded, got {reasons:?}"
    );

    // Case 3: TOCTOU executable swap after pre-flight verification.
    let mut swapped_bin = build_observations();
    for obs in &mut swapped_bin {
        if obs.run.role == ComparisonRunRole::Baseline {
            obs.run
                .editor_provenance
                .as_mut()
                .unwrap()
                .executable_sha256 = "swapped-toctou-binary-sha256".to_string();
        }
    }
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &swapped_bin) else {
        panic!("swapped baseline executable must reject comparison");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                sample_index: 0,
                reason,
                ..
            } if reason.contains("swapped-toctou-binary-sha256")
        )),
        "expected BaselineManifestProvenanceMismatch for swapped executable, got {reasons:?}"
    );
}

/// Ruling 4 (#14682 / #14858): `SuiteId::OrgEditingGuard` carries +1.0%
/// instruction-gated budgets on `org-editing` and `org-editing-heavy`, while
/// `SuiteId::Standard` keeps its original 8% budget / 2% drift ratchet untouched.
#[test]
fn org_editing_guard_gates_instructions_at_one_percent_and_preserves_standard_suite() {
    let guard_rows = SuiteId::OrgEditingGuard.scenarios();
    assert_eq!(guard_rows.len(), 2);
    assert_eq!(guard_rows[0].scenario, ScenarioId::OrgEditing);
    assert_eq!(guard_rows[0].maximum_regression_percent, 1.0);
    assert_eq!(guard_rows[0].maximum_drift_percent, None);
    assert_eq!(guard_rows[0].gate_metric, SuiteGateMetric::Instructions);
    assert_eq!(guard_rows[1].scenario, ScenarioId::OrgEditingHeavy);
    assert_eq!(guard_rows[1].maximum_regression_percent, 1.0);
    assert_eq!(guard_rows[1].maximum_drift_percent, None);
    assert_eq!(guard_rows[1].gate_metric, SuiteGateMetric::Instructions);

    // Passing headroom (+0.49% / -0.53% as measured on e0462adfbd vs v0.0.21).
    let passing = [
        SuiteScenarioResult {
            scenario: ScenarioId::OrgEditing,
            maximum_regression_percent: 1.0,
            maximum_drift_percent: None,
            gate_metric: SuiteGateMetric::Instructions,
            percent_change: Some(0.49),
            comparison_artifact: PathBuf::from("org-editing/comparison.json"),
        },
        SuiteScenarioResult {
            scenario: ScenarioId::OrgEditingHeavy,
            maximum_regression_percent: 1.0,
            maximum_drift_percent: None,
            gate_metric: SuiteGateMetric::Instructions,
            percent_change: Some(-0.53),
            comparison_artifact: PathBuf::from("org-editing-heavy/comparison.json"),
        },
    ];
    assert_eq!(evaluate_suite(&passing, None), SuiteVerdict::Passed);

    // A +1.01% instruction increase trips the guard.
    let regressed = [
        passing[0].clone(),
        SuiteScenarioResult {
            percent_change: Some(1.01),
            ..passing[1].clone()
        },
    ];
    let SuiteVerdict::Regressed { regressions } = evaluate_suite(&regressed, None) else {
        panic!("+1.01% instructions must trip the 1.0% org-editing-guard budget");
    };
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].scenario, ScenarioId::OrgEditingHeavy);

    // Standard suite remains completely unchanged (Ruling 4).
    let standard = SuiteId::Standard.scenarios();
    for expected_id in [ScenarioId::OrgEditing, ScenarioId::OrgEditingHeavy] {
        let row = standard
            .iter()
            .find(|row| row.scenario == expected_id)
            .expect("standard suite retains org row");
        assert_eq!(row.maximum_regression_percent, 8.0);
        assert_eq!(row.maximum_drift_percent, Some(2.0));
        assert_eq!(row.gate_metric, SuiteGateMetric::ScenarioPrimary);
    }
}

/// Ruling 2 (#14682 / #14858): `PerfHarness::suite` verifies `--baseline-manifest`
/// before creating any artifact directory or launching any editor.
#[test]
fn org_editing_guard_rejects_mismatched_baseline_manifest_before_launch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wrong_bin = dir.path().join("not-v0021-neomacs");
    fs::write(&wrong_bin, b"definitely-not-v0.0.21").expect("write wrong bin");

    let manifest = crate::workspace_root().join(".github/perf-baselines/v0.0.21.json");
    let request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &wrong_bin,
        &wrong_bin,
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(manifest));

    let err = PerfHarness::new(dir.path())
        .suite(&request)
        .expect_err("wrong baseline binary must fail manifest verification");
    assert!(matches!(
        err,
        PerfError::BaselineManifestMismatch {
            subject: "binary",
            ..
        }
    ));
    assert!(
        !dir.path().join("tmp/perf-suites").exists(),
        "baseline manifest mismatch must fail before creating suite artifacts"
    );
}

/// Bypass 1A (#14915): a relative `--baseline-editor` must be replaced by the
/// canonical absolute `VerifiedBaselineManifest.binary_path` when building the
/// `ComparisonRequest`, and `evaluate_comparison` must reject any relative,
/// different-absolute, or non-matching baseline executable path.
#[test]
fn org_editing_guard_canonicalizes_relative_baseline_editor_and_rejects_mismatch() {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let suite_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        "./v0.0.21/bin/neomacs",
        "target/release/neomacs",
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));

    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let input = suite_request
        .comparison_request_for(
            row,
            Some(verified.clone()),
            Some(resolved_test_guard_tools()),
        )
        .build_input();

    // Production SuiteRequest -> ComparisonRequest wiring must carry the
    // canonical absolute path, never the raw relative CLI string.
    assert_eq!(input.baseline_editor, verified.binary_path);
    assert!(input.baseline_editor.is_absolute());

    let build_observations = |baseline_editor: &PathBuf| {
        let mut observations = Vec::new();
        for sample_index in 0..samples.get() {
            let mut base = guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Baseline,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            );
            base.run.editor = baseline_editor.clone();
            observations.push(base);
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Candidate,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_050_000.0)),
            ));
        }
        observations
    };

    assert!(
        evaluate_comparison(&input, &build_observations(&input.baseline_editor)).is_valid(),
        "canonicalized manifest-pinned baseline must pass"
    );

    // Case 1: relative baseline path reaching evaluate_comparison is rejected
    // even if binary/pdump hashes match.
    let mut relative_input = input.clone();
    relative_input.baseline_editor = PathBuf::from("./v0.0.21/bin/neomacs");
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(
        &relative_input,
        &build_observations(&relative_input.baseline_editor),
    ) else {
        panic!("relative baseline_editor must be rejected when expected_baseline is set");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Baseline,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("canonical absolute path")
        )),
        "expected canonical absolute path rejection, got {reasons:?}"
    );

    // Case 2: observed canonical executable path differs from verified
    // `binary_path` (e.g., workspace_root resolved a different file).
    let mut wrong_path = build_observations(&input.baseline_editor);
    for obs in &mut wrong_path {
        if obs.run.role == ComparisonRunRole::Baseline {
            obs.run.editor_provenance.as_mut().unwrap().path =
                "/repo/v0.0.21/bin/neomacs".to_string();
        }
    }
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &wrong_path) else {
        panic!("baseline run whose observed canonical path differs must be rejected");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Baseline,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("/repo/v0.0.21/bin/neomacs")
        )),
        "expected observed path mismatch rejection, got {reasons:?}"
    );

    // Case 3: configured baseline_editor is absolute, yet differs from
    // `verified.binary_path` (pins `configured_baseline_editor != expected.binary_path`).
    let mut other_abs_input = input.clone();
    other_abs_input.baseline_editor = PathBuf::from("/opt/neomacs-perf/other/bin/neomacs");
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(
        &other_abs_input,
        &build_observations(&other_abs_input.baseline_editor),
    ) else {
        panic!("absolute-but-different baseline_editor must be rejected");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Baseline,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("baseline editor path mismatch")
                && reason.contains("/opt/neomacs-perf/other/bin/neomacs")
        )),
        "expected baseline editor path mismatch rejection, got {reasons:?}"
    );
}

fn guard_observations_with_env(
    row: &crate::SuiteScenario,
    iterations: u32,
    samples: ComparisonSampleCount,
    target_role: ComparisonRunRole,
    injected_env: BTreeMap<String, String>,
) -> Vec<ComparisonObservation> {
    let mut observations = Vec::new();
    for sample_index in 0..samples.get() {
        for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
            let mut obs = guard_observation(
                row.scenario,
                iterations,
                role,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            );
            if role == target_role {
                obs.run.passthrough_environment = Some(injected_env.clone());
            }
            observations.push(obs);
        }
    }
    observations
}

fn assert_guard_env_class_blocked_and_rejected(
    var_name: &str,
    var_value: &str,
    extra_ambient: &[(&str, &str)],
) {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let tools = resolved_test_guard_tools();

    let mut ambient_env = BTreeMap::from([(var_name.to_string(), OsString::from(var_value))]);
    for (k, v) in extra_ambient {
        ambient_env.insert((*k).to_string(), OsString::from(*v));
    }

    let guard_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &verified.binary_path,
        "target/release/neomacs",
        samples,
    )
    .with_machine_policy(MachinePolicy {
        cpu: Some(0),
        required_governor: None,
    })
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    let guard_row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let guard_comparison = guard_request
        .comparison_request_for(guard_row, Some(verified), Some(tools))
        .with_inherited_environment(ambient_env.clone());

    // Production ComparisonRequest::run_request_for constructs a closed empty
    // environment for BOTH Baseline and Candidate arms (never inheriting
    // `var_name` or any extra ambient knobs).
    for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
        let run_env = guard_comparison
            .run_request_for(role)
            .benchmark_environment();
        assert!(
            run_env.is_empty(),
            "manifest-pinned guard {role:?} arm must use closed empty env, got {run_env:?}"
        );
    }

    // Ruling 4: SuiteId::Standard (no baseline manifest) still forwards the
    // ambient environment untouched.
    let standard_request = SuiteRequest::new(
        SuiteId::Standard,
        "reference/gnu-emacs",
        "target/release/neomacs",
        samples,
    );
    let standard_row = SuiteId::Standard
        .scenarios()
        .iter()
        .find(|row| row.scenario == ScenarioId::OrgEditing)
        .expect("standard org-editing row");
    let standard_comparison = standard_request
        .comparison_request_for(standard_row, None, None)
        .with_inherited_environment(ambient_env);
    for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
        let std_env = standard_comparison
            .run_request_for(role)
            .benchmark_environment();
        assert_eq!(
            std_env.get(var_name),
            Some(&OsString::from(var_value)),
            "SuiteId::Standard must keep forwarding {var_name} on {role:?}"
        );
    }

    // Set-comparison in evaluate_comparison rejects `var_name` by name on BOTH
    // Baseline and Candidate arms.
    let input = guard_comparison.build_input();
    let injected = BTreeMap::from([(var_name.to_string(), var_value.to_string())]);
    for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
        let observations = guard_observations_with_env(
            guard_row,
            input.iterations.get(),
            samples,
            role,
            injected.clone(),
        );
        let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations)
        else {
            panic!("guard must reject {var_name} on {role:?} arm");
        };
        assert!(
            reasons.iter().any(|reason| matches!(
                reason,
                ComparisonRejection::BaselineManifestProvenanceMismatch {
                    role: rejected_role,
                    sample_index: 0,
                    reason,
                    ..
                } if *rejected_role == role
                    && reason.contains(var_name)
                    && reason.contains(var_value)
            )),
            "expected BaselineManifestProvenanceMismatch naming {var_name}={var_value} on {role:?}, got {reasons:?}"
        );
    }
}

/// #14928 negative class 1: dynamic linker overrides (`LD_LIBRARY_PATH` /
/// `DYLD_LIBRARY_PATH`) are excluded from the constructed guard environment and
/// rejected by name on both Baseline and Candidate arms.
#[test]
fn org_editing_guard_rejects_ld_library_path_on_both_arms() {
    assert_guard_env_class_blocked_and_rejected(
        "LD_LIBRARY_PATH",
        "/tmp/evil-shim-lib",
        &[("DYLD_LIBRARY_PATH", "/tmp/evil-dyld")],
    );
}

/// #14928 negative class 2: `PATH` shadowing `taskset` is rejected by name
/// during canonical tool resolution, resolved canonical `perf`/`taskset` paths
/// are wired into `RunRequest`, and `PATH` in child `passthrough_environment`
/// (or missing `taskset_path` with `--cpu`) is rejected by name on both arms.
#[cfg(unix)]
#[test]
fn org_editing_guard_rejects_path_shadowing_taskset_and_path_in_child_env() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let shadow_dir = dir.path().join("shadow");
    let sys_dir = dir.path().join("sys");
    fs::create_dir_all(&shadow_dir).expect("shadow dir");
    fs::create_dir_all(&sys_dir).expect("sys dir");

    let make_exec = |path: &PathBuf| {
        fs::write(path, b"#!/bin/sh\nexit 0\n").expect("write stub");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    };
    let shadow_taskset = shadow_dir.join("taskset");
    let sys_taskset = sys_dir.join("taskset");
    let sys_perf = sys_dir.join("perf");
    make_exec(&shadow_taskset);
    make_exec(&sys_taskset);
    make_exec(&sys_perf);

    // Shadowed `taskset` in PATH must fail with GuardToolPathViolation naming
    // PATH, taskset, and both canonical executables.
    let shadowed_path = std::env::join_paths([&shadow_dir, &sys_dir]).expect("join shadowed PATH");
    let err = resolve_guard_tools(Some(shadowed_path.as_os_str()), true)
        .expect_err("PATH shadowing taskset must be rejected");
    let PerfError::GuardToolPathViolation { tool, reason } = err else {
        panic!("expected GuardToolPathViolation, got {err:?}");
    };
    assert_eq!(tool, "taskset");
    assert!(
        reason.contains("PATH") && reason.contains("taskset") && reason.contains("shadows"),
        "rejection reason must name PATH and shadowed taskset: {reason}"
    );
    assert!(
        reason.contains(
            fs::canonicalize(&shadow_taskset)
                .unwrap()
                .to_string_lossy()
                .as_ref()
        ) && reason.contains(
            fs::canonicalize(&sys_taskset)
                .unwrap()
                .to_string_lossy()
                .as_ref()
        ),
        "rejection reason must name both executables: {reason}"
    );

    // Unambiguous PATH resolves to canonical absolute `perf` and `taskset` and
    // wires them through ComparisonRequest::run_request_for.
    let clean_path = std::env::join_paths([&sys_dir]).expect("join clean PATH");
    let resolved = resolve_guard_tools(Some(clean_path.as_os_str()), true)
        .expect("unambiguous PATH resolves tools");
    assert_eq!(resolved.perf_path, fs::canonicalize(&sys_perf).unwrap());
    assert_eq!(
        resolved.taskset_path,
        Some(fs::canonicalize(&sys_taskset).unwrap())
    );

    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let guard_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &verified.binary_path,
        "target/release/neomacs",
        samples,
    )
    .with_machine_policy(MachinePolicy {
        cpu: Some(0),
        required_governor: None,
    })
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let comparison =
        guard_request.comparison_request_for(row, Some(verified), Some(resolved.clone()));
    let baseline_run = comparison.run_request_for(ComparisonRunRole::Baseline);
    assert_eq!(
        baseline_run.resolved_perf(),
        Some(resolved.perf_path.as_path())
    );
    assert_eq!(
        baseline_run.resolved_taskset(),
        resolved.taskset_path.as_deref()
    );

    // Missing `taskset_path` when `--cpu` is set is rejected by name in
    // evaluate_comparison.
    let mut missing_taskset_input = comparison.build_input();
    missing_taskset_input.guard_tools = Some(ResolvedGuardTools {
        perf_path: resolved.perf_path,
        taskset_path: None,
    });
    let clean_observations = guard_observations_with_env(
        row,
        missing_taskset_input.iterations.get(),
        samples,
        ComparisonRunRole::Baseline,
        BTreeMap::new(),
    );
    let ComparisonVerdict::Rejected { reasons } =
        evaluate_comparison(&missing_taskset_input, &clean_observations)
    else {
        panic!("missing taskset_path with --cpu must be rejected");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch { reason, .. }
                if reason.contains("taskset")
        )),
        "expected rejection naming taskset, got {reasons:?}"
    );

    // `PATH` itself must also be excluded from the constructed guard env and
    // rejected if present in recorded `passthrough_environment` on either arm.
    assert_guard_env_class_blocked_and_rejected("PATH", "/tmp/evil-bin:/usr/bin", &[]);
}

/// #14928 negative class 3: JIT / GC / VM tuning knobs (`NEOVM_JIT=0`,
/// unbounded `NEOVM_JIT_*` prefix, `NEOVM_GC_STRESS`, `NEOVM_REGEX_DFA`,
/// `NEOVM_SYNTAX_PARSE_CACHE`) are excluded from the constructed guard
/// environment and rejected by name on both Baseline and Candidate arms.
#[test]
fn org_editing_guard_rejects_neovm_jit_zero_and_tuning_knobs_on_both_arms() {
    assert_guard_env_class_blocked_and_rejected(
        "NEOVM_JIT",
        "0",
        &[
            ("NEOVM_JIT_BG_AFFINITY", "0"),
            ("NEOVM_GC_STRESS", "1"),
            ("NEOVM_REGEX_DFA", "verify"),
            ("NEOVM_SYNTAX_PARSE_CACHE", "verify"),
        ],
    );
    assert_guard_env_class_blocked_and_rejected("NEOVM_JIT_BG_AFFINITY", "0", &[]);
    assert_guard_env_class_blocked_and_rejected("NEOVM_GC_STRESS", "1", &[]);
}

/// #14928 negative class 4: `NEOMACS_RUNTIME_ROOT` is excluded from the
/// constructed guard environment and rejected by name on both Baseline and
/// Candidate arms, while `SuiteId::Standard` remains untouched.
#[test]
fn org_editing_guard_rejects_neomacs_runtime_root_on_both_arms() {
    assert_guard_env_class_blocked_and_rejected("NEOMACS_RUNTIME_ROOT", "/tmp/alt-runtime", &[]);
}
