use std::time::Duration;

use crate::{ANNOTATE_DEPTH_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANNOTATE_DEPTH_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn annotate_depth_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANNOTATE_DEPTH_MELPA_PIN, source_file)
        .expect("prepare pinned annotate-depth source below ./tmp")
        .with_timeout(ANNOTATE_DEPTH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed annotate-depth parity test")
        .into()
}

/// Multi-probe batch for `assert_annotate_depth_parity` cases (2a).
pub(crate) fn assert_annotate_depth_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        annotate_depth_oracle("annotate-depth.el"),
        &name,
        "annotate_depth_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn annotate_depth_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_annotate_depth_batch(&cases);
}

// END generated package batch tests
