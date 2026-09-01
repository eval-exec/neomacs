use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DIFF_HL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'diff-hl)

(defun neomacs-diff-hl-test-root (name)
  "Create a deterministic repository sandbox for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "diff-hl-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-diff-hl-test-git (root &rest arguments)
  "Run Git with ARGUMENTS in ROOT and return trimmed output."
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (eq status 0)
          (error "git %S failed (%S): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-diff-hl-test-write (path contents)
  "Write CONTENTS to PATH with stable Unix encoding."
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-diff-hl-test-commit (root message)
  "Commit all changes in ROOT deterministically with MESSAGE."
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" "Parity Bot")
    (setenv "GIT_AUTHOR_EMAIL" "parity@example.test")
    (setenv "GIT_COMMITTER_NAME" "Parity Bot")
    (setenv "GIT_COMMITTER_EMAIL" "parity@example.test")
    (setenv "GIT_AUTHOR_DATE" "2024-01-02T03:04:05+0000")
    (setenv "GIT_COMMITTER_DATE" "2024-01-02T03:04:05+0000")
    (neomacs-diff-hl-test-git root "add" "--all")
    (neomacs-diff-hl-test-git
     root "commit" "--quiet" "--no-gpg-sign" "--message" message)
    (neomacs-diff-hl-test-git root "rev-parse" "HEAD")))

(defun neomacs-diff-hl-test-with-repository (name initial-content function)
  "Call FUNCTION with a committed repository, ROOT, and visited FILE."
  (let* ((root (neomacs-diff-hl-test-root name))
         (file (expand-file-name "service.conf" root))
         buffer)
    (unwind-protect
        (progn
          (neomacs-diff-hl-test-git root "init" "--quiet" "--initial-branch=main")
          (neomacs-diff-hl-test-git root "config" "core.hooksPath" "/dev/null")
          (neomacs-diff-hl-test-write file initial-content)
          (neomacs-diff-hl-test-commit root "Initial service configuration")
          (setq buffer (find-file-noselect file))
          (with-current-buffer buffer
            (set-window-buffer (selected-window) buffer)
            (vc-refresh-state)
            (funcall function root file)))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil)
          (when (bound-and-true-p diff-hl-mode)
            (diff-hl-mode -1)))
        (kill-buffer buffer))
      (when (file-directory-p root)
        (delete-directory root t)))))

