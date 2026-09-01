use std::time::Duration;

use crate::{AUTO_MINOR_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod advice;
mod filenames;
mod magic;
mod registry;
mod use_package;
mod workflows;

const AUTO_MINOR_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_MINOR_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar auto-minor-mode-test-events nil)
(defvar auto-minor-mode-test-alpha-mode nil)
(defvar auto-minor-mode-test-beta-mode nil)
(defvar auto-minor-mode-test-gamma-mode nil)
(defvar auto-minor-mode-test-unregistered-mode nil)

(defun auto-minor-mode-test-alpha-mode (&optional argument)
  (make-local-variable 'auto-minor-mode-test-alpha-mode)
  (setq auto-minor-mode-test-alpha-mode
        (> (prefix-numeric-value
            (or argument 1))
           0))
  (push
   (list
    :alpha
    argument
    auto-minor-mode-test-alpha-mode
    (point)
    major-mode)
   auto-minor-mode-test-events)
  auto-minor-mode-test-alpha-mode)

(defun auto-minor-mode-test-beta-mode (&optional argument)
  (make-local-variable 'auto-minor-mode-test-beta-mode)
  (setq auto-minor-mode-test-beta-mode
        (> (prefix-numeric-value
            (or argument 1))
           0))
  (push
   (list
    :beta
    argument
    auto-minor-mode-test-beta-mode
    (point)
    major-mode)
   auto-minor-mode-test-events)
  auto-minor-mode-test-beta-mode)

(defun auto-minor-mode-test-gamma-mode (&optional argument)
  (make-local-variable 'auto-minor-mode-test-gamma-mode)
  (setq auto-minor-mode-test-gamma-mode
        (> (prefix-numeric-value
            (or argument 1))
           0))
  (push
   (list
    :gamma
    argument
    auto-minor-mode-test-gamma-mode
    (point)
    major-mode)
   auto-minor-mode-test-events)
  auto-minor-mode-test-gamma-mode)

(defun auto-minor-mode-test-unregistered-mode (&optional argument)
  (make-local-variable 'auto-minor-mode-test-unregistered-mode)
  (setq auto-minor-mode-test-unregistered-mode
        (> (prefix-numeric-value
            (or argument 1))
           0))
  (push
   (list
    :unregistered
    argument
    auto-minor-mode-test-unregistered-mode
    (point)
    major-mode)
   auto-minor-mode-test-events)
  auto-minor-mode-test-unregistered-mode)

(dolist
    (mode
     '(auto-minor-mode-test-alpha-mode
       auto-minor-mode-test-beta-mode
       auto-minor-mode-test-gamma-mode))
  (add-to-list 'minor-mode-list mode))

(defun auto-minor-mode-test-reset ()
  (setq
   auto-minor-mode-test-events nil
   auto-minor-mode-test-alpha-mode nil
   auto-minor-mode-test-beta-mode nil
   auto-minor-mode-test-gamma-mode nil
   auto-minor-mode-test-unregistered-mode nil))

(defun auto-minor-mode-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list
      :signal
      (car error-data)
      (cdr error-data)))))

(defun auto-minor-mode-test-filename-match (_name)
  t)

(defun auto-minor-mode-test-root (name)
  (let ((root
         (file-name-as-directory
          (expand-file-name
           name
           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory root t)
    root))

(defun auto-minor-mode-test-write (file content)
  (make-directory
   (file-name-directory file)
   t)
  (with-temp-file file
    (insert content))
  file)
"##;

fn auto_minor_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_MINOR_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned auto-minor-mode source below ./tmp")
        .with_prelude(AUTO_MINOR_MODE_TEST_PRELUDE)
        .with_timeout(AUTO_MINOR_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-minor-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_minor_mode_autoload_parity` cases (2a).
pub(crate) fn assert_auto_minor_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_minor_mode_oracle("auto-minor-mode-autoloads.el"),
        &name,
        "auto_minor_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_minor_mode_parity` cases (2a).
pub(crate) fn assert_auto_minor_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_minor_mode_oracle("auto-minor-mode.el"),
        &name,
        "auto_minor_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_minor_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_minor_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_minor_mode_autoload_batch(&cases);
}

#[test]
fn auto_minor_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        advice::advice_public_surface_batch_cases(),
        filenames::filenames_public_surface_batch_cases(),
        magic::magic_public_surface_batch_cases(),
        registry::registry_auto_minor_mode_batch_cases(),
        use_package::use_package_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_minor_mode_batch(&cases);
}

// END generated package batch tests
