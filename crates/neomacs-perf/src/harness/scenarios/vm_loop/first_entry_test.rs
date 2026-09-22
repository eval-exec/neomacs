use super::*;
use crate::{PerfError, PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

fn valid_result() -> Value {
    json!({
        "schema_version": 1, "scenario": "first-hot-loop", "status": "ok", "error": null,
        "iterations": 3, "inner_iterations": 65536, "prepared_functions": 3,
        "bytecode_compiled": true, "completed_operations": 3,
        "results": [[65536, 2147450880_i64, 0], [65536, 2147450880_i64, 1], [65536, 2147450880_i64, 2]],
        "elapsed_us": 600, "elapsed_wall_us": 900,
    })
}

fn request() -> RunRequest {
    RunRequest::new(
        ScenarioId::FirstHotLoop,
        "/unused/editor",
        NonZeroU32::new(3).unwrap(),
    )
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("first-hot-loop-")
        .tempdir_in(root)
        .unwrap()
}

#[test]
fn first_hot_loop_measures_per_fresh_function_instead_of_inner_iteration() {
    let workspace = workspace();
    let report = PerfHarness::new(workspace.path())
        .record_fixture_result(&request(), &valid_result().to_string())
        .unwrap();
    let RunVerdict::Valid { measurements } = report.artifact.verdict else {
        panic!("valid first hot calls rejected")
    };
    for (name, expected) in [
        (MetricName::PerOperationWallTime, 300.0),
        (MetricName::PerOperationCpuTime, 200.0),
        (MetricName::OperationCount, 3.0),
    ] {
        let measurement = measurements.iter().find(|m| m.name == name).unwrap();
        assert_eq!(measurement.value, expected);
        assert_eq!(measurement.unit, name.canonical_unit());
    }
}

#[test]
fn first_hot_loop_rejects_each_incorrect_function_and_shortened_work() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let reject = |result: Value| {
        let report = harness
            .record_fixture_result(&request(), &result.to_string())
            .unwrap();
        assert!(
            matches!(
                report.artifact.verdict,
                RunVerdict::CorrectnessMismatch { .. }
            ),
            "invalid work published measurements: {result}"
        );
    };
    for function in 0..3 {
        for field in 0..3 {
            let mut result = valid_result();
            result["results"][function][field] = json!(-1);
            reject(result);
        }
    }
    for (key, wrong) in [
        ("schema_version", json!(2)),
        ("scenario", json!("lexical-loop")),
        ("iterations", json!(2)),
        ("prepared_functions", json!(2)),
        ("completed_operations", json!(2)),
        ("inner_iterations", json!(65535)),
        ("bytecode_compiled", json!(false)),
        ("elapsed_us", json!(0)),
        ("elapsed_wall_us", json!(0)),
        ("results", json!([])),
    ] {
        let mut result = valid_result();
        result[key] = wrong;
        reject(result);
    }
    let mut fewer = valid_result();
    for field in ["iterations", "prepared_functions", "completed_operations"] {
        fewer[field] = json!(2);
    }
    fewer["results"].as_array_mut().unwrap().pop();
    reject(fewer);
    let mut shorter = valid_result();
    shorter["inner_iterations"] = json!(100);
    for row in shorter["results"].as_array_mut().unwrap() {
        row[0] = json!(100);
        row[1] = json!(4950);
    }
    reject(shorter);
    let mut duplicate = valid_result();
    duplicate["results"][1] = duplicate["results"][0].clone();
    reject(duplicate);
    let mut failed = valid_result();
    failed["status"] = json!("error");
    failed["error"] = json!("failed hot call");
    reject(failed);
}

#[test]
fn first_hot_loop_requires_complete_unambiguous_results() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let reject = |result: Value| {
        assert!(
            matches!(
                harness.record_fixture_result(&request(), &result.to_string()),
                Err(PerfError::InvalidScenarioResult { .. })
            ),
            "invalid result schema accepted: {result}"
        );
    };
    let valid = valid_result();
    for key in valid.as_object().unwrap().keys() {
        let mut result = valid.clone();
        result.as_object_mut().unwrap().remove(key);
        reject(result);
    }
    for (key, value) in [
        ("error", json!("failed despite ok status")),
        ("unknown", json!(true)),
        ("results", json!([[65536, 0]])),
    ] {
        let mut result = valid.clone();
        result[key] = value;
        reject(result);
    }
}

#[test]
fn first_hot_loop_lengths_require_the_requested_work_for_every_function() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let cases = [
        (ScenarioId::FirstHotLoop8K, 8192, 33_550_336_i64),
        (ScenarioId::FirstHotLoop16K, 16384, 134_209_536_i64),
        (ScenarioId::FirstHotLoop32K, 32768, 536_854_528_i64),
        (ScenarioId::FirstHotLoop, 65536, 2_147_450_880_i64),
    ];
    for (id, inner, sum) in cases {
        assert_eq!(serde_json::to_value(id).unwrap(), json!(id.as_str()));
        let request = RunRequest::new(id, "/unused/editor", NonZeroU32::new(3).unwrap());
        let mut result = valid_result();
        result["scenario"] = json!(id.as_str());
        result["inner_iterations"] = json!(inner);
        result["results"] = json!([[inner, sum, 0], [inner, sum, 1], [inner, sum, 2]]);
        let verdict = |result: &Value| {
            harness
                .record_fixture_result(&request, &result.to_string())
                .unwrap()
                .artifact
                .verdict
        };
        let RunVerdict::Valid { measurements } = verdict(&result) else {
            panic!("valid first-call length rejected: {id}");
        };
        assert_eq!(
            measurements
                .iter()
                .find(|m| m.name == MetricName::PerOperationWallTime)
                .unwrap()
                .value,
            300.0
        );
        for (other_id, other_inner, other_sum) in cases {
            if other_id == id {
                continue;
            }
            let mut wrong_id = result.clone();
            wrong_id["scenario"] = json!(other_id.as_str());
            assert!(matches!(
                verdict(&wrong_id),
                RunVerdict::CorrectnessMismatch { .. }
            ));
            // Relabeling a shorter or longer complete result does not let it
            // enter this workload's time series, even if its own sum is valid.
            let mut wrong_work = result.clone();
            wrong_work["inner_iterations"] = json!(other_inner);
            wrong_work["results"] = json!([
                [other_inner, other_sum, 0],
                [other_inner, other_sum, 1],
                [other_inner, other_sum, 2],
            ]);
            assert!(matches!(
                verdict(&wrong_work),
                RunVerdict::CorrectnessMismatch { .. }
            ));
        }
        for index in 0..3 {
            let mut wrong = result.clone();
            wrong["results"][index][1] = json!(sum - 1);
            assert!(matches!(
                verdict(&wrong),
                RunVerdict::CorrectnessMismatch { .. }
            ));
        }
    }
}