(defun neomacs-diff-hl-test-highlight (layer overlay type shape)
  "Record stable LAYER, TYPE, and SHAPE metadata on OVERLAY."
  (overlay-put overlay 'neomacs-diff-hl-layer layer)
  (overlay-put overlay 'neomacs-diff-hl-type type)
  (overlay-put overlay 'neomacs-diff-hl-shape shape))

(defun neomacs-diff-hl-test-install-highlighters ()
  "Install deterministic highlighters for working and reference changes."
  (setq-local
   diff-hl-highlight-function
   (lambda (overlay type shape)
     (neomacs-diff-hl-test-highlight 'working overlay type shape)))
  (setq-local
   diff-hl-highlight-reference-function
   (lambda (overlay type shape)
     (neomacs-diff-hl-test-highlight 'reference overlay type shape))))

(defun neomacs-diff-hl-test-overlays ()
  "Return stable line and hunk overlay metadata in buffer order."
  (let ((overlays (append (car (overlay-lists)) (cdr (overlay-lists)))))
    (list
     :markers
     (mapcar
      (lambda (overlay)
        (list (line-number-at-pos (overlay-start overlay))
              (overlay-get overlay 'neomacs-diff-hl-layer)
              (overlay-get overlay 'neomacs-diff-hl-type)
              (overlay-get overlay 'neomacs-diff-hl-shape)))
      (sort
       (cl-remove-if-not
        (lambda (overlay)
          (overlay-get overlay 'neomacs-diff-hl-layer))
        (copy-sequence overlays))
       (lambda (left right)
         (< (overlay-start left) (overlay-start right)))))
     :hunks
     (mapcar
      (lambda (overlay)
        (list (line-number-at-pos (overlay-start overlay))
              (line-number-at-pos (max (overlay-start overlay)
                                       (1- (overlay-end overlay))))
              (overlay-get overlay 'diff-hl-hunk-type)))
      (sort
       (cl-remove-if-not
        (lambda (overlay) (overlay-get overlay 'diff-hl-hunk))
        (copy-sequence overlays))
       (lambda (left right)
         (< (overlay-start left) (overlay-start right))))))))

(defun neomacs-diff-hl-test-lifecycle ()
  "Return the local mode hooks whose installation is user-visible."
  (list
   :mode (not (null diff-hl-mode))
   :after-save (not (null (memq #'diff-hl-update after-save-hook)))
   :after-change (not (null (memq #'diff-hl-edit after-change-functions)))
   :after-revert (not (null (memq #'diff-hl-update-once after-revert-hook)))))
"####;

fn working_tree_hunks_render_and_navigate_real_separated_changes() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-diff-hl-test-with-repository
 "working-hunks"
 "owner=platform\nstatus=draft\nregion=east\nretries=3\nnotify=ops\ndeploy=manual\nfooter=yes\n"
 (lambda (_root file)
   (neomacs-diff-hl-test-write
    file
    "owner=platform\nstatus=ready\nregion=east\nretries=3\ntimeout=30\nnotify=ops\ndeploy=automatic\nfooter=yes\n")
   (revert-buffer t t)
   (neomacs-diff-hl-test-install-highlighters)
   (diff-hl-mode 1)
   (diff-hl-update)
   (goto-char (point-min))
   (let ((rendered (neomacs-diff-hl-test-overlays))
         navigation)
     (condition-case error-data
         (dotimes (_ 4)
           (diff-hl-next-hunk)
           (push (list (line-number-at-pos) (current-column)) navigation))
       (user-error
        (push (list :error (error-message-string error-data)) navigation)))
     (list :rendered rendered
           :navigation (nreverse navigation)
           :lifecycle (neomacs-diff-hl-test-lifecycle)))))
"####;
    let expected = expect![[
        r#"OK (:rendered (:markers ((2 working change single) (5 working insert single) (7 working change single)) :hunks ((2 2 change) (5 5 insert) (7 7 change))) :navigation ((2 0) (5 0) (7 0) (:error "No further hunks found")) :lifecycle (:mode t :after-save t :after-change t :after-revert t))"#
    ]];
    ParityBatchCase::value(
        "working_tree_hunks_render_and_navigate_real_separated_changes",
        elisp_form,
        expected,
    )
}

fn stage_current_hunk_separates_index_and_worktree_changes() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-diff-hl-test-with-repository
 "stage-hunk"
 "channel=stable\nowner=platform\nregion=east\nretries=2\nnotify=ops\n"
 (lambda (root file)
   (neomacs-diff-hl-test-write
    file
    "channel=canary\nowner=platform\nregion=east\nretries=2\nnotify=release-team\n")
   (revert-buffer t t)
   (setq-local diff-hl-show-staged-changes nil)
   (neomacs-diff-hl-test-install-highlighters)
   (diff-hl-mode 1)
   (diff-hl-update)
   (goto-char (point-min))
   (diff-hl-next-hunk)
   (diff-hl-stage-current-hunk)
   (vc-refresh-state)
   (diff-hl-update)
   (list
    :index (neomacs-diff-hl-test-git root "show" ":service.conf")
    :worktree (buffer-string)
    :cached-numstat
    (neomacs-diff-hl-test-git root "diff" "--cached" "--numstat" "--" "service.conf")
    :working-numstat
    (neomacs-diff-hl-test-git root "diff" "--numstat" "--" "service.conf")
    :rendered (neomacs-diff-hl-test-overlays))))
"####;
    let expected = expect![[
        r#"OK (:index "channel=stable\nowner=platform\nregion=east\nretries=2\nnotify=release-team" :worktree "channel=canary\nowner=platform\nregion=east\nretries=2\nnotify=release-team\n" :cached-numstat "1\0111\11service.conf" :working-numstat "1\0111\11service.conf" :rendered (:markers ((1 working change single) (5 reference change single)) :hunks ((1 1 change) (5 5 change))))"#
    ]];
    ParityBatchCase::value(
        "stage_current_hunk_separates_index_and_worktree_changes",
        elisp_form,
        expected,
    )
}

fn revert_hunk_restores_only_the_selected_change_and_saves_it() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-diff-hl-test-with-repository
 "revert-hunk"
 "channel=stable\nowner=platform\nregion=east\nretries=2\nnotify=ops\n"
 (lambda (root file)
   (neomacs-diff-hl-test-write
    file
    "channel=canary\nowner=platform\nregion=east\nretries=2\nnotify=release-team\n")
   (revert-buffer t t)
   (neomacs-diff-hl-test-install-highlighters)
   (diff-hl-mode 1)
   (diff-hl-update)
   (goto-char (point-max))
   (diff-hl-previous-hunk)
   (let ((diff-hl-ask-before-revert-hunk nil)
         (diff-hl-highlight-revert-hunk-function #'ignore))
     (diff-hl-revert-hunk))
   (vc-refresh-state)
   (diff-hl-update)
   (list
    :text (buffer-string)
    :disk (with-temp-buffer
            (insert-file-contents file)
            (buffer-string))
    :modified (buffer-modified-p)
    :numstat (neomacs-diff-hl-test-git
              root "diff" "--numstat" "--" "service.conf")
    :rendered (neomacs-diff-hl-test-overlays))))
"####;
    let expected = expect![[
        r#"OK (:text "channel=canary\nowner=platform\nregion=east\nretries=2\nnotify=ops\n" :disk "channel=canary\nowner=platform\nregion=east\nretries=2\nnotify=ops\n" :modified nil :numstat "1\0111\11service.conf" :rendered (:markers ((1 working change single)) :hunks ((1 1 change))))"#
    ]];
    ParityBatchCase::value(
        "revert_hunk_restores_only_the_selected_change_and_saves_it",
        elisp_form,
        expected,
    )
}

fn flydiff_highlights_unsaved_edits_without_touching_disk() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-diff-hl-test-with-repository
 "flydiff"
 "service=api\nstatus=ready\nregion=east\n"
 (lambda (_root file)
   (require 'diff-hl-flydiff)
   (neomacs-diff-hl-test-install-highlighters)
   (diff-hl-mode 1)
   (diff-hl-flydiff-mode 1)
   (unwind-protect
       (progn
         (goto-char (point-min))
         (search-forward "ready")
         (replace-match "degraded")
         (goto-char (point-max))
         (insert "owner=oncall\n")
         (diff-hl-flydiff-update)
         (list
          :buffer (buffer-string)
          :disk (with-temp-buffer
                  (insert-file-contents file)
                  (buffer-string))
          :modified (buffer-modified-p)
          :flydiff-mode diff-hl-flydiff-mode
          :rendered (neomacs-diff-hl-test-overlays)))
     (diff-hl-flydiff-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:buffer "service=api\nstatus=degraded\nregion=east\nowner=oncall\n" :disk "service=api\nstatus=ready\nregion=east\n" :modified t :flydiff-mode t :rendered (:markers ((2 working change single) (4 working insert single)) :hunks ((2 2 change) (4 4 insert))))"#
    ]];
    ParityBatchCase::value(
        "flydiff_highlights_unsaved_edits_without_touching_disk",
        elisp_form,
        expected,
    )
}

fn reference_revision_distinguishes_committed_and_uncommitted_layers() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-diff-hl-test-with-repository
 "reference"
 "version=1\nstatus=draft\nregion=east\nowner=platform\n"
 (lambda (root file)
   (let ((first (neomacs-diff-hl-test-git root "rev-parse" "HEAD")))
     (neomacs-diff-hl-test-write
      file
      "version=2\nstatus=ready\nregion=east\nowner=platform\n")
     (revert-buffer t t)
     (neomacs-diff-hl-test-commit root "Prepare version two")
     (goto-char (point-max))
     (insert "rollout=canary\n")
     (save-buffer)
     (setq-local diff-hl-reference-revision first)
     (neomacs-diff-hl-test-install-highlighters)
     (diff-hl-mode 1)
     (diff-hl-update)
     (let ((active (list :rendered (neomacs-diff-hl-test-overlays)
                         :lifecycle (neomacs-diff-hl-test-lifecycle))))
       (diff-hl-mode -1)
       (list :active active
             :disabled (list :rendered (neomacs-diff-hl-test-overlays)
                             :lifecycle (neomacs-diff-hl-test-lifecycle)
                             :reference-local
                             (local-variable-p 'diff-hl-reference-revision)))))))
"####;
    let expected = expect![
        "OK (:active (:rendered (:markers ((1 reference change top) (2 reference change bottom) (5 working insert single)) :hunks ((1 2 change) (5 5 insert))) :lifecycle (:mode t :after-save t :after-change t :after-revert t)) :disabled (:rendered (:markers nil :hunks nil) :lifecycle (:mode nil :after-save nil :after-change nil :after-revert nil) :reference-local nil))"
    ];
    ParityBatchCase::value(
        "reference_revision_distinguishes_committed_and_uncommitted_layers",
        elisp_form,
        expected,
    )
}

fn diff_hl_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DIFF_HL_MELPA_PIN, "diff-hl.el")
        .expect("prepare pinned Diff HL source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn diff_hl_practical_workflows_batch() {
    let cases = vec![
        working_tree_hunks_render_and_navigate_real_separated_changes(),
        stage_current_hunk_separates_index_and_worktree_changes(),
        revert_hunk_restores_only_the_selected_change_and_saves_it(),
        flydiff_highlights_unsaved_edits_without_touching_disk(),
        reference_revision_distinguishes_committed_and_uncommitted_layers(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("diff-hl parity batch");
    assert_oracle_batch_cases(diff_hl_oracle(), test_name, "diff-hl parity", &cases);
}
