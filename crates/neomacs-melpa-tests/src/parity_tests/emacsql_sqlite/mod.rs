use std::time::Duration;

use crate::{CachedMelpaOracle, EMACSQL_SQLITE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EMACSQL_SQLITE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const EMACSQL_SQLITE_TEST_PRELUDE: &str = r####"
(require 'warnings)

(defun neomacs-emacsql-sqlite-test-warning-state ()
  "Return the complete user-visible state of the migration warning."
  (if-let ((buffer (get-buffer "*Warnings*")))
      (with-current-buffer buffer
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :major-mode major-mode
              :read-only buffer-read-only
              :undo-disabled (eq buffer-undo-list t)))
    'no-warning-buffer))

(defun neomacs-emacsql-sqlite-test-message-state ()
  "Return the complete message-log output produced by the migration stub."
  (if-let ((buffer (get-buffer "*Messages*")))
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max)))
    'no-message-buffer))

(defmacro neomacs-emacsql-sqlite-test-with-fresh-load (&rest body)
  "Run BODY with package and warning state isolated from adjacent cases."
  (declare (indent 0) (debug body))
  `(let* ((saved-warning-buffer (get-buffer "*Warnings*"))
          (saved-warning-name
           (and saved-warning-buffer
                (generate-new-buffer-name
                 " *neomacs-emacsql-sqlite-saved-warnings*")))
          (saved-message-buffer (get-buffer "*Messages*"))
          (saved-message-name
           (and saved-message-buffer
                (generate-new-buffer-name
                 " *neomacs-emacsql-sqlite-saved-messages*"))))
     (unwind-protect
         (progn
           (when (featurep 'emacsql-sqlite)
             (unload-feature 'emacsql-sqlite t))
           (when saved-warning-buffer
             (with-current-buffer saved-warning-buffer
               (rename-buffer saved-warning-name)))
           (when saved-message-buffer
             (with-current-buffer saved-message-buffer
               (rename-buffer saved-message-name)))
           ,@body)
       (unwind-protect
           (when (featurep 'emacsql-sqlite)
             (unload-feature 'emacsql-sqlite t))
         (when-let ((test-warning-buffer (get-buffer "*Warnings*")))
           (kill-buffer test-warning-buffer))
         (when-let ((test-message-buffer (get-buffer "*Messages*")))
           (kill-buffer test-message-buffer))
         (when (and saved-warning-buffer
                    (buffer-live-p saved-warning-buffer))
           (with-current-buffer saved-warning-buffer
             (rename-buffer "*Warnings*")))
         (when (and saved-message-buffer
                    (buffer-live-p saved-message-buffer))
           (with-current-buffer saved-message-buffer
             (rename-buffer "*Messages*")))))))
"####;

fn emacsql_sqlite_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMACSQL_SQLITE_MELPA_PIN, "emacsql-sqlite.el")
        .expect("prepare the exact final EmacSQL SQLite migration stub below ./tmp")
        .with_prelude(EMACSQL_SQLITE_TEST_PRELUDE)
        .with_timeout(EMACSQL_SQLITE_TEST_TIMEOUT)
}

fn assert_emacsql_sqlite_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        emacsql_sqlite_oracle(),
        "emacsql-sqlite-package-batch",
        "emacsql_sqlite_parity",
        cases,
    );
}

#[test]
fn emacsql_sqlite_package_batch() {
    assert_emacsql_sqlite_batch(&workflows::workflow_batch_cases());
}
