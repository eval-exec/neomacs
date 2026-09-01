use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_TIMEMACHINE_MELPA_PIN, TRANSIENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const GIT_TIMEMACHINE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GIT_TIMEMACHINE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)

(defun neomacs-gtt-test-root (name)
  "Create a deterministic Git sandbox for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "git-timemachine-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-gtt-test-write (root relative contents)
  "Write CONTENTS to RELATIVE below ROOT and return its path."
  (let ((path (expand-file-name relative root))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-gtt-test-git (root &rest arguments)
  "Run Git with ARGUMENTS in ROOT and return trimmed standard output."
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (eq status 0)
          (error "git %S failed (%S): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-gtt-test-commit (root message author email timestamp)
  "Commit ROOT deterministically and return the resulting full hash."
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" author)
    (setenv "GIT_AUTHOR_EMAIL" email)
    (setenv "GIT_COMMITTER_NAME" author)
    (setenv "GIT_COMMITTER_EMAIL" email)
    (setenv "GIT_AUTHOR_DATE" timestamp)
    (setenv "GIT_COMMITTER_DATE" timestamp)
    (neomacs-gtt-test-git root "add" "--all")
    (neomacs-gtt-test-git
     root "commit" "--quiet" "--no-gpg-sign" "--message" message)
    (neomacs-gtt-test-git root "rev-parse" "HEAD")))

(defun neomacs-gtt-test-fixture (name rename-file)
  "Build a four-commit release runbook repository for NAME.
When RENAME-FILE is non-nil, rename the runbook in the third commit."
  (let* ((root (neomacs-gtt-test-root name))
         (original-relative "docs/release-runbook.txt")
         (current-relative
          (if rename-file "docs/deployment-runbook.txt" original-relative))
         first second third maintenance)
    (neomacs-gtt-test-git root "init" "--quiet" "--initial-branch=main")
    (neomacs-gtt-test-git root "config" "core.hooksPath" "/dev/null")
    (neomacs-gtt-test-write
     root original-relative
     "# Release runbook\nowner: platform\nsteps:\n- validate\n- publish\n")
    (setq first
          (neomacs-gtt-test-commit
           root "Create release runbook" "Alice Example" "alice@example.test"
           "2024-01-02T03:04:05+0000"))
    (neomacs-gtt-test-write
     root original-relative
     "# Release runbook\nowner: platform\nsteps:\n- validate\n- notify stakeholders\n- publish\n")
    (setq second
          (neomacs-gtt-test-commit
           root "Add stakeholder notification" "Bob Example" "bob@example.test"
           "2024-02-03T04:05:06+0000"))
    (when rename-file
      (neomacs-gtt-test-git root "mv" original-relative current-relative))
    (neomacs-gtt-test-write
     root current-relative
     "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n")
    (setq third
          (neomacs-gtt-test-commit
           root "Approve release 42 runbook" "Cara Example" "cara@example.test"
           "2024-03-04T05:06:07+0000"))
    (neomacs-gtt-test-git root "branch" "maintenance")
    (neomacs-gtt-test-git root "checkout" "--quiet" "maintenance")
    (neomacs-gtt-test-write
     root current-relative
     "# Release 42 runbook\nowner: platform\nstatus: maintenance\nsteps:\n- validate\n- notify stakeholders\n- publish\n- rollback on failure\n")
    (setq maintenance
          (neomacs-gtt-test-commit
           root "Document rollback procedure" "Dana Example" "dana@example.test"
           "2024-04-05T06:07:08+0000"))
    (neomacs-gtt-test-git root "checkout" "--quiet" "main")
    (list :root root
          :file (expand-file-name current-relative root)
          :relative current-relative
          :original-relative original-relative
          :first first
          :second second
          :third third
          :maintenance maintenance)))

(defun neomacs-gtt-test-revision-summary (revision)
  "Return stable public metadata from a git-timemachine REVISION."
  (list :hash (nth 0 revision)
        :file (nth 1 revision)
        :index (nth 2 revision)
        :date (nth 4 revision)
        :subject (nth 5 revision)
        :author (nth 6 revision)))

(defun neomacs-gtt-test-mode-line-summary ()
  "Return the stable parts of the historical buffer identification."
  (let ((identification mode-line-buffer-identification))
    (list :abbreviation (substring-no-properties (nth 2 identification))
          :file (nth 4 identification)
          :position
          (let ((tail (car (last identification))))
            (and (string-match "\\`(\\([0-9-]+/[0-9]+\\) " tail)
                 (match-string 1 tail))))))

(defun neomacs-gtt-test-view ()
  "Describe the complete stable state of the current historical buffer."
  (list :buffer (buffer-name)
        :file (file-name-nondirectory buffer-file-name)
        :text (buffer-string)
        :read-only buffer-read-only
        :modified (buffer-modified-p)
        :major-mode major-mode
        :minor-mode git-timemachine-mode
        :revision (neomacs-gtt-test-revision-summary git-timemachine-revision)
        :point (list (point) (line-number-at-pos) (current-column))
        :mode-line (neomacs-gtt-test-mode-line-summary)
        :bindings
        (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                '("p" "n" "g" "i" "w" "W" "q"))))

(defun neomacs-gtt-test-compact-view ()
  "Describe revision, text, and cursor state during history navigation."
  (list :hash (car git-timemachine-revision)
        :file (nth 1 git-timemachine-revision)
        :index (nth 2 git-timemachine-revision)
        :point (list (point) (line-number-at-pos) (current-column))
        :text (buffer-string)))

(defun neomacs-gtt-test-run (name rename-file function)
  "Run FUNCTION with a deterministic repository and clean editor state."
  (let ((process-environment (copy-sequence process-environment))
        fixture result)
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (setenv "GIT_DEFAULT_HASH" "sha1")
    (condition-case error
        (setq fixture (neomacs-gtt-test-fixture name rename-file))
      (error
       (error "fixture setup failed with sandbox %S: %S"
              (getenv "NEOMACS_TEST_SANDBOX_ROOT") error)))
    (unwind-protect
        (setq result
              (save-window-excursion
                (save-current-buffer
                  (condition-case error
                      (funcall function fixture)
                    (error
                     (error "fixture workflow failed for %S: %S"
                            (plist-get fixture :file) error))))))
      (when fixture
        (dolist (buffer (buffer-list))
          (when (and (buffer-file-name buffer)
                     (string-prefix-p
                      (plist-get fixture :root) (buffer-file-name buffer)))
            (with-current-buffer buffer
              (set-buffer-modified-p nil))
            (ignore-errors (kill-buffer buffer))))
        (when (file-exists-p (plist-get fixture :root))
          (delete-directory (plist-get fixture :root) t))))
    result))
"####;

fn git_timemachine_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_TIMEMACHINE_MELPA_PIN, "git-timemachine.el")
        .expect("prepare revision-pinned git-timemachine source below ./tmp")
        .with_melpa_dependency(TRANSIENT_MELPA_PIN)
        .expect("prepare revision-pinned Transient dependency below ./tmp")
        .with_prelude(GIT_TIMEMACHINE_TEST_PRELUDE)
        .with_timeout(GIT_TIMEMACHINE_TEST_TIMEOUT)
}

fn entering_and_quitting_preserves_the_unsaved_worktree_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "enter-and-quit" nil
 (lambda (fixture)
   (let* ((file (plist-get fixture :file))
          (source (find-file-noselect file))
          opened revisions source-before after)
     (switch-to-buffer source)
     (goto-char (point-max))
     (insert "- local verification only\n")
     (setq source-before
           (list :buffer (buffer-name)
                 :text (buffer-string)
                 :modified (buffer-modified-p)))
     (let ((git-timemachine-show-minibuffer-details nil)
           (git-timemachine-abbreviation-length 10))
       (git-timemachine)
       (set-buffer (window-buffer (selected-window)))
       (setq opened (neomacs-gtt-test-view))
       (setq revisions
             (mapcar #'neomacs-gtt-test-revision-summary
                     (git-timemachine--revisions)))
       (git-timemachine-quit))
     (setq after
           (list :same-source (eq (current-buffer) source)
                 :buffer (buffer-name)
                 :text (buffer-string)
                 :modified (buffer-modified-p)
                 :timemachine-live
                 (and (get-buffer "timemachine:release-runbook.txt") t)))
     (list :fixture-hashes
           (mapcar (lambda (key) (plist-get fixture key))
                   '(:first :second :third :maintenance))
           :source-before source-before
           :opened opened
           :revisions revisions
           :after after
           :worktree-status
           (neomacs-gtt-test-git (plist-get fixture :root) "status" "--short")))))
"####;
    let expected = expect![[
        r##"OK (:fixture-hashes ("0be97085a13f9d113d8c747d655adc4a7f2a9e8b" "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" "799136aec723aea9b8f7f9c242cd55f4d5375982" "07be30c10b01a7e2e2ff8ebd884663cc9a5d6a68") :source-before (:buffer "release-runbook.txt" :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n- local verification only\n" :modified t) :opened (:buffer "timemachine:release-runbook.txt" :file "release-runbook.txt" :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n" :read-only t :modified nil :major-mode text-mode :minor-mode t :revision (:hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :date "Mon Mar 4 05:06:07 2024 +0000" :subject "Approve release 42 runbook" :author "Cara Example") :point (102 8 0) :mode-line (:abbreviation "799136aec7" :file "docs/release-runbook.txt" :position "3/3") :bindings (("p" . git-timemachine-show-previous-revision) ("n" . git-timemachine-show-next-revision) ("g" . git-timemachine-show-nth-revision) ("i" . git-timemachine-show-revision-introducing) ("w" . git-timemachine-kill-abbreviated-revision) ("W" . git-timemachine-kill-revision) ("q" . git-timemachine-quit))) :revisions ((:hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :date "Mon Mar 4 05:06:07 2024 +0000" :subject "Approve release 42 runbook" :author "Cara Example") (:hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :file "docs/release-runbook.txt" :index 2 :date "Sat Feb 3 04:05:06 2024 +0000" :subject "Add stakeholder notification" :author "Bob Example") (:hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index 1 :date "Tue Jan 2 03:04:05 2024 +0000" :subject "Create release runbook" :author "Alice Example")) :after (:same-source t :buffer "release-runbook.txt" :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n- local verification only\n" :modified t :timemachine-live nil) :worktree-status "?? docs/.#release-runbook.txt")"##
    ]];
    ParityBatchCase::value(
        "entering_and_quitting_preserves_the_unsaved_worktree_buffer",
        elisp_form,
        expected,
    )
}

fn previous_and_next_navigation_tracks_the_same_logical_line() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "line-navigation" nil
 (lambda (fixture)
   (let ((source (find-file-noselect (plist-get fixture :file)))
         states)
     (switch-to-buffer source)
     (goto-char (point-min))
     (search-forward "- publish")
     (beginning-of-line)
     (let ((git-timemachine-show-minibuffer-details nil))
       (git-timemachine)
       (set-buffer (window-buffer (selected-window)))
       (push (cons 'latest (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-previous-revision)
       (push (cons 'previous (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-previous-revision)
       (push (cons 'oldest (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-previous-revision)
       (push (cons 'oldest-boundary (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-next-revision)
       (push (cons 'forward-one (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-next-revision)
       (push (cons 'forward-latest (neomacs-gtt-test-compact-view)) states)
       (git-timemachine-show-next-revision)
       (push (cons 'latest-boundary (neomacs-gtt-test-compact-view)) states))
     (nreverse states))))
"####;
    let expected = expect![[
        r##"OK ((latest :hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :point (92 7 0) :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n") (previous :hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :file "docs/release-runbook.txt" :index 2 :point (85 7 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- notify stakeholders\n- publish\n") (oldest :hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index 1 :point (63 6 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- publish\n") (oldest-boundary :hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index 1 :point (63 6 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- publish\n") (forward-one :hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :file "docs/release-runbook.txt" :index 2 :point (75 6 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- notify stakeholders\n- publish\n") (forward-latest :hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :point (70 6 0) :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n") (latest-boundary :hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :point (70 6 0) :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n"))"##
    ]];
    ParityBatchCase::value(
        "previous_and_next_navigation_tracks_the_same_logical_line",
        elisp_form,
        expected,
    )
}

fn renamed_file_history_supports_nth_fuzzy_and_nearest_selection() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "renamed-history" t
 (lambda (fixture)
   (let ((source (find-file-noselect (plist-get fixture :file)))
         prompt candidates oldest fuzzy nearest)
     (switch-to-buffer source)
     (let ((git-timemachine-show-minibuffer-details nil))
       (git-timemachine)
       (set-buffer (window-buffer (selected-window)))
       (git-timemachine-show-nth-revision 1)
       (setq oldest (neomacs-gtt-test-compact-view))
       (cl-letf (((symbol-function 'completing-read)
                  (lambda (actual-prompt collection &rest _arguments)
                    (setq prompt actual-prompt
                          candidates (copy-sequence collection))
                    "Add stakeholder notification")))
         (git-timemachine-show-revision-fuzzy))
       (setq fuzzy (neomacs-gtt-test-compact-view))
       (git-timemachine-show-nearest-revision (plist-get fixture :third))
       (setq nearest (neomacs-gtt-test-compact-view)))
     (list :hashes (mapcar (lambda (key) (plist-get fixture key))
                           '(:first :second :third))
           :prompt prompt
           :candidates candidates
           :oldest oldest
           :fuzzy fuzzy
           :nearest nearest))))
"####;
    let expected = expect![[
        r##"OK (:hashes ("0be97085a13f9d113d8c747d655adc4a7f2a9e8b" "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" "446f9c90b5ddc268adcde68738491782a0b13ec2") :prompt "Commit message: " :candidates ("Approve release 42 runbook" "Add stakeholder notification" "Create release runbook") :oldest (:hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index 1 :point (1 1 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- publish\n") :fuzzy (:hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :file "docs/release-runbook.txt" :index 2 :point (1 1 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- notify stakeholders\n- publish\n") :nearest (:hash "446f9c90b5ddc268adcde68738491782a0b13ec2" :file "docs/deployment-runbook.txt" :index -1 :point (1 1 0) :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n"))"##
    ]];
    ParityBatchCase::value(
        "renamed_file_history_supports_nth_fuzzy_and_nearest_selection",
        elisp_form,
        expected,
    )
}

fn branch_history_can_be_inspected_without_checking_out_the_branch() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "branch-history" nil
 (lambda (fixture)
   (let* ((root (plist-get fixture :root))
          (file (plist-get fixture :file))
          (source (find-file-noselect file))
          branch-view)
     (switch-to-buffer source)
     (let ((git-timemachine-show-minibuffer-details nil))
       (git-timemachine-switch-branch "maintenance")
       (set-buffer (window-buffer (selected-window)))
       (setq branch-view (neomacs-gtt-test-view))
       (git-timemachine-quit))
     (list :main-hash (plist-get fixture :third)
           :maintenance-hash (plist-get fixture :maintenance)
           :branch-view branch-view
           :checked-out-branch
           (neomacs-gtt-test-git root "branch" "--show-current")
           :worktree-text
           (with-temp-buffer
             (insert-file-contents file)
             (buffer-string))
           :source-text (with-current-buffer source (buffer-string))))))
"####;
    let expected = expect![[
        r##"OK (:main-hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :maintenance-hash "07be30c10b01a7e2e2ff8ebd884663cc9a5d6a68" :branch-view (:buffer "timemachine:release-runbook.txt" :file "release-runbook.txt" :text "# Release 42 runbook\nowner: platform\nstatus: maintenance\nsteps:\n- validate\n- notify stakeholders\n- publish\n- rollback on failure\n" :read-only t :modified nil :major-mode text-mode :minor-mode t :revision (:hash "07be30c10b01a7e2e2ff8ebd884663cc9a5d6a68" :file "docs/release-runbook.txt" :index 4 :date "Fri Apr 5 06:07:08 2024 +0000" :subject "Document rollback procedure" :author "Dana Example") :point (1 1 0) :mode-line (:abbreviation "07be30c10b01" :file "docs/release-runbook.txt" :position "4/4") :bindings (("p" . git-timemachine-show-previous-revision) ("n" . git-timemachine-show-next-revision) ("g" . git-timemachine-show-nth-revision) ("i" . git-timemachine-show-revision-introducing) ("w" . git-timemachine-kill-abbreviated-revision) ("W" . git-timemachine-kill-revision) ("q" . git-timemachine-quit))) :checked-out-branch "main" :worktree-text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n" :source-text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n")"##
    ]];
    ParityBatchCase::value(
        "branch_history_can_be_inspected_without_checking_out_the_branch",
        elisp_form,
        expected,
    )
}

fn introducing_revision_and_hash_copy_workflow_updates_the_kill_ring() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "introducing-and-copy" nil
 (lambda (fixture)
   (let ((source (find-file-noselect (plist-get fixture :file)))
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         (interprogram-cut-function nil)
         introduced messages)
     (switch-to-buffer source)
     (let ((git-timemachine-show-minibuffer-details nil)
           (git-timemachine-abbreviation-length 10))
       (git-timemachine)
       (set-buffer (window-buffer (selected-window)))
       (let ((original-message (symbol-function 'message)))
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text
                             (apply #'format-message
                                    format-string arguments)))
                        (push text messages)
                        (funcall original-message "%s" text)))))
           (git-timemachine-show-revision-introducing "notify stakeholders")
           (setq introduced (neomacs-gtt-test-compact-view))
           (git-timemachine-kill-abbreviated-revision)
           (git-timemachine-kill-revision)
           (git-timemachine-show-revision-introducing "security approval"))))
     (list :expected-introducing-hash (plist-get fixture :second)
           :introduced introduced
           :kill-ring (mapcar #'substring-no-properties kill-ring)
           :yank-pointer-front (eq kill-ring-yank-pointer kill-ring)
           :messages (nreverse messages)))))
"####;
    let expected = expect![[
        r##"OK (:expected-introducing-hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :introduced (:hash "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" :file "docs/release-runbook.txt" :index 2 :point (1 1 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- notify stakeholders\n- publish\n") :kill-ring ("51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" "51e8c0d5c1") :yank-pointer-front t :messages ("51e8c0d5c1" "51e8c0d5c12e879e7846ba1ffabcc5aab8018d21" "Buffer does not contain: security approval"))"##
    ]];
    ParityBatchCase::value(
        "introducing_revision_and_hash_copy_workflow_updates_the_kill_ring",
        elisp_form,
        expected,
    )
}

fn validation_and_revision_boundaries_report_actionable_failures() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gtt-test-run
 "validation-and-boundaries" nil
 (lambda (fixture)
   (let* ((root (plist-get fixture :root))
          (file (plist-get fixture :file))
          (untracked-file
           (neomacs-gtt-test-write root "notes/untracked.txt" "draft\n"))
          no-file untracked messages before after-nearest)
     (setq no-file
           (with-temp-buffer
             (condition-case error
                 (progn (git-timemachine) :unexpected-success)
               (error (cons (car error) (cdr error))))))
     (let ((buffer (find-file-noselect untracked-file)))
       (setq untracked
             (with-current-buffer buffer
               (condition-case error
                   (progn (git-timemachine) :unexpected-success)
                 (error (cons (car error) (cdr error)))))))
     (switch-to-buffer (find-file-noselect file))
     (let ((git-timemachine-show-minibuffer-details nil))
       (git-timemachine)
       (set-buffer (window-buffer (selected-window)))
       (setq before (neomacs-gtt-test-compact-view))
       (let ((original-message (symbol-function 'message)))
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text
                             (apply #'format-message
                                    format-string arguments)))
                        (push text messages)
                        (funcall original-message "%s" text)))))
           (git-timemachine-show-nth-revision 99)
           (git-timemachine-show-nearest-revision (plist-get fixture :first))
           (setq after-nearest (neomacs-gtt-test-compact-view))
           (git-timemachine-show-nearest-revision "deadbeef"))))
     (list :no-file no-file
           :untracked untracked
           :before before
           :after-nearest after-nearest
           :messages (nreverse messages)
           :revision-after-invalid
           (neomacs-gtt-test-revision-summary git-timemachine-revision)))))
"####;
    let expected = expect![[
        r##"OK (:no-file (error "This buffer is not visiting a file") :untracked (error "This file is not git tracked") :before (:hash "799136aec723aea9b8f7f9c242cd55f4d5375982" :file "docs/release-runbook.txt" :index 3 :point (1 1 0) :text "# Release 42 runbook\nowner: platform\nstatus: ready\nsteps:\n- validate\n- notify stakeholders\n- publish\n") :after-nearest (:hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index -1 :point (1 1 0) :text "# Release runbook\nowner: platform\nsteps:\n- validate\n- publish\n") :messages ("Only 3 revisions exist." "No such commit deadbeef") :revision-after-invalid (:hash "0be97085a13f9d113d8c747d655adc4a7f2a9e8b" :file "docs/release-runbook.txt" :index -1 :date "Tue Jan 2 03:04:05 2024 +0000" :subject "Create release runbook" :author "Alice Example"))"##
    ]];
    ParityBatchCase::value(
        "validation_and_revision_boundaries_report_actionable_failures",
        elisp_form,
        expected,
    )
}

#[test]
fn git_timemachine_package_batch() {
    let cases = vec![
        entering_and_quitting_preserves_the_unsaved_worktree_buffer(),
        previous_and_next_navigation_tracks_the_same_logical_line(),
        renamed_file_history_supports_nth_fuzzy_and_nearest_selection(),
        branch_history_can_be_inspected_without_checking_out_the_branch(),
        introducing_revision_and_hash_copy_workflow_updates_the_kill_ring(),
        validation_and_revision_boundaries_report_actionable_failures(),
    ];
    assert_oracle_batch_cases(
        git_timemachine_oracle(),
        "git-timemachine-package-batch",
        "git-timemachine",
        &cases,
    );
}
