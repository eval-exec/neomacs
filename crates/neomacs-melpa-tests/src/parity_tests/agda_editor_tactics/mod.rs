use std::time::Duration;

use crate::{AGDA_EDITOR_TACTICS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod indentation;
mod mode;
mod parsing;
mod registry;
mod rendering;
mod workflows;

const AGDA_EDITOR_TACTICS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn agda_editor_tactics_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGDA_EDITOR_TACTICS_MELPA_PIN, source_file)
        .expect("prepare pinned agda-editor-tactics source below ./tmp")
        .with_timeout(AGDA_EDITOR_TACTICS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed agda-editor-tactics parity test")
        .into()
}

/// Multi-probe batch for `assert_agda_editor_tactics_autoload_parity` cases (2a).
pub(crate) fn assert_agda_editor_tactics_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agda_editor_tactics_oracle("agda-editor-tactics-autoloads.el"),
        &name,
        "agda_editor_tactics_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_agda_editor_tactics_parity` cases (2a).
pub(crate) fn assert_agda_editor_tactics_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agda_editor_tactics_oracle("agda-editor-tactics.el"),
        &name,
        "agda_editor_tactics_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn agda_editor_tactics_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_agda_editor_tactics_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_agda_editor_tactics_autoload_batch(&cases);
}

#[test]
fn agda_editor_tactics_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        indentation::indentation_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        registry::registry_agda_editor_tactics_batch_cases(),
        rendering::rendering_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_agda_editor_tactics_batch(&cases);
}

// END generated package batch tests
