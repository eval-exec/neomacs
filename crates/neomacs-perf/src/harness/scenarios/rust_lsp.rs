//! The rust-lsp-typing scenario: Rust Tree-sitter editing under
//! revision-pinned LSP Mode with deterministic diagnostic replay.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use neomacs_melpa_test_support::MelpaSandbox;
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit,
    PreparedScenario, PreparedWorkload, RunRequest, SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId,
    ScenarioOutcome, ScenarioStatus, benchmark_passthrough_environment, collect_editor_provenance,
    collect_host_provenance, deserialize_optional_error, mismatch, prepare_gui_runtime_directory,
    scenario_outcome, sha256_file,
};
use crate::harness::{GrammarProvenance, PackageProvenance, prepare_cached_tree_sitter_grammar};
use neomacs_melpa_test_support::{EmacsRuntime, PreparedPackageSet, locked_melpa_sources};

const RUST_LSP_TYPING_OVERLAY_COUNT: u64 = 4;
const RUST_LSP_TYPING_DIAGNOSTIC_COUNT: u64 = 4;
const RUST_GRAMMAR_REPOSITORY: &str = "https://github.com/tree-sitter/tree-sitter-rust";
const RUST_GRAMMAR_REVISION: &str = "18b0515fca567f5a10aee9978c6d2640e878671a";

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let lsp_mode_source = locked_melpa_sources()?
        .into_iter()
        .find(|source| source.package().0 == "lsp-mode")
        .ok_or_else(|| "the MELPA source lock does not contain lsp-mode".to_string())?;
    let lsp_mode = lsp_mode_source.package();
    let packages =
        PreparedPackageSet::from_locked_melpa(&EmacsRuntime::gnu_emacs(), lsp_mode, "lsp-mode.el")?;
    let cached_grammar = prepare_cached_tree_sitter_grammar(
        &EmacsRuntime::gnu_emacs(),
        "rust",
        RUST_GRAMMAR_REPOSITORY,
        RUST_GRAMMAR_REVISION,
    )?;
    let grammar_directory = run_directory.join("tree-sitter");
    let grammar_libraries =
        copy_grammar_libraries(&cached_grammar, &grammar_directory, "tree-sitter-rust")?;
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let startup = packages.write_startup_file(run_directory)?;
    let fixture_root = workspace_root.join("crates/neomacs-perf/fixtures");
    let fixture_source = fixture_root.join("rust-lsp-typing.el");
    let source_source = fixture_root.join("rust-lsp-typing.rs");
    let replay_source = fixture_root.join("rust-lsp-diagnostics.json");
    for required in [&fixture_source, &source_source, &replay_source] {
        if !required.is_file() {
            return Err(format!(
                "missing committed performance fixture {}",
                required.display()
            ));
        }
    }
    let fixture = run_directory.join("rust-lsp-typing.el");
    let source = run_directory.join("rust-lsp-typing.rs");
    let replay = run_directory.join("rust-lsp-diagnostics.json");
    for (input, output) in [
        (&fixture_source, &fixture),
        (&source_source, &source),
        (&replay_source, &replay),
    ] {
        fs::copy(input, output).map_err(|error| {
            format!(
                "failed to copy performance fixture {} to {}: {error}",
                input.display(),
                output.display()
            )
        })?;
    }
    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = RustLspInputProvenanceManifest {
        lsp_mode: PackageProvenance {
            name: lsp_mode.0,
            version: lsp_mode.1,
            repository: lsp_mode_source.repository(),
            revision: lsp_mode_source.revision(),
            upstream_repository: lsp_mode_source.upstream_repository(),
            upstream_revision: lsp_mode_source.upstream_revision(),
        },
        tree_sitter_grammar: GrammarProvenance {
            language: "rust",
            repository: RUST_GRAMMAR_REPOSITORY,
            revision: RUST_GRAMMAR_REVISION,
        },
        editor,
        host: collect_host_provenance(request.machine_policy()),
        workload_source: "crates/neomacs-perf/fixtures/rust-lsp-typing.rs",
        workload_source_sha256: sha256_file(&source_source)?,
        environment_policy: "closed-v1",
        passthrough_environment: benchmark_passthrough_environment()
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
    let gui_runtime_directory = prepare_gui_runtime_directory(workspace_root)?;
    Ok(PreparedScenario {
        fixture,
        provenance,
        result: run_directory.join("scenario-result.json"),
        sentinel: run_directory.join("completed"),
        terminal_bytes: run_directory.join("terminal.ansi"),
        gui_app_log: run_directory.join("gui-app.log"),
        gui_weston_log: run_directory.join("weston.log"),
        gui_runtime_directory,
        sandbox,
        workload: PreparedWorkload::RustLspTyping {
            startup,
            source,
            replay,
            grammar_directory,
            grammar_libraries,
            packages: Box::new(packages),
        },
    })
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "RustLspTypingResultWire")]
pub(crate) struct RustLspTypingResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    major_mode: String,
    lsp_mode_loaded: bool,
    treesit_parser_language: String,
    text_unchanged: bool,
    point_unchanged: bool,
    overlay_count: u64,
    lsp_diagnostic_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RustLspTypingResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    major_mode: String,
    lsp_mode_loaded: bool,
    treesit_parser_language: String,
    text_unchanged: bool,
    point_unchanged: bool,
    overlay_count: u64,
    lsp_diagnostic_count: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<RustLspTypingResultWire> for RustLspTypingResult {
    type Error = String;

    fn try_from(wire: RustLspTypingResultWire) -> Result<Self, Self::Error> {
        let outcome = scenario_outcome(wire.status, wire.error)?;
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            major_mode: wire.major_mode,
            lsp_mode_loaded: wire.lsp_mode_loaded,
            treesit_parser_language: wire.treesit_parser_language,
            text_unchanged: wire.text_unchanged,
            point_unchanged: wire.point_unchanged,
            overlay_count: wire.overlay_count,
            lsp_diagnostic_count: wire.lsp_diagnostic_count,
        })
    }
}

