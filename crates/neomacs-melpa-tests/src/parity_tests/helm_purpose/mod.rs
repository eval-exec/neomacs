use std::time::Duration;

use crate::{CachedMelpaOracle, HELM_MELPA_PIN, HELM_PURPOSE_MELPA_PIN, WINDOW_PURPOSE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_PURPOSE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const HELM_PURPOSE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'helm-purpose)

(defmacro neomacs-helm-purpose-test-with-configuration (&rest body)
  "Run BODY with an isolated Purpose name configuration and named buffers."
  (declare (indent 0) (debug body))
  `(let ((purpose-use-default-configuration nil)
         (purpose-user-mode-purposes nil)
         (purpose-user-name-purposes
          '(("edit-alpha" . edit)
            ("edit-beta" . edit)
            ("help-alpha" . help)
            ("term-alpha" . terminal)))
         (purpose-user-regexp-purposes nil)
         (purpose-extended-configuration nil)
         (default-purpose 'general)
         (default-file-purpose 'edit)
         (purpose--user-mode-purposes (make-hash-table))
         (purpose--user-name-purposes (make-hash-table :test #'equal))
         (purpose--user-regexp-purposes (make-hash-table :test #'equal))
         (purpose--extended-mode-purposes (make-hash-table))
         (purpose--extended-name-purposes (make-hash-table :test #'equal))
         (purpose--extended-regexp-purposes (make-hash-table :test #'equal))
         (purpose--default-mode-purposes (make-hash-table))
         (purpose--default-name-purposes (make-hash-table :test #'equal))
         (purpose--default-regexp-purposes (make-hash-table :test #'equal))
         (purpose-preferred-prompt 'default)
         buffers)
     (purpose-compile-default-configuration)
     (purpose-compile-extended-configuration)
     (purpose-compile-user-configuration)
     (unwind-protect
         (progn
           (dolist (name '("edit-alpha" "edit-beta" "help-alpha" "term-alpha"))
             (push (get-buffer-create name) buffers))
           (setq buffers (nreverse buffers))
           ,@body)
       (let ((kill-buffer-hook nil)
             (kill-buffer-query-functions nil))
         (dolist (buffer buffers)
           (when (buffer-live-p buffer)
             (kill-buffer buffer)))
         (when (get-buffer "*helm purpose*")
           (kill-buffer "*helm purpose*"))))))

(defun neomacs-helm-purpose-test-source-buffers ()
  "Return buffer names from the Purpose Helm source under the current purpose."
  (let ((getter (helm-attr 'buffer-list helm-source-purpose-buffers-list)))
    (sort (copy-sequence (funcall getter)) #'string<)))

(defun neomacs-helm-purpose-test-capture-helm (thunk)
  "Call THUNK while capturing the next `helm' invocation."
  (let (captured)
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest plist)
                 (setq captured
                       (list
                        :sources (plist-get plist :sources)
                        :helm-buffer (plist-get plist :buffer)
                        :prompt (plist-get plist :prompt)
                        :purpose helm-purpose--current-purpose
                        :candidates (neomacs-helm-purpose-test-source-buffers)))
                 nil)))
      (funcall thunk)
      captured)))
"####;

fn helm_purpose_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_PURPOSE_MELPA_PIN, "helm-purpose.el")
        .expect("prepare exact shallow helm-purpose source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare exact shallow Helm dependency below ./tmp")
        .with_melpa_dependency(WINDOW_PURPOSE_MELPA_PIN)
        .expect("prepare exact shallow window-purpose dependency below ./tmp")
        .with_prelude(HELM_PURPOSE_TEST_PRELUDE)
        .with_timeout(HELM_PURPOSE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed helm-purpose parity test")
        .into()
}

fn assert_helm_purpose_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_purpose_oracle(),
        &current_test_name(),
        "helm_purpose_parity",
        cases,
    );
}

#[test]
fn helm_purpose_package_batch() {
    assert_helm_purpose_batch(&workflows::workflow_batch_cases());
}
