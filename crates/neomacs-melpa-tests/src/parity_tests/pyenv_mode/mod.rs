use std::time::Duration;

use crate::{CachedMelpaOracle, PYENV_MODE_MELPA_PIN, PYTHONIC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PYENV_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PYENV_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'pyenv-mode)

(defvar neomacs-pyenv-mode-test-root nil)
(defvar neomacs-pyenv-mode-test-versions nil)

(defun neomacs-pyenv-mode-test-with-fixture (function)
  "Run FUNCTION with pyenv root/versions stubbed under the sandbox."
  (let* ((root (file-name-as-directory
                (expand-file-name "pyenv-mode"
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (neomacs-pyenv-mode-test-root root)
         (neomacs-pyenv-mode-test-versions '("3.11.0" "3.12.1"))
         (process-environment (copy-sequence process-environment))
         (pyenv-mode nil)
         activations)
    (when (file-exists-p root) (delete-directory root t))
    (make-directory (expand-file-name "versions/3.11.0" root) t)
    (make-directory (expand-file-name "versions/3.12.1" root) t)
    (setenv "PYENV_VERSION" nil)
    (cl-letf (((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (cond
                  ((string-match-p "pyenv root" command)
                   (concat root "\n"))
                  ((string-match-p "pyenv versions" command)
                   (mapconcat #'identity neomacs-pyenv-mode-test-versions "\n"))
                  (t ""))))
              ((symbol-function 'executable-find)
               (lambda (command)
                 (if (string= command "pyenv") "/usr/bin/pyenv" nil)))
              ((symbol-function 'pythonic-activate)
               (lambda (path)
                 (push (list :activate path) activations)
                 path))
              ((symbol-function 'pythonic-deactivate)
               (lambda ()
                 (push (list :deactivate) activations)
                 t))
              ((symbol-function 'force-mode-line-update) #'ignore))
      (unwind-protect
          (funcall function (lambda () (nreverse activations)))
        (when pyenv-mode (pyenv-mode -1))
        (setenv "PYENV_VERSION" nil)
        (when (file-exists-p root) (delete-directory root t))))))
"####;

fn pyenv_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYENV_MODE_MELPA_PIN, "pyenv-mode.el")
        .expect("prepare exact shallow pyenv-mode source below ./tmp")
        .with_melpa_dependency(PYTHONIC_MELPA_PIN)
        .expect("prepare exact shallow pythonic dependency below ./tmp")
        .with_prelude(PYENV_MODE_TEST_PRELUDE)
        .with_timeout(PYENV_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed pyenv-mode parity test")
        .into()
}

fn assert_pyenv_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        pyenv_mode_oracle(),
        &current_test_name(),
        "pyenv_mode_parity",
        cases,
    );
}

#[test]
fn pyenv_mode_package_batch() {
    assert_pyenv_mode_batch(&workflows::workflow_batch_cases());
}