#[derive(Serialize)]
struct RustLspInputProvenanceManifest<'a> {
    lsp_mode: PackageProvenance<'a>,
    tree_sitter_grammar: GrammarProvenance<'a>,
    editor: EditorProvenance,
    host: HostProvenance,
    workload_source: &'a str,
    workload_source_sha256: String,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

pub(crate) fn validate_rust_lsp_typing_result(
    request: &RunRequest,
    result: &RustLspTypingResult,
) -> Vec<CorrectnessMismatch> {
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
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
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "major-mode",
        "rust-ts-mode",
        result.major_mode.as_str(),
    );
    mismatch(
        &mut mismatches,
        "lsp-mode-loaded",
        true,
        result.lsp_mode_loaded,
    );
    mismatch(
        &mut mismatches,
        "treesit-parser-language",
        "rust",
        result.treesit_parser_language.as_str(),
    );
    mismatch(
        &mut mismatches,
        "final-buffer-text",
        true,
        result.text_unchanged,
    );
    mismatch(&mut mismatches, "final-point", true, result.point_unchanged);
    mismatch(
        &mut mismatches,
        "overlay-count",
        RUST_LSP_TYPING_OVERLAY_COUNT,
        result.overlay_count,
    );
    mismatch(
        &mut mismatches,
        "lsp-diagnostic-count",
        RUST_LSP_TYPING_DIAGNOSTIC_COUNT,
        result.lsp_diagnostic_count,
    );
    mismatches
}

pub(crate) fn valid_rust_lsp_typing_measurements(
    result: &RustLspTypingResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    let edits = u64::from(result.iterations) * 2;
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerEditCpuTime,
            value: result.elapsed_us as f64 / edits.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerEdit,
        },
        Measurement {
            name: MetricName::Edits,
            value: edits as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Redisplays,
            value: edits as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::Iterations,
            value: f64::from(result.iterations),
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::OverlayCount,
            value: result.overlay_count as f64,
            unit: MetricUnit::Count,
        },
        Measurement {
            name: MetricName::LspDiagnosticCount,
            value: result.lsp_diagnostic_count as f64,
            unit: MetricUnit::Count,
        },
    ]
}

fn copy_grammar_libraries(
    cached_directory: &Path,
    run_directory: &Path,
    library_stem: &str,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(run_directory).map_err(|error| {
        format!(
            "failed to create run-local Tree-sitter directory {}: {error}",
            run_directory.display()
        )
    })?;
    let entries = fs::read_dir(cached_directory).map_err(|error| {
        format!(
            "failed to enumerate cached Tree-sitter directory {}: {error}",
            cached_directory.display()
        )
    })?;
    let mut copied = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read cached Tree-sitter entry below {}: {error}",
                cached_directory.display()
            )
        })?;
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file())
            || !entry.file_name().to_string_lossy().contains(library_stem)
        {
            continue;
        }
        let destination = run_directory.join(entry.file_name());
        fs::copy(entry.path(), &destination).map_err(|error| {
            format!(
                "failed to copy Tree-sitter grammar {} to {}: {error}",
                entry.path().display(),
                destination.display()
            )
        })?;
        copied.push(destination);
    }
    if copied.is_empty() {
        return Err(format!(
            "cached Tree-sitter directory {} contains no `{library_stem}` library",
            cached_directory.display()
        ));
    }
    copied.sort();
    Ok(copied)
}
