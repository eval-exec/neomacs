use std::time::Duration;

use crate::{ASTYLE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod arguments;
mod commands;
mod mode;
mod registry;
mod workflows;

const ASTYLE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASTYLE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun astyle-test-path (filename)
  (expand-file-name
   filename
   (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun astyle-test-read-file (filename)
  (with-temp-buffer
    (insert-file-contents-literally filename)
    (buffer-string)))

(defun astyle-test-install-formatter ()
  (let* ((bin-directory
          (file-name-as-directory
           (astyle-test-path "bin")))
         (program
          (expand-file-name
           "astyle"
           bin-directory))
         (argument-log
          (astyle-test-path
           "astyle-arguments.log")))
    (make-directory bin-directory t)
    (with-temp-file program
      (insert
       "#!/bin/sh\n"
       "printf '%s\\n' \"$@\" > \"$ASTYLE_TEST_ARG_LOG\"\n"
       "if [ \"$ASTYLE_TEST_FAIL\" = 1 ]; then\n"
       "  printf '\\033[31mfixture formatter failed\\033[0m\\n' >&2\n"
       "  exit 7\n"
       "fi\n"
       "sed -e 's/int main(){/int main() {/' "
       "-e 's/^return 0;$/    return 0;/'\n"))
    (set-file-modes program #o755)
    (setq exec-path
          (cons
           bin-directory
           exec-path))
    (setenv
     "PATH"
     (concat
      bin-directory
      path-separator
      (or (getenv "PATH") "")))
    (setenv
     "ASTYLE_TEST_ARG_LOG"
     argument-log)
    (setenv
     "ASTYLE_TEST_FAIL"
     nil)
    (list program argument-log)))

(defun astyle-test-kill-error-buffer ()
  (when-let ((buffer
              (get-buffer
               "*astyle errors*")))
    (kill-buffer buffer)))
"##;

fn astyle_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASTYLE_MELPA_PIN, source_file)
        .expect("prepare pinned astyle source below ./tmp")
        .with_prelude(ASTYLE_TEST_PRELUDE)
        .with_timeout(ASTYLE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed astyle parity test").into()
}

/// Multi-probe batch for `assert_astyle_autoload_parity` cases (2a).
pub(crate) fn assert_astyle_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        astyle_oracle("astyle-autoloads.el"),
        &name,
        "astyle_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_astyle_parity` cases (2a).
pub(crate) fn assert_astyle_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(astyle_oracle("astyle.el"), &name, "astyle_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn astyle_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_astyle_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_astyle_autoload_batch(&cases);
}

#[test]
fn astyle_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        arguments::arguments_public_surface_batch_cases(),
        commands::commands_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        registry::registry_astyle_batch_cases(),
        workflows::workflows_practical_formatting_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_astyle_batch(&cases);
}

// END generated package batch tests
