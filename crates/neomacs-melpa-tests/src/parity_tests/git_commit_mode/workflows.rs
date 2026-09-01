use expect_test::expect;

use super::ParityBatchCase;

const TEMPLATE: &str = r####"A summary [skip ci] that is intentionally much longer than fifty columns Ω
second line is occupied

Body paragraph.

Signed-off-by: Existing Person <existing@example.test>
; On branch main
; Changes to be committed:
;	modified:   src/main.rs
;
diff --git a/src/main.rs b/src/main.rs
index 3367afd..7f15e7a 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-old
+new
"####;

fn activation_and_real_template() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_activates_and_renders_a_real_commit_template",
        format!(
            r####"
(git363-test-run
 "activation"
 (lambda (_world)
   (let* ((repo (git363-test-make-repo "activation repo 界/" ";"))
          (path (git363-test-write
                 "activation repo 界/.git/COMMIT_EDITMSG" {template:?}))
          (visit (git363-test-visit path t)))
     (list :provenance (git363-test-provenance)
           :visit (plist-get visit :usage)
           :activation (git363-test-activation-state)
           :buffer (git363-test-buffer-state)
           :ring (git363-test-ring-state)
           :processes (git363-test-process-state)
           :properties (git363-test-property-runs)
           :repo (git363-test-normalize-string repo)))))
"####,
            template = TEMPLATE
        ),
        expect![[
            r#"OK (:result (:provenance (:library "git-commit-mode.el" :source-sha256 "4c7eb92813c4c001b8776cef1edc9f491087b0cee8ee43fe8b989a1135b20dab" :installed-sha256 "11d673f5934a2d3d74955b5eee4d7dc1a076ef6ceb627237dac0890a2948597d" :feature t :modern-feature nil :global t :find-registration 1 :font-registration 1 :major-default text-mode :setup-default (git-commit-save-message git-commit-setup-changelog-support git-commit-turn-on-auto-fill git-commit-propertize-diff with-editor-usage-message) :finish-default (git-commit-check-style-conventions) :summary-default 50 :fill-default 72 :headers-default ("Signed-off-by" "Acked-by" "Cc" "Suggested-by" "Reported-by" "Tested-by" "Reviewed-by")) :visit (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 1 :buffers-live nil :cancelled 1) :undo-boundary (:created 1 :pending-for-cleanup 1) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :activation (:major text-mode :git-commit t :with-editor t :comment ";" :comment-skip "^;+[ \11]*" :fill 72 :auto-fill do-auto-fill :finish-hooks (git-commit-finish-query-functions t) :cancel-hooks (git-commit-save-message t) :cancel-message git-commit-cancel-message :kill-query (with-editor-kill-buffer-noop t) :keys (("C-c C-s" . git-commit-signoff) ("C-c C-a" . git-commit-ack) ("C-c C-o" . git-commit-cc) ("C-c M-s" . git-commit-save-message) ("M-p" . git-commit-prev-message) ("M-n" . git-commit-next-message) ("C-c C-c" . with-editor-finish) ("C-c C-k" . with-editor-cancel))) :buffer (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "A summary [skip ci] that is intentionally much longer than fifty columns Ω\nsecond line is occupied\n\nBody paragraph.\n\nSigned-off-by: Existing Person <existing@example.test>\n; On branch main\n; Changes to be committed:\n;\11modified:   src/main.rs\n;\ndiff --git a/src/main.rs b/src/main.rs\nindex 3367afd..7f15e7a 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n" :point 1 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 1 :elements ("A summary [skip ci] that is intentionally much longer than fifty columns Ω\nsecond line is occupied\n\nBody paragraph.\n\nSigned-off-by: Existing Person <existing@example.test>\ndiff --git a/src/main.rs b/src/main.rs\nindex 3367afd..7f15e7a 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n") :index 0) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/activation repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/activation repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/activation repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/activation repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff ((:program "[DIFF]" :argv ("-ad" :old :new) :cwd "[ROOT]/activation repo 界/.git/" :status 1 :buffer :temporary :streams :combined :combined-output "2,4c2,4\n< old\n< old\n< old\n---\n> new\n> new\n> new\n" :headers :normal-diff-emits-no-paths :old (:sha256 "392ffc72eae5aec5f3c0dfa90dacdddfbd72bb89027e61d44d9d90535453c895" :text "-\nold\nold\nold\n \n") :new (:sha256 "6c8571a35f056315d021b6fdba01b9c9729cfa863551a7dc2c20f748739d5f2b" :text "-\nnew\nnew\nnew\n \n") :temp-clean t))) :properties ((1 0 1 10 git-commit-summary nil "A summary ") (1 10 1 19 git-commit-note nil "[skip ci]") (1 19 1 50 git-commit-summary nil " that is intentionally much lon") (1 50 1 74 git-commit-overlong-summary nil "ger than fifty columns Ω") (2 0 2 23 git-commit-nonempty-second-line nil "second line is occupied") (6 0 6 14 git-commit-known-pseudo-header nil "Signed-off-by:") (6 14 6 54 git-commit-pseudo-header nil " Existing Person <existing@example.test>") (7 0 7 12 font-lock-comment-face nil "; On branch ") (7 12 7 16 git-commit-comment-branch nil "main") (8 0 8 2 font-lock-comment-face nil "; ") (8 2 8 26 git-commit-comment-heading nil "Changes to be committed:") (9 0 9 8 font-lock-comment-face nil ";\11") (9 8 9 16 git-commit-comment-action nil "modified") (9 16 9 20 font-lock-comment-face nil ":   ") (9 20 9 31 git-commit-comment-file nil "src/main.rs") (10 0 10 1 font-lock-comment-face nil ";") (11 10 12 0 nil diff-context " a/src/main.rs b/src/main.rs\n") (12 0 13 4 nil diff-header "index 3367afd..7f15e7a 100644\n--- ") (13 4 13 17 nil (diff-file-header diff-header) "a/src/main.rs") (13 17 14 4 nil diff-header "\n+++ ") (14 4 14 17 nil (diff-file-header diff-header) "b/src/main.rs") (14 17 15 0 nil diff-header "\n") (15 0 15 11 nil diff-hunk-header "@@ -1 +1 @@") (16 0 16 1 nil diff-indicator-removed "-") (16 1 17 0 nil diff-removed "old\n") (17 0 17 1 nil diff-indicator-added "+")) :repo "[ROOT]/activation repo 界/") :cleanup clean)"#
        ]],
    )
}

