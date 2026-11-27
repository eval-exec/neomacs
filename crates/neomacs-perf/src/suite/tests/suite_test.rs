use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{
    ARTIFACT_SCHEMA_VERSION, ComparisonObservation, ComparisonRejection, ComparisonRun,
    ComparisonRunOutcome, ComparisonRunRole, ComparisonSampleCount, ComparisonVerdict,
    CounterScope, EditorCapabilities, EditorKind, EditorProvenance, Frontend, HostProvenance,
    InstructionCalibration, MachinePolicy, Measurement, MetricName, MetricUnit,
    NonConvertibleTimingReason, PerfError, PerfHarness, PortableDumpProvenance, ResolvedGuardTools,
    RunArtifact, RunReport, RunVerdict, SUITE_ARTIFACT_SCHEMA_VERSION, ScenarioId, SuiteArtifact,
    SuiteDriftSkip, SuiteDriftSkipReason, SuiteEvaluationContext, SuiteGateMetric, SuiteId,
    SuiteRequest, SuiteScenarioResult, SuiteVerdict, VerifiedBaselineManifest,
    comparison_run_from_child, evaluate_comparison, evaluate_suite, evaluate_suite_with_context,
    read_history_link, resolve_guard_tools,
};

fn result(scenario: ScenarioId, threshold: f64, change: Option<f64>) -> SuiteScenarioResult {
    SuiteScenarioResult {
        scenario,
        maximum_regression_percent: threshold,
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::Instructions,
        non_convertible_reason: None,
        iterations: None,
        percent_change: change,
        gate_summary: None,
        context_metrics: Vec::new(),
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
        host_loadavg: None,
        provenance: None,
        started_unix_ms: 1,
        total_elapsed_us: 2,
        previous_suite: None,
        baseline_manifest: None,
        guard_tools: None,
        scenarios: Vec::new(),
        skipped_drifts: Vec::new(),
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
        gate_metric: SuiteGateMetric::Instructions,
        non_convertible_reason: None,
        iterations: None,
        percent_change: change,
        gate_summary: None,
        context_metrics: Vec::new(),
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
        host_loadavg: None,
        provenance: None,
        started_unix_ms: 1,
        total_elapsed_us: 2,
        previous_suite: None,
        baseline_manifest: None,
        guard_tools: None,
        scenarios,
        skipped_drifts: Vec::new(),
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

const CANONICAL_TEST_CANDIDATE: &str = "/opt/neomacs-perf/Candidate/neomacs";

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
                ComparisonRunRole::Candidate => CANONICAL_TEST_CANDIDATE,
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

    // SuiteId::Standard requires --hardware-counters (for its 22 instruction-gated
    // rows) but does NOT require a frozen --baseline-manifest.
    SuiteRequest::new(
        SuiteId::Standard,
        "reference/gnu-emacs",
        "target/release/neomacs",
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .with_counters(Some(CounterScope::EditLoop))
    .validate()
    .expect("standard suite with hardware counters must validate without --baseline-manifest");
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
                Some(Path::new(CANONICAL_TEST_CANDIDATE)),
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
            non_convertible_reason: row.non_convertible_reason,
            iterations: Some(input.iterations.get()),
            percent_change: None,
            gate_summary: None,
            context_metrics: Vec::new(),
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
            Some(Path::new(CANONICAL_TEST_CANDIDATE)),
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
            non_convertible_reason: None,
            iterations: None,
            percent_change: Some(0.49),
            gate_summary: None,
            context_metrics: Vec::new(),
            comparison_artifact: PathBuf::from("org-editing/comparison.json"),
        },
        SuiteScenarioResult {
            scenario: ScenarioId::OrgEditingHeavy,
            maximum_regression_percent: 1.0,
            maximum_drift_percent: None,
            gate_metric: SuiteGateMetric::Instructions,
            non_convertible_reason: None,
            iterations: None,
            percent_change: Some(-0.53),
            gate_summary: None,
            context_metrics: Vec::new(),
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

    // Standard suite shares the same 1.0% instruction calibration floor as
    // OrgEditingGuard on OrgEditing and OrgEditingHeavy, plus a 0.5% drift
    // ratchet derived from multi-run clean A/A calibration.
    let standard = SuiteId::Standard.scenarios();
    for expected_id in [ScenarioId::OrgEditing, ScenarioId::OrgEditingHeavy] {
        let row = standard
            .iter()
            .find(|row| row.scenario == expected_id)
            .expect("standard suite retains org row");
        assert_eq!(row.maximum_regression_percent, 1.0);
        assert_eq!(row.maximum_drift_percent, Some(0.5));
        assert_eq!(row.gate_metric, SuiteGateMetric::Instructions);
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
            Some(Path::new(CANONICAL_TEST_CANDIDATE)),
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
        allow_local: false,
    })
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    let guard_row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let guard_comparison = guard_request
        .comparison_request_for(
            guard_row,
            Some(verified),
            Some(tools),
            Some(Path::new(CANONICAL_TEST_CANDIDATE)),
        )
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
        .comparison_request_for(standard_row, None, None, None)
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
        allow_local: false,
    })
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let comparison = guard_request.comparison_request_for(
        row,
        Some(verified),
        Some(resolved.clone()),
        Some(Path::new(CANONICAL_TEST_CANDIDATE)),
    );
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

