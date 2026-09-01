use std::time::Duration;

use crate::{CachedMelpaOracle, SMEARGLE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SMEARGLE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SMEARGLE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'smeargle)

(defun neomacs-smeargle-test-git (directory &rest arguments)
  "Run Git with ARGUMENTS in DIRECTORY and return trimmed stdout."
  (let ((default-directory (file-name-as-directory directory)))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (zerop status)
          (error "git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-smeargle-test-commit (root file contents timestamp message)
  "Write CONTENTS to FILE and commit it at TIMESTAMP with MESSAGE."
  (with-temp-file file (insert contents))
  (neomacs-smeargle-test-git root "add" "--all")
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" "Smeargle Parity")
    (setenv "GIT_AUTHOR_EMAIL" "smeargle@example.test")
    (setenv "GIT_AUTHOR_DATE" timestamp)
    (setenv "GIT_COMMITTER_NAME" "Smeargle Parity")
    (setenv "GIT_COMMITTER_EMAIL" "smeargle@example.test")
    (setenv "GIT_COMMITTER_DATE" timestamp)
    (neomacs-smeargle-test-git
     root "commit" "--no-gpg-sign" "-m" message)))

(defun neomacs-smeargle-test-repository ()
  "Create a deterministic Git repository with six differently-aged lines."
  (let* ((root (file-name-as-directory
                (expand-file-name "smeargle-repo"
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (file (expand-file-name "release.txt" root)))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    (neomacs-smeargle-test-git root "init" "-b" "main" ".")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month\nmonth\nweeks\ndays\nyesterday\n"
     "2025-06-01T12:00:00+0000" "initial")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month updated\nmonth\nweeks\ndays\nyesterday\n"
     "2026-01-01T12:00:00+0000" "six month")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month updated\nmonth updated\nweeks\ndays\nyesterday\n"
     "2026-06-01T12:00:00+0000" "month")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month updated\nmonth updated\nweeks updated\ndays\nyesterday\n"
     "2026-07-20T12:00:00+0000" "weeks")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month updated\nmonth updated\nweeks updated\ndays updated\nyesterday\n"
     "2026-08-02T12:00:00+0000" "days")
    (neomacs-smeargle-test-commit
     root file
     "ancient\nsix-month updated\nmonth updated\nweeks updated\ndays updated\nyesterday updated\n"
     "2026-08-06T12:00:00+0000" "yesterday")
    (list root file)))

(defun neomacs-smeargle-test-wait (file)
  "Wait until Smeargle's async blame buffer for FILE is gone."
  (let ((name (format " *smeargle-%s*" file))
        (limit 200))
    (while (and (> limit 0) (get-buffer name))
      (accept-process-output nil 0.05)
      (setq limit (1- limit)))
    (when (get-buffer name)
      (error "smeargle blame timed out"))))

(defun neomacs-smeargle-test-overlays ()
  "Describe Smeargle overlays in line order."
  (mapcar
   (lambda (overlay)
     (list :start-line (line-number-at-pos (overlay-start overlay))
           :end-line (line-number-at-pos
                      (max (overlay-start overlay)
                           (1- (overlay-end overlay))))
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))
           :face (overlay-get overlay 'face)))
   (sort (seq-filter (lambda (overlay) (overlay-get overlay 'smeargle))
                     (overlays-in (point-min) (point-max)))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defmacro neomacs-smeargle-test-with-repo (&rest body)
  "Run BODY in a visited deterministic repository file."
  (declare (indent 0) (debug t))
  `(pcase-let* ((`(,root ,file) (neomacs-smeargle-test-repository))
                (buffer (find-file-noselect file)))
     (unwind-protect
         (with-current-buffer buffer
           (let ((default-directory root))
             ,@body))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (when (file-exists-p root) (delete-directory root t)))))
"##;

fn smeargle_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SMEARGLE_MELPA_PIN, "smeargle.el")
        .expect("prepare exact shallow Smeargle source below ./tmp")
        .with_prelude(SMEARGLE_TEST_PRELUDE)
        .with_timeout(SMEARGLE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Smeargle parity test")
        .into()
}

fn assert_smeargle_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        smeargle_oracle(),
        &current_test_name(),
        "smeargle_parity",
        cases,
    );
}

#[test]
fn smeargle_package_batch() {
    assert_smeargle_batch(&workflows::workflow_batch_cases());
}
