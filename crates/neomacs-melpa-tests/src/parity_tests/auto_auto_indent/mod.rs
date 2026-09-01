use std::time::Duration;

use crate::{AUTO_AUTO_INDENT_MELPA_PIN, CachedMelpaOracle, ES_LIB_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod indentation;
mod lifecycle;
mod post_command;
mod registry;
mod timers;
mod workflows;

const AUTO_AUTO_INDENT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_AUTO_INDENT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'cl)
(require 'seq)

(defun auto-auto-indent-test-error-data (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun auto-auto-indent-test-buffer-state ()
  (list
   (buffer-string)
   (point)
   (line-number-at-pos)
   (current-column)
   (when (mark t)
     (marker-position
      (mark-marker)))
   (region-active-p)
   (buffer-modified-p)))

(defun auto-auto-indent-test-hook-count
    (function hook)
  (length
   (seq-filter
    (lambda (candidate)
      (eq candidate function))
    (symbol-value hook))))
"##;

fn auto_auto_indent_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_AUTO_INDENT_MELPA_PIN, source_file)
        .expect("prepare pinned auto-auto-indent source below ./tmp")
        .with_melpa_dependency(ES_LIB_MELPA_PIN)
        .expect("prepare pinned es-lib dependency below ./tmp")
        .with_prelude(AUTO_AUTO_INDENT_TEST_PRELUDE)
        .with_timeout(AUTO_AUTO_INDENT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-auto-indent parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_auto_indent_autoload_parity` cases (2a).
pub(crate) fn assert_auto_auto_indent_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_auto_indent_oracle("auto-auto-indent-autoloads.el"),
        &name,
        "auto_auto_indent_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_auto_indent_parity` cases (2a).
pub(crate) fn assert_auto_auto_indent_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_auto_indent_oracle("auto-auto-indent.el"),
        &name,
        "auto_auto_indent_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_auto_indent_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_auto_indent_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_auto_indent_autoload_batch(&cases);
}

#[test]
fn auto_auto_indent_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        indentation::indentation_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        post_command::post_command_public_surface_batch_cases(),
        registry::registry_auto_auto_indent_batch_cases(),
        timers::timers_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_auto_indent_batch(&cases);
}

// END generated package batch tests
