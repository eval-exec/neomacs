use std::time::Duration;

use crate::{ANNOTATION_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANNOTATION_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn annotation_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANNOTATION_MELPA_PIN, source_file)
        .expect("prepare pinned annotation source below ./tmp")
        .with_timeout(ANNOTATION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed annotation parity test")
        .into()
}

/// Multi-probe batch for `assert_annotation_parity` cases (2a).
pub(crate) fn assert_annotation_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        annotation_oracle("annotation.el"),
        &name,
        "annotation_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn annotation_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_annotation_batch(&cases);
}

// END generated package batch tests
