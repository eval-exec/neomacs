//! Bounded searches alternating with distant edits. Preparation and complete
//! content validation stay outside the timed loop so buffer copying by the
//! harness cannot hide search-view scaling.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName,
    PreparedScenario, PreparedWorkload, RunRequest, SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId,
    ScenarioOutcome, ScenarioStatus, collect_editor_provenance, collect_host_provenance,
    deserialize_optional_error, mismatch, prepare_gui_runtime_directory, scenario_outcome,
    sha256_file,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/bounded-search.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("bounded-search.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = BoundedSearchInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/bounded-search.el",
        workload_source_sha256: sha256_file(&fixture_source)?,
        settings: settings(request.scenario),
        environment_policy: "closed-v1",
        passthrough_environment: request
            .benchmark_environment()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
            .collect(),
    };
    let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
        .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
    fs::write(&provenance, provenance_json).map_err(|error| {
        format!(
            "failed to write input provenance {}: {error}",
            provenance.display()
        )
    })?;
    Ok(PreparedScenario {
        fixture,
        provenance,
        result: run_directory.join("scenario-result.json"),
        sentinel: run_directory.join("completed"),
        terminal_bytes: run_directory.join("terminal.ansi"),
        gui_app_log: run_directory.join("gui-app.log"),
        gui_weston_log: run_directory.join("weston.log"),
        gui_runtime_directory: prepare_gui_runtime_directory(workspace_root)?,
        sandbox,
        workload: PreparedWorkload::BoundedSearch(settings(request.scenario)),
    })
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Settings {
    pub(crate) buffer_size: usize,
    pub(crate) edit: bool,
    pub(crate) search: bool,
}

pub(crate) const fn settings(scenario: ScenarioId) -> Settings {
    match scenario {
        ScenarioId::BoundedSearchEditSmall => Settings {
            buffer_size: 1024,
            edit: true,
            search: true,
        },
        ScenarioId::BoundedSearchEditLarge => Settings {
            buffer_size: 8 * 1024 * 1024,
            edit: true,
            search: true,
        },
        ScenarioId::BoundedSearchEditOnly => Settings {
            buffer_size: 8 * 1024 * 1024,
            edit: true,
            search: false,
        },
        ScenarioId::BoundedSearchNoEdit => Settings {
            buffer_size: 8 * 1024 * 1024,
            edit: false,
            search: true,
        },
        _ => unreachable!(),
    }
}

#[derive(Serialize)]
struct BoundedSearchInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    settings: Settings,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "BoundedSearchResultWire")]
pub(crate) struct BoundedSearchResult {
    wire: BoundedSearchResultWire,
    outcome: ScenarioOutcome,
}

#[cfg(test)]
impl BoundedSearchResult {
    pub(crate) const fn elapsed_us(&self) -> u64 {
        self.wire.elapsed_us
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedSearchResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    buffer_size: usize,
    buffer_bytes: usize,
    multibyte: bool,
    edit: bool,
    search: bool,
    completed_operations: u32,
    failed_searches: u32,
    point: u32,
    match_data: Vec<u32>,
    initial_checksum: String,
    final_checksum: String,
    bytecode_compiled: bool,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<BoundedSearchResultWire> for BoundedSearchResult {
    type Error = String;

    fn try_from(mut wire: BoundedSearchResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error.take())?;
        Ok(Self { wire, outcome })
    }
}

fn expected_checksum(size: usize) -> String {
    use sha2::{Digest, Sha256};
    // The Lisp fixture inserts eight ASCII characters, one three-byte
    // character immediately after the bound, then ASCII to SIZE characters.
    let mut hash = Sha256::new();
    hash.update("aaaaaaaa中".as_bytes());
    let chunk = [b'a'; 4096];
    for _ in 0..(size - 9) / chunk.len() {
        hash.update(chunk);
    }
    hash.update(&chunk[..(size - 9) % chunk.len()]);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn validate_bounded_search_result(
    request: &RunRequest,
    result: &BoundedSearchResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    let r = &result.wire;
    let expected = settings(request.scenario);
    let checksum = expected_checksum(expected.buffer_size);
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
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        r.iterations,
    );
    mismatch(
        &mut mismatches,
        "completed-operations",
        request.iterations.get(),
        r.completed_operations,
    );
    mismatch(
        &mut mismatches,
        "failed-searches",
        if expected.search {
            request.iterations.get()
        } else {
            0
        },
        r.failed_searches,
    );
    mismatch(
        &mut mismatches,
        "buffer-size",
        expected.buffer_size,
        r.buffer_size,
    );
    mismatch(
        &mut mismatches,
        "buffer-bytes",
        expected.buffer_size + 2,
        r.buffer_bytes,
    );
    mismatch(&mut mismatches, "multibyte", true, r.multibyte);
    mismatch(&mut mismatches, "edit-enabled", expected.edit, r.edit);
    mismatch(&mut mismatches, "search-enabled", expected.search, r.search);
    mismatch(&mut mismatches, "point", 1, r.point);
    mismatch(
        &mut mismatches,
        "match-data",
        format!("{:?}", [7, 9]),
        format!("{:?}", r.match_data),
    );
    mismatch(
        &mut mismatches,
        "initial-content",
        checksum.as_str(),
        r.initial_checksum.as_str(),
    );
    mismatch(
        &mut mismatches,
        "final-content",
        checksum.as_str(),
        r.final_checksum.as_str(),
    );
    mismatch(
        &mut mismatches,
        "bytecode-compiled",
        true,
        r.bytecode_compiled,
    );
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-cpu-time", r.elapsed_us);
    crate::harness::require_positive_phase(&mut mismatches, "elapsed-wall-time", r.elapsed_wall_us);
    mismatches
}

pub(crate) fn valid_bounded_search_measurements(
    result: &BoundedSearchResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let r = &result.wire;
    [
        (MetricName::ProcessWallTime, wall_elapsed_us as f64),
        (MetricName::WorkloadCpuTime, r.elapsed_us as f64),
        (MetricName::WorkloadWallTime, r.elapsed_wall_us as f64),
        (
            MetricName::PerOperationCpuTime,
            r.elapsed_us as f64 / f64::from(r.iterations),
        ),
        (
            MetricName::PerOperationWallTime,
            r.elapsed_wall_us as f64 / f64::from(r.iterations),
        ),
        (
            MetricName::OperationCount,
            f64::from(r.completed_operations),
        ),
        (MetricName::Iterations, f64::from(r.iterations)),
    ]
    .into_iter()
    .map(|(name, value)| Measurement {
        name,
        value,
        unit: name.canonical_unit(),
    })
    .collect()
}

#[cfg(test)]
#[path = "bounded_search_test.rs"]
mod tests;
