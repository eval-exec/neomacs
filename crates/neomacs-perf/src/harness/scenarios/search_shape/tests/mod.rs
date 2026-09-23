use super::*;
use crate::{PerfError, PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

// GNU Emacs 31.1 fixture oracles: point/result and integer match data.
const CASES: &[(ScenarioId, u32, &str)] = &[
    (ScenarioId::SearchLiteralForward, 4, "(2 4 t)"),
    (ScenarioId::SearchLiteralBackward, 5, "(5 7 t)"),
    (ScenarioId::SearchRegexpForward, 4, "(2 4 2 4 t)"),
    (ScenarioId::SearchRegexpBackward, 5, "(5 7 5 7 t)"),
    (ScenarioId::SearchPosixForward, 4, "(2 4 2 4 t)"),
    (ScenarioId::SearchPosixBackward, 5, "(5 7 5 7 t)"),
];

fn valid_result(scenario: ScenarioId) -> Value {
    let &(_, value, data) = CASES.iter().find(|&&(id, _, _)| id == scenario).unwrap();
    json!({
        "schema_version": 1, "scenario": scenario, "status": "ok", "error": null,
        "iterations": 10, "completed_operations": 10,
        "elapsed_us": 20, "elapsed_wall_us": 25,
        "result_value": value, "point": value, "match_data": data,
        "buffer_contents": "éab abc", "buffer_size": 7, "buffer_bytes": 8, "multibyte": true,
        "warmup_single_calls": 2000, "warmup_batch_calls": 20,
        "warmup_batch_iterations": 100, "warmup_validations": 2020,
        "bytecode_compiled": true,
    })
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("search-shape-")
        .tempdir_in(root)
        .unwrap()
}

#[test]
fn search_shapes_validate_all_cases_and_measure_per_invocation_including_loop_overhead() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for &(scenario, _, _) in CASES {
        let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
        let report = harness
            .record_fixture_result(&request, &valid_result(scenario).to_string())
            .unwrap();
        let RunVerdict::Valid { measurements } = report.artifact.verdict else {
            panic!("valid {scenario} rejected")
        };
        for (name, expected) in [
            (MetricName::PerOperationWallTime, 2.5),
            (MetricName::PerOperationCpuTime, 2.0),
            (MetricName::OperationCount, 10.0),
        ] {
            let actual = measurements.iter().find(|m| m.name == name).unwrap();
            assert_eq!(actual.value, expected);
            assert_eq!(actual.unit, name.canonical_unit());
        }
    }
}

#[test]
fn search_shapes_reject_wrong_search_state_work_counts_and_zero_phases() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for &(scenario, _, _) in CASES {
        let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
        let reject = |result: Value| {
            let report = harness
                .record_fixture_result(&request, &result.to_string())
                .unwrap();
            assert!(
                matches!(
                    report.artifact.verdict,
                    RunVerdict::CorrectnessMismatch { .. }
                ),
                "invalid work published measurements: {result}"
            );
        };
        for (key, wrong) in [
            ("schema_version", json!(2)),
            ("scenario", json!("lexical-loop")),
            ("iterations", json!(9)),
            ("completed_operations", json!(9)),
            ("result_value", json!(0)),
            ("point", json!(0)),
            ("match_data", json!("(2 4)")),
            ("buffer_contents", json!("changed")),
            ("buffer_size", json!(8)),
            ("buffer_bytes", json!(7)),
            ("multibyte", json!(false)),
            ("warmup_single_calls", json!(1999)),
            ("warmup_batch_calls", json!(19)),
            ("warmup_batch_iterations", json!(99)),
            ("warmup_validations", json!(2019)),
            ("bytecode_compiled", json!(false)),
            ("elapsed_us", json!(0)),
            ("elapsed_wall_us", json!(0)),
        ] {
            let mut result = valid_result(scenario);
            result[key] = wrong;
            reject(result);
        }
        let mut shortened = valid_result(scenario);
        shortened["iterations"] = json!(9);
        shortened["completed_operations"] = json!(9);
        reject(shortened);
        let mut failed = valid_result(scenario);
        failed["status"] = json!("error");
        failed["error"] = json!("failed search");
        reject(failed);
    }
}

#[test]
fn search_shapes_require_complete_unambiguous_results() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let scenario = ScenarioId::SearchLiteralForward;
    let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
    let reject = |result: Value| {
        assert!(
            matches!(
                harness.record_fixture_result(&request, &result.to_string()),
                Err(PerfError::InvalidScenarioResult { .. })
            ),
            "accepted {result}"
        );
    };
    let valid = valid_result(scenario);
    for key in valid.as_object().unwrap().keys() {
        let mut result = valid.clone();
        result.as_object_mut().unwrap().remove(key);
        reject(result);
    }
    for (key, wrong) in [
        ("error", json!("failed despite ok status")),
        ("unknown", json!(true)),
        ("result_value", json!("wrong")),
        ("match_data", json!([1])),
    ] {
        let mut result = valid.clone();
        result[key] = wrong;
        reject(result);
    }
}
