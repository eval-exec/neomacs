use std::time::Duration;

use crate::{CachedMelpaOracle, DIRED_QUICK_SORT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DIRED_QUICK_SORT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DIRED_QUICK_SORT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'dired)
(require 'dired-quick-sort)

(defun neomacs-dired-quick-sort-test-reset ()
  "Restore default last-used sorting state."
  (setq dired-quick-sort-sort-by-last "version"
        dired-quick-sort-reverse-last ?n
        dired-quick-sort-group-directories-last ?n
        dired-quick-sort-time-last "default"
        dired-listing-switches "-al"))
"####;

fn dired_quick_sort_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DIRED_QUICK_SORT_MELPA_PIN, "dired-quick-sort.el")
        .expect("prepare exact shallow dired-quick-sort source below ./tmp")
        .with_prelude(DIRED_QUICK_SORT_TEST_PRELUDE)
        .with_timeout(DIRED_QUICK_SORT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed dired-quick-sort parity test")
        .into()
}

fn assert_dired_quick_sort_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        dired_quick_sort_oracle(),
        &current_test_name(),
        "dired_quick_sort_parity",
        cases,
    );
}

#[test]
fn dired_quick_sort_package_batch() {
    assert_dired_quick_sort_batch(&workflows::workflow_batch_cases());
}
