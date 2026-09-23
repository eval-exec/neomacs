use super::*;
use crate::{PerfError, PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

const CASES: &[ScenarioId] = &[
    ScenarioId::BuiltinCallPoint,
    ScenarioId::BuiltinCallStringBytes,
    ScenarioId::BuiltinCallStringLessp,
    ScenarioId::BuiltinCallGetTextProperty,
    ScenarioId::BuiltinCallMultibyteStringP,
    ScenarioId::BuiltinCallCharOrStringP,
    ScenarioId::BuiltinCallMaxChar,
];

fn valid_result(scenario: ScenarioId) -> Value {
    let (alias, value) = expected(scenario);
    json!({
        "schema_version": 1, "scenario": scenario, "status": "ok", "error": null,
        "iterations": 10, "completed_operations": 10,
        "elapsed_us": 20, "elapsed_wall_us": 25,
        "result_value": value, "call_alias": alias,
        "warmup_iterations": 2000, "warmup_results": vec![value; 40],
        "bytecode_compiled": true, "alias_retained": true,
    })
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("builtin-call-")
        .tempdir_in(root)
        .unwrap()
}

#[test]
fn builtin_calls_validate_all_cases_and_measure_per_invocation_including_loop_overhead() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for &scenario in CASES {
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
fn builtin_calls_reject_wrong_values_aliases_work_counts_and_zero_phases() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for &scenario in CASES {
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
            ("result_value", json!("wrong")),
            ("call_alias", json!("point")),
            ("warmup_iterations", json!(1999)),
            ("warmup_results", json!([])),
            ("bytecode_compiled", json!(false)),
            ("alias_retained", json!(false)),
            ("elapsed_us", json!(0)),
            ("elapsed_wall_us", json!(0)),
        ] {
            let mut result = valid_result(scenario);
            result[key] = wrong;
            reject(result);
        }
        for index in 0..40 {
            let mut result = valid_result(scenario);
            result["warmup_results"][index] = json!("wrong");
            reject(result);
        }
        let mut shortened = valid_result(scenario);
        shortened["iterations"] = json!(9);
        shortened["completed_operations"] = json!(9);
        reject(shortened);
        let mut failed = valid_result(scenario);
        failed["status"] = json!("error");
        failed["error"] = json!("failed builtin call");
        reject(failed);
    }
}

#[test]
fn builtin_calls_require_complete_unambiguous_results() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let scenario = ScenarioId::BuiltinCallPoint;
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
        ("result_value", json!(1)),
        ("warmup_results", json!([1])),
    ] {
        let mut result = valid.clone();
        result[key] = wrong;
        reject(result);
    }
}