/// #14946 candidate canonicalization: a relative `--candidate-editor` must be
/// replaced by its canonical absolute path on manifest-pinned guard runs, and
/// `evaluate_comparison` must reject any relative candidate path or observed
/// canonical path mismatch on the Candidate arm.
#[test]
fn org_editing_guard_canonicalizes_relative_candidate_editor_and_rejects_mismatch() {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let canonical_candidate = PathBuf::from(CANONICAL_TEST_CANDIDATE);
    let suite_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &verified.binary_path,
        "./target/release/neomacs",
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));

    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let input = suite_request
        .comparison_request_for(
            row,
            Some(verified),
            Some(resolved_test_guard_tools()),
            Some(&canonical_candidate),
        )
        .build_input();

    assert_eq!(input.candidate_editor, canonical_candidate);
    assert!(input.candidate_editor.is_absolute());

    let build_observations = |candidate_editor: &Path| {
        let mut observations = Vec::new();
        for sample_index in 0..samples.get() {
            observations.push(guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Baseline,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            ));
            let mut cand = guard_observation(
                row.scenario,
                input.iterations.get(),
                ComparisonRunRole::Candidate,
                sample_index,
                guard_measurements(1000.0, 5_000_000.0, Some(10_050_000.0)),
            );
            cand.run.editor = candidate_editor.to_path_buf();
            observations.push(cand);
        }
        observations
    };

    assert!(
        evaluate_comparison(&input, &build_observations(&input.candidate_editor)).is_valid(),
        "canonicalized candidate editor must pass"
    );

    // Case 1: relative candidate_editor reaching evaluate_comparison is rejected.
    let mut relative_input = input.clone();
    relative_input.candidate_editor = PathBuf::from("./target/release/neomacs");
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(
        &relative_input,
        &build_observations(&relative_input.candidate_editor),
    ) else {
        panic!("relative candidate_editor must be rejected on manifest-pinned guard");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Candidate,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("canonical absolute candidate")
        )),
        "expected canonical candidate path rejection, got {reasons:?}"
    );

    // Case 2: observed canonical candidate path differs from configured canonical path.
    let mut wrong_candidate_path = build_observations(&input.candidate_editor);
    for obs in &mut wrong_candidate_path {
        if obs.run.role == ComparisonRunRole::Candidate {
            obs.run.editor_provenance.as_mut().unwrap().path =
                "/other/workspace/target/release/neomacs".to_string();
        }
    }
    let ComparisonVerdict::Rejected { reasons } =
        evaluate_comparison(&input, &wrong_candidate_path)
    else {
        panic!("mismatched observed candidate canonical path must be rejected");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Candidate,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("/other/workspace/target/release/neomacs")
        )),
        "expected observed candidate path mismatch rejection, got {reasons:?}"
    );

    // Case 3: PerfHarness::suite rejects a missing candidate editor before
    // creating any suite artifacts.
    let dir = tempfile::tempdir().expect("tempdir");
    let (manifest_path, bin_path) = write_temp_verified_baseline(dir.path());
    let missing_candidate_req = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &bin_path,
        dir.path().join("does-not-exist-candidate"),
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(manifest_path));
    let err = PerfHarness::new(dir.path())
        .suite(&missing_candidate_req)
        .expect_err("missing candidate editor must fail before launch");
    assert!(
        matches!(err, PerfError::CandidateEditorInvalid { .. }),
        "expected CandidateEditorInvalid, got {err:?}"
    );
    assert!(
        !dir.path().join("tmp/perf-suites").exists(),
        "invalid candidate must not create suite artifacts"
    );
}

