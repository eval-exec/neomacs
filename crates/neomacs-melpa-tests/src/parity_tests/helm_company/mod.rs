use std::time::Duration;

use crate::{COMPANY_MELPA_PIN, CachedMelpaOracle, HELM_COMPANY_MELPA_PIN, HELM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_COMPANY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HELM_COMPANY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'helm-company)

(defvar-local neomacs-helm-company-test-events nil)

(defun neomacs-helm-company-test-prefix ()
  "Return the production identifier immediately before point."
  (buffer-substring-no-properties
   (save-excursion
     (skip-chars-backward "[:word:]-")
     (point))
   (point)))

(defun neomacs-helm-company-test-candidate (name environment)
  "Return completion NAME carrying its ENVIRONMENT metadata."
  (propertize name 'neomacs-environment environment))

(defun neomacs-helm-company-test-backend (command &optional argument &rest _)
  "Complete deployment operations through Company's public backend protocol."
  (pcase command
    (`prefix (neomacs-helm-company-test-prefix))
    (`candidates
     (all-completions
      argument
      (list
       (neomacs-helm-company-test-candidate "deploy-preview" 'canary)
       (neomacs-helm-company-test-candidate "deploy-production" 'primary)
       (neomacs-helm-company-test-candidate "deploy-preproduction" 'staging))))
    (`sorted t)
    (`annotation
     (format "  %s" (get-text-property 0 'neomacs-environment argument)))
    (`kind 'function)
    (`doc-buffer
     (push (list :document
                 (substring-no-properties argument)
                 (get-text-property 0 'neomacs-environment argument))
           neomacs-helm-company-test-events)
     (let ((buffer
            (get-buffer-create
             (format "*deployment %s help*"
                     (get-text-property 0 'neomacs-environment argument)))))
       (with-current-buffer buffer
         (erase-buffer)
         (insert (format "%s deploys to %s.\n"
                         (substring-no-properties argument)
                         (get-text-property 0 'neomacs-environment argument))))
       buffer))
    (`location
     (push (list :location
                 (substring-no-properties argument)
                 (get-text-property 0 'neomacs-environment argument))
           neomacs-helm-company-test-events)
     (pcase (get-text-property 0 'neomacs-environment argument)
       (`primary
        (let ((buffer (get-buffer-create "*deployment definitions*")))
          (with-current-buffer buffer
            (erase-buffer)
            (insert "Deployment targets\n\nProduction target\nOwner: release-team\n")
            (goto-char (point-min))
            (search-forward "Production"))
          (cons buffer 21)))
       (`staging
        (cons (expand-file-name "deployment-targets.el"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
              3))))
    (`post-completion
     (push (list :completed
                 (substring-no-properties argument)
                 (get-text-property 0 'neomacs-environment argument))
           neomacs-helm-company-test-events))))

(defun neomacs-helm-company-test-start (prefix)
  "Insert PREFIX and activate the deterministic deployment backend."
  (insert prefix)
  (company-mode 1)
  (company-begin-backend 'neomacs-helm-company-test-backend))

(defun neomacs-helm-company-test-candidate-shape (candidate)
  "Describe CANDIDATE without discarding backend metadata."
  (list (substring-no-properties candidate)
        (get-text-property 0 'neomacs-environment candidate)))

(defun neomacs-helm-company-test-after-completion ()
  "Record Helm Company's package-specific completion hook."
  (push :helm-company-hook neomacs-helm-company-test-events))

(defun neomacs-helm-company-test-company-finished (candidate)
  "Record Company's finished hook for CANDIDATE."
  (push (list :company-finished (substring-no-properties candidate))
        neomacs-helm-company-test-events))
"##;

fn helm_company_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_COMPANY_MELPA_PIN, "helm-company.el")
        .expect("prepare exact shallow Helm Company source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare exact shallow Company dependency below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare exact shallow Helm dependency below ./tmp")
        .with_prelude(HELM_COMPANY_TEST_PRELUDE)
        .with_timeout(HELM_COMPANY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Helm Company parity test")
        .into()
}

fn assert_helm_company_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_company_oracle(),
        &current_test_name(),
        "helm_company_parity",
        cases,
    );
}

#[test]
fn helm_company_package_batch() {
    assert_helm_company_batch(&workflows::workflow_batch_cases());
}
