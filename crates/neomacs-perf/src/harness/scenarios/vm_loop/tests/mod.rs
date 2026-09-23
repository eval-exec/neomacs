use super::*;
use crate::{PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

fn reads_variable(scenario: ScenarioId) -> bool {
    matches!(
        scenario,
        ScenarioId::DynamicVariableReadLoop
            | ScenarioId::DynamicAliasReadLoop
            | ScenarioId::BufferLocalReadLoop
    )
}

fn valid_result(scenario: ScenarioId) -> Value {
    let mut result = json!({
        "schema_version": 1, "scenario": scenario, "status": "ok", "error": null,
        "iterations": 10, "elapsed_us": 20, "elapsed_wall_us": 25,
        "completed_operations": 10, "result_sum": if reads_variable(scenario) { 70 } else { 45 }, "held_value": 7,
        "outer_value_before": 17, "outer_value_after": 17,
        "warmup_result": [100, if reads_variable(scenario) { 700 } else { 4950 }, 7], "warmup_outer_value": 17,
        "dynamic_binding": scenario != ScenarioId::LexicalLoop,
        "bytecode_compiled": true,
    });
    if matches!(
        scenario,
        ScenarioId::DynamicAliasReadLoop | ScenarioId::BufferLocalReadLoop
    ) {
        result["global_value_after"] = json!(17);
        result["warmup_global_value"] = json!(17);
        result["buffer_local"] = json!(scenario == ScenarioId::BufferLocalReadLoop);
    }
    result
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).expect("scratch root");
    tempfile::Builder::new()
        .prefix("vm-loop-result-")
        .tempdir_in(root)
        .expect("workspace-local scratch")
}

#[test]
fn vm_loop_accepts_checked_results_and_per_iteration_measurements() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for scenario in [
        ScenarioId::LexicalLoop,
        ScenarioId::DynamicBindingLoop,
        ScenarioId::DynamicVariableReadLoop,
        ScenarioId::DynamicRebindingLoop,
        ScenarioId::DynamicAliasReadLoop,
        ScenarioId::BufferLocalReadLoop,
    ] {
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
            let metric = measurements.iter().find(|m| m.name == name).unwrap();
            assert_eq!(metric.value, expected);
            assert_eq!(metric.unit, name.canonical_unit());
        }
    }
}

#[test]
fn vm_loop_rejects_wrong_work_and_binding_restoration_before_publishing_timings() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for scenario in [
        ScenarioId::LexicalLoop,
        ScenarioId::DynamicBindingLoop,
        ScenarioId::DynamicVariableReadLoop,
        ScenarioId::DynamicRebindingLoop,
        ScenarioId::DynamicAliasReadLoop,
        ScenarioId::BufferLocalReadLoop,
    ] {
        let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
        let warmup_sum = if reads_variable(scenario) { 700 } else { 4950 };
        for (key, wrong) in [
            ("iterations", json!(9)),
            ("completed_operations", json!(9)),
            ("result_sum", json!(44)),
            ("held_value", json!(17)),
            ("outer_value_before", json!(7)),
            ("outer_value_after", json!(7)),
            ("warmup_result", json!([99, warmup_sum, 7])),
            ("warmup_result", json!([100, warmup_sum - 1, 7])),
            ("warmup_result", json!([100, warmup_sum, 17])),
            ("warmup_outer_value", json!(7)),
            (
                "dynamic_binding",
                json!(scenario == ScenarioId::LexicalLoop),
            ),
            ("bytecode_compiled", json!(false)),
            ("elapsed_us", json!(0)),
            ("elapsed_wall_us", json!(0)),
            ("scenario", json!("bytecode-call-loop")),
            ("schema_version", json!(2)),
        ] {
            let mut result = valid_result(scenario);
            result[key] = wrong;
            let report = harness
                .record_fixture_result(&request, &result.to_string())
                .unwrap();
            assert!(
                matches!(
                    report.artifact.verdict,
                    RunVerdict::CorrectnessMismatch { .. }
                ),
                "{scenario} {key} mismatch must not publish measurements"
            );
        }
        // Internally consistent shortened work must still fail against the request.
        let mut result = valid_result(scenario);
        result["iterations"] = json!(9);
        result["completed_operations"] = json!(9);
        result["result_sum"] = json!(if reads_variable(scenario) { 63 } else { 36 });
        let report = harness
            .record_fixture_result(&request, &result.to_string())
            .unwrap();
        let RunVerdict::CorrectnessMismatch { mismatches } = report.artifact.verdict else {
            panic!("shortened work accepted")
        };
        assert!(mismatches.iter().any(|m| m.invariant == "sum"));
    }
}