/// #14946 counter-scope binding: `OrgEditingGuard` requires
/// `CounterScope::EditLoop` in both `SuiteRequest::validate()` and
/// `evaluate_comparison`, while `SuiteId::Standard` continues to accept any
/// counter scope.
#[test]
fn org_editing_guard_binds_counter_scope_to_edit_loop() {
    let dir = tempfile::tempdir().expect("tempdir");
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();

    for wrong_scope in [
        CounterScope::WholeProcess,
        CounterScope::EditLoopMainThread,
        CounterScope::WholeProcessMainThread,
    ] {
        let request = SuiteRequest::new(
            SuiteId::OrgEditingGuard,
            &verified.binary_path,
            CANONICAL_TEST_CANDIDATE,
            samples,
        )
        .with_counters(Some(wrong_scope))
        .with_baseline_manifest(Some(verified.manifest_path.clone()));

        let err = request
            .validate()
            .expect_err("non-edit-loop counter scope must fail OrgEditingGuard validation");
        assert!(
            matches!(
                err,
                PerfError::CounterScopeMismatch {
                    suite: SuiteId::OrgEditingGuard,
                    expected: CounterScope::EditLoop,
                    actual,
                } if actual == wrong_scope
            ),
            "expected CounterScopeMismatch for {wrong_scope:?}, got {err:?}"
        );

        let run_err = PerfHarness::new(dir.path())
            .suite(&request)
            .expect_err("PerfHarness::suite must reject non-edit-loop counter scope");
        assert!(matches!(run_err, PerfError::CounterScopeMismatch { .. }));
        assert!(
            !dir.path().join("tmp/perf-suites").exists(),
            "rejected counter scope must not create suite artifacts"
        );

        // evaluate_comparison also rejects non-EditLoop instruction guard inputs.
        let row = &SuiteId::OrgEditingGuard.scenarios()[0];
        let input = request
            .comparison_request_for(
                row,
                Some(verified.clone()),
                Some(resolved_test_guard_tools()),
                Some(Path::new(CANONICAL_TEST_CANDIDATE)),
            )
            .build_input();
        let observations = guard_observations_with_env(
            row,
            input.iterations.get(),
            samples,
            ComparisonRunRole::Baseline,
            BTreeMap::new(),
        );
        let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations)
        else {
            panic!("evaluate_comparison must reject {wrong_scope:?} on instruction guard");
        };
        assert!(
            reasons.iter().any(|reason| matches!(
                reason,
                ComparisonRejection::BaselineManifestProvenanceMismatch { reason, .. }
                    if reason.contains("EditLoop")
            )),
            "expected EditLoop counter scope rejection for {wrong_scope:?}, got {reasons:?}"
        );

        // SuiteId::Standard is instruction-gated on edit-loop rows (with
        // Startup overriding to WholeProcess), so it accepts EditLoop and
        // EditLoopMainThread while rejecting WholeProcess scopes that would
        // silently mix startup instructions into edit-loop rows.
        let standard_req = SuiteRequest::new(
            SuiteId::Standard,
            "reference/gnu-emacs",
            "target/release/neomacs",
            samples,
        )
        .with_counters(Some(wrong_scope));
        if matches!(wrong_scope, CounterScope::EditLoopMainThread) {
            standard_req
                .validate()
                .expect("SuiteId::Standard must accept EditLoopMainThread");
        } else {
            let err = standard_req
                .validate()
                .expect_err("SuiteId::Standard must reject WholeProcess counter scopes");
            assert!(
                matches!(
                    err,
                    PerfError::CounterScopeMismatch {
                        suite: SuiteId::Standard,
                        expected: CounterScope::EditLoop,
                        actual,
                    } if actual == wrong_scope
                ),
                "expected Standard CounterScopeMismatch for {wrong_scope:?}, got {err:?}"
            );
        }
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_temp_verified_baseline(root: &Path) -> (PathBuf, PathBuf) {
    let bin_dir = root.join("bin");
    let pdump_dir = root.join("libexec/neomacs/0.0.21");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(&pdump_dir).expect("pdump dir");
    let bin_path = bin_dir.join("neomacs");
    let pdump_path = pdump_dir.join("neomacs.pdump");
    let bin_bytes = b"fake-baseline-binary";
    let pdump_bytes = b"fake-baseline-pdump";
    fs::write(&bin_path, bin_bytes).expect("write bin");
    fs::write(&pdump_path, pdump_bytes).expect("write pdump");
    let manifest_path = root.join("v0.0.21.json");
    let manifest_json = serde_json::json!({
        "schema_version": 1,
        "version": "v0.0.21",
        "git_commit": "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "target": "x86_64-unknown-linux-gnu",
        "release_asset": {
            "name": "neomacs.tar.gz",
            "url": "https://example.invalid/neomacs.tar.gz",
            "size_bytes": 1,
            "sha256": hex_sha256(b"tarball")
        },
        "binary": {
            "archive_path": "bin/neomacs",
            "size_bytes": bin_bytes.len(),
            "sha256": hex_sha256(bin_bytes)
        },
        "pdump": {
            "archive_path": "libexec/neomacs/0.0.21/neomacs.pdump",
            "relative_to_binary": "../libexec/neomacs/0.0.21/neomacs.pdump",
            "size_bytes": pdump_bytes.len(),
            "sha256": hex_sha256(pdump_bytes)
        }
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest_json).expect("serialize manifest"),
    )
    .expect("write manifest");
    (manifest_path, bin_path)
}

/// #14946 wiring-edge test: `PerfHarness::suite` itself MUST invoke
/// `resolve_guard_tools` before creating suite artifacts so a shadowed
/// `taskset` in `PATH` fails with `GuardToolPathViolation`. Reverting the call
/// in `PerfHarness::suite` fails this test.
#[cfg(unix)]
#[test]
fn perf_harness_suite_rejects_shadowed_taskset_via_resolve_guard_tools() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let (manifest_path, bin_path) = write_temp_verified_baseline(dir.path());
    let candidate_path = dir.path().join("candidate-neomacs");
    fs::write(&candidate_path, b"fake-candidate-binary").expect("write candidate");

    let shadow_dir = dir.path().join("shadow");
    let sys_dir = dir.path().join("sys");
    fs::create_dir_all(&shadow_dir).expect("shadow dir");
    fs::create_dir_all(&sys_dir).expect("sys dir");
    for tool in [
        shadow_dir.join("taskset"),
        sys_dir.join("taskset"),
        sys_dir.join("perf"),
    ] {
        fs::write(&tool, b"#!/bin/sh\nexit 0\n").expect("write stub");
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    }
    let shadowed_path = std::env::join_paths([&shadow_dir, &sys_dir]).expect("join PATH");

    let request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &bin_path,
        &candidate_path,
        ComparisonSampleCount::new(3).expect("valid sample count"),
    )
    .with_machine_policy(MachinePolicy {
        cpu: Some(0),
        required_governor: None,
        allow_local: false,
    })
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(manifest_path))
    .with_guard_path_override(Some(shadowed_path));

    let err = PerfHarness::new(dir.path())
        .suite(&request)
        .expect_err("PerfHarness::suite must reject shadowed taskset before launch");
    let PerfError::GuardToolPathViolation { tool, reason } = err else {
        panic!("expected GuardToolPathViolation, got {err:?}");
    };
    assert_eq!(tool, "taskset");
    assert!(
        reason.contains("PATH") && reason.contains("taskset") && reason.contains("shadows"),
        "expected PATH shadowing message, got: {reason}"
    );
    assert!(
        !dir.path().join("tmp/perf-suites").exists(),
        "shadowed tool must fail before creating suite artifacts"
    );
}

