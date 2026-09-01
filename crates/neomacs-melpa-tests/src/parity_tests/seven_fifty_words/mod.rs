use std::time::Duration;

use crate::{CachedMelpaOracle, SEVEN_FIFTY_WORDS_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod authentication;
mod posting;

const SEVEN_FIFTY_WORDS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn seven_fifty_words_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SEVEN_FIFTY_WORDS_MELPA_PIN, "750words.el")
        .expect("prepare pinned 750words source below ./tmp")
        .with_timeout(SEVEN_FIFTY_WORDS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed 750words parity test")
        .into()
}

/// Multi-probe batch for `assert_seven_fifty_words_parity` cases (2a).
pub(crate) fn assert_seven_fifty_words_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        seven_fifty_words_oracle(),
        &name,
        "seven_fifty_words_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn seven_fifty_words_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        authentication::authentication_public_surface_batch_cases(),
        posting::posting_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_seven_fifty_words_batch(&cases);
}

// END generated package batch tests
