use std::time::Duration;

use crate::{CachedMelpaOracle, HELM_C_YASNIPPET_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_C_YASNIPPET_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const HELM_C_YASNIPPET_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'helm-c-yasnippet)

(setq yas-verbosity 0
      user-full-name "Parity Author")

(defvar neomacs-helm-c-yas-test-selection nil)
(defvar neomacs-helm-c-yas-test-action nil)
(defvar neomacs-helm-c-yas-test-pattern "")
(defvar neomacs-helm-c-yas-test-last-session nil)

(defun neomacs-helm-c-yas-test-root (name)
  "Return an isolated package-workflow directory named NAME."
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-helm-c-yas-test-write-file (file contents)
  "Write CONTENTS to FILE, creating its parent directory."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents))
  file)

(defun neomacs-helm-c-yas-test-file-contents (file)
  "Return FILE's exact contents."
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-helm-c-yas-test-reset-yas (root)
  "Make ROOT the sole real snippet tree and load it immediately."
  (setq yas-snippet-dirs (list root))
  (yas-reload-all t)
  root)

(defun neomacs-helm-c-yas-test-display (candidate)
  "Return CANDIDATE's display string without presentation properties."
  (substring-no-properties
   (if (consp candidate) (car candidate) candidate)))

(defun neomacs-helm-c-yas-test-action-list (source)
  "Resolve SOURCE's actions the same way Helm resolves a source attribute."
  (helm-get-attr 'action source 'ignorefn))

(defun neomacs-helm-c-yas-test-helm (&rest arguments)
  "Drive a Helm source deterministically at the unattended UI boundary.

The package still owns source initialization, candidate production and
transformation, matching, and action execution.  This adapter only supplies
the pattern, selected visible candidate, and action that a user would choose."
  (let* ((sources-argument
          (if (keywordp (car arguments))
              (plist-get arguments :sources)
            (car arguments)))
         (source (car (helm-get-sources sources-argument)))
         (helm-current-buffer (current-buffer))
         (helm-pattern neomacs-helm-c-yas-test-pattern)
         (helm-input helm-pattern)
         (helm-candidate-cache (make-hash-table :test #'equal)))
    (helm-compute-attr-in-sources 'init (list source))
    (let* ((candidates (helm-get-candidates source))
           (matches
            (helm-match-from-candidates
             candidates (helm-match-functions source)
             (assoc-default 'match-part source) 1000 source))
           (selected
            (and neomacs-helm-c-yas-test-selection
                 (cl-find-if
                  (lambda (candidate)
                    (equal
                     (neomacs-helm-c-yas-test-display candidate)
                     neomacs-helm-c-yas-test-selection))
                  matches)))
           (actions (neomacs-helm-c-yas-test-action-list source))
           (snippet-source-p
            (equal (assoc-default 'name source) "Yasnippet"))
           (action
            (and neomacs-helm-c-yas-test-action
                 (cdr (assoc neomacs-helm-c-yas-test-action actions))))
           (session
            (list
             :source (assoc-default 'name source)
             :initial-input
             (and snippet-source-p
                  (boundp 'helm-yas-initial-input)
                  helm-yas-initial-input)
             :replacement-span
             (and snippet-source-p
                  (boundp 'helm-yas-point-start)
                  (list helm-yas-point-start helm-yas-point-end))
             :selected-text
             (and snippet-source-p
                  (boundp 'helm-yas-selected-text)
                  helm-yas-selected-text)
             :pattern helm-pattern
             :candidates
             (mapcar #'neomacs-helm-c-yas-test-display candidates)
             :matches
             (mapcar #'neomacs-helm-c-yas-test-display matches)
             :selected
             (and selected (neomacs-helm-c-yas-test-display selected))
             :actions (mapcar #'car actions)
             :action neomacs-helm-c-yas-test-action
             :action-invoked (and selected action t))))
      (setq neomacs-helm-c-yas-test-last-session session)
      (when (and selected action)
        (let ((result
               (funcall action
                        (if (consp selected) (cdr selected) selected))))
          (setq neomacs-helm-c-yas-test-last-session
                (append session (list :action-result-truthy (and result t))))
          result)))))

(defun neomacs-helm-c-yas-test-session-summary (session)
  "Select stable user-visible fields from SESSION."
  (list :source (plist-get session :source)
        :initial-input (plist-get session :initial-input)
        :replacement-span (plist-get session :replacement-span)
        :selected-text (plist-get session :selected-text)
        :pattern (plist-get session :pattern)
        :candidates (plist-get session :candidates)
        :matches (plist-get session :matches)
        :selected (plist-get session :selected)
        :actions (plist-get session :actions)
        :action (plist-get session :action)
        :action-invoked (plist-get session :action-invoked)
        :action-result-truthy (plist-get session :action-result-truthy)))

(defun neomacs-helm-c-yas-test-error (thunk)
  "Call THUNK and return its exact nonlocal error shape."
  (condition-case error-data
      (progn (funcall thunk) :no-error)
    (error (cons (car error-data) (cdr error-data)))))

(defun neomacs-helm-c-yas-test-kill-file-buffer (file)
  "Kill the buffer visiting FILE without a save query."
  (let ((buffer (get-file-buffer file)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##;

fn helm_c_yasnippet_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_C_YASNIPPET_MELPA_PIN, "helm-c-yasnippet.el")
        .expect("prepare exact Helm C Yasnippet source and dependencies below ./tmp")
        .with_prelude(HELM_C_YASNIPPET_TEST_PRELUDE)
        .with_timeout(HELM_C_YASNIPPET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Helm C Yasnippet parity test")
        .into()
}

fn assert_helm_c_yasnippet_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_c_yasnippet_oracle(),
        &current_test_name(),
        "helm_c_yasnippet_parity",
        cases,
    );
}

#[test]
fn helm_c_yasnippet_package_batch() {
    assert_helm_c_yasnippet_batch(&workflows::workflow_batch_cases());
}
