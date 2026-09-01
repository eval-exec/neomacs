use std::time::Duration;

use crate::{CachedMelpaOracle, PROJECTILE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod async_process;
mod core;
mod filesystem;
mod relations;
mod state;
mod tasks;

const PROJECTILE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn projectile_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PROJECTILE_MELPA_PIN, "projectile.el")
        .expect("prepare pinned Projectile source and dependencies below ./tmp")
        .with_timeout(PROJECTILE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Projectile parity test")
        .into()
}

/// Multi-probe batch for `assert_projectile_parity` cases (2a).
pub(crate) fn assert_projectile_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(projectile_oracle(), &name, "projectile_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn projectile_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        async_process::async_process_public_surface_batch_cases(),
        core::core_public_surface_batch_cases(),
        filesystem::filesystem_public_surface_batch_cases(),
        relations::relations_public_surface_batch_cases(),
        state::state_public_surface_batch_cases(),
        tasks::tasks_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_projectile_batch(&cases);
}

// END generated package batch tests
