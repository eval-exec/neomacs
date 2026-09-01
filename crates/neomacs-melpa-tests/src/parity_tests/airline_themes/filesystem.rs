use expect_test::expect;

use super::ParityBatchCase;

fn airline_themes_reads_symbolic_and_detached_git_heads_with_exact_shortening() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_reads_symbolic_and_detached_git_heads_with_exact_shortening",
        r##"(let* ((root
                 (expand-file-name
                  "git-heads/"
                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (symbolic (expand-file-name "symbolic/HEAD" root))
                (detached (expand-file-name "detached/HEAD" root))
                (malformed (expand-file-name "malformed/HEAD" root)))
         (make-directory (file-name-directory symbolic) t)
         (make-directory (file-name-directory detached) t)
         (make-directory (file-name-directory malformed) t)
         (with-temp-file symbolic
           (insert "ref: refs/heads/feature/practical-airline\n"))
         (with-temp-file detached
           (insert "0123456789abcdef0123456789abcdef01234567\n"))
         (with-temp-file malformed
           (insert "not a git head"))
         (list
          (airline--git-branch-from-head-file symbolic)
          (airline--git-branch-from-head-file detached)
          (airline--git-branch-from-head-file malformed)
          (airline--git-branch-from-head-file
           (expand-file-name "missing/HEAD" root))))"##,
        expect![[r#"OK ("practical-airline" "0123456" nil nil)"#]],
    )
}

