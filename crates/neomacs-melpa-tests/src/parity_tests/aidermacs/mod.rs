use std::time::Duration;

use crate::{AIDERMACS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backends;
mod commands;
mod files;
mod models;
mod session;
mod surface;
mod workflows;

const AIDERMACS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn aidermacs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIDERMACS_MELPA_PIN, "aidermacs.el")
        .expect("prepare pinned aidermacs source below ./tmp")
        .with_timeout(AIDERMACS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aidermacs parity test")
        .into()
}

/// Multi-probe batch for `assert_aidermacs_parity` cases (2a).
pub(crate) fn assert_aidermacs_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aidermacs_oracle(), &name, "aidermacs_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aidermacs_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_public_surface_batch_cases(),
        commands::commands_public_surface_batch_cases(),
        files::files_public_surface_batch_cases(),
        models::models_public_surface_batch_cases(),
        session::session_public_surface_batch_cases(),
        surface::surface_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_aidermacs_batch(&cases);
}

// END generated package batch tests