/// #14946 wiring-edge test: `comparison_run_from_child` (the production helper
/// called by `PerfHarness::compare`) MUST read the full unfiltered
/// `passthrough_environment` from serialized `input-provenance.json` into
/// `ComparisonRun.passthrough_environment`, and `evaluate_comparison` must
/// reject every deviated key by name. Reverting `comparison_run_from_child`
/// to filter or drop `passthrough_environment` fails this test.
#[test]
fn comparison_run_from_child_reads_full_passthrough_environment_and_rejects_deviations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let verified = verified_guard_baseline();
    let row = &SuiteId::OrgEditingGuard.scenarios()[0];
    let suite_request = SuiteRequest::new(
        SuiteId::OrgEditingGuard,
        &verified.binary_path,
        CANONICAL_TEST_CANDIDATE,
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop))
    .with_baseline_manifest(Some(verified.manifest_path.clone()));
    let input = suite_request
        .comparison_request_for(
            row,
            Some(verified),
            Some(resolved_test_guard_tools()),
            Some(Path::new(CANONICAL_TEST_CANDIDATE)),
        )
        .build_input();

    let run_dir = dir.path().join("child-run-0");
    fs::create_dir_all(&run_dir).expect("create child run dir");
    let artifact_path = run_dir.join("artifact.json");
    let provenance_path = run_dir.join("input-provenance.json");
    let leaked_env = BTreeMap::from([
        (
            "LD_LIBRARY_PATH".to_string(),
            "/tmp/evil-shim-lib".to_string(),
        ),
        (
            "NEOMACS_RUNTIME_ROOT".to_string(),
            "/tmp/alt-runtime".to_string(),
        ),
        ("NEOVM_GC_STRESS".to_string(), "1".to_string()),
        ("NEOVM_JIT".to_string(), "0".to_string()),
    ]);
    let provenance_doc = serde_json::json!({
        "editor": guard_provenance(ComparisonRunRole::Baseline),
        "passthrough_environment": leaked_env,
    });
    fs::write(
        &provenance_path,
        serde_json::to_vec_pretty(&provenance_doc).expect("serialize provenance"),
    )
    .expect("write input-provenance.json");

    let child_report = RunReport {
        artifact: RunArtifact {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            run_id: "org-editing-Baseline-0".to_string(),
            scenario: row.scenario,
            frontend: Frontend::Batch,
            editor: input.baseline_editor.clone(),
            iterations: input.iterations.get(),
            started_unix_ms: 1,
            total_elapsed_us: 2,
            host: HostProvenance {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                kernel_release: None,
                cpu_model: None,
                logical_cpu_count: 1,
                allowed_cpus: None,
                selected_cpu: None,
                scaling_governor: None,
                perf_event_paranoid: None,
                continuous_integration: false,
                loadavg: None,
                provenance: None,
            },
            native_video_execution: None,
            verdict: RunVerdict::Valid {
                measurements: guard_measurements(1000.0, 5_000_000.0, Some(10_000_000.0)),
            },
            files: Vec::new(),
        },
        artifact_path,
    };

    let extracted_run = comparison_run_from_child(ComparisonRunRole::Baseline, 0, &child_report);
    assert_eq!(
        extracted_run.passthrough_environment.as_ref(),
        Some(&leaked_env),
        "comparison_run_from_child must preserve the full unfiltered passthrough_environment"
    );

    let mut observations = guard_observations_with_env(
        row,
        input.iterations.get(),
        samples,
        ComparisonRunRole::Baseline,
        BTreeMap::new(),
    );
    observations[0] = ComparisonObservation {
        run: extracted_run,
        verdict: child_report.artifact.verdict,
    };
    let ComparisonVerdict::Rejected { reasons } = evaluate_comparison(&input, &observations) else {
        panic!("full passthrough_environment read from disk must reject comparison");
    };
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            ComparisonRejection::BaselineManifestProvenanceMismatch {
                role: ComparisonRunRole::Baseline,
                sample_index: 0,
                reason,
                ..
            } if reason.contains("LD_LIBRARY_PATH")
                && reason.contains("NEOMACS_RUNTIME_ROOT")
                && reason.contains("NEOVM_GC_STRESS")
                && reason.contains("NEOVM_JIT")
        )),
        "expected rejection naming all 4 leaked vars from disk provenance, got {reasons:?}"
    );
}

#[test]
fn scrolling_long_lines_guard_requires_edit_loop_instructions_and_gates_at_two_percent() {
    let rows = SuiteId::ScrollingLongLinesGuard.scenarios();
    assert_eq!(rows.len(), 1);
    let row = rows[0];
    assert_eq!(row.scenario, ScenarioId::ScrollingLongLines);
    assert_eq!(row.gate_metric, SuiteGateMetric::Instructions);
    assert_eq!(row.maximum_regression_percent, 2.0);
    assert_eq!(row.maximum_drift_percent, None);
    assert!(!SuiteId::ScrollingLongLinesGuard.requires_baseline_manifest());
    assert!(SuiteId::ScrollingLongLinesGuard.uses_guard_hardening());
    assert_eq!(
        SuiteId::ScrollingLongLinesGuard.required_counter_scope(),
        Some(CounterScope::EditLoop)
    );

    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let missing_counters = SuiteRequest::new(
        SuiteId::ScrollingLongLinesGuard,
        "/baseline/emacs",
        "/candidate/neomacs",
        samples,
    );
    assert!(matches!(
        missing_counters.validate(),
        Err(PerfError::HardwareCountersRequired {
            suite: SuiteId::ScrollingLongLinesGuard,
            ..
        })
    ));

    let wrong_scope = missing_counters
        .clone()
        .with_counters(Some(CounterScope::WholeProcess));
    assert!(matches!(
        wrong_scope.validate(),
        Err(PerfError::CounterScopeMismatch {
            suite: SuiteId::ScrollingLongLinesGuard,
            expected: CounterScope::EditLoop,
            actual: CounterScope::WholeProcess,
        })
    ));

    let valid = missing_counters.with_counters(Some(CounterScope::EditLoop));
    assert!(valid.validate().is_ok());

    // Nit 3C regression guard: ScrollingLongLinesGuard uses guard hardening
    // (strips NEOVM_*/NEOMACS_* knobs + narrows TUI PATH + resolves canonical
    // perf/taskset tools) even without a baseline manifest.
    let tools = resolved_test_guard_tools();
    let mut ambient_env = BTreeMap::new();
    ambient_env.insert("NEOVM_JIT".to_string(), OsString::from("0"));
    ambient_env.insert("NEOMACS_TEXT_SNAPSHOT".to_string(), OsString::from("copy"));
    ambient_env.insert(
        "NEOMACS_RUNTIME_ROOT".to_string(),
        OsString::from("/tmp/other"),
    );
    let comparison = valid
        .comparison_request_for(
            &row,
            None,
            Some(tools.clone()),
            Some(Path::new(CANONICAL_TEST_CANDIDATE)),
        )
        .with_inherited_environment(ambient_env);
    for role in [ComparisonRunRole::Baseline, ComparisonRunRole::Candidate] {
        let run_request = comparison.run_request_for(role);
        let run_env = run_request.benchmark_environment();
        assert!(
            !run_env.contains_key("NEOVM_JIT")
                && !run_env.contains_key("NEOMACS_TEXT_SNAPSHOT")
                && !run_env.contains_key("NEOMACS_RUNTIME_ROOT"),
            "ScrollingLongLinesGuard {role:?} arm must strip NEOVM_*/NEOMACS_* knobs, got {run_env:?}"
        );
        for key in run_env.keys() {
            assert_eq!(
                key, "PATH",
                "ScrollingLongLinesGuard {role:?} TUI arm may only carry minimal tool PATH, found {key}"
            );
        }
        assert_eq!(
            run_request.resolved_perf(),
            Some(tools.perf_path.as_path()),
            "ScrollingLongLinesGuard {role:?} arm must use resolved canonical perf"
        );
    }
}

