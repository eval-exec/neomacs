use std::time::Duration;

use crate::{AIKEN_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod font_lock;
mod mode;
mod workflows;

const AIKEN_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn aiken_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIKEN_MODE_MELPA_PIN, "aiken-mode.el")
        .expect("prepare pinned aiken-mode source below ./tmp")
        .with_prelude(
            r##"
(require 'cl-lib)
(require 'compile)
(require 'project)
(require 'thingatpt)
"##,
        )
        .with_timeout(AIKEN_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aiken-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_aiken_mode_parity` cases (2a).
pub(crate) fn assert_aiken_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aiken_mode_oracle(), &name, "aiken_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aiken_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        font_lock::font_lock_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_aiken_mode_batch(&cases);
}

// END generated package batch tests
