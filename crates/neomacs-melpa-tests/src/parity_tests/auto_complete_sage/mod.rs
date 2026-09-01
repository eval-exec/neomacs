use std::time::Duration;

use crate::{
    AUTO_COMPLETE_MELPA_PIN, AUTO_COMPLETE_SAGE_MELPA_PIN, CachedMelpaOracle, DEFERRED_MELPA_PIN,
    LET_ALIST_GNU_ELPA_PIN, POPUP_MELPA_PIN, SAGE_SHELL_MODE_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod cache_docs;
mod edit;
mod registry;
mod repl;
mod workflows;

const AUTO_COMPLETE_SAGE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AUTO_COMPLETE_SAGE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun acsage-test-error (thunk)
  (condition-case error-data
      (list :value
            (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun acsage-test-posn-at-point (&rest _arguments)
  'acsage-test-position)

(defun acsage-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos
    (point))))

(fset 'posn-at-point
      #'acsage-test-posn-at-point)
(fset 'posn-col-row
      #'acsage-test-posn-col-row)
"##;

fn auto_complete_sage_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_SAGE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-sage source below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(DEFERRED_MELPA_PIN)
        .expect("prepare pinned deferred dependency below ./tmp")
        .with_gnu_elpa_dependency(LET_ALIST_GNU_ELPA_PIN)
        .expect("prepare pinned let-alist dependency below ./tmp")
        .with_melpa_dependency(SAGE_SHELL_MODE_MELPA_PIN)
        .expect("prepare pinned sage-shell-mode dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_SAGE_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_SAGE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-sage parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_sage_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_sage_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_sage_oracle("auto-complete-sage-autoloads.el"),
        &name,
        "auto_complete_sage_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_sage_parity` cases (2a).
pub(crate) fn assert_auto_complete_sage_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_sage_oracle("auto-complete-sage.el"),
        &name,
        "auto_complete_sage_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_sage_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_sage_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_sage_autoload_batch(&cases);
}

#[test]
fn auto_complete_sage_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        cache_docs::cache_docs_public_surface_batch_cases(),
        edit::edit_public_surface_batch_cases(),
        registry::registry_auto_complete_sage_batch_cases(),
        repl::repl_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_sage_batch(&cases);
}

// END generated package batch tests
