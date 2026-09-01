use expect_test::expect;

use super::ParityBatchCase;

fn magit_repository_identity_config_and_branch_queries_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_repository_identity_config_and_branch_queries_match",
        r##"(let* ((root (make-temp-file "magit-repo-" t))
                    (default-directory (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (magit-git "commit" "-m" "init" "--allow-empty")
                     (magit-git "config" "a.b" "first")
                     (magit-git "config" "--add" "a.b" "second")
                     (magit-git "config" "feature.enabled" "true")
                     (magit-git
                      "update-ref"
                      "refs/remotes/origin/master"
                      "master")
                     (magit-git
                      "update-ref"
                      "refs/remotes/upstream/main"
                      "master")
                     (list
                      (equal (magit-toplevel) default-directory)
                      (magit-bare-repo-p)
                      (magit-get-all "a.b")
                      (magit-get "a" "b")
                      (magit-get-boolean "feature.enabled")
                      (magit-list-branch-names)
                      (magit-list-local-branch-names)
                      (magit-list-remote-branch-names)
                      (magit-list-remote-branch-names "origin")
                      (magit-list-remote-branch-names "origin" t)))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK (t nil ("first" "second") "second" t ("master" "origin/master" "upstream/main") ("master") ("origin/master" "upstream/main") ("origin/master") ("master"))"#
        ]],
    )
}

fn magit_tag_queries_distinguish_current_reachable_and_next_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_tag_queries_distinguish_current_reachable_and_next_tags",
        r##"(let* ((root (make-temp-file "magit-tags-" t))
                    (default-directory (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (magit-git "commit" "-m" "one" "--allow-empty")
                     (let ((before
                            (list
                             (magit-get-current-tag)
                             (magit-get-next-tag))))
                       (magit-git "tag" "v1")
                       (let ((at-v1
                              (list
                               (magit-get-current-tag)
                               (magit-get-next-tag))))
                         (magit-git
                          "commit" "-m" "two" "--allow-empty")
                         (magit-git "tag" "v2")
                         (magit-git
                          "commit" "-m" "three" "--allow-empty")
                         (let ((after-v2
                                (list
                                 (magit-get-current-tag)
                                 (magit-get-next-tag))))
                           (magit-git
                            "commit" "-m" "four" "--allow-empty")
                           (magit-git "tag" "v4")
                           (magit-git "reset" "--hard" "HEAD~")
                           (list
                            before
                            at-v1
                            after-v2
                            (magit-get-current-tag)
                            (magit-get-next-tag))))))
                 (delete-directory root t)))"##,
        expect![[r#"OK ((nil nil) ("v1" nil) ("v2" nil) "v2" "v4")"#]],
    )
}

fn magit_toplevel_handles_nested_directories_gitdir_and_symlink_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_toplevel_handles_nested_directories_gitdir_and_symlink_entry",
        r##"(let* ((root (make-temp-file "magit-top-" t))
                    (default-directory (file-name-as-directory root))
                    (repo (expand-file-name "repo/" root))
                    (find-file-visit-truename nil))
               (unwind-protect
                   (progn
                     (make-directory repo)
                     (let ((default-directory repo))
                       (magit-git "init" "."))
                     (make-directory
                      (expand-file-name "sub/inner/" repo) t)
                     (make-symbolic-link "repo"
                                         (expand-file-name "repo-link" root))
                     (list
                      (equal (magit-toplevel repo) repo)
                      (equal
                       (magit-toplevel
                        (expand-file-name "sub/inner/" repo))
                       repo)
                      (equal
                       (magit-toplevel
                        (expand-file-name ".git/objects/" repo))
                       repo)
                      (equal
                       (magit-toplevel
                        (expand-file-name "repo-link/" root))
                       (expand-file-name "repo-link/" root))))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t t t t)"#]],
    )
}

fn magit_bare_repository_detection_distinguishes_repository_kinds() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_bare_repository_detection_distinguishes_repository_kinds",
        r##"(let* ((root (make-temp-file "magit-bare-" t))
                    (bare (expand-file-name "bare.git/" root))
                    (work (expand-file-name "work/" root)))
               (unwind-protect
                   (progn
                     (make-directory bare)
                     (make-directory work)
                     (let ((default-directory bare))
                       (magit-git "init" "--bare" "."))
                     (let ((default-directory work))
                       (magit-git "init" "."))
                     (list
                      (let ((default-directory bare))
                        (magit-bare-repo-p))
                      (let ((default-directory work))
                        (magit-bare-repo-p))
                      (file-name-absolute-p
                       (magit-toplevel bare))
                      (equal
                       (magit-toplevel work)
                       (file-name-as-directory work))))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t nil t t)"#]],
    )
}

fn magit_revision_and_process_queries_preserve_values_order_and_exit_codes() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_revision_and_process_queries_preserve_values_order_and_exit_codes",
        r##"(let* ((root (make-temp-file "magit-revisions-" t))
                    (default-directory (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (with-temp-file "tracked.txt" (insert "one\n"))
                     (magit-git "add" "tracked.txt")
                     (magit-git "commit" "-m" "alpha")
                     (with-temp-file "tracked.txt" (insert "two\n"))
                     (magit-git "commit" "-am" "beta")
                     (let* ((short (magit-rev-parse "--short" "HEAD"))
                            (full (magit-rev-parse "HEAD"))
                            (clean-code
                             (magit-git-exit-code
                              "diff" "--quiet" "HEAD")))
                       (with-temp-file "tracked.txt" (insert "dirty\n"))
                       (list
                        (and
                         (string-match-p
                          "\\`[[:xdigit:]]+\\'" short)
                         t)
                        (equal (magit-rev-verify "HEAD") full)
                        (equal (magit-commit-p "HEAD") full)
                        (magit-commit-p "missing-revision")
                        (magit-git-lines "log" "--format=%s")
                        (magit-git-string
                         "rev-parse" "--abbrev-ref" "HEAD")
                        clean-code
                        (magit-git-exit-code
                         "diff" "--quiet" "HEAD"))))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t t t nil ("beta" "alpha") "master" 0 1)"#]],
    )
}

pub(super) fn git_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_repository_identity_config_and_branch_queries_match(),
        magit_tag_queries_distinguish_current_reachable_and_next_tags(),
        magit_toplevel_handles_nested_directories_gitdir_and_symlink_entry(),
        magit_bare_repository_detection_distinguishes_repository_kinds(),
        magit_revision_and_process_queries_preserve_values_order_and_exit_codes(),
    ]
}