fn typing_fill_and_trailers() -> ParityBatchCase {
    ParityBatchCase::value(
        "types_wraps_and_inserts_ordered_trailers_through_keys",
        r####"
(git363-test-run
 "typing-trailers"
 (lambda (_world)
   (setq git-commit-fill-column 24)
   (let* ((repo (git363-test-make-repo "typing repo 界/" ";"))
          (path (git363-test-write
                 "typing repo 界/.git/COMMIT_EDITMSG"
                 (concat
                  "\nSigned-off-by: Existing Person <existing@example.test>\n"
                  "; generated context stays after the message\n"
                  "diff --git a/a b/a\n-old\n+new\n")))
          (visit (git363-test-visit path t)))
     (setenv "GIT_AUTHOR_NAME" "Typing Author")
     (setenv "GIT_AUTHOR_EMAIL" "typing@example.test")
     (setq git363-test-read-answers
           '(("Name: " . "Peer Ω")
             ("Email: " . "peer@example.test")))
     (goto-char (point-min))
     (let ((phases
            (git363-test-edit-macro
             '(:text "Summary words continue beyond configured boundary\n\nBody words also continue beyond configured boundary\n\n")
             '(:keys "C-c C-s C-c C-a C-c C-o")
             '(:text "Peer Ω") '(:keys "RET")
             '(:text "peer@example.test") '(:keys "RET")
             '(:keys "C-c C-s C-_ C-c C-s"))))
     (font-lock-ensure (point-min) (point-max))
     (list :usage (plist-get visit :usage)
           :phases phases
           :state (git363-test-buffer-state)
           :history (copy-tree minibuffer-history)
           :reads (nreverse (copy-tree git363-test-read-events))
           :properties (git363-test-property-runs)
           :ring (git363-test-ring-state)
           :processes (git363-test-process-state)
           :repo (git363-test-normalize-string repo))))))
"####,
        expect![[
            r#"OK (:result (:usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 1 :buffers-live nil :cancelled 1) :undo-boundary (:created 1 :pending-for-cleanup 1) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :phases ((:command git-commit-signoff :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-ack :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-cc :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\nCc: Peer Ω <peer@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-signoff :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\nCc: Peer Ω <peer@example.test>\nSigned-off-by: Typing Author <typing@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present)) (:command undo :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\nCc: Peer Ω <peer@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-signoff :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\nCc: Peer Ω <peer@example.test>\nSigned-off-by: Typing Author <typing@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present))) :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Summary words continue\nbeyond configured\nboundary\n\nBody words also continue\nbeyond configured\nboundary\n\n\n\nSigned-off-by: Existing Person <existing@example.test>\nSigned-off-by: Typing Author <typing@example.test>\nAcked-by: Typing Author <typing@example.test>\nCc: Peer Ω <peer@example.test>\nSigned-off-by: Typing Author <typing@example.test>\n; generated context stays after the message\ndiff --git a/a b/a\n-old\n+new\n" :point 105 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present) :history ("peer@example.test" "Peer Ω") :reads ((:prompt "Name: " :answer "Peer Ω" :history-after ("Peer Ω")) (:prompt "Email: " :answer "peer@example.test" :history-after ("peer@example.test" "Peer Ω"))) :properties ((1 0 1 22 git-commit-summary nil "Summary words continue") (2 0 2 17 git-commit-nonempty-second-line nil "beyond configured") (11 0 11 14 git-commit-known-pseudo-header nil "Signed-off-by:") (11 14 11 54 git-commit-pseudo-header nil " Existing Person <existing@example.test>") (12 0 12 14 git-commit-known-pseudo-header nil "Signed-off-by:") (12 14 12 50 git-commit-pseudo-header nil " Typing Author <typing@example.test>") (13 0 13 9 git-commit-known-pseudo-header nil "Acked-by:") (13 9 13 45 git-commit-pseudo-header nil " Typing Author <typing@example.test>") (14 0 14 3 git-commit-known-pseudo-header nil "Cc:") (14 3 14 30 git-commit-pseudo-header nil " Peer Ω <peer@example.test>") (15 0 15 14 git-commit-known-pseudo-header nil "Signed-off-by:") (15 14 15 50 git-commit-pseudo-header nil " Typing Author <typing@example.test>") (16 0 16 43 font-lock-comment-face nil "; generated context stays after the message") (17 10 18 0 nil diff-context " a/a b/a\n") (18 0 18 1 nil diff-indicator-removed "-") (18 1 19 0 nil diff-removed "old\n") (19 0 19 1 nil diff-indicator-added "+")) :ring (:length 1 :elements ("\nSigned-off-by: Existing Person <existing@example.test>\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/typing repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/typing repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/typing repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/typing repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil) :repo "[ROOT]/typing repo 界/") :cleanup clean)"#
        ]],
    )
}

fn public_history_ring() -> ParityBatchCase {
    ParityBatchCase::value(
        "cycles_real_commit_history_without_replacing_comments",
        r####"
(git363-test-run
 "history"
 (lambda (_world)
   (let* ((repo (git363-test-make-repo "history repo 界/" ";"))
          (path
           (git363-test-write
            "history repo 界/.git/COMMIT_EDITMSG"
            (concat
             "History one\n\nBody one\n\n"
             "; ------------------------ >8 ------------------------\n"
             "; comment omitted\n"
             "diff --git a/a b/a\n-old\n+new\n")))
          (visit (git363-test-visit path t))
          (initial-ring (git363-test-ring-state)))
     (let* ((edits
             (git363-test-edit-macro
              '(:keys "C-x h C-w")
              '(:text "History two\n\nBody two\n\n; comment omitted\n")
              '(:keys "C-c M-s C-c M-s")))
            (before-navigation (git363-test-ring-state))
           (events (git363-test-history-macro "M-p M-p M-n M-n")))
       (list :usage (plist-get visit :usage)
             :initial initial-ring :edits edits :before before-navigation
             :events events :final (git363-test-buffer-state)
             :ring (git363-test-ring-state)
             :messages (nreverse (copy-tree git363-test-message-events))
             :processes (git363-test-process-state)
             :repo (git363-test-normalize-string repo))))))
"####,
        expect![[
            r#"OK (:result (:usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 1 :buffers-live nil :cancelled 1) :undo-boundary (:created 1 :pending-for-cleanup 1) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :initial (:length 1 :elements ("History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :edits ((:command kill-region :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "" :point 1 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-save-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History two\n\nBody two\n\n; comment omitted\n" :point 42 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present)) (:command git-commit-save-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History two\n\nBody two\n\n; comment omitted\n" :point 42 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present))) :before (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :events ((:command git-commit-prev-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n\n; comment omitted\n" :point 53 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 1) :message "Comment 2") (:command git-commit-prev-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History two\n\nBody two\n\n; comment omitted\n" :point 23 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :message "Comment 1") (:command git-commit-next-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n\n; comment omitted\n" :point 53 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 1) :message "Comment 2") (:command git-commit-next-message :state (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History two\n\nBody two\n\n; comment omitted\n" :point 23 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :message "Comment 1")) :final (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "History two\n\nBody two\n\n; comment omitted\n" :point 23 :mark 1 :active t :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("History two\n\nBody two\n" "History one\n\nBody one\n\ndiff --git a/a b/a\n-old\n+new\n") :index 0) :messages ("" "Comment 2" "" "Comment 1" "" "Comment 2" "" "Comment 1") :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/history repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/history repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/history repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/history repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil) :repo "[ROOT]/history repo 界/") :cleanup clean)"#
        ]],
    )
}

fn style_reject_then_force_finish() -> ParityBatchCase {
    ParityBatchCase::value(
        "rejects_style_then_force_finishes_the_edit_session",
        r####"
(git363-test-run
 "finish"
 (lambda (_world)
   (let* ((repo (git363-test-make-repo "finish repo 界/" ";"))
          (text
           (concat
            "This summary is deliberately longer than the historical limit Ω\n"
            "second line is occupied\n\nBody\n"))
          (path (git363-test-write
                 "finish repo 界/.git/COMMIT_EDITMSG" text))
          (visit (git363-test-visit path t))
          (before (git363-test-buffer-state))
          (disk-before (git363-test-file-bytes path)))
     (setq git363-test-prompt-events nil
           git363-test-prompt-answers
           '(("Summary line is to long.  Commit anyway? " . t)
             ("Second line is not empty.  Commit anyway? " . nil)))
     (git363-test-kbd '(:keys "C-c C-c"))
     (let ((rejected (git363-test-buffer-state))
           (prompts (git363-test-prompt-state))
           (disk-rejected (git363-test-file-bytes path)))
       (setq git363-test-prompt-events nil)
       (git363-test-kbd '(:keys "C-u C-c C-c"))
       (list :usage (plist-get visit :usage)
             :before before :disk-before disk-before
             :prompts prompts :rejected rejected
             :disk-rejected disk-rejected
             :forced (list
                      :buffer-live (buffer-live-p (plist-get visit :buffer))
                      :disk (git363-test-file-bytes path)
                      :selected (buffer-name (window-buffer (selected-window)))
                      :prompt-events git363-test-prompt-events)
             :ring (git363-test-ring-state)
             :processes (git363-test-process-state)
             :repo (git363-test-normalize-string repo))))))
"####,
        expect![[
            r#"OK (:result (:usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 0 :buffers-live nil :cancelled 0) :undo-boundary (:created 0 :pending-for-cleanup 0) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :before (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n" :point 1 :mark nil :active nil :modified nil :read-only nil :narrowed nil :undo :empty) :disk-before "This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n" :prompts ((:prompt "Summary line is to long.  Commit anyway? " :answer t) (:prompt "Second line is not empty.  Commit anyway? " :answer nil)) :rejected (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n" :point 1 :mark nil :active nil :modified nil :read-only nil :narrowed nil :undo :empty) :disk-rejected "This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n" :forced (:buffer-live nil :disk "This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n" :selected "*scratch*" :prompt-events nil) :ring (:length 1 :elements ("This summary is deliberately longer than the historical limit Ω\nsecond line is occupied\n\nBody\n") :index 0) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/finish repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/finish repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/finish repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/finish repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil) :repo "[ROOT]/finish repo 界/") :cleanup clean)"#
        ]],
    )
}

fn cancel_dependency_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "cancel_saves_history_and_applies_real_dependency_lifecycle",
        r####"
(git363-test-run
 "cancel"
 (lambda (_world)
   (let* ((repo (git363-test-make-repo "cancel repo 界/" ";"))
          (path (git363-test-write
                 "cancel repo 界/.git/COMMIT_EDITMSG"
                 "Cancel subject\n\nBody\n\n; generated comment\n"))
          (visit (git363-test-visit path t)))
     (goto-char (point-min))
     (git363-test-kbd '(:text "Edited "))
     (let ((before (git363-test-buffer-state))
           (ring-before (git363-test-ring-state)))
       (setq git363-test-message-events nil)
       (git363-test-kbd '(:keys "C-c C-k"))
       (list :usage (plist-get visit :usage)
             :before before :ring-before ring-before
             :after (list
                     :buffer-live (buffer-live-p (plist-get visit :buffer))
                     :file (git363-test-file-bytes path)
                     :selected (buffer-name (window-buffer (selected-window)))
                     :messages (nreverse
                                (copy-tree git363-test-message-events)))
             :ring-after (git363-test-ring-state)
             :processes (git363-test-process-state)
             :repo (git363-test-normalize-string repo))))))
"####,
        expect![[
            r#"OK (:result (:usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 0 :buffers-live nil :cancelled 0) :undo-boundary (:created 0 :pending-for-cleanup 0) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :before (:file "COMMIT_EDITMSG" :mode text-mode :git-commit t :with-editor t :text "Edited Cancel subject\n\nBody\n\n; generated comment\n" :point 8 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present) :ring-before (:length 1 :elements ("Cancel subject\n\nBody\n") :index 0) :after (:buffer-live nil :file :missing :selected "*scratch*" :messages ("" "Commit canceled.  Message saved to ‘log-edit-comment-ring’" "Commit canceled.  Message saved to ‘log-edit-comment-ring’")) :ring-after (:length 2 :elements ("Edited Cancel subject\n\nBody\n" "Cancel subject\n\nBody\n") :index 0) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/cancel repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/cancel repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/cancel repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/cancel repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil) :repo "[ROOT]/cancel repo 界/") :cleanup clean)"#
        ]],
    )
}

fn routing_missing_git_and_recovery() -> ParityBatchCase {
    ParityBatchCase::value(
        "routes_custom_major_mode_and_recovers_from_missing_git",
        r####"
(git363-test-run
 "routing-recovery"
 (lambda (world)
   (let* ((repo (git363-test-make-repo "route repo 界/" ";"))
          (recognized (git363-test-write
                       "route repo 界/.git/MSG" "Disabled route\n"))
          (near (git363-test-write
                 "route repo 界/.git/COMMIT_EDITMSG.backup" "Near miss\n")))
     (global-git-commit-mode -1)
     (let* ((disabled-visit (git363-test-visit recognized nil))
            (disabled (git363-test-activation-state))
            (disabled-buffer (git363-test-buffer-state))
            (disabled-properties (git363-test-property-runs))
            (disabled-processes (git363-test-process-state)))
       (setq git363-test-process-records nil
             git363-test-diff-records nil)
       (git363-test-visit near nil)
       (let ((near-state (git363-test-activation-state))
             (near-processes (git363-test-process-state)))
         (global-git-commit-mode 1)
         (setq git-commit-major-mode 'log-edit-mode
               git363-test-process-records nil
               exec-path (list (directory-file-name
                                (plist-get world :empty-bin))))
         (setenv "PATH" (directory-file-name (plist-get world :empty-bin)))
         (let* ((missing-path
                 (git363-test-write
                  "route repo 界/.git/MERGE_MSG"
                  "Missing Git route\n\nBody remains separated\n\n# fallback comment\n"))
                (missing-visit (git363-test-visit missing-path t))
                (missing (git363-test-activation-state))
                (missing-buffer (git363-test-buffer-state))
                (missing-properties (git363-test-property-runs))
                (missing-processes (git363-test-process-state)))
           (setq exec-path
                 (delete-dups
                  (list (directory-file-name (plist-get world :bin))
                        (directory-file-name (plist-get world :diff-bin)))))
           (setenv "PATH"
                   (mapconcat #'identity exec-path path-separator))
           (setq git363-test-process-records nil
                 git363-test-diff-records nil)
           (let* ((recovery-repo
                   (git363-test-make-repo "recovery repo 界/" ";"))
                  (recovery-path
                   (git363-test-write
                    "recovery repo 界/.git/COMMIT_EDITMSG"
                    (concat "Boundary subject\n\nBody\n\n"
                            "; On branch main\n"
                            ";\tmodified:   src/recovery.txt\n")))
                  (recovery-visit (git363-test-visit recovery-path t))
                  (recovered (git363-test-activation-state))
                  (recovered-properties (git363-test-property-runs))
                  (before-read-only (git363-test-buffer-state)))
             (setenv "GIT_AUTHOR_NAME" "Config User")
             (setenv "GIT_AUTHOR_EMAIL" "config@example.test")
             (setq buffer-read-only t)
             (let ((failure
                    (condition-case condition
                        (progn
                          (git363-test-kbd '(:keys "C-c C-s"))
                          :no-signal)
                      (t (git363-test-condition-state condition))))
                   (after-failure (git363-test-buffer-state)))
               (setq buffer-read-only nil)
               (git363-test-kbd '(:keys "C-c C-s"))
               (list
                :disabled (list
                           :visit-buffer
                           (buffer-name (plist-get disabled-visit :buffer))
                           :state disabled :buffer disabled-buffer
                           :properties disabled-properties
                           :processes disabled-processes)
                :near (list :state near-state :processes near-processes)
                :missing (list
                          :usage (plist-get missing-visit :usage)
                          :state missing :buffer missing-buffer
                          :properties missing-properties
                          :processes missing-processes)
                :recovery (list
                           :repo (git363-test-normalize-string recovery-repo)
                           :usage (plist-get recovery-visit :usage)
                           :state recovered :properties recovered-properties
                           :processes (git363-test-process-state)
                           :before-read-only before-read-only
                           :failure failure :after-failure after-failure
                           :after-retry (git363-test-buffer-state)
                           :ring (git363-test-ring-state)))))))))))
"####,
        expect![[
            r##"OK (:result (:disabled (:visit-buffer "MSG" :state (:major fundamental-mode :git-commit nil :with-editor nil :comment ";" :comment-skip "^;+[ \11]*" :fill 70 :auto-fill nil :finish-hooks nil :cancel-hooks nil :cancel-message nil :kill-query (process-kill-buffer-query-function) :keys (("C-c C-s") ("C-c C-a") ("C-c C-o") ("C-c M-s") ("M-p") ("M-n") ("C-c C-c") ("C-c C-k"))) :buffer (:file "MSG" :mode fundamental-mode :git-commit nil :with-editor nil :text "Disabled route\n" :point 1 :mark nil :active nil :modified nil :read-only nil :narrowed nil :undo :empty) :properties ((1 0 1 14 git-commit-summary nil "Disabled route")) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil)) :near (:state (:major fundamental-mode :git-commit nil :with-editor nil :comment nil :comment-skip nil :fill 70 :auto-fill nil :finish-hooks nil :cancel-hooks nil :cancel-message nil :kill-query (process-kill-buffer-query-function) :keys (("C-c C-s") ("C-c C-a") ("C-c C-o") ("C-c M-s") ("M-p") ("M-n") ("C-c C-c") ("C-c C-k"))) :processes (:git nil :diff nil)) :missing (:usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 0 :buffers-live nil :cancelled 0) :undo-boundary (:created 0 :pending-for-cleanup 0) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :state (:major log-edit-mode :git-commit t :with-editor t :comment "#" :comment-skip "^#+[ \11]*" :fill 72 :auto-fill log-edit-do-auto-fill :finish-hooks (git-commit-finish-query-functions t) :cancel-hooks (git-commit-save-message t) :cancel-message git-commit-cancel-message :kill-query (with-editor-kill-buffer-noop t) :keys (("C-c C-s" . git-commit-signoff) ("C-c C-a" . git-commit-ack) ("C-c C-o" . git-commit-cc) ("C-c M-s" . git-commit-save-message) ("M-p" . git-commit-prev-message) ("M-n" . git-commit-next-message) ("C-c C-c" . with-editor-finish) ("C-c C-k" . with-editor-cancel))) :buffer (:file "MERGE_MSG" :mode log-edit-mode :git-commit t :with-editor t :text "Missing Git route\n\nBody remains separated\n\n# fallback comment\n" :point 1 :mark nil :active nil :modified nil :read-only nil :narrowed nil :undo :empty) :properties ((1 0 1 17 git-commit-summary nil "Missing Git route") (5 0 5 18 font-lock-comment-face nil "# fallback comment")) :processes (:git ((:program "git" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched nil :condition (:symbol file-missing :data ("Searching for program" "No such file or directory" "git") :message "Searching for program: No such file or directory, git")) (:program "git" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched nil :condition (:symbol file-missing :data ("Searching for program" "No such file or directory" "git") :message "Searching for program: No such file or directory, git")) (:program "git" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched nil :condition (:symbol file-missing :data ("Searching for program" "No such file or directory" "git") :message "Searching for program: No such file or directory, git")) (:program "git" :argv ("config" "core.commentchar") :cwd "[ROOT]/route repo 界/.git/" :launched nil :condition (:symbol file-missing :data ("Searching for program" "No such file or directory" "git") :message "Searching for program: No such file or directory, git"))) :diff nil)) :recovery (:repo "[ROOT]/recovery repo 界/" :usage (:registration (:time 0.05 :repeat nil :buffer-live t :identity-captured t) :track-changes (:created 0 :buffers-live nil :cancelled 0) :undo-boundary (:created 0 :pending-for-cleanup 0) :pre-message nil :messages ("Type C-c C-c to finish, or C-c C-k to cancel") :usage-pending 0) :state (:major log-edit-mode :git-commit t :with-editor t :comment ";" :comment-skip "^;+[ \11]*" :fill 72 :auto-fill log-edit-do-auto-fill :finish-hooks (git-commit-finish-query-functions t) :cancel-hooks (git-commit-save-message t) :cancel-message git-commit-cancel-message :kill-query (with-editor-kill-buffer-noop t) :keys (("C-c C-s" . git-commit-signoff) ("C-c C-a" . git-commit-ack) ("C-c C-o" . git-commit-cc) ("C-c M-s" . git-commit-save-message) ("M-p" . git-commit-prev-message) ("M-n" . git-commit-next-message) ("C-c C-c" . with-editor-finish) ("C-c C-k" . with-editor-cancel))) :properties ((1 0 1 16 git-commit-summary nil "Boundary subject") (5 0 5 12 font-lock-comment-face nil "; On branch ") (5 12 5 16 git-commit-comment-branch nil "main") (6 0 6 8 font-lock-comment-face nil ";\11") (6 8 6 16 git-commit-comment-action nil "modified") (6 16 6 20 font-lock-comment-face nil ":   ") (6 20 6 36 git-commit-comment-file nil "src/recovery.txt")) :processes (:git ((:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/recovery repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/recovery repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/recovery repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";")) (:program "[GIT]" :argv ("config" "core.commentchar") :cwd "[ROOT]/recovery repo 界/.git/" :launched t :status 0 :streams :combined :combined-output (";"))) :diff nil) :before-read-only (:file "COMMIT_EDITMSG" :mode log-edit-mode :git-commit t :with-editor t :text "Boundary subject\n\nBody\n\n; On branch main\n;\11modified:   src/recovery.txt\n" :point 1 :mark nil :active nil :modified nil :read-only nil :narrowed nil :undo :empty) :failure (:symbol buffer-read-only :data (:buffer) :message "Buffer is read-only: #<buffer COMMIT_EDITMSG>") :after-failure (:file "COMMIT_EDITMSG" :mode log-edit-mode :git-commit t :with-editor t :text "Boundary subject\n\nBody\n\n; On branch main\n;\11modified:   src/recovery.txt\n" :point 1 :mark nil :active nil :modified nil :read-only t :narrowed nil :undo :empty) :after-retry (:file "COMMIT_EDITMSG" :mode log-edit-mode :git-commit t :with-editor t :text "Boundary subject\n\nBody\n\nSigned-off-by: Config User <config@example.test>\n\n; On branch main\n;\11modified:   src/recovery.txt\n" :point 1 :mark nil :active nil :modified t :read-only nil :narrowed nil :undo :present) :ring (:length 2 :elements ("Boundary subject\n\nBody\n" "Missing Git route\n\nBody remains separated\n") :index 0))) :cleanup clean)"##
        ]],
    )
}

pub(super) fn git_commit_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        activation_and_real_template(),
        typing_fill_and_trailers(),
        public_history_ring(),
        style_reject_then_force_finish(),
        cancel_dependency_lifecycle(),
        routing_missing_git_and_recovery(),
    ]
}