#[test]
fn standard_suite_instruction_gates_all_convertible_rows_and_argues_every_exception() {
    let rows = SuiteId::Standard.scenarios();
    assert_eq!(rows.len(), 23);

    let mut instruction_rows = Vec::new();
    let mut report_rows = Vec::new();
    for row in rows {
        match row.gate_metric {
            SuiteGateMetric::Instructions => {
                assert_eq!(
                    row.non_convertible_reason, None,
                    "instruction-gated row {} must not carry a non-convertible reason",
                    row.scenario
                );
                let cal = row
                    .calibration
                    .expect("every instruction-gated row must carry InstructionCalibration");
                cal.validate(2)
                    .unwrap_or_else(|err| panic!("invalid calibration on {}: {err}", row.scenario));
                assert_eq!(
                    row.maximum_regression_percent,
                    cal.budget_percent(),
                    "{}: budget must come directly from InstructionCalibration::budget_percent()",
                    row.scenario
                );
                assert!(
                    row.maximum_regression_percent > cal.max_span_percent,
                    "{}: budget ({}) must strictly exceed observed A/A span ({})",
                    row.scenario,
                    row.maximum_regression_percent,
                    cal.max_span_percent
                );
                if let Some(drift) = row.maximum_drift_percent {
                    assert_eq!(
                        Some(drift),
                        cal.drift_budget_percent(),
                        "{}: drift ratchet must come directly from InstructionCalibration",
                        row.scenario
                    );
                    assert!(
                        drift > cal.max_aa_drift_percent,
                        "{}: drift ratchet ({drift}) must exceed observed A/A drift ({})",
                        row.scenario,
                        cal.max_aa_drift_percent
                    );
                }
                instruction_rows.push(row.scenario);
            }
            SuiteGateMetric::NonGatingReport => {
                assert!(
                    row.non_convertible_reason.is_some(),
                    "non-gating row {} must declare a physical duration NonConvertibleTimingReason",
                    row.scenario
                );
                report_rows.push((row.scenario, row.non_convertible_reason));
            }
            SuiteGateMetric::ScenarioPrimary => {
                panic!(
                    "STANDARD_SCENARIOS must not retain wall/CPU-gated row {}",
                    row.scenario
                );
            }
        }
    }
    assert_eq!(instruction_rows.len(), 22);
    assert_eq!(
        report_rows,
        vec![(
            ScenarioId::GuiInputLatency,
            Some(NonConvertibleTimingReason::InputToRedisplayLatency)
        )]
    );

    // SuiteId::Standard without --hardware-counters must reject before launch
    // naming all 22 instruction-gated rows (zero silent fallback to wall time).
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let no_counters = SuiteRequest::new(
        SuiteId::Standard,
        "reference/gnu-emacs",
        "target/release/neomacs",
        samples,
    );
    let err = no_counters
        .validate()
        .expect_err("standard suite without --hardware-counters must fail");
    assert!(
        matches!(
            &err,
            PerfError::HardwareCountersRequired {
                suite: SuiteId::Standard,
                scenarios,
            } if scenarios == &instruction_rows
        ),
        "expected HardwareCountersRequired listing all 22 convertible rows, got {err:?}"
    );

    // Startup overrides EditLoop -> WholeProcess (and preserves main-thread
    // selection) so the gate counts full process startup, not an empty
    // post-startup `(redisplay t)`.
    let startup_row = rows
        .iter()
        .find(|row| row.scenario == ScenarioId::Startup)
        .expect("startup row");
    assert_eq!(
        startup_row.counter_scope_override,
        Some(CounterScope::WholeProcess)
    );
    let edit_loop_req = no_counters
        .clone()
        .with_counters(Some(CounterScope::EditLoop));
    let startup_cmp = edit_loop_req
        .comparison_request_for(startup_row, None, None, None)
        .build_input();
    assert_eq!(startup_cmp.counters, Some(CounterScope::WholeProcess));
    assert_eq!(startup_cmp.primary_metric, MetricName::Instructions);

    let main_thread_req = no_counters.with_counters(Some(CounterScope::EditLoopMainThread));
    let startup_main_cmp = main_thread_req
        .comparison_request_for(startup_row, None, None, None)
        .build_input();
    assert_eq!(
        startup_main_cmp.counters,
        Some(CounterScope::WholeProcessMainThread)
    );
}

