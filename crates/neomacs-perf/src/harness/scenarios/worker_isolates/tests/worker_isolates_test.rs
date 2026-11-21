use std::num::NonZeroU32;

use crate::{RunRequest, ScenarioId};

use super::WorkerIsolatesResult;

fn result_json(status: &str, error: serde_json::Value, equal: bool) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario": "worker-isolates-loop",
        "status": status,
        "elapsed_us": 100,
        "num_processors": 4,
        "pool_size": 4,
        "first_task_latency_us": 0,
        "boot_max_us": 300,
        "boot_mean_us": 200,
        "samples": [
            {"tier": "interpreted-lambda", "tasks": 4, "serial_seconds": 4.0,
             "parallel_first_seconds": 2.0, "parallel_seconds": 1.0,
             "serial_parallel_ratio": 4.0, "results_equal": equal},
            {"tier": "interpreted-lambda", "tasks": 16, "serial_seconds": 4.0,
             "parallel_first_seconds": 2.0, "parallel_seconds": 1.0,
             "serial_parallel_ratio": 4.0, "results_equal": equal},
            {"tier": "bytecode-jit-on", "tasks": 4, "serial_seconds": 4.0,
             "parallel_first_seconds": 2.0, "parallel_seconds": 1.0,
             "serial_parallel_ratio": 4.0, "results_equal": equal},
            {"tier": "bytecode-jit-on", "tasks": 16, "serial_seconds": 4.0,
             "parallel_first_seconds": 2.0, "parallel_seconds": 1.0,
             "serial_parallel_ratio": 4.0, "results_equal": equal}
        ],
        "error": error
    })
    .to_string()
}

fn request() -> RunRequest {
    RunRequest::new(
        ScenarioId::WorkerIsolatesLoop,
        "workspace",
        NonZeroU32::new(1).expect("one is positive"),
    )
}

#[test]
fn valid_result_requires_equal_worker_values_and_positive_timing_data() {
    let result: WorkerIsolatesResult =
        serde_json::from_str(&result_json("ok", serde_json::Value::Null, true))
            .expect("valid fixture result deserializes");
    assert!(super::validate(&request(), &result).is_empty());

    let unequal: WorkerIsolatesResult =
        serde_json::from_str(&result_json("ok", serde_json::Value::Null, false))
            .expect("mismatch remains a typed result");
    assert!(
        super::validate(&request(), &unequal)
            .iter()
            .any(|mismatch| mismatch.invariant == "sample-0-results-equal")
    );
}

#[test]
fn unsupported_editor_is_a_typed_invalid_scenario_result() {
    let result: WorkerIsolatesResult = serde_json::from_str(&result_json(
        "error",
        serde_json::Value::String("editor lacks neomacs-worker-run".to_string()),
        true,
    ))
    .expect("unsupported editor response deserializes");
    assert!(
        super::validate(&request(), &result)
            .iter()
            .any(|mismatch| mismatch.invariant == "scenario-outcome")
    );
}

#[test]
fn unknown_result_fields_are_rejected() {
    let raw = result_json("ok", serde_json::Value::Null, true)
        .replace("\"error\":null", "\"error\":null,\"timing_guess\":1");
    assert!(serde_json::from_str::<WorkerIsolatesResult>(&raw).is_err());
}
