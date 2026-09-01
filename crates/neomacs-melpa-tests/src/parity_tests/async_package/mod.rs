use std::time::Duration;

use crate::{ASYNC_GNU_ELPA_PIN, CachedPackageOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod futures;
mod processes;
mod serialization;

const ASYNC_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn async_oracle() -> CachedPackageOracle {
    CachedPackageOracle::new_from_gnu_elpa(ASYNC_GNU_ELPA_PIN, "async.el")
        .expect("prepare pinned Async source and dependencies below ./tmp")
        .with_timeout(ASYNC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Async parity test").into()
}

/// Multi-probe batch for `assert_async_parity` cases (2a).
pub(crate) fn assert_async_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(async_oracle(), &name, "async_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn async_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        futures::futures_public_surface_batch_cases(),
        processes::processes_public_surface_batch_cases(),
        serialization::serialization_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async_batch(&cases);
}

// END generated package batch tests
