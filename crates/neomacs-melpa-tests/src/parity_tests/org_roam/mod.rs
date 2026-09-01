use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_ROAM_MELPA_PIN, SQLITE3_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORG_ROAM_TEST_TIMEOUT: Duration = Duration::from_secs(300);
const ORG_ROAM_TEST_PRELUDE: &str = r##"
(let ((load-suffixes (append load-suffixes (list module-file-suffix))))
  (require 'sqlite3))
(require 'cl-lib)

(defun neomacs-org-roam-test-write (file contents)
  "Write CONTENTS to FILE, creating its parent directory."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun neomacs-org-roam-test-node-state (node root)
  "Describe NODE with paths normalized relative to ROOT."
  (and node
       (list :id (org-roam-node-id node)
             :title (org-roam-node-title node)
             :file (file-relative-name (org-roam-node-file node) root)
             :level (org-roam-node-level node)
             :todo (org-roam-node-todo node)
             :priority (org-roam-node-priority node)
             :scheduled (org-roam-node-scheduled node)
             :deadline (org-roam-node-deadline node)
             :olp (org-roam-node-olp node)
             :tags (sort (copy-sequence (org-roam-node-tags node)) #'string<)
             :aliases (sort (copy-sequence (org-roam-node-aliases node)) #'string<)
             :refs (sort (copy-sequence (org-roam-node-refs node)) #'string<))))

(defun neomacs-org-roam-test-close ()
  "Close Org-roam databases and related visiting buffers."
  (org-roam-db--close-all)
  (dolist (buffer (buffer-list))
    (when-let* ((file (buffer-file-name buffer))
                ((stringp org-roam-directory))
                ((file-in-directory-p file org-roam-directory)))
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))

(defmacro neomacs-org-roam-test-with-kb (&rest body)
  "Create a deterministic real Org-roam knowledge base and run BODY."
  (declare (indent 0) (debug t))
  `(let* ((root (file-name-as-directory
                 (expand-file-name "knowledge-base"
                                   (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
          (org-roam-directory root)
          (org-roam-db-location
           (expand-file-name "state/org-roam.db"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (org-roam-file-extensions '("org"))
          (org-roam-file-exclude-regexp nil)
          (org-roam-list-files-commands nil)
          (org-id-locations-file
           (expand-file-name "state/org-id-locations"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (org-roam-db--connection (make-hash-table :test #'equal)))
     (when (file-exists-p root) (delete-directory root t))
     (make-directory root t)
     (neomacs-org-roam-test-write
      (expand-file-name "alpha.org" root)
      (concat
       ":PROPERTIES:\n"
       ":ID: alpha-id\n"
       ":ROAM_ALIASES: \"First Note\" Origin\n"
       ":ROAM_REFS: https://example.test/alpha\n"
       ":END:\n"
       "#+title: Alpha λ\n"
       "#+filetags: :project:unicode:\n"
       "\nAlpha body links to [[id:beta-id][Beta]].\n"))
     (neomacs-org-roam-test-write
      (expand-file-name "beta.org" root)
      (concat
       ":PROPERTIES:\n"
       ":ID: beta-id\n"
       ":END:\n"
       "#+title: Beta\n"
       "#+filetags: :project:\n"
       "\nBeta body.\n"
       "\n* TODO Milestone\n"
       "SCHEDULED: <2026-08-10 Mon> DEADLINE: <2026-08-12 Wed>\n"
       ":PROPERTIES:\n"
       ":ID: milestone-id\n"
       ":ROAM_ALIASES: Checkpoint\n"
       ":END:\n"
       "Milestone links to [[id:alpha-id][Alpha λ]].\n"))
     (neomacs-org-roam-test-write
      (expand-file-name "notes/gamma.org" root)
      (concat
       ":PROPERTIES:\n"
       ":ID: gamma-id\n"
       ":END:\n"
       "#+title: Gamma\n"
       "#+filetags: :archive:\n"
       "\nGamma references [[id:alpha-id][Alpha λ]].\n"))
     (unwind-protect
         (progn
           (org-roam-db-sync t)
           ,@body)
       (neomacs-org-roam-test-close))))
"##;

fn org_roam_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_ROAM_MELPA_PIN, "org-roam.el")
        .expect("prepare exact shallow Org-roam source and dependencies below ./tmp")
        .with_melpa_dependency(SQLITE3_MELPA_PIN)
        .expect("prepare exact shallow SQLite module backend below ./tmp")
        .with_prelude(ORG_ROAM_TEST_PRELUDE)
        .with_timeout(ORG_ROAM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Org-roam parity test")
        .into()
}

fn assert_org_roam_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        org_roam_oracle(),
        &current_test_name(),
        "org_roam_parity",
        cases,
    );
}

#[test]
fn org_roam_package_batch() {
    assert_org_roam_batch(&workflows::workflow_batch_cases());
}
