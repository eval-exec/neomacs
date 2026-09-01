use std::time::Duration;

use crate::{
    AUTO_COMPLETE_MELPA_PIN, AUTO_COMPLETE_PCMP_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN,
    LOG4E_MELPA_PIN, POPUP_MELPA_PIN, YAXCEPTION_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod actions;
mod advice;
mod candidates;
mod registry;
mod workflows;

const AUTO_COMPLETE_PCMP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_PCMP_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)

(defun ac-pcmp-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data) (cdr error-data)))))

(defun ac-pcmp-test-state ()
  (list
   :active ac-pcmp--active-p
   :candidates ac-pcmp--candidates
   :status ac-pcmp--status
   :point ac-pcmp--point
   :last-length
   (and (boundp 'pcomplete-last-completion-length)
        pcomplete-last-completion-length)
   :last-stub
   (and (boundp 'pcomplete-last-completion-stub)
        pcomplete-last-completion-stub)))
"##;

fn auto_complete_pcmp_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_PCMP_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-pcmp source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_melpa_dependency(LOG4E_MELPA_PIN)
        .expect("prepare pinned log4e dependency below ./tmp")
        .with_melpa_dependency(YAXCEPTION_MELPA_PIN)
        .expect("prepare pinned yaxception dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_PCMP_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_PCMP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-pcmp parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_pcmp_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_pcmp_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_pcmp_oracle("auto-complete-pcmp-autoloads.el"),
        &name,
        "auto_complete_pcmp_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_pcmp_parity` cases (2a).
pub(crate) fn assert_auto_complete_pcmp_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_pcmp_oracle("auto-complete-pcmp.el"),
        &name,
        "auto_complete_pcmp_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_pcmp_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_pcmp_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_pcmp_autoload_batch(&cases);
}

#[test]
fn auto_complete_pcmp_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        actions::actions_public_surface_batch_cases(),
        advice::advice_public_surface_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        registry::registry_auto_complete_pcmp_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_pcmp_batch(&cases);
}

// END generated package batch tests