fn airline_themes_discovers_real_nested_repository_branches_and_non_repositories() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_discovers_real_nested_repository_branches_and_non_repositories",
        r##"(progn
         (require 'esh-ext)
         (let* ((root
                 (expand-file-name
                  "repository/"
                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (nested
                 (expand-file-name
                  "src/deep/component/" root))
                (head
                 (expand-file-name ".git/HEAD" root))
                ;; `airline-curr-dir-git-branch-string' resolves through
                ;; `locate-dominating-file', and the sandbox lives inside the
                ;; neomacs worktree - so no directory under it is outside a
                ;; repository.  A sandbox path here reports whatever branch
                ;; this checkout happens to be on, which would make the
                ;; expectation pass on `main' and fail on every feature
                ;; branch.  The filesystem root is the only directory with no
                ;; repository above it.
                (outside "/"))
         (make-directory nested t)
         (make-directory (file-name-directory head) t)
         (with-temp-file head
           (insert "ref: refs/heads/integration/modeline\n"))
         (list
          (airline-curr-dir-git-branch-string root)
          (airline-curr-dir-git-branch-string nested)
          (airline-curr-dir-git-branch-string outside)
          (airline-curr-dir-git-branch-string
           "/ssh:host:/work/project/"))))"##,
        expect![[r#"OK ("modeline" "modeline" nil nil)"#]],
    )
}

fn airline_themes_resolves_one_level_gitdir_indirection_like_a_real_submodule() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_resolves_one_level_gitdir_indirection_like_a_real_submodule",
        r##"(progn
         (require 'esh-ext)
         (let* ((sandbox
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                (root
                 (expand-file-name "super/widget/" sandbox))
                (git-file (expand-file-name ".git" root))
                (admin
                 (expand-file-name
                  "../admin/modules/widget/"
                  root))
                (head (expand-file-name "HEAD" admin)))
         (make-directory root t)
         (make-directory admin t)
         (make-directory
          (expand-file-name "nested/" root) t)
         (with-temp-file git-file
           (insert "gitdir: ../admin/modules/widget\n"))
         (with-temp-file head
           (insert "ref: refs/heads/submodule-release\n"))
         (list
          (file-regular-p git-file)
          (file-regular-p head)
          (airline-curr-dir-git-branch-string root)
          (airline-curr-dir-git-branch-string
           (expand-file-name "nested/" root)))))"##,
        expect![[r#"OK (t t "submodule-release" "submodule-release")"#]],
    )
}

fn airline_themes_shortens_real_directory_shapes_to_the_requested_budget() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_shortens_real_directory_shapes_to_the_requested_budget",
        r##"(let ((cases
                '(("/alpha/beta/gamma/delta/file.el" 80)
                  ("/alpha/beta/gamma/delta/file.el" 24)
                  ("/alpha/beta/gamma/delta/file.el" 14)
                  ("/one/two/three/" 10)
                  ("relative/deeply/nested/path" 12)
                  ("/single" 1)
                  ("/" 0))))
         (mapcar
          (lambda (case)
            (let ((result
                   (airline-shorten-directory
                    (car case) (cadr case))))
              (list case result (length result))))
          cases))"##,
        expect![[
            r#"OK ((("/alpha/beta/gamma/delta/file.el" 80) "/alpha/beta/gamma/delta/file.el" 31) (("/alpha/beta/gamma/delta/file.el" 24) "/a/b/g/delta/file.el" 20) (("/alpha/beta/gamma/delta/file.el" 14) "/a/b/g/d/file.el" 16) (("/one/two/three/" 10) "/o/t/t/" 7) (("relative/deeply/nested/path" 12) "r/d/n/path" 10) (("/single" 1) "/single" 7) (("/" 0) "/" 1))"#
        ]],
    )
}

fn airline_themes_eshell_prompt_renders_real_directory_branch_and_face_segments() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_eshell_prompt_renders_real_directory_branch_and_face_segments",
        r##"(condition-case prompt-error
         (let* ((root
                 (expand-file-name
                  "prompt/project/very/long/component/"
                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (head
                 (expand-file-name
                  "../../../../.git/HEAD" root)))
         (make-directory root t)
         (make-directory (file-name-directory head) t)
         (with-temp-file head
           (insert "ref: refs/heads/prompt-workflow\n"))
         (require 'esh-ext)
         (require 'em-dirs)
         (with-temp-buffer
           (setq default-directory root
                 airline-display-directory
                 'airline-directory-full)
           (airline-themes-set-eshell-prompt)
           (let* ((prompt
                   (funcall eshell-prompt-function))
                  (directory-position
                   (string-match "component" prompt))
                  (branch-position
                   (string-match "prompt-workflow" prompt)))
             (list
              (substring-no-properties prompt)
              eshell-highlight-prompt
              eshell-prompt-regexp
              (mapcar
               (lambda (sample)
                 (list
                  (car sample)
                  (aref prompt (cdr sample))
                  (copy-tree
                   (text-properties-at
                    (cdr sample) prompt))))
               `((leading-space . 0)
                 (first-separator . 1)
                 (directory . ,directory-position)
                 (branch . ,branch-position)
                 (trailing-space
                  . ,(1- (length prompt)))))))))
      (error (list 'prompt-error prompt-error)))"##,
        expect![[
            r##"OK ("  [ORACLE-SANDBOX]/prompt/project/very/long/component  prompt-workflow  $ " t "^ [^#$]* [#$] " ((leading-space 32 (face (:foreground "#141413" :background "#0a9dff"))) (first-separator 57520 (face (:foreground "#0a9dff" :background "#005faf"))) (directory 99 (face (:foreground "#f4cf86" :background "#005faf"))) (branch 112 (face (:foreground "#0a9dff" :background "#242321"))) (trailing-space 32 (face nil))))"##
        ]],
    )
}

pub(super) fn filesystem_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        airline_themes_reads_symbolic_and_detached_git_heads_with_exact_shortening(),
        airline_themes_discovers_real_nested_repository_branches_and_non_repositories(),
        airline_themes_resolves_one_level_gitdir_indirection_like_a_real_submodule(),
        airline_themes_shortens_real_directory_shapes_to_the_requested_budget(),
        airline_themes_eshell_prompt_renders_real_directory_branch_and_face_segments(),
    ]
}
