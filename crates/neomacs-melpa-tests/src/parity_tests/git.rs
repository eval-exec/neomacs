use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'git)

(defun neomacs-git-test-write (path text)
  (with-temp-file (expand-file-name path git-repo) (insert text)))

(defun neomacs-git-test-read (path)
  (with-temp-buffer
    (insert-file-contents (expand-file-name path git-repo))
    (buffer-string)))

(defun neomacs-git-test-log-summary ()
  (mapcar
   (lambda (entry)
     (list :author (plist-get entry :author-name)
           :email (plist-get entry :author-email)
           :committer (plist-get entry :comitter-name)
           :message (plist-get entry :message)))
   (git-log)))

(defun neomacs-git-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (cl-labels ((normalize
                  (value)
                  (if (stringp value)
                      (replace-regexp-in-string
                       (regexp-quote git-executable) "<git>" value t t)
                    value)))
       (list :error (car error-data)
             :data (mapcar #'normalize (cdr error-data))
             :message (normalize (error-message-string error-data)))))))

(defun neomacs-git-test-in-repository (name function)
  (let* ((root (file-name-as-directory (getenv "TMPDIR")))
         (repository (make-temp-file (expand-file-name (format "git-el-%s-" name) root) t))
         (git-repo repository)
         (default-directory repository)
         (process-environment (copy-sequence process-environment)))
    (unwind-protect
        (progn
          (setenv "GIT_AUTHOR_DATE" "2001-02-03T04:05:06+0000")
          (setenv "GIT_COMMITTER_DATE" "2001-02-03T04:05:06+0000")
          (git-init repository)
          (git-run "symbolic-ref" "HEAD" "refs/heads/main")
          (git-config "user.name" "Release Bot")
          (git-config "user.email" "release@example.invalid")
          (funcall function repository))
      (when (file-directory-p repository)
        (delete-directory repository t)))))
"####;

fn release_repository_lifecycle_tracks_files_branches_tags_and_history() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-git-test-in-repository
 "lifecycle"
 (lambda (_repository)
   (neomacs-git-test-write "README.md" "# Product\n")
   (neomacs-git-test-write "notes.txt" "draft\n")
   (let ((untracked (git-untracked-files)))
     (git-add "README.md")
     (let ((staged (git-staged-files))
           (remaining (git-untracked-files)))
       (git-commit "Bootstrap product" "README.md")
       (git-branch "release/1.x")
       (git-tag "v1.0.0")
       (git-add "notes.txt")
       (git-commit "Publish release notes" "notes.txt")
       (list :repo (git-repo? git-repo)
             :untracked untracked :staged staged :remaining remaining
             :branch (git-on-branch) :branches (git-branches)
             :branch-exists (git-branch? "release/1.x")
             :tags (git-tags) :tag-exists (git-tag? "v1.0.0")
             :history (neomacs-git-test-log-summary))))))
"####;
    let expected = expect![[
        r#"OK (:repo t :untracked ("README.md" "notes.txt") :staged ("README.md") :remaining ("notes.txt") :branch "main" :branches ("main" "release/1.x") :branch-exists ("release/1.x") :tags ("v1.0.0") :tag-exists ("v1.0.0") :history ((:author "Release Bot" :email "release@example.invalid" :committer "Release Bot" :message "Publish release notes") (:author "Release Bot" :email "release@example.invalid" :committer "Release Bot" :message "Bootstrap product")))"#
    ]];
    ParityBatchCase::value(
        "release_repository_lifecycle_tracks_files_branches_tags_and_history",
        elisp_form,
        expected,
    )
}

fn checkout_reset_and_remove_restore_expected_worktree_versions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-git-test-in-repository
 "checkout-reset"
 (lambda (_repository)
   (neomacs-git-test-write "config.ini" "channel=stable\n")
   (git-add "config.ini") (git-commit "Add stable config" "config.ini")
   (git-branch "stable")
   (neomacs-git-test-write "config.ini" "channel=next\n")
   (git-commit "Switch to next channel" "config.ini")
   (let ((main-text (neomacs-git-test-read "config.ini")))
     (git-checkout "stable")
     (let ((stable-text (neomacs-git-test-read "config.ini")))
       (git-checkout "main")
       (git-rm "config.ini")
       (let ((removed (list (file-exists-p (expand-file-name "config.ini" git-repo))
                            (git-staged-files))))
         (git-reset "HEAD" 'hard)
         (list :main main-text :stable stable-text :removed removed
               :restored (neomacs-git-test-read "config.ini")
               :staged-after-reset (git-staged-files)
               :history (neomacs-git-test-log-summary)))))))
"####;
    let expected = expect![[
        r#"OK (:main "channel=next\n" :stable "channel=stable\n" :removed (nil ("config.ini")) :restored "channel=next\n" :staged-after-reset nil :history ((:author "Release Bot" :email "release@example.invalid" :committer "Release Bot" :message "Switch to next channel") (:author "Release Bot" :email "release@example.invalid" :committer "Release Bot" :message "Add stable config")))"#
    ]];
    ParityBatchCase::value(
        "checkout_reset_and_remove_restore_expected_worktree_versions",
        elisp_form,
        expected,
    )
}

