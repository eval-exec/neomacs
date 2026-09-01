use std::time::Duration;

use crate::{AUTO_COMPILE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod compilation;
mod modes;
mod registry;
mod source_files;
mod workflows;

const AUTO_COMPILE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AUTO_COMPILE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defun auto-compile-test-root ()
  (file-name-as-directory
   (expand-file-name
    "auto-compile-fixture"
    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun auto-compile-test-path (name)
  (expand-file-name name (auto-compile-test-root)))

(defun auto-compile-test-write (name contents)
  (let ((file (auto-compile-test-path name)))
    (make-directory (file-name-directory file) t)
    (with-temp-file file
      (insert contents))
    file))

(defun auto-compile-test-read (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun auto-compile-test-error (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun auto-compile-test-set-time (file seconds)
  (set-file-times file (seconds-to-time seconds))
  file)

(defun auto-compile-test-dest (source)
  (byte-compile-dest-file source))

(defun auto-compile-test-mode-line-text (value)
  (substring-no-properties (format-mode-line value)))
"##;

fn auto_compile_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPILE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-compile source below ./tmp")
        .with_prelude(AUTO_COMPILE_TEST_PRELUDE)
        .with_timeout(AUTO_COMPILE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-compile parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_compile_autoload_parity` cases (2a).
pub(crate) fn assert_auto_compile_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_compile_oracle("auto-compile-autoloads.el"),
        &name,
        "auto_compile_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_compile_parity` cases (2a).
pub(crate) fn assert_auto_compile_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_compile_oracle("auto-compile.el"),
        &name,
        "auto_compile_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_compile_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_compile_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_compile_autoload_batch(&cases);
}

#[test]
fn auto_compile_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        compilation::compilation_public_surface_batch_cases(),
        modes::modes_public_surface_batch_cases(),
        registry::registry_auto_compile_batch_cases(),
        source_files::source_files_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_compile_batch(&cases);
}

// END generated package batch tests
