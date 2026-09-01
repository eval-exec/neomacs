use std::time::Duration;

use crate::{CLOJURE_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const CLOJURE_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn clojure_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CLOJURE_MODE_MELPA_PIN, "clojure-mode.el")
        .expect("prepare pinned Clojure Mode source below ./tmp")
        .with_timeout(CLOJURE_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Clojure Mode parity test")
        .into()
}

pub(crate) fn assert_clojure_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(clojure_mode_oracle(), &name, "clojure_mode_parity", cases);
}

#[test]
fn clojure_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_clojure_mode_batch(&cases);
}
