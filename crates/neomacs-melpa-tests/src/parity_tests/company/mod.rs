use std::time::Duration;

use crate::{COMPANY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const COMPANY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const COMPANY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'company-capf)
(require 'company-files)

(defvar-local neomacs-company-test-events nil)

(defun neomacs-company-test-write-file (path contents)
  "Write CONTENTS to PATH, creating its parent directory."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun neomacs-company-test-plain-candidates ()
  "Return current completion candidates without display properties."
  (mapcar #'substring-no-properties company-candidates))

(defun neomacs-company-test-environment-capf ()
  "Complete deployment environment names at point with useful metadata."
  (let ((end (point))
        (start (save-excursion
                 (skip-chars-backward "[:word:]-")
                 (point))))
    (list
     start end
     '("preview" "preproduction" "production")
     :annotation-function
     (lambda (candidate)
       (format "  environment:%s" (substring candidate 0 3)))
     :company-kind (lambda (_) 'constant)
     :company-docsig
     (lambda (candidate)
       (format "Deploy using the %s environment" candidate))
     :exit-function
     (lambda (candidate status)
       (push (list :completed candidate :status status)
             neomacs-company-test-events)))))

(defun neomacs-company-test-remote-backend (command &optional argument &rest _)
  "A deterministic asynchronous backend resembling a remote code index."
  (pcase command
    (`prefix
     (let ((end (point))
           (start (save-excursion
                    (skip-chars-backward "[:word:]-")
                    (point))))
       (buffer-substring-no-properties start end)))
    (`candidates
     (cons
      :async
      (lambda (callback)
        (run-at-time
         0.02 nil callback
         (all-completions
          argument
          '("repository-clone"
            "repository-find"
            "repository-open"))))))
    (`sorted t)
    (`annotation "  remote index")
    (`kind 'function)
    (`meta (format "Workspace action: %s" argument))
    (`post-completion
     (push (list :remote-completed argument)
           neomacs-company-test-events))))
"##;

fn company_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_MELPA_PIN, "company.el")
        .expect("prepare pinned Company source below ./tmp")
        .with_prelude(COMPANY_TEST_PRELUDE)
        .with_timeout(COMPANY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Company parity test")
        .into()
}

pub(crate) fn assert_company_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(company_oracle(), &name, "company_parity", cases);
}

#[test]
fn company_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_company_batch(&cases);
}
