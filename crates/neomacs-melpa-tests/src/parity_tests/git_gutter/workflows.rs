use expect_test::expect;

use super::ParityBatchCase;

fn local_mode_detects_real_git_hunks_and_renders_exact_signs() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_mode_detects_real_git_hunks_and_renders_exact_signs",
        r####"
(neomacs-git-gutter-test-run
 "detect"
 (lambda (_fixture)
   (git-gutter-mode 1)
   (neomacs-git-gutter-test-wait)
   (list :mode git-gutter-mode
         :vcs git-gutter:vcs-type
         :enabled git-gutter:enabled
         :hunks (neomacs-git-gutter-test-hunks)
         :count (git-gutter:buffer-hunks)
         :statistic (git-gutter:statistic)
         :overlays (neomacs-git-gutter-test-overlays)
         :margin (window-margins))))
"####,
        expect![[
            r#"OK (:mode t :vcs git :enabled t :hunks ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 9 :end 9 :content "@@ -9 +9,0 @@ notes:\n-- legacy\n")) :count 3 :statistic (1 . 1) :overlays ((:line 2 :rendered "=" :faces (git-gutter:modified)) (:line 6 :rendered "+" :faces (git-gutter:added)) (:line 9 :rendered "-" :faces (git-gutter:deleted))) :margin (1))"#
        ]],
    )
}

