use std::time::Duration;

use crate::{AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle, POPUP_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod completion;
mod config;
mod dictionaries;
mod lifecycle;
mod matching;
mod registry;

const AUTO_COMPLETE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar auto-complete-test-actions nil)
(defvar auto-complete-test-available-calls 0)
(defvar auto-complete-test-candidate-calls 0)
(defvar auto-complete-test-hook-calls 0)
(defvar auto-complete-test-init-calls 0)
(defvar auto-complete-test-shell-command nil)

;; popup.el normally obtains these coordinates from the display engine.
;; These deterministic batch coordinates retain the real popup/overlay
;; lifecycle while making it independent of a workstation's frame geometry.
(defun auto-complete-test-posn-at-point (&rest _arguments)
  'auto-complete-test-position)

(defun auto-complete-test-posn-col-row (_position)
  (cons (current-column)
        (line-number-at-pos (point))))

(fset 'posn-at-point #'auto-complete-test-posn-at-point)
(fset 'posn-col-row #'auto-complete-test-posn-col-row)

(defun auto-complete-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))
"##;

fn auto_complete_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete source below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_oracle("auto-complete-autoloads.el"),
        &name,
        "auto_complete_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_parity` cases (2a).
pub(crate) fn assert_auto_complete_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_oracle("auto-complete.el"),
        &name,
        "auto_complete_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_config_parity` cases (2a).
pub(crate) fn assert_auto_complete_config_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_oracle("auto-complete-config.el"),
        &name,
        "auto_complete_config_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_complete_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_complete_autoload_batch(&cases);
}

#[test]
fn auto_complete_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        completion::completion_public_surface_batch_cases(),
        dictionaries::dictionaries_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        matching::matching_public_surface_batch_cases(),
        registry::registry_auto_complete_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_batch(&cases);
}

#[test]
fn auto_complete_config_package_batch() {
    let cases: Vec<ParityBatchCase> = [config::config_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_complete_config_batch(&cases);
}

// END generated package batch tests
