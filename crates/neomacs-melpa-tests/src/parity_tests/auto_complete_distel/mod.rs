use std::time::Duration;

use crate::{
    AUTO_COMPLETE_DISTEL_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle,
    DISTEL_COMPLETION_LIB_MELPA_PIN, POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod candidates;
mod documentation;
mod prefixes;
mod registry;
mod workflows;

const AUTO_COMPLETE_DISTEL_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_DISTEL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; Distel is an intentionally undeclared external prerequisite of the pinned
;; companion library. The package only needs its feature while loading; each
;; behavioral case supplies the exact asynchronous protocol seam it exercises.
(provide 'distel)
(defvar erl-nodename-cache 'neomacs-test@localhost)

(defun auto-complete-distel-test-error (thunk)
  (condition-case error-data
      (list :value
            (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

;; popup.el normally obtains these coordinates from the display engine.
(defun auto-complete-distel-test-posn-at-point (&rest _arguments)
  'auto-complete-distel-test-position)

(defun auto-complete-distel-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos
    (point))))

(fset
 'posn-at-point
 #'auto-complete-distel-test-posn-at-point)
(fset
 'posn-col-row
 #'auto-complete-distel-test-posn-col-row)
"##;

fn auto_complete_distel_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_DISTEL_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-distel source below ./tmp")
        .with_melpa_dependency(DISTEL_COMPLETION_LIB_MELPA_PIN)
        .expect("prepare pinned distel-completion-lib dependency below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup transitive dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_DISTEL_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_DISTEL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-distel parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_distel_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_distel_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_distel_oracle("auto-complete-distel-autoloads.el"),
        &name,
        "auto_complete_distel_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_distel_parity` cases (2a).
pub(crate) fn assert_auto_complete_distel_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_distel_oracle("auto-complete-distel.el"),
        &name,
        "auto_complete_distel_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_distel_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_distel_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_distel_autoload_batch(&cases);
}

#[test]
fn auto_complete_distel_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        candidates::candidates_public_surface_batch_cases(),
        documentation::documentation_public_surface_batch_cases(),
        prefixes::prefixes_public_surface_batch_cases(),
        registry::registry_auto_complete_distel_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_distel_batch(&cases);
}

// END generated package batch tests