fn navigation_marking_and_popup_follow_hunks_with_wraparound() -> ParityBatchCase {
    ParityBatchCase::value(
        "navigation_marking_and_popup_follow_hunks_with_wraparound",
        r####"
(neomacs-git-gutter-test-run
 "navigate"
 (lambda (_fixture)
   (git-gutter-mode 1)
   (neomacs-git-gutter-test-wait)
   (goto-char (point-min))
   (let (positions marked popup)
     (dotimes (_ 4)
       (git-gutter:next-hunk 1)
       (push (list (line-number-at-pos) (thing-at-point 'line t)) positions))
     (setq positions (nreverse positions))
     (git-gutter:previous-hunk 1)
     (git-gutter:mark-hunk)
     (setq marked
           (list :point-line (line-number-at-pos)
                 :mark-line (line-number-at-pos (mark))
                 :text (buffer-substring-no-properties
                        (region-beginning) (region-end))))
     (git-gutter:end-of-hunk)
     (git-gutter:popup-hunk)
     (setq popup
           (with-current-buffer git-gutter:popup-buffer
             (list :text (buffer-substring-no-properties (point-min) (point-max))
                   :mode major-mode :view view-mode)))
     (list :positions positions
           :after-previous (line-number-at-pos)
           :marked marked
           :popup popup))))
"####,
        expect![[
            r#"OK (:positions ((2 "owner: delivery\n") (6 "- notify\n") (9 "notes:\n") (2 "owner: delivery\n")) :after-previous 9 :marked (:point-line 9 :mark-line 10 :text "notes:\n") :popup (:text "@@ -9 +9,0 @@ notes:\n-- legacy\n\n" :mode diff-mode :view t))"#
        ]],
    )
}

fn staging_one_hunk_updates_the_index_and_leaves_other_changes_unstaged() -> ParityBatchCase {
    ParityBatchCase::value(
        "staging_one_hunk_updates_the_index_and_leaves_other_changes_unstaged",
        r####"
(neomacs-git-gutter-test-run
 "stage"
 (lambda (fixture)
   (let ((git-gutter:ask-p nil))
     (git-gutter-mode 1)
     (neomacs-git-gutter-test-wait)
     (goto-char (point-min))
     (forward-line 1)
     (git-gutter:stage-hunk)
     (neomacs-git-gutter-test-wait)
     (list :cached (neomacs-git-gutter-test-git
                    (plist-get fixture :root) "diff" "--cached" "--unified=0")
           :unstaged (neomacs-git-gutter-test-git
                      (plist-get fixture :root) "diff" "--unified=0")
           :hunks (neomacs-git-gutter-test-hunks)
           :statistic (git-gutter:statistic)))))
"####,
        expect![[
            r#"OK (:cached "diff --git a/release.txt b/release.txt\nindex cedc2e6..db5e88c 100644\n--- a/release.txt\n+++ b/release.txt\n@@ -2 +2 @@\n-owner: platform\n+owner: delivery" :unstaged "diff --git a/release.txt b/release.txt\nindex db5e88c..1691f16 100644\n--- a/release.txt\n+++ b/release.txt\n@@ -5,0 +6 @@ steps:\n+- notify\n@@ -9 +9,0 @@ notes:\n-- legacy" :hunks ((:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 9 :end 9 :content "@@ -9 +9,0 @@ notes:\n-- legacy\n")) :statistic (0 . 0))"#
        ]],
    )
}

fn reverting_an_added_hunk_saves_the_file_and_preserves_other_edits() -> ParityBatchCase {
    ParityBatchCase::value(
        "reverting_an_added_hunk_saves_the_file_and_preserves_other_edits",
        r####"
(neomacs-git-gutter-test-run
 "revert"
 (lambda (fixture)
   (let ((git-gutter:ask-p nil))
     (git-gutter-mode 1)
     (neomacs-git-gutter-test-wait)
     (goto-char (point-min))
     (forward-line 5)
     (git-gutter:revert-hunk)
     (neomacs-git-gutter-test-wait)
     (list :text (buffer-substring-no-properties (point-min) (point-max))
           :disk (with-temp-buffer
                   (insert-file-contents (plist-get fixture :file))
                   (buffer-string))
           :modified (buffer-modified-p)
           :hunks (neomacs-git-gutter-test-hunks)
           :status (neomacs-git-gutter-test-git
                    (plist-get fixture :root) "status" "--short")))))
"####,
        expect![[
            r##"OK (:text "# Release\nowner: delivery\n\nsteps:\n- validate\n- publish\n\nnotes:\nend\n" :disk "# Release\nowner: delivery\n\nsteps:\n- validate\n- publish\n\nnotes:\nend\n" :modified nil :hunks ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type deleted :start 8 :end 8 :content "@@ -9 +8,0 @@ notes:\n-- legacy\n")) :status " M release.txt")"##
        ]],
    )
}

fn custom_signs_separator_and_unchanged_policy_render_every_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_signs_separator_and_unchanged_policy_render_every_line",
        r####"
(neomacs-git-gutter-test-run
 "custom"
 (lambda (_fixture)
   (let ((git-gutter:modified-sign "M")
         (git-gutter:added-sign "A")
         (git-gutter:deleted-sign "D")
         (git-gutter:unchanged-sign ".")
         (git-gutter:separator-sign "|"))
     (git-gutter-mode 1)
     (neomacs-git-gutter-test-wait)
     (list :width (git-gutter:window-margin)
           :overlays (neomacs-git-gutter-test-overlays)
           :margin (window-margins)))))
"####,
        expect![[
            r#"OK (:width 2 :overlays ((:line 1 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 2 :rendered "M|" :faces (git-gutter:modified git-gutter:separator)) (:line 3 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 4 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 5 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 6 :rendered "A|" :faces (git-gutter:added git-gutter:separator)) (:line 7 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 8 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator)) (:line 9 :rendered "D|" :faces (git-gutter:deleted git-gutter:separator)) (:line 10 :rendered ".|" :faces (git-gutter:unchanged git-gutter:separator))) :margin (2))"#
        ]],
    )
}

fn mode_refuses_non_repository_buffers_and_disabled_global_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_refuses_non_repository_buffers_and_disabled_global_modes",
        r####"
(let ((outside (file-name-as-directory
                (expand-file-name "git-gutter-outside"
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
  (when (file-exists-p outside) (delete-directory outside t))
  (make-directory outside t)
  (unwind-protect
      (list
       :unvisited
       (with-temp-buffer
         (git-gutter-mode 1)
         (list :mode git-gutter-mode :vcs git-gutter:vcs-type))
       :outside
       (let* ((file (expand-file-name "plain.txt" outside))
              (buffer (progn (with-temp-file file (insert "plain\n"))
                             (find-file-noselect file))))
         (unwind-protect
             (with-current-buffer buffer
               (let ((default-directory outside))
                 (git-gutter-mode 1)
                 (list :mode git-gutter-mode :vcs git-gutter:vcs-type)))
           (kill-buffer buffer)))
       :disabled
       (neomacs-git-gutter-test-run
        "disabled"
        (lambda (_fixture)
          (let ((git-gutter:disabled-modes '(text-mode)))
            (global-git-gutter-mode 1)
            (prog1 (list :global global-git-gutter-mode
                         :local git-gutter-mode)
              (global-git-gutter-mode -1))))))
    (delete-directory outside t)))
"####,
        expect![
            "OK (:unvisited (:mode nil :vcs nil) :outside (:mode nil :vcs nil) :disabled (:global t :local nil))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        local_mode_detects_real_git_hunks_and_renders_exact_signs(),
        navigation_marking_and_popup_follow_hunks_with_wraparound(),
        staging_one_hunk_updates_the_index_and_leaves_other_changes_unstaged(),
        reverting_an_added_hunk_saves_the_file_and_preserves_other_edits(),
        custom_signs_separator_and_unchanged_policy_render_every_line(),
        mode_refuses_non_repository_buffers_and_disabled_global_modes(),
    ]
}