#[test]
fn instruction_gate_survives_wall_time_load_inflation_and_trips_on_instruction_regression() {
    let samples = ComparisonSampleCount::new(3).expect("valid sample count");
    let suite_request = SuiteRequest::new(
        SuiteId::Standard,
        "reference/gnu-emacs",
        "target/release/neomacs",
        samples,
    )
    .with_counters(Some(CounterScope::EditLoop));

    // One representative row from each converted row group with regressions
    // sized near the calibrated budget (not oversized wall-clock deltas):
    // OrgEditing (1.0%), BytecodeCallLoop (1.0%), SustainedEditing (11.0%),
    // Startup (32.5%).
    let group_rows = [
        (
            ScenarioId::OrgEditing,
            10_000_000.0,
            10_020_000.0,
            10_150_000.0, // +1.5% > 1.0% budget
        ),
        (
            ScenarioId::BytecodeCallLoop,
            50_000_000.0,
            50_050_000.0,
            50_750_000.0, // +1.5% > 1.0% budget
        ),
        (
            ScenarioId::SustainedEditing,
            20_000_000.0,
            20_040_000.0,
            22_400_000.0, // +12.0% > 11.0% budget
        ),
        (
            ScenarioId::Startup,
            80_000_000.0,
            80_080_000.0,
            106_800_000.0, // +33.5% > 32.5% budget
        ),
    ];

    let build_observations = |scenario_id: ScenarioId,
                              iterations: u32,
                              base_wall: [f64; 3],
                              cand_wall: [f64; 3],
                              base_ins: [f64; 3],
                              cand_ins: [f64; 3],
                              include_instructions: bool| {
        let wall_metric = crate::scenario(scenario_id).primary_metric;
        let wall_unit = wall_metric.canonical_unit();
        let mut obs = Vec::new();
        for sample_index in 0..3u32 {
            for (role, wall, ins) in [
                (
                    ComparisonRunRole::Baseline,
                    base_wall[sample_index as usize],
                    base_ins[sample_index as usize],
                ),
                (
                    ComparisonRunRole::Candidate,
                    cand_wall[sample_index as usize],
                    cand_ins[sample_index as usize],
                ),
            ] {
                let mut measurements = vec![
                    Measurement {
                        name: wall_metric,
                        value: wall,
                        unit: wall_unit,
                    },
                    Measurement {
                        name: MetricName::CpuCycles,
                        value: ins * 0.75,
                        unit: MetricUnit::Count,
                    },
                ];
                if include_instructions {
                    measurements.push(Measurement {
                        name: MetricName::Instructions,
                        value: ins,
                        unit: MetricUnit::Count,
                    });
                }
                obs.push(ComparisonObservation {
                    run: ComparisonRun {
                        role,
                        sample_index,
                        run_id: format!("{scenario_id}-{role:?}-{sample_index}"),
                        artifact_path: PathBuf::from(format!(
                            "{scenario_id}/{role:?}-{sample_index}/artifact.json"
                        )),
                        scenario: scenario_id,
                        frontend: crate::scenario(scenario_id).default_frontend,
                        editor: PathBuf::from(match role {
                            ComparisonRunRole::Baseline => "reference/gnu-emacs",
                            ComparisonRunRole::Candidate => "target/release/neomacs",
                        }),
                        iterations,
                        editor_provenance: Some(guard_provenance(role)),
                        execution_environment: Some(Default::default()),
                        passthrough_environment: None,
                        native_video_input: None,
                        native_video_execution: None,
                        outcome: ComparisonRunOutcome::Valid,
                    },
                    verdict: RunVerdict::Valid { measurements },
                });
            }
        }
        obs
    };

    // Case 1: Heavy box load inflates wall/CPU time by +45% with large spread,
    // while retired instructions stay flat (+0.1%..+0.2%). Every group MUST
    // PASS and report BOTH gate (Instructions) and non-gating wall context.
    let mut load_results = Vec::new();
    for (scenario_id, base_ins, flat_cand_ins, _) in group_rows {
        let row = SuiteId::Standard
            .scenarios()
            .iter()
            .find(|r| r.scenario == scenario_id)
            .expect("standard row");
        let input = suite_request
            .comparison_request_for(row, None, None, None)
            .build_input();
        let obs = build_observations(
            scenario_id,
            input.iterations.get(),
            [1000.0, 1020.0, 1040.0],
            [1350.0, 1480.0, 1620.0],
            [base_ins - 500.0, base_ins, base_ins + 500.0],
            [flat_cand_ins - 500.0, flat_cand_ins, flat_cand_ins + 500.0],
            true,
        );
        let ComparisonVerdict::Valid {
            summary,
            context_metrics,
        } = evaluate_comparison(&input, &obs)
        else {
            panic!("{scenario_id}: valid run with instructions must pass");
        };
        assert_eq!(summary.metric, MetricName::Instructions);
        let wall_metric = crate::scenario(scenario_id).primary_metric;
        let wall_context = context_metrics
            .iter()
            .find(|m| m.metric == wall_metric)
            .expect("wall/CPU time must be preserved as non-gating context");
        assert!(
            wall_context.percent_change > 40.0,
            "simulated wall inflation must be preserved in context: {wall_context:?}"
        );
        assert!(
            wall_context.candidate_median_absolute_deviation > 0.0,
            "wall context must keep MAD spread"
        );
        assert!(
            context_metrics
                .iter()
                .any(|m| m.metric == MetricName::CpuCycles),
            "CpuCycles context must also be preserved"
        );
        let row_result = SuiteScenarioResult {
            scenario: scenario_id,
            maximum_regression_percent: row.maximum_regression_percent,
            maximum_drift_percent: row.maximum_drift_percent,
            gate_metric: row.gate_metric,
            non_convertible_reason: row.non_convertible_reason,
            iterations: Some(input.iterations.get()),
            percent_change: Some(summary.percent_change),
            gate_summary: Some(summary),
            context_metrics,
            comparison_artifact: PathBuf::from(format!("{scenario_id}/comparison.json")),
        };
        let rendered = crate::cli::format_suite_row_summary(&row_result);
        assert!(
            rendered.contains("GATE [Instructions")
                && rendered.contains("/op")
                && rendered.contains("CONTEXT [non-gating]:")
                && rendered.contains(&format!("{wall_metric:?}")),
            "row output must clearly separate GATE vs non-gating CONTEXT: {rendered}"
        );
        load_results.push(row_result);
    }
    assert_eq!(
        evaluate_suite(&load_results, None),
        SuiteVerdict::Passed,
        "wall-time inflation under load must NOT fail instruction-gated rows"
    );

    // Case 2: Mutation regression in EVERY converted group pushes instructions
    // past threshold even while wall time improves -10% -> MUST go RED.
    for (scenario_id, base_ins, _, regressed_cand_ins) in group_rows {
        let row = SuiteId::Standard
            .scenarios()
            .iter()
            .find(|r| r.scenario == scenario_id)
            .expect("standard row");
        let input = suite_request
            .comparison_request_for(row, None, None, None)
            .build_input();
        let obs = build_observations(
            scenario_id,
            input.iterations.get(),
            [1000.0, 1020.0, 1040.0],
            [900.0, 918.0, 936.0],
            [base_ins, base_ins, base_ins],
            [regressed_cand_ins, regressed_cand_ins, regressed_cand_ins],
            true,
        );
        let ComparisonVerdict::Valid {
            summary,
            context_metrics,
        } = evaluate_comparison(&input, &obs)
        else {
            panic!("{scenario_id}: valid comparison expected");
        };
        let row_result = SuiteScenarioResult {
            scenario: scenario_id,
            maximum_regression_percent: row.maximum_regression_percent,
            maximum_drift_percent: row.maximum_drift_percent,
            gate_metric: row.gate_metric,
            non_convertible_reason: row.non_convertible_reason,
            iterations: Some(input.iterations.get()),
            percent_change: Some(summary.percent_change),
            gate_summary: Some(summary),
            context_metrics,
            comparison_artifact: PathBuf::from(format!("{scenario_id}/comparison.json")),
        };
        assert!(
            matches!(
                evaluate_suite(&[row_result], None),
                SuiteVerdict::Regressed { ref regressions }
                    if regressions.len() == 1 && regressions[0].scenario == scenario_id
            ),
            "{scenario_id} instruction regression must fail gate even when wall time improves"
        );
    }

    // Case 3: Missing Instructions on a converted row MUST reject rather than
    // falling back to wall time.
    let startup_row = SuiteId::Standard
        .scenarios()
        .iter()
        .find(|r| r.scenario == ScenarioId::Startup)
        .expect("startup row");
    let startup_input = suite_request
        .comparison_request_for(startup_row, None, None, None)
        .build_input();
    let missing_obs = build_observations(
        ScenarioId::Startup,
        startup_input.iterations.get(),
        [1000.0, 1000.0, 1000.0],
        [1000.0, 1000.0, 1000.0],
        [80_000_000.0; 3],
        [80_000_000.0; 3],
        false,
    );
    assert!(
        matches!(
            evaluate_comparison(&startup_input, &missing_obs),
            ComparisonVerdict::Rejected { .. }
        ),
        "missing Instructions must reject comparison without wall-time fallback"
    );
}