fn multiple_stashes_can_be_applied_by_name_and_popped_in_order() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-git-test-in-repository
 "stash"
 (lambda (_repository)
   (neomacs-git-test-write "README" "base\n")
   (git-add "README") (git-commit "Initial commit" "README")
   (neomacs-git-test-write "first.txt" "first\n")
   (git-add "first.txt")
   (let ((first-name (git-stash "first change")))
     (neomacs-git-test-write "second.txt" "second\n")
     (git-add "second.txt")
     (let ((second-name (git-stash "second change"))
           (before (git-stashes)))
       (git-stash-apply "stash@{1}")
       (let ((after-apply (list (git-staged-files) (git-stashes))))
         (git-reset "HEAD" 'hard)
         (git-stash-pop)
         (list :names (list first-name second-name) :before before
               :after-apply after-apply
               :after-pop (list (git-staged-files) (git-stashes))))))))
"####;
    let expected = expect![[
        r#"OK (:names ("stash@{0}" "stash@{0}") :before ((:name "stash@{0}" :branch "main" :message "second change") (:name "stash@{1}" :branch "main" :message "first change")) :after-apply (("first.txt") ((:name "stash@{0}" :branch "main" :message "second change") (:name "stash@{1}" :branch "main" :message "first change"))) :after-pop (("second.txt") ((:name "stash@{0}" :branch "main" :message "first change"))))"#
    ]];
    ParityBatchCase::value(
        "multiple_stashes_can_be_applied_by_name_and_popped_in_order",
        elisp_form,
        expected,
    )
}

fn bare_repository_and_remote_management_support_local_delivery_topology() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-git-test-in-repository
 "remotes"
 (lambda (repository)
   (let ((bare (expand-file-name "delivery.git" repository)))
     (unwind-protect
         (progn
           (make-directory bare)
           (git-init bare t)
           (git-remote-add "delivery" bare)
           (git-remote-add "backup" "ssh://example.invalid/product.git")
           (let ((before (git-remotes)))
             (git-remote-remove "backup")
             (list :bare (git-repo? bare) :before before
                   :after (git-remotes)
                   :delivery (git-remote? "delivery")
                   :missing (git-remote? "backup")
                   :remove-missing
                   (neomacs-git-test-capture
                    (lambda () (git-remote-remove "missing"))))))
       (when (file-directory-p bare) (delete-directory bare t))))))
"####;
    let expected = expect![[
        r#"OK (:bare t :before ("backup" "delivery") :after ("delivery") :delivery ("delivery") :missing nil :remove-missing (:error git-error :data ("No such remote missing") :message "GIT Error: \"No such remote missing\""))"#
    ]];
    ParityBatchCase::value(
        "bare_repository_and_remote_management_support_local_delivery_topology",
        elisp_form,
        expected,
    )
}

fn command_arguments_config_and_failures_preserve_structured_api_results() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-git-test-in-repository
 "commands"
 (lambda (_repository)
   (git-config "product.channel" "candidate")
   (let ((status
          (let ((git-args '("--untracked-files=no")))
            (git-run "status" "--short"))))
     (list :config (git-config "product.channel")
           :missing-config (git-config "product.missing")
           :status status
           :unknown-branch
           (neomacs-git-test-capture (lambda () (git-checkout "does-not-exist")))
           :uninitialized-branch
           (neomacs-git-test-capture (lambda () (git-on-branch)))))))
"####;
    let expected = expect![[
        r#"OK (:config "candidate" :missing-config nil :status "" :unknown-branch (:error git-error :data ("Error running command: <git> --no-pager checkout does-not-exist\nerror: pathspec 'does-not-exist' did not match any file(s) known to git\n") :message "GIT Error: \"Error running command: <git> --no-pager checkout does-not-exist\\nerror: pathspec 'does-not-exist' did not match any file(s) known to git\\n\"") :uninitialized-branch (:error git-error :data ("Repository not initialized") :message "GIT Error: \"Repository not initialized\""))"#
    ]];
    ParityBatchCase::value(
        "command_arguments_config_and_failures_preserve_structured_api_results",
        elisp_form,
        expected,
    )
}

#[test]
fn git_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GIT_MELPA_PIN, "git.el")
            .expect("prepare revision-pinned Git.el source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "git-package-batch",
        "Git.el",
        &[
            release_repository_lifecycle_tracks_files_branches_tags_and_history(),
            checkout_reset_and_remove_restore_expected_worktree_versions(),
            multiple_stashes_can_be_applied_by_name_and_popped_in_order(),
            bare_repository_and_remote_management_support_local_delivery_topology(),
            command_arguments_config_and_failures_preserve_structured_api_results(),
        ],
    );
}