#[test]
fn vm_loop_requires_all_fields_and_consistent_status() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let scenario = ScenarioId::DynamicBindingLoop;
    let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
    let valid = valid_result(scenario);
    for key in valid.as_object().unwrap().keys() {
        let mut incomplete = valid.clone();
        incomplete.as_object_mut().unwrap().remove(key);
        assert!(
            matches!(
                harness.record_fixture_result(&request, &incomplete.to_string()),
                Err(crate::PerfError::InvalidScenarioResult { .. })
            ),
            "missing {key} accepted"
        );
    }
    let mut contradictory = valid;
    contradictory["error"] = json!("failed despite ok status");
    assert!(matches!(
        harness.record_fixture_result(&request, &contradictory.to_string()),
        Err(crate::PerfError::InvalidScenarioResult { .. })
    ));
}

#[test]
fn vm_loop_sum_oracle_handles_full_request_range_without_overflow() {
    assert_eq!(expected_sum(1), 0);
    assert_eq!(expected_sum(100), 4950);
    assert_eq!(expected_sum(1_000_000), 499_999_500_000);
    assert_eq!(expected_sum(u32::MAX), 9_223_372_030_412_324_865);
}

#[test]
fn vm_loop_requires_alias_and_local_context_and_checks_optional_legacy_values() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for scenario in [
        ScenarioId::LexicalLoop,
        ScenarioId::DynamicBindingLoop,
        ScenarioId::DynamicVariableReadLoop,
        ScenarioId::DynamicRebindingLoop,
        ScenarioId::DynamicAliasReadLoop,
        ScenarioId::BufferLocalReadLoop,
    ] {
        let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
        let required = matches!(
            scenario,
            ScenarioId::DynamicAliasReadLoop | ScenarioId::BufferLocalReadLoop
        );
        for (field, invariant, correct, wrong) in [
            (
                "global_value_after",
                "global-value-after",
                json!(17),
                json!(7),
            ),
            (
                "warmup_global_value",
                "warmup-global-value",
                json!(17),
                json!(7),
            ),
            (
                "buffer_local",
                "buffer-local",
                json!(scenario == ScenarioId::BufferLocalReadLoop),
                json!(scenario != ScenarioId::BufferLocalReadLoop),
            ),
        ] {
            let mut result = valid_result(scenario);
            result[field] = correct;
            assert!(matches!(
                harness
                    .record_fixture_result(&request, &result.to_string())
                    .unwrap()
                    .artifact
                    .verdict,
                RunVerdict::Valid { .. }
            ));
            for missing in [false, true] {
                let mut result = valid_result(scenario);
                if missing {
                    result.as_object_mut().unwrap().remove(field);
                } else {
                    result[field] = wrong.clone();
                }
                let verdict = harness
                    .record_fixture_result(&request, &result.to_string())
                    .unwrap()
                    .artifact
                    .verdict;
                if missing && !required {
                    assert!(matches!(verdict, RunVerdict::Valid { .. }));
                } else {
                    let RunVerdict::CorrectnessMismatch { mismatches } = verdict else {
                        panic!("{scenario} accepted {field} missing={missing}")
                    };
                    assert!(mismatches.iter().any(|m| m.invariant == invariant));
                }
            }
        }
    }
}