#[test]
fn drift_ratchet_skips_cross_gate_metric_lineage_and_enforces_matching_metric() {
    // Legacy wall-time (ScenarioPrimary) predecessor at -10% vs new
    // Instructions run at +0.5% must NOT trigger a spurious +10.5% drift
    // across incompatible gate metrics.
    let legacy_previous = suite_history(vec![SuiteScenarioResult {
        gate_metric: SuiteGateMetric::ScenarioPrimary,
        ..drifting_result(ScenarioId::OrgEditing, 2.0, Some(1.0), Some(-10.0))
    }]);
    let current_instructions = [SuiteScenarioResult {
        gate_metric: SuiteGateMetric::Instructions,
        ..drifting_result(ScenarioId::OrgEditing, 2.0, Some(1.0), Some(0.5))
    }];
    let (verdict, skips) = evaluate_suite_with_context(
        &current_instructions,
        Some(&legacy_previous),
        Some(SuiteEvaluationContext {
            suite: SuiteId::Standard,
            baseline_editor: Path::new("baseline-emacs"),
            counters: None,
        }),
    );
    assert_eq!(
        verdict,
        SuiteVerdict::Passed,
        "incompatible gate_metric lineage must re-baseline rather than subtract wall % from instruction %"
    );
    assert_eq!(
        skips,
        vec![SuiteDriftSkip {
            scenario: ScenarioId::OrgEditing,
            reason: SuiteDriftSkipReason::GateMetricChanged,
        }],
        "skipped drift check must be recorded explicitly, never silently disarmed"
    );

    // Cross-scope, cross-baseline, cross-suite, and cross-iterations lineage
    // must ALSO skip with explicit typed reasons instead of comparing apples
    // and oranges.
    let matching_previous = suite_history(vec![SuiteScenarioResult {
        gate_metric: SuiteGateMetric::Instructions,
        iterations: Some(20),
        ..drifting_result(ScenarioId::OrgEditing, 2.0, Some(1.0), Some(-0.8))
    }]);
    let current_with_iters = [SuiteScenarioResult {
        gate_metric: SuiteGateMetric::Instructions,
        iterations: Some(20),
        ..drifting_result(ScenarioId::OrgEditing, 2.0, Some(1.0), Some(0.5))
    }];

    let (scope_verdict, scope_skips) = evaluate_suite_with_context(
        &current_with_iters,
        Some(&matching_previous),
        Some(SuiteEvaluationContext {
            suite: SuiteId::Standard,
            baseline_editor: Path::new("baseline-emacs"),
            counters: Some(CounterScope::EditLoopMainThread),
        }),
    );
    assert_eq!(scope_verdict, SuiteVerdict::Passed);
    assert_eq!(
        scope_skips,
        vec![SuiteDriftSkip {
            scenario: ScenarioId::OrgEditing,
            reason: SuiteDriftSkipReason::CounterScopeChanged,
        }]
    );

    let (base_verdict, base_skips) = evaluate_suite_with_context(
        &current_with_iters,
        Some(&matching_previous),
        Some(SuiteEvaluationContext {
            suite: SuiteId::Standard,
            baseline_editor: Path::new("different-baseline"),
            counters: None,
        }),
    );
    assert_eq!(base_verdict, SuiteVerdict::Passed);
    assert_eq!(
        base_skips,
        vec![SuiteDriftSkip {
            scenario: ScenarioId::OrgEditing,
            reason: SuiteDriftSkipReason::BaselineEditorChanged,
        }]
    );

    let current_different_iters = [SuiteScenarioResult {
        iterations: Some(40),
        ..current_with_iters[0].clone()
    }];
    let (iter_verdict, iter_skips) = evaluate_suite_with_context(
        &current_different_iters,
        Some(&matching_previous),
        Some(SuiteEvaluationContext {
            suite: SuiteId::Standard,
            baseline_editor: Path::new("baseline-emacs"),
            counters: None,
        }),
    );
    assert_eq!(iter_verdict, SuiteVerdict::Passed);
    assert_eq!(
        iter_skips,
        vec![SuiteDriftSkip {
            scenario: ScenarioId::OrgEditing,
            reason: SuiteDriftSkipReason::IterationsChanged,
        }]
    );

    // Once all lineage dimensions match, drift still trips.
    let (drifted_verdict, drifted_skips) = evaluate_suite_with_context(
        &current_with_iters,
        Some(&matching_previous),
        Some(SuiteEvaluationContext {
            suite: SuiteId::Standard,
            baseline_editor: Path::new("baseline-emacs"),
            counters: None,
        }),
    );
    assert!(drifted_skips.is_empty());
    let SuiteVerdict::Drifted { drifts } = drifted_verdict else {
        panic!("matching Instructions lineage exceeding drift ceiling must fail with Drifted");
    };
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].scenario, ScenarioId::OrgEditing);
    assert!((drifts[0].drift_percent - 1.3).abs() < 1e-9);
}

