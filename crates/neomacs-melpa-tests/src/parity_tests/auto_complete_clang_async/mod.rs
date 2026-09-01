use std::time::Duration;

use crate::{
    AUTO_COMPLETE_CLANG_ASYNC_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle,
    POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod arguments;
mod async_process;
mod parsing;
mod protocol;
mod registry;
mod templates;
mod workflows;

const AUTO_COMPLETE_CLANG_ASYNC_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AUTO_COMPLETE_CLANG_ASYNC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun acclang-test-error (thunk)
  (condition-case error-data
      (list :value
            (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun acclang-test-candidate-summary (candidate)
  (list
   (substring-no-properties candidate)
   (get-text-property
    0
    'ac-clang-help
    candidate)
   (get-text-property
    0
    'raw-args
    candidate)))

(defun acclang-test-start-cat (name)
  (let* ((buffer
          (generate-new-buffer
           (format " *%s-buffer*" name)))
         (process-connection-type
          nil)
         (process
          (start-process
           name
           buffer
           (or
            (executable-find "cat")
            (error "cat executable is unavailable")))))
    (set-process-query-on-exit-flag
     process
     nil)
    (cons process buffer)))

(defun acclang-test-finish-process (process buffer)
  (when
      (process-live-p process)
    (delete-process process))
  (when
      (buffer-live-p buffer)
    (kill-buffer buffer)))

(defun acclang-test-process-buffer-string (process)
  (with-current-buffer
      (process-buffer process)
    (buffer-substring-no-properties
     (point-min)
     (point-max))))
"##;

fn auto_complete_clang_async_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_CLANG_ASYNC_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-clang-async source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_CLANG_ASYNC_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_CLANG_ASYNC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-clang-async parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_clang_async_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_clang_async_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_clang_async_oracle("auto-complete-clang-async-autoloads.el"),
        &name,
        "auto_complete_clang_async_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_clang_async_parity` cases (2a).
pub(crate) fn assert_auto_complete_clang_async_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_clang_async_oracle("auto-complete-clang-async.el"),
        &name,
        "auto_complete_clang_async_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_clang_async_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_clang_async_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_clang_async_autoload_batch(&cases);
}

#[test]
fn auto_complete_clang_async_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        arguments::arguments_public_surface_batch_cases(),
        async_process::async_process_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        protocol::protocol_public_surface_batch_cases(),
        registry::registry_auto_complete_clang_async_batch_cases(),
        templates::templates_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_clang_async_batch(&cases);
}

// END generated package batch tests
