use std::time::Duration;

use crate::{AUTO_INDENT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod hooks;
mod kill;
mod lifecycle;
mod registry;
mod repository;
mod workflows;

const AUTO_INDENT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_INDENT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)

(defun auto-indent-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data) (cdr error-data)))))

(defun auto-indent-test-relative-or-value (value root)
  (if (stringp value)
      (file-relative-name value root)
    value))

(defun auto-indent-test-advice-state (function)
  (list
   function
   (not
    (null
     (ad-find-advice
      function 'around 'auto-indent-mode-advice)))
   (ad-is-active function)))
"##;

fn auto_indent_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_INDENT_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-indent-mode source below ./tmp")
        .with_prelude(AUTO_INDENT_MODE_TEST_PRELUDE)
        .with_timeout(AUTO_INDENT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-indent-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_indent_mode_autoload_parity` cases (2a).
pub(crate) fn assert_auto_indent_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_indent_mode_oracle("auto-indent-mode-autoloads.el"),
        &name,
        "auto_indent_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_indent_mode_parity` cases (2a).
pub(crate) fn assert_auto_indent_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_indent_mode_oracle("auto-indent-mode.el"),
        &name,
        "auto_indent_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_indent_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_indent_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_indent_mode_autoload_batch(&cases);
}

#[test]
fn auto_indent_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        hooks::hooks_public_surface_batch_cases(),
        kill::kill_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        registry::registry_auto_indent_mode_batch_cases(),
        repository::repository_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_indent_mode_batch(&cases);
}

// END generated package batch tests