#[test]
fn evaluate_suite_fails_closed_on_nan_infinite_zero_budgets_and_metric_mismatch() {
    for bad_budget in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
        let row = result(ScenarioId::OrgEditing, bad_budget, Some(500.0));
        assert_eq!(
            evaluate_suite(&[row], None),
            SuiteVerdict::Rejected {
                scenarios: vec![ScenarioId::OrgEditing],
            },
            "invalid budget {bad_budget:?} must fail closed as Rejected, never Passed"
        );
    }

    for bad_drift in [f64::NAN, f64::INFINITY, 0.0, -0.5] {
        let row = drifting_result(ScenarioId::OrgEditing, 1.0, Some(bad_drift), Some(0.1));
        assert_eq!(
            evaluate_suite(&[row], None),
            SuiteVerdict::Rejected {
                scenarios: vec![ScenarioId::OrgEditing],
            },
            "invalid drift budget {bad_drift:?} must fail closed as Rejected"
        );
    }

    // Non-finite percent_change fails closed.
    let nan_change = result(ScenarioId::OrgEditing, 1.0, Some(f64::NAN));
    assert_eq!(
        evaluate_suite(&[nan_change], None),
        SuiteVerdict::Rejected {
            scenarios: vec![ScenarioId::OrgEditing],
        }
    );

    // Mismatched gate_summary.metric vs gate_metric fails closed.
    let wrong_metric = SuiteScenarioResult {
        gate_summary: Some(crate::ComparisonMetricSummary {
            metric: MetricName::PerOperationWallTime,
            unit: MetricUnit::MicrosecondsPerOperation,
            baseline_samples: vec![100.0, 100.0, 100.0],
            candidate_samples: vec![100.0, 100.0, 100.0],
            paired_samples: vec![],
            baseline_median: 100.0,
            candidate_median: 100.0,
            baseline_median_absolute_deviation: 0.0,
            candidate_median_absolute_deviation: 0.0,
            candidate_to_baseline_ratio: 1.0,
            percent_change: 0.0,
        }),
        ..result(ScenarioId::OrgEditing, 1.0, Some(0.0))
    };
    assert_eq!(
        evaluate_suite(&[wrong_metric], None),
        SuiteVerdict::Rejected {
            scenarios: vec![ScenarioId::OrgEditing],
        }
    );

    // InstructionCalibration::validate rejects NaN, < 2 runs, < 5 samples, and
    // non-positive floors.
    let bad_cal = InstructionCalibration {
        runs: 1,
        samples_per_side: 5,
        max_rel_mad_percent: 0.01,
        max_span_percent: 0.05,
        max_aa_delta_percent: 0.01,
        max_aa_drift_percent: 0.01,
        tier_floor_percent: 1.0,
    };
    assert!(bad_cal.validate(2).is_err());
    let nan_cal = InstructionCalibration {
        runs: 2,
        max_rel_mad_percent: f64::NAN,
        ..bad_cal
    };
    assert!(nan_cal.budget_percent().is_nan());
    assert!(nan_cal.validate(2).is_err());
    let nan_floor = InstructionCalibration {
        runs: 2,
        tier_floor_percent: f64::NAN,
        ..bad_cal
    };
    assert!(nan_floor.budget_percent().is_nan());
    assert!(nan_floor.validate(2).is_err());
}

#[test]
fn non_gating_report_row_never_false_reds_on_latency_but_rejects_failures() {
    let latency_swing = SuiteScenarioResult {
        scenario: ScenarioId::GuiInputLatency,
        maximum_regression_percent: 0.0,
        maximum_drift_percent: None,
        gate_metric: SuiteGateMetric::NonGatingReport,
        non_convertible_reason: Some(NonConvertibleTimingReason::InputToRedisplayLatency),
        iterations: Some(1000),
        percent_change: Some(25.0),
        gate_summary: None,
        context_metrics: Vec::new(),
        comparison_artifact: PathBuf::from("gui-input-latency/comparison.json"),
    };
    assert_eq!(
        evaluate_suite(&[latency_swing.clone()], None),
        SuiteVerdict::Passed,
        "non-gating report row must not fail suite on latency swing"
    );

    let failed_state = SuiteScenarioResult {
        percent_change: None,
        ..latency_swing
    };
    assert_eq!(
        evaluate_suite(&[failed_state], None),
        SuiteVerdict::Rejected {
            scenarios: vec![ScenarioId::GuiInputLatency],
        },
        "non-gating report row must still reject suite when redisplay/correctness fails"
    );
}

#[test]
fn every_converted_standard_row_trips_just_above_its_calibrated_budget() {
    for row in SuiteId::Standard.scenarios() {
        if row.gate_metric != SuiteGateMetric::Instructions {
            continue;
        }
        let budget = row.maximum_regression_percent;
        let just_below = result(row.scenario, budget, Some(budget - 0.01));
        assert_eq!(
            evaluate_suite(&[just_below], None),
            SuiteVerdict::Passed,
            "{} must pass just below calibrated budget {budget}%",
            row.scenario
        );
        let just_above = result(row.scenario, budget, Some(budget + 0.01));
        assert!(
            matches!(
                evaluate_suite(&[just_above], None),
                SuiteVerdict::Regressed { ref regressions }
                    if regressions.len() == 1 && regressions[0].scenario == row.scenario
            ),
            "{} must trip Regressed just above calibrated budget {budget}%",
            row.scenario
        );
    }
}
