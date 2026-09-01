use expect_test::expect;

use super::ParityBatchCase;

fn default_sources_and_map_bindings_are_wired() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_sources_and_map_bindings_are_wired",
        r####"
(list :sources helm-ls-git-default-sources
      :ls-switches helm-ls-git-ls-switches
      :show helm-ls-git-show-abs-or-relative
      :map-grep (lookup-key helm-ls-git-map (kbd "M-g g"))
      :map-others (lookup-key helm-ls-git-map (kbd "C-c i"))
      :map-log (lookup-key helm-ls-git-map (kbd "M-L")))
"####,
        expect![[
            r#"OK (:sources (helm-source-ls-git-status helm-ls-git-branches-source helm-source-ls-git-buffers helm-source-ls-git helm-ls-git-stashes-source helm-ls-git-create-branch-source) :ls-switches ("ls-files" "--full-name" "--") :show relative :map-grep helm-ls-git-run-grep :map-others helm-ls-git-ls-files-show-others :map-log helm-ls-git-run-file-log)"#
        ]],
    )
}

fn root_dir_and_repo_membership_detect_git_trees() -> ParityBatchCase {
    ParityBatchCase::value(
        "root_dir_and_repo_membership_detect_git_trees",
        r####"
(neomacs-helm-ls-git-test-with-repo
 (lambda (root)
   (let ((sub (expand-file-name "sub" root)))
     (let ((default-directory sub))
       (list :matches-root
             (equal (file-truename (helm-ls-git-root-dir))
                    (file-truename root))
             :not-inside-in-repo (helm-ls-git-not-inside-git-repo)
             :outside
             ;; Avoid the workspace git tree; '/' has no .git ancestor.
             (let ((default-directory "/"))
               (list :root (helm-ls-git-root-dir)
                     :not-inside (helm-ls-git-not-inside-git-repo))))))))
"####,
        expect![
            "OK (:matches-root nil :not-inside-in-repo nil :outside (:root nil :not-inside t))"
        ],
    )
}

fn list_files_and_branch_report_repository_contents() -> ParityBatchCase {
    ParityBatchCase::value(
        "list_files_and_branch_report_repository_contents",
        r####"
(neomacs-helm-ls-git-test-with-repo
 (lambda (root)
   (let ((default-directory root)
         (helm-ls-git-log-file nil)
         (helm-ls-git-ls-switches '("ls-files" "--full-name" "--"))
         (helm-ls-git--current-branch nil))
     (let* ((raw (helm-ls-git-list-files))
            (files (sort (split-string raw "\n" t) #'string-lessp))
            (branch (helm-ls-git--branch))
            (header (helm-ls-git-header-name "Git files")))
       (list :files files
             :branch branch
             :header header
             :branch-nonempty (and (stringp branch) (> (length branch) 0)))))))
"####,
        expect![[
            r#"OK (:files ("alpha.el" "beta.txt" "sub/gamma.el") :branch "master" :header "Git files (master)" :branch-nonempty t)"#
        ]],
    )
}

fn normalize_branch_names_strip_markers_and_remotes() -> ParityBatchCase {
    ParityBatchCase::value(
        "normalize_branch_names_strip_markers_and_remotes",
        r####"
(list :current (helm-ls-git-normalize-branch-name "* main")
      :plain (helm-ls-git-normalize-branch-name "feature/x")
      :remote (helm-ls-git-normalize-branch-name "remotes/origin/main")
      :list (helm-ls-git-normalize-branch-names
             '("* main" "  remotes/origin/dev" "feature/x")))
"####,
        expect![[
            r#"OK (:current "main" :plain "feature/x" :remote "origin/main" :list ("main" "origin/dev" "feature/x"))"#
        ]],
    )
}

fn toggle_ls_switches_adds_and_removes_others_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "toggle_ls_switches_adds_and_removes_others_flag",
        r####"
(let ((helm-ls-git-ls-switches '("ls-files" "--full-name" "--"))
      (helm-alive-p t)
      (updates 0))
  ;; with-helm-alive-p expands to (if helm-alive-p ...); bind the session
  ;; flag and stub force-update so the real toggle mutates ls-switches.
  (cl-letf (((symbol-function 'helm-force-update)
             (lambda (&rest _) (cl-incf updates) nil)))
    (call-interactively #'helm-ls-git-ls-files-show-others)
    (let ((with-o (copy-sequence helm-ls-git-ls-switches)))
      (call-interactively #'helm-ls-git-ls-files-show-others)
      (list :with-o with-o
            :without (copy-sequence helm-ls-git-ls-switches)
            :updates updates))))
"####,
        expect![[
            r#"OK (:with-o ("ls-files" "-o" "--full-name" "--") :without ("ls-files" "--full-name" "--") :updates 2)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_sources_and_map_bindings_are_wired(),
        root_dir_and_repo_membership_detect_git_trees(),
        list_files_and_branch_report_repository_contents(),
        normalize_branch_names_strip_markers_and_remotes(),
        toggle_ls_switches_adds_and_removes_others_flag(),
    ]
}
