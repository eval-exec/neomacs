use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PERSP_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PERSP_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PERSP_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-persp-test-root (name)
  "Create a deterministic sandbox directory for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "persp-mode-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-persp-test-write (root relative contents)
  "Write CONTENTS below ROOT at RELATIVE and return the resulting path."
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-persp-test-buffer-names (persp)
  "Return PERSP's live buffer names in stable order."
  (sort (mapcar #'buffer-name (safe-persp-buffers persp)) #'string<))

(defun neomacs-persp-test-names ()
  "Return registered perspective names in stable order."
  (sort (copy-sequence (persp-names)) #'string<))

(defun neomacs-persp-test-window-summary ()
  "Describe the selected frame's live editing windows in display order."
  (mapcar
   (lambda (window)
     (list :buffer (buffer-name (window-buffer window))
           :point (window-point window)
           :selected (eq window (selected-window))))
   (window-list nil 'no-minibuf)))

(defun neomacs-persp-test-cleanup (root original-window-state original-buffer)
  "Restore editor state and remove every buffer and file owned by ROOT."
  (when persp-mode
    (ignore-errors (persp-mode -1)))
  (dolist (buffer (buffer-list))
    (when (or (string-prefix-p "neomacs-persp-" (buffer-name buffer))
              (and (buffer-file-name buffer)
                   (string-prefix-p root (buffer-file-name buffer))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (ignore-errors (kill-buffer buffer))))
  (when original-window-state
    (ignore-errors
      (window-state-put original-window-state (frame-root-window) 'safe)))
  (when (buffer-live-p original-buffer)
    (set-buffer original-buffer))
  (when (file-exists-p root)
    (delete-directory root t)))

(defun neomacs-persp-test-run (name function)
  "Run FUNCTION in a clean real persp-mode session rooted at NAME."
  (when persp-mode
    (persp-mode -1))
  (let* ((root (neomacs-persp-test-root name))
         (original-window-state (window-state-get (frame-root-window) t))
         (original-buffer (current-buffer))
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
         (persp-after-load-state-functions nil)
         result)
    (unwind-protect
        (progn
          (persp-mode 1)
          (setq result (funcall function root)))
      (neomacs-persp-test-cleanup
       root original-window-state original-buffer))
    result))
"####;

fn persp_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PERSP_MODE_MELPA_PIN, "persp-mode.el")
        .expect("prepare revision-pinned persp-mode source below ./tmp")
        .with_prelude(PERSP_MODE_TEST_PRELUDE)
        .with_timeout(PERSP_MODE_TEST_TIMEOUT)
}

fn release_workspace_lifecycle_preserves_membership_parameters_and_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-persp-test-run
 "workspace-lifecycle"
 (lambda (root)
   (let* ((deploy-file
           (neomacs-persp-test-write
            root "services/deploy.el" "(defun deploy () 'staged)\n"))
          (ops-file
           (neomacs-persp-test-write
            root "operations/runbook.org" "* Rollback\nRestore release 41.\n"))
          (deploy-buffer (find-file-noselect deploy-file))
          (ops-buffer (find-file-noselect ops-file))
          (shared-buffer (get-buffer-create "neomacs-persp-release-log"))
          events deploy operations candidate)
     (add-hook 'persp-created-functions
               (lambda (persp _hash)
                 (push (list 'created (safe-persp-name persp)) events)))
     (add-hook 'persp-renamed-functions
               (lambda (_persp old-name new-name)
                 (push (list 'renamed old-name new-name) events)))
     (add-hook 'persp-before-kill-functions
               (lambda (persp)
                 (push (list 'killed (safe-persp-name persp)) events)))
     (persp-switch "deployment")
     (setq deploy (get-current-persp))
     (persp-add-buffer (list deploy-buffer shared-buffer) deploy nil nil)
     (modify-persp-parameters
      '((environment . "staging") (release . 41)) deploy)
     (persp-switch "operations")
     (setq operations (get-current-persp))
     (persp-add-buffer (list ops-buffer shared-buffer) operations nil nil)
     (set-persp-parameter 'owner "release-engineering" operations)
     (persp-switch "deployment")
     (setq candidate (persp-copy "release-candidate" 'no-switch nil))
     (persp-rename "release-ready" candidate)
     (persp-import-buffers "operations" deploy)
     (persp-remove-buffer ops-buffer deploy nil nil nil nil)
     (persp-hide "release-ready")
     (let ((hidden (safe-persp-hidden candidate)))
       (persp-unhide "release-ready")
       (persp-kill "operations" t nil)
       (list
        :current (safe-persp-name (get-current-persp))
        :names (neomacs-persp-test-names)
        :deployment-buffers (neomacs-persp-test-buffer-names deploy)
        :deployment-parameters (copy-tree (safe-persp-parameters deploy))
        :candidate-buffers (neomacs-persp-test-buffer-names candidate)
        :candidate-parameters (copy-tree (safe-persp-parameters candidate))
        :candidate-hidden (list hidden (safe-persp-hidden candidate))
        :shared-membership
        (mapcar
         (lambda (name)
           (cons name
                 (and (persp-contain-buffer-p
                       shared-buffer (persp-get-by-name name)) t)))
         '("deployment" "release-ready"))
        :operations-buffer-live (buffer-live-p ops-buffer)
        :events (nreverse events))))))
"####;
    let expected = expect![[
        r#"OK (:current "deployment" :names ("deployment" "none" "release-ready") :deployment-buffers ("deploy.el" "neomacs-persp-release-log") :deployment-parameters ((release . 41) (environment . "staging")) :candidate-buffers ("deploy.el" "neomacs-persp-release-log") :candidate-parameters ((release . 41) (environment . "staging")) :candidate-hidden (t nil) :shared-membership (("deployment" . t) ("release-ready" . t)) :operations-buffer-live t :events ((created "deployment") (created "operations") (created "release-candidate") (renamed "release-candidate" "release-ready") (killed "operations")))"#
    ]];
    ParityBatchCase::value(
        "release_workspace_lifecycle_preserves_membership_parameters_and_hooks",
        elisp_form,
        expected,
    )
}

fn switching_workspaces_round_trips_real_window_layout_buffers_and_points() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-persp-test-run
 "window-layout"
 (lambda (_root)
   (let ((plan (get-buffer-create "neomacs-persp-plan"))
         (log (get-buffer-create "neomacs-persp-log"))
         (runbook (get-buffer-create "neomacs-persp-runbook")))
     (with-current-buffer plan (erase-buffer) (insert "plan alpha beta gamma\n"))
     (with-current-buffer log (erase-buffer) (insert "build 40\nbuild 41 green\n"))
     (with-current-buffer runbook (erase-buffer) (insert "rollback\nverify\n"))
     (persp-switch "deployment")
     (let ((deployment (get-current-persp)))
       (persp-add-buffer (list plan log) deployment nil nil)
       (delete-other-windows)
       (switch-to-buffer plan)
       (goto-char 7)
       (let ((right (split-window-right)))
         (set-window-buffer right log)
         (set-window-point right 10))
       (persp-frame-save-state)
       (let ((deployment-before (neomacs-persp-test-window-summary)))
         (persp-switch "operations")
         (persp-add-buffer runbook (get-current-persp) nil nil)
         (delete-other-windows)
         (switch-to-buffer runbook)
         (goto-char 10)
         (persp-frame-save-state)
         (let ((operations-before (neomacs-persp-test-window-summary)))
           (persp-switch "deployment")
           (let ((deployment-restored (neomacs-persp-test-window-summary)))
             (persp-switch "operations")
             (list
              :deployment-before deployment-before
              :operations-before operations-before
              :deployment-restored deployment-restored
              :operations-restored (neomacs-persp-test-window-summary)
              :current (safe-persp-name (get-current-persp))))))))))
"####;
    let expected = expect![[
        r#"OK (:deployment-before ((:buffer "neomacs-persp-plan" :point 7 :selected t) (:buffer "neomacs-persp-log" :point 10 :selected nil)) :operations-before ((:buffer "neomacs-persp-runbook" :point 10 :selected t)) :deployment-restored ((:buffer "neomacs-persp-plan" :point 7 :selected t) (:buffer "neomacs-persp-log" :point 10 :selected nil)) :operations-restored ((:buffer "neomacs-persp-runbook" :point 10 :selected t)) :current "operations")"#
    ]];
    ParityBatchCase::value(
        "switching_workspaces_round_trips_real_window_layout_buffers_and_points",
        elisp_form,
        expected,
    )
}

fn save_and_load_restores_file_buffers_metadata_and_hidden_workspace() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-persp-test-run
 "save-load"
 (lambda (root)
   (let* ((source-file
           (neomacs-persp-test-write
            root "checkout/release.el"
            "(setq release-status 'approved)\n"))
          (notes-file
           (neomacs-persp-test-write
            root "checkout/notes.txt" "deploy at 14:00 UTC\n"))
          (state-file (expand-file-name "state/perspectives.el" root))
          (source-buffer (find-file-noselect source-file))
          (notes-buffer (find-file-noselect notes-file))
          saved-names loaded-names)
     (persp-switch "release-41")
     (persp-add-buffer
      (list source-buffer notes-buffer) (get-current-persp) nil nil)
     (modify-persp-parameters
      '((environment . "production")
        (owner . "release-engineering")
        (attempt . 3)))
     (persp-hide "release-41")
     (cl-letf (((symbol-function 'make-frame)
                (lambda (&rest _) (selected-frame)))
               ((symbol-function 'make-frame-invisible)
                (lambda (&rest _) nil))
               ((symbol-function 'delete-frame)
                (lambda (&rest _) nil)))
       (persp-save-state-to-file state-file *persp-hash* nil)
       (setq saved-names (persp-list-persp-names-in-file state-file))
       (persp-kill "release-41" t nil)
       (kill-buffer source-buffer)
       (kill-buffer notes-buffer)
       (setq loaded-names
             (persp-load-state-from-file state-file *persp-hash* nil t)))
     (let ((restored (persp-get-by-name "release-41")))
       (list
        :state-file-exists (file-exists-p state-file)
        :saved-names saved-names
        :loaded-names loaded-names
        :registered (neomacs-persp-test-names)
        :buffers (neomacs-persp-test-buffer-names restored)
        :files
        (sort
         (mapcar
          (lambda (buffer)
            (file-relative-name (buffer-file-name buffer) root))
          (safe-persp-buffers restored))
         #'string<)
        :parameters (copy-tree (safe-persp-parameters restored))
        :hidden (safe-persp-hidden restored)
        :contents
        (mapcar
         (lambda (buffer)
           (with-current-buffer buffer (buffer-string)))
         (sort (copy-sequence (safe-persp-buffers restored))
               (lambda (left right)
                 (string< (buffer-name left) (buffer-name right))))))))))
"####;
    let expected = expect![[
        r#"OK (:state-file-exists t :saved-names ("none" "release-41") :loaded-names ("none" "release-41") :registered ("none" "release-41") :buffers ("notes.txt" "release.el") :files ("checkout/notes.txt" "checkout/release.el") :parameters ((persp-file . "[ORACLE-SANDBOX]/persp-mode-save-load/state/perspectives.el") (environment . "production") (owner . "release-engineering") (attempt . 3)) :hidden t :contents ("deploy at 14:00 UTC\n" #("(setq release-status 'approved)\n" 0 32 (fontified nil))))"#
    ]];
    ParityBatchCase::value(
        "save_and_load_restores_file_buffers_metadata_and_hidden_workspace",
        elisp_form,
        expected,
    )
}

fn auto_workspace_tracks_matching_files_and_hides_after_last_buffer_leaves() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-persp-test-run
 "auto-workspace"
 (lambda (root)
   (let* ((release-file
           (neomacs-persp-test-write
            root "services/release-42.el" "(defconst release-id 42)\n"))
          (notes-file
           (neomacs-persp-test-write
            root "notes/incident.txt" "incident notes\n"))
          (persp-autokill-persp-when-removed-last-buffer 'hide-auto)
          release-buffer notes-buffer)
     (persp-def-auto-persp
      "release-files"
      :file-name "/services/release-[0-9]+\\.el\\'"
      :parameters '((kind . release) (owner . "delivery"))
      :dont-pick-up-buffers t)
     (setq release-buffer (find-file-noselect release-file))
     (setq notes-buffer (find-file-noselect notes-file))
     (with-current-buffer release-buffer
       (run-hooks 'find-file-hook))
     (with-current-buffer notes-buffer
       (run-hooks 'find-file-hook))
     (let* ((auto-persp (persp-get-by-name "release-files"))
            (before
             (list
              :buffers (neomacs-persp-test-buffer-names auto-persp)
              :parameters (copy-tree (safe-persp-parameters auto-persp))
              :auto (safe-persp-auto auto-persp)
              :hidden (safe-persp-hidden auto-persp)
              :release-contained
              (and (persp-contain-buffer-p release-buffer auto-persp) t)
              :notes-contained
              (and (persp-contain-buffer-p notes-buffer auto-persp) t))))
       (persp-remove-buffer release-buffer auto-persp nil nil nil nil)
       (list
        :before before
        :after
        (list :buffers (neomacs-persp-test-buffer-names auto-persp)
              :auto (safe-persp-auto auto-persp)
              :hidden (safe-persp-hidden auto-persp)
              :registered (member "release-files"
                                  (neomacs-persp-test-names))))))))
"####;
    let expected = expect![[
        r#"OK (:before (:buffers ("release-42.el") :parameters ((owner . "delivery") (kind . release)) :auto t :hidden nil :release-contained t :notes-contained nil) :after (:buffers nil :auto t :hidden t :registered ("release-files")))"#
    ]];
    ParityBatchCase::value(
        "auto_workspace_tracks_matching_files_and_hides_after_last_buffer_leaves",
        elisp_form,
        expected,
    )
}

fn shared_buffer_kill_removes_membership_before_destroying_the_last_owner() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-persp-test-run
 "shared-buffer-kill"
 (lambda (_root)
   (let ((shared (get-buffer-create "neomacs-persp-shared-console"))
         (persp-use-kill-buffer-advice t)
         (persp-kill-foreign-buffer-behaviour 'kill)
         first-kill second-kill)
     (persp-activate-kill-buffer-advice)
     (unwind-protect
         (progn
           (with-current-buffer shared
             (erase-buffer)
             (insert "release 42\nhealth green\n"))
           (persp-switch "deployment")
           (persp-add-buffer shared (get-current-persp) nil nil)
           (persp-switch "operations")
           (persp-add-buffer shared (get-current-persp) nil nil)
           (setq first-kill (kill-buffer shared))
           (let ((after-first
                  (list
                   :return first-kill
                   :live (buffer-live-p shared)
                   :deployment
                   (and (persp-contain-buffer-p
                         shared (persp-get-by-name "deployment")) t)
                   :operations
                   (and (persp-contain-buffer-p
                         shared (persp-get-by-name "operations")) t))))
             (persp-switch "deployment")
             (setq second-kill (kill-buffer shared))
             (list
              :after-first after-first
              :after-second
              (list :return second-kill
                    :live (buffer-live-p shared)
                    :deployment-buffers
                    (neomacs-persp-test-buffer-names
                     (persp-get-by-name "deployment"))
                    :operations-buffers
                    (neomacs-persp-test-buffer-names
                     (persp-get-by-name "operations"))))))
       (persp-deactivate-kill-buffer-advice)))))
"####;
    let expected = expect![
        "OK (:after-first (:return nil :live t :deployment t :operations nil) :after-second (:return t :live nil :deployment-buffers nil :operations-buffers nil))"
    ];
    ParityBatchCase::value(
        "shared_buffer_kill_removes_membership_before_destroying_the_last_owner",
        elisp_form,
        expected,
    )
}

#[test]
fn persp_mode_package_batch() {
    let cases = vec![
        release_workspace_lifecycle_preserves_membership_parameters_and_hooks(),
        switching_workspaces_round_trips_real_window_layout_buffers_and_points(),
        save_and_load_restores_file_buffers_metadata_and_hidden_workspace(),
        auto_workspace_tracks_matching_files_and_hides_after_last_buffer_leaves(),
        shared_buffer_kill_removes_membership_before_destroying_the_last_owner(),
    ];
    assert_oracle_batch_cases(
        persp_mode_oracle(),
        "persp-mode-package-batch",
        "persp-mode",
        &cases,
    );
}
