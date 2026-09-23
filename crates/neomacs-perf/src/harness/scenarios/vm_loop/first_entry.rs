//! End-to-end first hot calls, including interpreter heat-up and native compilation.

use super::*;

#[derive(Debug, Deserialize)]
#[serde(try_from = "FirstHotLoopResultWire")]
pub(crate) struct FirstHotLoopResult {
    wire: FirstHotLoopResultWire,
    outcome: ScenarioOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstHotLoopResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    inner_iterations: u32,
    #[serde(default)]
    branches_per_iteration: Option<u32>,
    prepared_functions: u32,
    bytecode_compiled: bool,
    completed_operations: u32,
    results: Vec<[i64; 3]>,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<FirstHotLoopResultWire> for FirstHotLoopResult {
    type Error = String;

    fn try_from(mut wire: FirstHotLoopResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error.take())?;
        Ok(Self { wire, outcome })
    }
}

#[cfg(test)]
impl FirstHotLoopResult {
    pub(crate) const fn elapsed_us(&self) -> u64 {
        self.wire.elapsed_us
    }
}

pub(crate) fn validate(
    request: &RunRequest,
    result: &FirstHotLoopResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    let r = &result.wire;
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        r.schema_version,
    );
    mismatch(&mut mismatches, "scenario-id", request.scenario, r.scenario);
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    let inner_iterations = request
        .scenario
        .first_hot_loop_iterations()
        .expect("first-call result requires a first-call scenario");
    let count = request.iterations.get();
    mismatch(&mut mismatches, "iterations", count, r.iterations);
    mismatch(
        &mut mismatches,
        "prepared-functions",
        count,
        r.prepared_functions,
    );
    mismatch(
        &mut mismatches,
        "completed-operations",
        count,
        r.completed_operations,
    );
    mismatch(
        &mut mismatches,
        "result-count",
        count as usize,
        r.results.len(),
    );
    mismatch(
        &mut mismatches,
        "inner-iterations",
        inner_iterations,
        r.inner_iterations,
    );
    mismatch(
        &mut mismatches,
        "bytecode-compiled",
        true,
        r.bytecode_compiled,
    );
    let branches = request.scenario.first_call_branches();
    mismatch(
        &mut mismatches,
        "branches-per-iteration",
        format!("{branches:?}"),
        format!("{:?}", r.branches_per_iteration),
    );
    let sum = branches.map_or_else(
        || expected_sum(inner_iterations),
        |branches| {
            // Each k=1..D adds 1 for the first k iterations and 2 thereafter.
            let d = i64::from(branches);
            2 * i64::from(inner_iterations) * d - d * (d + 1) / 2
        },
    );
    for (index, values) in r.results.iter().enumerate() {
        mismatch(
            &mut mismatches,
            &format!("function-{index}-iterations"),
            i64::from(inner_iterations),
            values[0],
        );
        mismatch(
            &mut mismatches,
            &format!("function-{index}-sum"),
            sum,
            values[1],
        );
        mismatch(
            &mut mismatches,
            &format!("function-{index}-identity"),
            index as i64,
            values[2],
        );
    }
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-cpu-time", r.elapsed_us);
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-wall-time", r.elapsed_wall_us);
    mismatches
}

pub(crate) fn measurements(result: &FirstHotLoopResult, process_us: u128) -> Vec<Measurement> {
    let r = &result.wire;
    // One operation is one fresh function's first call, not one inner iteration.
    loop_measurements(
        r.iterations,
        r.completed_operations,
        r.elapsed_us,
        r.elapsed_wall_us,
        process_us,
    )
}

#[cfg(test)]
#[path = "first_entry_test.rs"]
mod tests;
