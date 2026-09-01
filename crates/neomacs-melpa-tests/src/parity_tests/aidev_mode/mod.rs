use std::time::Duration;

use crate::{AIDEV_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AIDEV_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn aidev_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIDEV_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned aidev-mode source below ./tmp")
        .with_prelude(
            r##"(setenv
                 "AIDEV_OLLAMA_ADDRESS"
                 "http://frozen-ollama.invalid:11434")"##,
        )
        .with_timeout(AIDEV_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aidev-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_aidev_mode_parity` cases (2a).
pub(crate) fn assert_aidev_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aidev_mode_oracle("aidev-mode.el"),
        &name,
        "aidev_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn aidev_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aidev_mode_batch(&cases);
}

// END generated package batch tests
