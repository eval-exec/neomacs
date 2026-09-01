use std::time::Duration;

use crate::{CachedMelpaOracle, MAGIT_SECTION_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod hierarchy;
mod matching;
mod visibility;

const MAGIT_SECTION_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn magit_section_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAGIT_SECTION_MELPA_PIN, "magit-section.el")
        .expect("prepare pinned magit-section source and dependencies below ./tmp")
        .with_timeout(MAGIT_SECTION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed magit-section parity test")
        .into()
}

/// Multi-probe batch for `assert_magit_section_parity` cases (2a).
pub(crate) fn assert_magit_section_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(magit_section_oracle(), &name, "magit_section_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn magit_section_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        hierarchy::hierarchy_public_surface_batch_cases(),
        matching::matching_public_surface_batch_cases(),
        visibility::visibility_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_magit_section_batch(&cases);
}

// END generated package batch tests
