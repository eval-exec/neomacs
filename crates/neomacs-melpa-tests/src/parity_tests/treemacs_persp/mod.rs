use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, PERSP_MODE_MELPA_PIN, TREEMACS_MELPA_PIN,
    TREEMACS_PERSP_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TREEMACS_PERSP_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const TREEMACS_PERSP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'treemacs)
(require 'persp-mode)
(require 'treemacs-persp)

(defun neomacs-treemacs-persp-test-root (name)
  "Create a sandbox directory for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "treemacs-persp-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-treemacs-persp-test-workspace-names ()
  "Return Treemacs workspace names in stable order."
  (sort (mapcar #'treemacs-workspace->name treemacs--workspaces) #'string<))

(defun neomacs-treemacs-persp-test-hooks ()
  "Describe the hooks installed by the Perspectives scope."
  (list :activated (and (memq #'treemacs-persp--on-perspective-switch
                              persp-activated-functions)
                        t)
        :renamed (and (memq #'treemacs-persp--on-perspective-rename
                            persp-renamed-functions)
                      t)
        :before-kill (and (memq #'treemacs--on-scope-kill
                                persp-before-kill-functions)
                          t)))

(defun neomacs-treemacs-persp-test-run (name function)
  "Run FUNCTION under a clean persp-mode + Perspectives Treemacs scope."
  (when persp-mode (ignore-errors (persp-mode -1)))
  (let* ((root (neomacs-treemacs-persp-test-root name))
         (original-buffer (current-buffer))
         (original-scope treemacs--current-scope-type)
         (original-workspaces (copy-sequence treemacs--workspaces))
         (original-storage (copy-sequence treemacs--scope-storage))
         (original-current (treemacs-current-workspace))
         (persp-auto-save-opt 0)
         (persp-auto-resume-time -1)
         (persp-auto-save-persps-to-their-file nil)
         (persp-auto-save-persps-to-their-file-before-kill nil)
         (persp-use-kill-buffer-advice nil)
         (persp-add-buffer-on-find-file nil)
         (persp-add-buffer-on-after-change-major-mode nil)
         (persp-hook-up-emacs-buffer-completion nil)
         (persp-set-read-buffer-function nil)
         (persp-set-ido-hooks nil)
         (persp-set-frame-buffer-predicate nil)
         (persp-restore-window-conf-method t)
         (persp-reset-windows-on-nil-window-conf t)
         (persp-common-buffer-filter-functions nil)
         (persp-auto-persp-alist nil)
         (persp-created-functions nil)
         (persp-renamed-functions nil)
         (persp-before-kill-functions nil)
         (persp-before-switch-functions nil)
         (persp-activated-functions nil)
         (persp-before-deactivate-functions nil)
         result)
    (unwind-protect
        (cl-letf (((symbol-function 'run-with-timer)
                   (lambda (_sec _repeat function &rest args)
                     (apply function args)))
                  ((symbol-function 'treemacs--change-buffer-on-scope-change)
                   (lambda (&rest _)
                     :buffer-change-skipped))
                  ((symbol-function 'treemacs--find-current-user-project)
                   (lambda () root))
                  ((symbol-function 'treemacs--invalidate-buffer-project-cache)
                   #'ignore)
                  ((symbol-function 'treemacs--follow)
                   #'ignore)
                  ((symbol-function 'treemacs--do-follow)
                   #'ignore))
          (persp-mode 1)
          (treemacs-set-scope-type 'Perspectives)
          (setq result (funcall function root)))
      (ignore-errors (treemacs-set-scope-type 'Frames))
      (when persp-mode (ignore-errors (persp-mode -1)))
      (setq treemacs--current-scope-type original-scope
            treemacs--workspaces original-workspaces
            treemacs--scope-storage original-storage)
      (setf (treemacs-current-workspace) original-current)
      (when (buffer-live-p original-buffer)
        (set-buffer original-buffer))
      (when (file-exists-p root)
        (delete-directory root t)))
    result))
"####;

fn treemacs_persp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREEMACS_PERSP_MELPA_PIN, "treemacs-persp.el")
        .expect("prepare exact shallow treemacs-persp source below ./tmp")
        .with_melpa_dependency(TREEMACS_MELPA_PIN)
        .expect("prepare exact shallow Treemacs dependency below ./tmp")
        .with_melpa_dependency(PERSP_MODE_MELPA_PIN)
        .expect("prepare exact shallow persp-mode dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow dash dependency below ./tmp")
        .with_prelude(TREEMACS_PERSP_TEST_PRELUDE)
        .with_timeout(TREEMACS_PERSP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed treemacs-persp parity test")
        .into()
}

fn assert_treemacs_persp_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        treemacs_persp_oracle(),
        &current_test_name(),
        "treemacs_persp_parity",
        cases,
    );
}

#[test]
fn treemacs_persp_package_batch() {
    assert_treemacs_persp_batch(&workflows::workflow_batch_cases());
}
