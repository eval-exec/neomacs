use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_MESSENGER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'subr-x)
(require 'git-messenger)

(defun neomacs-gm-test-root (name)
  "Create a deterministic Git Messenger sandbox for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "git-messenger-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-gm-test-write (root relative contents)
  "Write CONTENTS to RELATIVE below ROOT and return its path."
  (let ((path (expand-file-name relative root))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-gm-test-git (root &rest arguments)
  "Run Git with ARGUMENTS in ROOT and return trimmed standard output."
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (eq status 0)
          (error "git %S failed (%S): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-gm-test-commit (root subject body author email timestamp)
  "Commit ROOT deterministically and return the resulting full hash."
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" author)
    (setenv "GIT_AUTHOR_EMAIL" email)
    (setenv "GIT_COMMITTER_NAME" author)
    (setenv "GIT_COMMITTER_EMAIL" email)
    (setenv "GIT_AUTHOR_DATE" timestamp)
    (setenv "GIT_COMMITTER_DATE" timestamp)
    (neomacs-gm-test-git root "add" "--all")
    (neomacs-gm-test-git
     root "commit" "--quiet" "--no-gpg-sign"
     "--message" subject "--message" body)
    (neomacs-gm-test-git root "rev-parse" "HEAD")))

(defun neomacs-gm-test-fixture (name)
  "Build a deterministic two-commit service-ownership repository."
  (let* ((root (neomacs-gm-test-root name))
         (relative "src/service.conf")
         (file (neomacs-gm-test-write
                root relative
                "service=checkout\nowner=platform\ntimeout=30\n"))
         first second)
    (neomacs-gm-test-git root "init" "--quiet" "--initial-branch=main")
    (neomacs-gm-test-git root "config" "core.hooksPath" "/dev/null")
    (setq first
          (neomacs-gm-test-commit
           root "Introduce checkout service"
           "Document the original platform ownership."
           "Alice Example" "alice@example.test"
           "2024-01-02T03:04:05+0000"))
    (neomacs-gm-test-write
     root relative
     "service=checkout\nowner=payments\ntimeout=30\n")
    (setq second
          (neomacs-gm-test-commit
           root "Transfer checkout ownership"
           "Route on-call questions to Payments."
           "Bob Example" "bob@example.test"
           "2024-02-05T06:07:08+0000"))
    (list :root root :file file :relative relative
          :first first :second second)))

(defun neomacs-gm-test-error (function)
  "Return FUNCTION's value or stable error details."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-gm-test-popup-buffer-state ()
  "Return stable state from the Git Messenger revision buffer."
  (with-current-buffer "*git-messenger*"
    (list :mode major-mode
          :view (and view-mode t)
          :read-only buffer-read-only
          :point (point)
          :text (buffer-substring-no-properties (point-min) (point-max)))))

(defun neomacs-gm-test-run (name function)
  "Run FUNCTION with a deterministic repository and clean editor state."
  (let ((process-environment (copy-sequence process-environment))
        fixture result)
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (setenv "GIT_DEFAULT_HASH" "sha1")
    (setq fixture (neomacs-gm-test-fixture name))
    (unwind-protect
        (setq result
              (save-window-excursion
                (save-current-buffer
                  (funcall function fixture))))
      (dolist (buffer (buffer-list))
        (when (and (buffer-file-name buffer)
                   (string-prefix-p
                    (plist-get fixture :root) (buffer-file-name buffer)))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer))))
      (when (get-buffer "*git-messenger*")
        (kill-buffer "*git-messenger*"))
      (when (file-exists-p (plist-get fixture :root))
        (delete-directory (plist-get fixture :root) t)))
    result))
"###;

fn package_contract_and_keymap_expose_the_blame_navigation_workflow() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'git-messenger package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'git-messenger) t))
   :defaults
   (list :detail git-messenger:show-detail
         :backends git-messenger:handled-backends
         :magit git-messenger:use-magit-popup
         :last-message git-messenger:last-message
         :last-commit git-messenger:last-commit-id)
   :commands
   (mapcar #'commandp
           '(git-messenger:popup-message git-messenger:copy-message
             git-messenger:copy-commit-id git-messenger:popup-diff
             git-messenger:popup-show git-messenger:popup-show-verbose
             git-messenger:show-parent git-messenger:popup-close))
   :bindings
   (mapcar (lambda (key)
             (cons key (lookup-key git-messenger-map (kbd key))))
           '("q" "c" "d" "s" "S" "M-w" ","))
   :prompt (git-messenger:prompt)
   :arguments
   (list :git (git-messenger:blame-arguments 'git "/repo/src/a.el" 42)
         :svn (git-messenger:blame-arguments 'svn "/repo/src/a.el" 42)
         :hg (git-messenger:blame-arguments 'hg "/repo/src/a.el" 42)
         :cat (git-messenger:cat-file-arguments "abc123"))))
"###;
    let expected = expect![[
        r#"OK (:package (:name git-messenger :version "20201202.1637" :requirements ((emacs (24 3)) (popup (0 5 3))) :feature t) :defaults (:detail nil :backends (git svn hg) :magit nil :last-message nil :last-commit nil) :commands (t t t t t t t t) :bindings (("q" . git-messenger:popup-close) ("c" . git-messenger:copy-commit-id) ("d" . git-messenger:popup-diff) ("s" . git-messenger:popup-show) ("S" . git-messenger:popup-show-verbose) ("M-w" . git-messenger:copy-message) ("," . git-messenger:show-parent)) :prompt "[s]Show [S]Show verbose [q]Close [c]Copy hash [d]Diff [M-w]Copy message [,]Go Parent [q]Quit " :arguments (:git ("--no-pager" "blame" "-w" "-L" "42,+1" "--porcelain" "a.el") :svn ("blame" "a.el") :hg ("blame" "-wuc" "a.el") :cat ("--no-pager" "cat-file" "commit" "abc123")))"#
    ]];
    ParityBatchCase::value(
        "package_contract_and_keymap_expose_the_blame_navigation_workflow",
        elisp_form,
        expected,
    )
}

fn popup_message_blames_the_real_line_and_runs_ui_hooks_in_order() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "popup"
 (lambda (fixture)
   (let* ((events nil)
          (buffer (find-file-noselect (plist-get fixture :file)))
          (git-messenger:show-detail nil)
          (git-messenger:before-popup-hook
           (list (lambda (message) (push (list 'before message) events))))
          (git-messenger:after-popup-hook
           (list (lambda (message) (push (list 'after message) events)))))
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 1)
       (cl-letf (((symbol-function 'popup-tip)
                  (lambda (message &rest properties)
                    (push (list 'popup-tip message properties) events)
                    :menu))
                 ((symbol-function 'popup-menu-event-loop)
                  (lambda (menu map fallback &rest properties)
                    (push
                     (list 'event-loop menu fallback
                           (plist-get properties :prompt)
                           (mapcar (lambda (key)
                                     (lookup-key map (kbd key)))
                                   '("c" "d" "s" "M-w" ",")))
                     events)))
                 ((symbol-function 'popup-delete)
                  (lambda (menu) (push (list 'deleted menu) events))))
         (git-messenger:popup-message)
         (list :fixture
               (list (plist-get fixture :first) (plist-get fixture :second))
               :line (list (line-number-at-pos)
                           (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position)))
               :vcs git-messenger:vcs
               :commit git-messenger:last-commit-id
               :message git-messenger:last-message
               :events (nreverse events)))))))
"###;
    let expected = expect![[
        r#"OK (:fixture ("6bad618e264e439148d430c1059395f6dabbd11f" "213ee69aabe7dd95bb1001572459eb45cf9c0c23") :line (2 "owner=payments") :vcs git :commit "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :message "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" :events ((before "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n") (popup-tip "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" (:nowait t)) (event-loop :menu popup-menu-fallback "[s]Show [S]Show verbose [q]Close [c]Copy hash [d]Diff [M-w]Copy message [,]Go Parent [q]Quit " (git-messenger:copy-commit-id git-messenger:popup-diff git-messenger:popup-show git-messenger:copy-message git-messenger:show-parent)) (deleted :menu) (after "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n")))"#
    ]];
    ParityBatchCase::value(
        "popup_message_blames_the_real_line_and_runs_ui_hooks_in_order",
        elisp_form,
        expected,
    )
}

fn prefix_detail_reports_real_commit_author_date_and_message() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "detail"
 (lambda (fixture)
   (let ((buffer (find-file-noselect (plist-get fixture :file)))
         (current-prefix-arg '(4))
         (git-messenger:show-detail nil)
         popup-message)
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 1)
       (cl-letf (((symbol-function 'popup-tip)
                  (lambda (message &rest _)
                    (setq popup-message message)
                    :menu))
                 ((symbol-function 'popup-menu-event-loop) (lambda (&rest _)))
                 ((symbol-function 'popup-delete) (lambda (&rest _))))
         (git-messenger:popup-message)
         (list :expected-commit (plist-get fixture :second)
               :commit git-messenger:last-commit-id
               :popup popup-message
               :stored git-messenger:last-message
               :detail-policy
               (mapcar (lambda (value)
                         (let ((current-prefix-arg value))
                           (git-messenger:show-detail-p
                            git-messenger:last-commit-id)))
                       '(nil (4)))))))))
"###;
    let expected = expect![[
        r#"OK (:expected-commit "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :commit "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :popup "commit : 213ee69a \nAuthor : Bob Example\nDate   : Mon Feb 5 06:07:08 2024 +0000 \n\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" :stored "commit : 213ee69a \nAuthor : Bob Example\nDate   : Mon Feb 5 06:07:08 2024 +0000 \n\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" :detail-policy (nil t))"#
    ]];
    ParityBatchCase::value(
        "prefix_detail_reports_real_commit_author_date_and_message",
        elisp_form,
        expected,
    )
}

fn an_uncommitted_worktree_line_is_reported_without_fabricated_detail() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "uncommitted"
 (lambda (fixture)
   (neomacs-gm-test-write
    (plist-get fixture :root) (plist-get fixture :relative)
    "service=checkout\nowner=incident-response\ntimeout=30\n")
   (let ((buffer (find-file-noselect (plist-get fixture :file)))
         (current-prefix-arg '(4))
         (git-messenger:show-detail t)
         popup-message)
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 1)
       (cl-letf (((symbol-function 'popup-tip)
                  (lambda (message &rest _)
                    (setq popup-message message)
                    :menu))
                 ((symbol-function 'popup-menu-event-loop) (lambda (&rest _)))
                 ((symbol-function 'popup-delete) (lambda (&rest _))))
         (git-messenger:popup-message)
         (list :line (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position))
               :commit git-messenger:last-commit-id
               :uncommitted
               (git-messenger:not-committed-id-p
                git-messenger:last-commit-id)
               :detail (git-messenger:show-detail-p
                        git-messenger:last-commit-id)
               :popup popup-message
               :status (neomacs-gm-test-git
                        (plist-get fixture :root) "status" "--short")))))))
"###;
    let expected = expect![[
        r#"OK (:line "owner=incident-response" :commit "0000000000000000000000000000000000000000" :uncommitted 0 :detail nil :popup "* not yet committed *" :status " M src/service.conf")"#
    ]];
    ParityBatchCase::value(
        "an_uncommitted_worktree_line_is_reported_without_fabricated_detail",
        elisp_form,
        expected,
    )
}

fn copying_the_observed_message_and_hash_populates_the_real_kill_ring() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "copy"
 (lambda (fixture)
   (let ((buffer (find-file-noselect (plist-get fixture :file)))
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         (interprogram-cut-function nil)
         (git-messenger:show-detail nil))
     (with-current-buffer buffer
       (goto-char (point-min))
       (forward-line 1)
       (let* ((info (git-messenger:commit-info-at-line
                     'git (buffer-file-name) (line-number-at-pos)))
              (commit (car info))
              (message (git-messenger:commit-message 'git commit)))
         (setq git-messenger:last-commit-id commit
               git-messenger:last-message message)
         (catch 'git-messenger-loop
           (git-messenger:copy-message))
         (let ((after-message (copy-sequence kill-ring)))
           (catch 'git-messenger-loop
             (git-messenger:copy-commit-id))
           (list :author (cdr info)
                 :commit commit
                 :message message
                 :after-message after-message
                 :after-hash kill-ring
                 :yank (car kill-ring-yank-pointer))))))))
"###;
    let expected = expect![[
        r#"OK (:author "Bob Example" :commit "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :message "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" :after-message ("\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n") :after-hash ("213ee69aabe7dd95bb1001572459eb45cf9c0c23" "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n") :yank "213ee69aabe7dd95bb1001572459eb45cf9c0c23")"#
    ]];
    ParityBatchCase::value(
        "copying_the_observed_message_and_hash_populates_the_real_kill_ring",
        elisp_form,
        expected,
    )
}

fn diff_show_and_verbose_actions_render_real_revision_buffers() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "revision-buffers"
 (lambda (fixture)
   (let* ((events nil)
          (default-directory (plist-get fixture :root))
          (git-messenger:vcs 'git)
          (git-messenger:last-commit-id (plist-get fixture :second))
          (git-messenger:use-magit-popup nil)
          (git-messenger:popup-buffer-hook
           (list (lambda ()
                   (push (list 'buffer-hook major-mode buffer-read-only)
                         events))))
          states)
     (cl-letf (((symbol-function 'magit-show-commit)
                (lambda (commit) (push (list 'magit commit) events)))
               ((symbol-function 'pop-to-buffer)
                (lambda (buffer &rest _)
                  (push (list 'pop (buffer-name buffer)) events)
                  buffer)))
       (dolist (command '(git-messenger:popup-diff
                          git-messenger:popup-show
                          git-messenger:popup-show-verbose))
         (catch 'git-messenger-loop
           (funcall command))
         (push (cons command (neomacs-gm-test-popup-buffer-state)) states))
       (list :commit (plist-get fixture :second)
             :states (nreverse states)
             :events (nreverse events))))))
"###;
    let expected = expect![[
        r#"OK (:commit "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :states ((git-messenger:popup-diff :mode diff-mode :view t :read-only t :point 1 :text "diff --git a/src/service.conf b/src/service.conf\nindex a90c5fe..efb26d4 100644\n--- a/src/service.conf\n+++ b/src/service.conf\n@@ -1,3 +1,3 @@\n service=checkout\n-owner=platform\n+owner=payments\n timeout=30\n") (git-messenger:popup-show :mode fundamental-mode :view t :read-only t :point 1 :text "commit 213ee69aabe7dd95bb1001572459eb45cf9c0c23\nAuthor: Bob Example <bob@example.test>\nDate:   Mon Feb 5 06:07:08 2024 +0000\n\n    Transfer checkout ownership\n    \n    Route on-call questions to Payments.\n\n src/service.conf | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n") (git-messenger:popup-show-verbose :mode fundamental-mode :view t :read-only t :point 1 :text "commit 213ee69aabe7dd95bb1001572459eb45cf9c0c23\nAuthor: Bob Example <bob@example.test>\nDate:   Mon Feb 5 06:07:08 2024 +0000\n\n    Transfer checkout ownership\n    \n    Route on-call questions to Payments.\n---\n src/service.conf | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n\ndiff --git a/src/service.conf b/src/service.conf\nindex a90c5fe..efb26d4 100644\n--- a/src/service.conf\n+++ b/src/service.conf\n@@ -1,3 +1,3 @@\n service=checkout\n-owner=platform\n+owner=payments\n timeout=30\n")) :events ((pop "*git-messenger*") (buffer-hook diff-mode nil) (pop "*git-messenger*") (buffer-hook fundamental-mode nil) (pop "*git-messenger*") (buffer-hook fundamental-mode nil)))"#
    ]];
    ParityBatchCase::value(
        "diff_show_and_verbose_actions_render_real_revision_buffers",
        elisp_form,
        expected,
    )
}

fn parent_navigation_replaces_the_blame_with_the_previous_commit_message() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-gm-test-run
 "parent"
 (lambda (fixture)
   (let ((buffer (find-file-noselect (plist-get fixture :file)))
         (git-messenger:vcs 'git)
         (git-messenger:last-commit-id (plist-get fixture :second))
         before)
     (with-current-buffer buffer
       (let ((default-directory (plist-get fixture :root)))
         (setq before
               (git-messenger:commit-message
                'git git-messenger:last-commit-id)
               git-messenger:last-message before)
         (catch 'git-messenger-loop
           (git-messenger:show-parent))
         (list :first (plist-get fixture :first)
               :second (plist-get fixture :second)
               :before before
               :after-id git-messenger:last-commit-id
               :after-message git-messenger:last-message))))))
"###;
    let expected = expect![[
        r#"OK (:first "6bad618e264e439148d430c1059395f6dabbd11f" :second "213ee69aabe7dd95bb1001572459eb45cf9c0c23" :before "\nTransfer checkout ownership\n\nRoute on-call questions to Payments.\n" :after-id "6bad618e264e439148d430c1059395f6dabbd11f" :after-message "\nIntroduce checkout service\n\nDocument the original platform ownership.\n")"#
    ]];
    ParityBatchCase::value(
        "parent_navigation_replaces_the_blame_with_the_previous_commit_message",
        elisp_form,
        expected,
    )
}

fn repository_precedence_blame_parsers_and_failures_cover_supported_boundaries() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-gm-test-run
 "backends"
 (lambda (fixture)
   (let* ((nested (file-name-as-directory
                   (expand-file-name "services/api/deep"
                                     (plist-get fixture :root))))
          (hg-root (file-name-as-directory
                    (expand-file-name "services/api"
                                      (plist-get fixture :root))))
          (default-directory nested)
          git-choice hg-choice git-info svn-info hg-info)
     (make-directory nested t)
     (make-directory (expand-file-name ".hg" hg-root) t)
     (let ((git-messenger:handled-backends '(git)))
       (setq git-choice (git-messenger:find-vcs)))
     (let ((git-messenger:handled-backends '(git svn hg)))
       (setq hg-choice (git-messenger:find-vcs)))
     (with-temp-buffer
       (insert (plist-get fixture :second)
               " 2 2 1\nauthor Bob Example\nauthor-mail <bob@example.test>\n")
       (goto-char (point-min))
       (setq git-info (git-messenger:git-commit-info-at-line)))
     (with-temp-buffer
       (insert "  12 alice first\n  27 bob second\n")
       (goto-char (point-min))
       (setq svn-info (git-messenger:svn-commit-info-at-line 2)))
     (with-temp-buffer
       (insert " alice abc123 first\n bob def456 second\n")
       (goto-char (point-min))
       (setq hg-info (git-messenger:hg-commit-info-at-line 2)))
     (list
      :precedence (list git-choice hg-choice)
      :parsers (list :git git-info :svn svn-info :hg hg-info)
      :uncommitted
      (mapcar (lambda (id)
                (list id (git-messenger:not-committed-id-p id)))
              '("00000000" "-" "abc123" "00a0"))
      :missing-file
      (neomacs-gm-test-error
       (lambda ()
         (git-messenger:commit-info-at-line
          'git (expand-file-name "missing.conf" (plist-get fixture :root)) 1)))
      :invalid-commit
      (neomacs-gm-test-error
       (lambda () (git-messenger:commit-message 'git "not-a-commit")))
      :unsupported-parent
      (let ((git-messenger:vcs 'svn))
        (neomacs-gm-test-error #'git-messenger:show-parent))))))
"###;
    let expected = expect![[
        r#"OK (:precedence (git hg) :parsers (:git ("213ee69aabe7dd95bb1001572459eb45cf9c0c23" . "Bob Example") :svn ("27" . "bob") :hg ("def456" . "bob")) :uncommitted (("00000000" 0) ("-" 0) ("abc123" nil) ("00a0" nil)) :missing-file (:error error :data ("Failed: ’git blame’") :message "Failed: ’git blame’") :invalid-commit (:error error :data ("Failed: ’git cat-file’") :message "Failed: ’git cat-file’") :unsupported-parent (:error error :data ("svn does not support for getting parent commit ID") :message "svn does not support for getting parent commit ID"))"#
    ]];
    ParityBatchCase::value(
        "repository_precedence_blame_parsers_and_failures_cover_supported_boundaries",
        elisp_form,
        expected,
    )
}

#[test]
fn git_messenger_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GIT_MESSENGER_MELPA_PIN, "git-messenger.el")
            .expect("prepare revision-pinned Git Messenger below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "git-messenger-package-batch",
        "Git Messenger",
        &[
            package_contract_and_keymap_expose_the_blame_navigation_workflow(),
            popup_message_blames_the_real_line_and_runs_ui_hooks_in_order(),
            prefix_detail_reports_real_commit_author_date_and_message(),
            an_uncommitted_worktree_line_is_reported_without_fabricated_detail(),
            copying_the_observed_message_and_hash_populates_the_real_kill_ring(),
            diff_show_and_verbose_actions_render_real_revision_buffers(),
            parent_navigation_replaces_the_blame_with_the_previous_commit_message(),
            repository_precedence_blame_parsers_and_failures_cover_supported_boundaries(),
        ],
    );
}
