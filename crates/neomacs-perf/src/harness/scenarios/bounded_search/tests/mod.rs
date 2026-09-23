use super::*;
use crate::{MetricName, PerfHarness, RunVerdict};
use serde_json::{Value, json};
use std::num::NonZeroU32;

fn valid_result(scenario: ScenarioId) -> Value {
    let config = settings(scenario);
    let checksum = expected_checksum(config.buffer_size);
    json!({
        "schema_version": 1, "scenario": scenario, "status": "ok", "error": null,
        "iterations": 10, "elapsed_us": 20, "elapsed_wall_us": 25,
        "buffer_size": config.buffer_size, "buffer_bytes": config.buffer_size + 2,
        "multibyte": true, "edit": config.edit, "search": config.search,
        "completed_operations": 10, "failed_searches": if config.search { 10 } else { 0 },
        "point": 1, "match_data": [7, 9], "bytecode_compiled": true,
        "initial_checksum": checksum, "final_checksum": checksum,
    })
}

fn workspace() -> tempfile::TempDir {
    let root = crate::workspace_root().join("tmp");
    fs::create_dir_all(&root).expect("scratch root");
    tempfile::Builder::new()
        .prefix("bounded-search-result-")
        .tempdir_in(root)
        .expect("workspace-local scratch")
}

#[test]
fn bounded_search_accepts_all_four_valid_scenarios() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    for scenario in [
        ScenarioId::BoundedSearchEditSmall,
        ScenarioId::BoundedSearchEditLarge,
        ScenarioId::BoundedSearchEditOnly,
        ScenarioId::BoundedSearchNoEdit,
    ] {
        let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
        let report = harness
            .record_fixture_result(&request, &valid_result(scenario).to_string())
            .unwrap();
        let RunVerdict::Valid { measurements } = report.artifact.verdict else {
            panic!("valid {scenario} rejected")
        };
        let metric = measurements
            .iter()
            .find(|m| m.name == MetricName::PerOperationWallTime)
            .unwrap();
        assert_eq!(metric.value, 2.5);
        assert_eq!(
            metric.unit,
            MetricName::PerOperationWallTime.canonical_unit()
        );
    }
}

#[test]
fn bounded_search_rejects_incomplete_or_changed_work_before_publishing_timings() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let scenario = ScenarioId::BoundedSearchEditSmall;
    let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
    for (key, wrong) in [
        ("iterations", json!(9)),
        ("completed_operations", json!(9)),
        ("failed_searches", json!(9)),
        ("buffer_size", json!(100)),
        ("buffer_bytes", json!(1024)),
        ("multibyte", json!(false)),
        ("edit", json!(false)),
        ("search", json!(false)),
        ("point", json!(2)),
        ("match_data", json!([1, 2])),
        ("bytecode_compiled", json!(false)),
        ("initial_checksum", json!("changed")),
        ("final_checksum", json!("changed")),
        ("elapsed_us", json!(0)),
        ("elapsed_wall_us", json!(0)),
        ("scenario", json!("bounded-search-edit-large")),
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
            "{key} mismatch must not publish measurements"
        );
    }
    // Comparing the two hashes only with one another would accept this wrong input.
    let mut result = valid_result(scenario);
    result["initial_checksum"] = json!("same-wrong-input");
    result["final_checksum"] = json!("same-wrong-input");
    let report = harness
        .record_fixture_result(&request, &result.to_string())
        .unwrap();
    assert!(matches!(
        report.artifact.verdict,
        RunVerdict::CorrectnessMismatch { .. }
    ));
}

#[test]
fn bounded_search_requires_every_result_field_and_rejects_contradictory_status() {
    let workspace = workspace();
    let harness = PerfHarness::new(workspace.path());
    let scenario = ScenarioId::BoundedSearchEditSmall;
    let request = RunRequest::new(scenario, "/unused/editor", NonZeroU32::new(10).unwrap());
    let valid = valid_result(scenario);
    for key in valid.as_object().unwrap().keys() {
        let mut incomplete = valid.clone();
        incomplete.as_object_mut().unwrap().remove(key);
        let report = harness.record_fixture_result(&request, &incomplete.to_string());
        assert!(
            matches!(report, Err(crate::PerfError::InvalidScenarioResult { .. })),
            "missing {key} must not publish measurements"
        );
    }
    let mut result = valid;
    result["error"] = json!("failed despite ok status");
    let report = harness.record_fixture_result(&request, &result.to_string());
    assert!(matches!(
        report,
        Err(crate::PerfError::InvalidScenarioResult { .. })
    ));
}

#[test]
fn bounded_search_input_hash_agrees_with_gnu_buffer_bytes() {
    // Independently collected from GNU Emacs 31.1 using secure-hash on the buffer.
    assert_eq!(
        expected_checksum(1024),
        "cf106d86c56be5da6fd86c63126346f1d48299b346719e6d1248453c87df6ddc"
    );
}
