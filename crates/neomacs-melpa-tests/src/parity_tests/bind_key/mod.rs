use std::time::Duration;

use crate::{BIND_KEY_GNU_ELPA_PIN, CachedPackageOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod bindings;
mod groups;
mod reporting;

const BIND_KEY_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn bind_key_oracle() -> CachedPackageOracle {
    CachedPackageOracle::new_from_gnu_elpa(BIND_KEY_GNU_ELPA_PIN, "bind-key.el")
        .expect("prepare pinned Bind-Key source and dependencies below ./tmp")
        .with_timeout(BIND_KEY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Bind-Key parity test")
        .into()
}

/// Multi-probe batch for `assert_bind_key_parity` cases (2a).
pub(crate) fn assert_bind_key_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(bind_key_oracle(), &name, "bind_key_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn bind_key_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        bindings::bindings_public_surface_batch_cases(),
        groups::groups_public_surface_batch_cases(),
        reporting::reporting_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_bind_key_batch(&cases);
}

// END generated package batch tests
