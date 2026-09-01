use std::time::Duration;

use crate::{ANKI_VOCABULARY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANKI_VOCABULARY_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn anki_vocabulary_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANKI_VOCABULARY_MELPA_PIN, source_file)
        .expect("prepare pinned anki-vocabulary source below ./tmp")
        .with_timeout(ANKI_VOCABULARY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anki-vocabulary parity test")
        .into()
}

/// Multi-probe batch for `assert_anki_vocabulary_parity` cases (2a).
pub(crate) fn assert_anki_vocabulary_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anki_vocabulary_oracle("anki-vocabulary.el"),
        &name,
        "anki_vocabulary_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn anki_vocabulary_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anki_vocabulary_batch(&cases);
}

// END generated package batch tests
