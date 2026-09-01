use std::time::Duration;

use crate::{CachedMelpaOracle, ORGIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORGIT_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const ORGIT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'org)
(require 'ox)
(require 'magit)

(setq magit-git-global-arguments
      (append
       '("-c" "init.defaultBranch=main"
         "-c" "user.name=Neomacs Orgit Test"
         "-c" "user.email=orgit@example.test")
       magit-git-global-arguments))

(defun neomacs-orgit-test-git (directory &rest arguments)
  "Run Git with ARGUMENTS in DIRECTORY and return trimmed stdout."
  (let ((default-directory (file-name-as-directory directory)))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (zerop status)
          (error "git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-orgit-test-write (file text)
  "Write TEXT to FILE, creating parent directories."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert text)))

(defun neomacs-orgit-test-commit (root message timestamp)
  "Commit ROOT with deterministic MESSAGE and TIMESTAMP, returning its oid."
  (neomacs-orgit-test-git root "add" "--all")
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" "Neomacs Orgit Test")
    (setenv "GIT_AUTHOR_EMAIL" "orgit@example.test")
    (setenv "GIT_AUTHOR_DATE" timestamp)
    (setenv "GIT_COMMITTER_NAME" "Neomacs Orgit Test")
    (setenv "GIT_COMMITTER_EMAIL" "orgit@example.test")
    (setenv "GIT_COMMITTER_DATE" timestamp)
    (neomacs-orgit-test-git root "commit" "--no-gpg-sign" "-m" message))
  (neomacs-orgit-test-git root "rev-parse" "HEAD"))

(defun neomacs-orgit-test-repository (name)
  "Create a deterministic two-commit repository named NAME."
  (let* ((root (file-name-as-directory
                (expand-file-name name
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (file (expand-file-name "docs/release λ notes.txt" root)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    (neomacs-orgit-test-git root "init" "-b" "main" ".")
    (neomacs-orgit-test-write
     file
     "alpha\nbravo λ\ncharlie\n四行目\necho\n")
    (let ((first (neomacs-orgit-test-commit
                  root "Initial release notes"
                  "2026-07-01T12:00:00+0000")))
      (neomacs-orgit-test-write
       (expand-file-name "README.md" root)
       "# Orgit parity\n")
      (let ((second (neomacs-orgit-test-commit
                     root "Document Orgit workflow"
                     "2026-07-02T12:00:00+0000")))
        (list root file first second)))))

(defun neomacs-orgit-test-clean-buffers (root)
  "Kill buffers associated with ROOT."
  (dolist (buffer (buffer-list))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (when (or (and buffer-file-name
                       (file-in-directory-p buffer-file-name root))
                  (and (stringp default-directory)
                       (file-in-directory-p default-directory root)))
          (set-buffer-modified-p nil)
          (kill-buffer buffer))))))

(defun neomacs-orgit-test-run (name function)
  "Call FUNCTION with a deterministic repository named NAME and clean it."
  (pcase-let* ((repo-data (neomacs-orgit-test-repository name))
               (root (nth 0 repo-data)))
    (unwind-protect
        (apply function repo-data)
      (neomacs-orgit-test-clean-buffers root)
      (when (file-exists-p root)
        (delete-directory root t)))))

(defun neomacs-orgit-test-normalize (value root first second)
  "Normalize repository paths and commit ids in VALUE."
  (cond
   ((stringp value)
    (let ((text value))
      (setq text
            (replace-regexp-in-string
             (regexp-quote root) "<REPO>/" text t t))
      (dolist (entry (list (cons first "<FIRST>")
                           (cons (substring first 0 7) "<FIRST7>")
                           (cons second "<SECOND>")
                           (cons (substring second 0 7) "<SECOND7>")))
        (setq text
              (replace-regexp-in-string
               (regexp-quote (car entry)) (cdr entry) text t t)))
      text))
   ((consp value)
    (mapcar (lambda (item)
              (neomacs-orgit-test-normalize item root first second))
            value))
   (t value)))

(defun neomacs-orgit-test-outcome (function)
  "Return FUNCTION's value or exact signal identity and message."
  (condition-case err
      (list :value (funcall function))
    (error
     (list :signal (car err)
           :data (cdr err)
           :message (error-message-string err)))))
"##;

fn orgit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORGIT_MELPA_PIN, "orgit.el")
        .expect("prepare exact shallow Orgit source and dependencies below ./tmp")
        .with_prelude(ORGIT_TEST_PRELUDE)
        .with_timeout(ORGIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Orgit parity test")
        .into()
}

fn assert_orgit_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(orgit_oracle(), &current_test_name(), "orgit_parity", cases);
}

#[test]
fn orgit_package_batch() {
    assert_orgit_batch(&workflows::workflow_batch_cases());
}
