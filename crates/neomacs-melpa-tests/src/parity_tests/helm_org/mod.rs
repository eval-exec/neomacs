use std::time::Duration;

use crate::{CachedMelpaOracle, HELM_MELPA_PIN, HELM_ORG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_ORG_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const HELM_ORG_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'org)
(require 'helm-org)

(defun neomacs-helm-org-test-with-buffer (body)
  "Call BODY with an isolated Org buffer containing sample headings."
  (let ((buffer (generate-new-buffer " *neomacs-helm-org-test*")))
    (unwind-protect
        (with-current-buffer buffer
          (org-mode)
          (insert
           "* Alpha\n"
           "body alpha\n"
           "** Beta\n"
           "body beta\n"
           "*** Gamma\n"
           "body gamma\n"
           "* Delta\n"
           "body delta\n")
          (goto-char (point-min))
          (funcall body buffer))
      (when (buffer-live-p buffer)
        (let ((kill-buffer-hook nil)
              (kill-buffer-query-functions nil))
          (kill-buffer buffer))))))

(defun neomacs-helm-org-test-capture-helm (thunk)
  "Call THUNK while capturing the next `helm' invocation."
  (let (captured)
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest plist)
                 (setq captured
                       (list
                        :buffer (plist-get plist :buffer)
                        :preselect (plist-get plist :preselect)
                        :truncate (plist-get plist :truncate-lines)
                        :source-count (length (plist-get plist :sources))
                        :source-name
                        (and (plist-get plist :sources)
                             (helm-attr 'name (car (plist-get plist :sources))))
                        :candidates
                        (and (plist-get plist :sources)
                             (let ((src (car (plist-get plist :sources))))
                               (funcall (helm-attr 'candidates src))))))
                 nil)))
      (funcall thunk)
      captured)))
"####;

fn helm_org_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_ORG_MELPA_PIN, "helm-org.el")
        .expect("prepare exact shallow helm-org source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare exact shallow Helm dependency below ./tmp")
        .with_prelude(HELM_ORG_TEST_PRELUDE)
        .with_timeout(HELM_ORG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed helm-org parity test")
        .into()
}

fn assert_helm_org_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_org_oracle(),
        &current_test_name(),
        "helm_org_parity",
        cases,
    );
}

#[test]
fn helm_org_package_batch() {
    assert_helm_org_batch(&workflows::workflow_batch_cases());
}
