use std::time::Duration;

use crate::{
    AUTO_COMPLETE_CLANG_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle, POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod actions;
mod arguments;
mod candidates;
mod parsing;
mod registry;
mod templates;
mod workflows;

const AUTO_COMPLETE_CLANG_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_CLANG_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun ac-clang-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun ac-clang-test-candidate-state (candidate)
  (list
   (substring-no-properties candidate)
   (get-text-property
    0 'ac-clang-help candidate)
   (get-text-property
    0 'raw-args candidate)))

(defun ac-clang-test-reset-file (file content)
  (make-directory
   (file-name-directory file)
   t)
  (with-temp-file file
    (insert content))
  file)
"##;

fn auto_complete_clang_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_CLANG_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-clang source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_CLANG_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_CLANG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-clang parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_clang_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_clang_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_clang_oracle("auto-complete-clang-autoloads.el"),
        &name,
        "auto_complete_clang_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_clang_parity` cases (2a).
pub(crate) fn assert_auto_complete_clang_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_clang_oracle("auto-complete-clang.el"),
        &name,
        "auto_complete_clang_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_clang_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_clang_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_clang_autoload_batch(&cases);
}

#[test]
fn auto_complete_clang_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        actions::actions_public_surface_batch_cases(),
        arguments::arguments_public_surface_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        registry::registry_auto_complete_clang_batch_cases(),
        templates::templates_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_clang_batch(&cases);
}

// END generated package batch tests
