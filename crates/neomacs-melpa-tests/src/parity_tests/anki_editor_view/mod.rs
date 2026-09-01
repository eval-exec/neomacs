use std::time::Duration;

use crate::{ANKI_EDITOR_VIEW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANKI_EDITOR_VIEW_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn anki_editor_view_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANKI_EDITOR_VIEW_MELPA_PIN, source_file)
        .expect("prepare pinned anki-editor-view source below ./tmp")
        .with_timeout(ANKI_EDITOR_VIEW_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anki-editor-view parity test")
        .into()
}

/// Multi-probe batch for `assert_anki_editor_view_parity` cases (2a).
pub(crate) fn assert_anki_editor_view_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anki_editor_view_oracle("anki-editor-view.el"),
        &name,
        "anki_editor_view_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn anki_editor_view_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anki_editor_view_batch(&cases);
}

// END generated package batch tests
