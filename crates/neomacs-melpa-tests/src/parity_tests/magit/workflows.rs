use expect_test::expect;

use super::ParityBatchCase;

fn magit_stage_unstage_and_extend_preserve_partial_worktree_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_stage_unstage_and_extend_preserve_partial_worktree_changes",
        r##"(let* ((root (make-temp-file "magit-partial-workflow-" t))
                    (default-directory (file-name-as-directory root))
                    (tracked "tracked.txt")
                    (notes "release notes λ.txt"))
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (with-temp-file tracked
                       (insert "base\n"))
                     (magit-git "add" "--" tracked)
                     (magit-git "commit" "-m" "initial")
                     (let ((original-head (magit-rev-parse "HEAD")))
                       (with-temp-file tracked
                         (insert "base\nstaged line\n"))
                       (magit-stage-files (list tracked))
                       (with-temp-file tracked
                         (insert "base\nstaged line\nunstaged line\n"))
                       (with-temp-file notes
                         (insert "release note α\n"))
                       (magit-stage-files (list notes))
                       (let ((before-unstage
                              (list
                               (magit-staged-files)
                               (magit-unstaged-files)
                               (magit-untracked-files)
                               (magit-git-lines "status" "--short")
                               (magit-git-lines "show" ":tracked.txt"))))
                         (magit-unstage-files (list notes))
                         (let ((after-unstage
                                (list
                                 (magit-staged-files)
                                 (magit-unstaged-files)
                                 (magit-untracked-files)
                                 (magit-git-lines "status" "--short"))))
                           (magit-stage-files (list notes))
                           (let ((process (magit-commit-extend nil t)))
                             (neomacs-magit-test-wait-for-process process)
                             (list
                              before-unstage
                              after-unstage
                              (list
                               (process-status process)
                               (process-exit-status process))
                              (not
                               (equal original-head
                                      (magit-rev-parse "HEAD")))
                              (magit-git-string "rev-list" "--count" "HEAD")
                              (magit-git-string "log" "-1" "--format=%s")
                              (magit-git-lines "show" "HEAD:tracked.txt")
                              (magit-git-lines
                               "show" (concat "HEAD:" notes))
                              (with-temp-buffer
                                (insert-file-contents tracked)
                                (buffer-string))
                              (magit-staged-files)
                              (magit-unstaged-files)
                              (magit-untracked-files)
                              (magit-git-lines "status" "--short")))))))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK ((("release notes λ.txt" "tracked.txt") ("tracked.txt") nil ("A  \"release notes \\316\\273.txt\"" "MM tracked.txt") ("base" "staged line")) (("tracked.txt") ("tracked.txt") ("release notes λ.txt") ("MM tracked.txt" "?? \"release notes \\316\\273.txt\"")) (exit 0) t "1" "initial" ("base" "staged line") ("release note α") "base\nstaged line\nunstaged line\n" nil ("tracked.txt") nil (" M tracked.txt"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![magit_stage_unstage_and_extend_preserve_partial_worktree_changes()]
}
