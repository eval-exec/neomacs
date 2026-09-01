use std::time::Duration;

use crate::{CachedMelpaOracle, FLYCHECK_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const FLYCHECK_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FLYCHECK_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-flycheck-test-write-file (path contents)
  "Write CONTENTS to PATH, creating its parent directory."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun neomacs-flycheck-test-run-to-completion ()
  "Run the configured Flycheck checker and wait for its completion hook."
  (let (finished (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (flycheck-mode 1)
    (flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out waiting for Flycheck; status is %S"
             flycheck-last-status-change))
    finished))

(defun neomacs-flycheck-test-reset-to-completion (checker)
  "Reset CHECKER's cached eligibility and wait for the resulting check."
  (let (finished (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (flycheck-reset-enabled-checker checker)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out after resetting %S; status is %S"
             checker flycheck-last-status-change))
    finished))

(defun neomacs-flycheck-test-diagnostics (base)
  "Return every visible diagnostic with filenames relative to BASE."
  (mapcar
   (lambda (diagnostic)
     (list
      :file (and (flycheck-error-filename diagnostic)
                 (file-relative-name
                  (flycheck-error-filename diagnostic) base))
      :line (flycheck-error-line diagnostic)
      :column (flycheck-error-column diagnostic)
      :level (flycheck-error-level diagnostic)
      :checker (flycheck-error-checker diagnostic)
      :message (flycheck-error-message diagnostic)))
   flycheck-current-errors))
"##;

fn flycheck_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_MELPA_PIN, "flycheck.el")
        .expect("prepare pinned Flycheck source below ./tmp")
        .with_prelude(FLYCHECK_TEST_PRELUDE)
        .with_timeout(FLYCHECK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Flycheck parity test")
        .into()
}

pub(crate) fn assert_flycheck_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(flycheck_oracle(), &name, "flycheck_parity", cases);
}

#[test]
fn flycheck_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_flycheck_batch(&cases);
}
