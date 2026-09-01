use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TREEMACS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TREEMACS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const TREEMACS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'treemacs)

(defun neomacs-treemacs-test-project-state (project root)
  "Describe PROJECT with its path relative to deterministic ROOT."
  (list :name (treemacs-project->name project)
        :path (file-relative-name (treemacs-project->path project) root)
        :status (treemacs-project->path-status project)
        :disabled (treemacs-project->is-disabled? project)))

(defun neomacs-treemacs-test-workspace-state (workspace root)
  "Describe WORKSPACE and its ordered projects relative to ROOT."
  (list :name (treemacs-workspace->name workspace)
        :disabled (treemacs-workspace->is-disabled? workspace)
        :projects
        (mapcar (lambda (project)
                  (neomacs-treemacs-test-project-state project root))
                (treemacs-workspace->projects workspace))))

(defun neomacs-treemacs-test-validation-state (result root)
  "Normalize workspace-edit validation RESULT relative to ROOT."
  (pcase result
    ('success 'success)
    (`(error ,line ,message)
     (list 'error
           (if (stringp line)
               (replace-regexp-in-string
                (regexp-quote (directory-file-name root))
                "$ROOT" (substring-no-properties line) t t)
             line)
           (replace-regexp-in-string
            (regexp-quote (directory-file-name root))
            "$ROOT" (substring-no-properties message) t t)))))

(defun neomacs-treemacs-test-visible-nodes (root)
  "Capture the ordered visible tree nodes relative to ROOT."
  (save-excursion
    (goto-char (point-min))
    (let ((button (next-button (point-min) t))
          nodes)
      (while button
        (let ((path (treemacs-button-get button :path)))
          (push
           (list :label (treemacs--get-label-of button)
                 :path (cond
                        ((not (stringp path)) path)
                        ((equal path root) ".")
                        (t (file-relative-name path root)))
                 :state (treemacs-button-get button :state)
                 :depth (treemacs-button-get button :depth))
           nodes))
        (setq button (next-button (treemacs-button-end button))))
      (nreverse nodes))))

(defmacro neomacs-treemacs-test-with-state (&rest body)
  "Run BODY with an isolated real filesystem and Treemacs workspace state."
  (declare (indent 0) (debug t))
  `(let* ((test-root (make-temp-file "neomacs-treemacs-" t))
          (treemacs-persist-file (expand-file-name "state/treemacs-persist" test-root))
          (treemacs-last-error-persist-file
           (expand-file-name "state/treemacs-persist-error" test-root))
          (default-workspace (treemacs-workspace->create! :name "Default"))
          (treemacs--workspaces (list default-workspace))
          (treemacs--disabled-workspaces nil)
          (treemacs--scope-storage nil)
          (treemacs-create-project-functions nil)
          (treemacs-delete-project-functions nil)
          (treemacs-create-workspace-functions nil)
          (treemacs-delete-workspace-functions nil)
          (treemacs-rename-workspace-functions nil))
     (unwind-protect
         (progn
           (setf (treemacs-current-workspace) default-workspace)
           ,@body)
       (delete-directory test-root t))))
"##;

fn treemacs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREEMACS_MELPA_PIN, "treemacs.el")
        .expect("prepare revision-pinned Treemacs source below ./tmp")
        .with_prelude(TREEMACS_TEST_PRELUDE)
        .with_timeout(TREEMACS_TEST_TIMEOUT)
}

fn project_admission_rejects_overlaps_and_preserves_workspace_order() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let* ((application (expand-file-name "application" test-root))
         (nested (expand-file-name "src/generated" application))
         (docs (expand-file-name "docs" test-root))
         (tools (expand-file-name "tools" test-root))
         (missing (expand-file-name "missing" test-root)))
    (make-directory nested t)
    (make-directory docs t)
    (make-directory tools t)
    (with-temp-file (expand-file-name "deploy.el" application)
      (insert "(provide 'deploy)\n"))
    (let* ((add-application
            (treemacs-do-add-project-to-workspace application "Application"))
           (add-docs (treemacs-do-add-project-to-workspace docs "Docs"))
           (duplicate
            (treemacs-do-add-project-to-workspace application "Application Copy"))
           (nested-overlap
            (treemacs-do-add-project-to-workspace nested "Generated"))
           (parent-overlap
            (treemacs-do-add-project-to-workspace test-root "Monorepo"))
           (duplicate-name
            (treemacs-do-add-project-to-workspace tools "Application"))
           (invalid
            (treemacs-do-add-project-to-workspace missing "Missing"))
           (file-project
            (treemacs-is-path (expand-file-name "deploy.el" application)
                              :in-workspace)))
      (list
       :added
       (mapcar (lambda (result)
                 (list (car result)
                       (neomacs-treemacs-test-project-state
                        (cadr result) test-root)))
               (list add-application add-docs))
       :duplicate (list (car duplicate)
                        (treemacs-project->name (cadr duplicate)))
       :nested-overlap (list (car nested-overlap)
                             (treemacs-project->name (cadr nested-overlap)))
       :parent-overlap (list (car parent-overlap)
                             (treemacs-project->name (cadr parent-overlap)))
       :duplicate-name (list (car duplicate-name)
                             (treemacs-project->name (cadr duplicate-name)))
       :invalid invalid
       :lookup (neomacs-treemacs-test-project-state file-project test-root)
       :workspace-order
       (mapcar (lambda (project)
                 (neomacs-treemacs-test-project-state project test-root))
               (treemacs-workspace->projects (treemacs-current-workspace)))))))
"##;
    let expected = expect![[
        r####"OK (:added ((success (:name "Application" :path "application" :status local-readable :disabled nil)) (success (:name "Docs" :path "docs" :status local-readable :disabled nil))) :duplicate (duplicate-project "Application") :nested-overlap (duplicate-project "Application") :parent-overlap (includes-project "Application") :duplicate-name (duplicate-name "Application") :invalid (invalid-path "Path is not readable does not exist.") :lookup (:name "Application" :path "application" :status local-readable :disabled nil) :workspace-order ((:name "Application" :path "application" :status local-readable :disabled nil) (:name "Docs" :path "docs" :status local-readable :disabled nil)))"####
    ]];
    ParityBatchCase::value(
        "project_admission_rejects_overlaps_and_preserves_workspace_order",
        elisp_form,
        expected,
    )
}

fn workspace_lifecycle_reports_validation_switching_and_hook_order() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let (events)
    (setq treemacs-create-workspace-functions
          (list (lambda (workspace)
                  (push (list :created (treemacs-workspace->name workspace)) events)))
          treemacs-rename-workspace-functions
          (list (lambda (workspace old-name)
                  (push (list :renamed old-name
                              (treemacs-workspace->name workspace))
                        events)))
          treemacs-delete-workspace-functions
          (list (lambda (workspace)
                  (push (list :deleted (treemacs-workspace->name workspace)) events))))
    (let* ((create-deploy (treemacs-do-create-workspace "Deploy"))
           (deploy (cadr create-deploy))
           (create-review (treemacs-do-create-workspace "Review"))
           (review (cadr create-review))
           (invalid-blank (treemacs-do-create-workspace "   "))
           (invalid-multiline (treemacs-do-create-workspace "Bad\nName"))
           (duplicate (treemacs-do-create-workspace "Deploy"))
           (rename (treemacs-do-rename-workspace review "Code Review"))
           (invalid-rename (treemacs-do-rename-workspace deploy ""))
           (switch (treemacs-do-switch-workspace "Deploy"))
           (missing-switch (treemacs-do-switch-workspace "Missing"))
           (current-after-switch (treemacs-current-workspace))
           (remove-review (treemacs-do-remove-workspace "Code Review" nil))
           (remove-default (treemacs-do-remove-workspace "Default" nil))
           (remove-last (treemacs-do-remove-workspace "Deploy" nil)))
      (list
       :created
       (mapcar (lambda (result)
                 (list (car result) (treemacs-workspace->name (cadr result))))
               (list create-deploy create-review))
       :invalid (list invalid-blank invalid-multiline)
       :duplicate (list (car duplicate)
                        (treemacs-workspace->name (cadr duplicate)))
       :rename (list (car rename) (cadr rename)
                     (treemacs-workspace->name (caddr rename)))
       :invalid-rename invalid-rename
       :switch (list (car switch)
                     (treemacs-workspace->name (cadr switch)))
       :missing-switch missing-switch
       :current (treemacs-workspace->name current-after-switch)
       :remove-review
       (list (car remove-review)
             (treemacs-workspace->name (cadr remove-review))
             (mapcar #'treemacs-workspace->name (caddr remove-review)))
       :remove-default
       (list (car remove-default)
             (treemacs-workspace->name (cadr remove-default))
             (mapcar #'treemacs-workspace->name (caddr remove-default)))
       :remove-last remove-last
       :remaining
       (mapcar (lambda (workspace)
                 (neomacs-treemacs-test-workspace-state workspace test-root))
               (treemacs-workspaces))
       :hooks (nreverse events)))))
"##;
    let expected = expect![[
        r####"OK (:created ((success "Deploy") (success "Code Review")) :invalid ((invalid-name "   ") (invalid-name "Bad\nName")) :duplicate (duplicate-name "Deploy") :rename (success "Review" "Code Review") :invalid-rename (invalid-name "") :switch (success "Deploy") :missing-switch (workspace-not-found "Missing") :current "Deploy" :remove-review (success "Code Review" ("Default" "Deploy")) :remove-default (success "Default" ("Deploy")) :remove-last only-one-workspace :remaining ((:name "Deploy" :disabled nil :projects nil)) :hooks ((:created "Deploy") (:created "Review") (:renamed "Review" "Code Review") (:deleted "Code Review") (:deleted "Default")))"####
    ]];
    ParityBatchCase::value(
        "workspace_lifecycle_reports_validation_switching_and_hook_order",
        elisp_form,
        expected,
    )
}

fn persisted_workspaces_round_trip_real_project_paths_and_order() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let ((process-environment (copy-sequence process-environment))
        (noninteractive nil)
        (original-restored (get 'treemacs :state-is-restored)))
    (unwind-protect
        (progn
          (setenv "CI" nil)
          (put 'treemacs :state-is-restored t)
          (let ((application (expand-file-name "application" test-root))
                (docs (expand-file-name "docs" test-root))
                (review-notes (expand-file-name "review-notes" test-root)))
            (make-directory application t)
            (make-directory docs t)
            (make-directory review-notes t)
            (let* ((deploy-result (treemacs-do-create-workspace "Deploy"))
                   (deploy (cadr deploy-result)))
              (treemacs-do-switch-workspace deploy)
              (treemacs-do-add-project-to-workspace application "Application")
              (treemacs-do-add-project-to-workspace docs "Docs")
              (let* ((review-result (treemacs-do-create-workspace "Review"))
                     (review (cadr review-result)))
                (treemacs-do-switch-workspace review)
                (treemacs-do-add-project-to-workspace review-notes "Notes")
                (let* ((persisted
                        (with-temp-buffer
                          (insert-file-contents treemacs-persist-file)
                          (buffer-string)))
                       (persisted-display
                        (replace-regexp-in-string
                         (regexp-quote (directory-file-name test-root))
                         "$ROOT" persisted t t))
                       (validation
                        (with-temp-buffer
                          (treemacs--validate-persist-lines
                           (treemacs--read-persist-lines persisted))))
                       (reset-workspace
                        (treemacs-workspace->create! :name "Reset")))
                  (setq treemacs--workspaces (list reset-workspace)
                        treemacs--disabled-workspaces nil)
                  (setf (treemacs-current-workspace) reset-workspace)
                  (treemacs--restore)
                  (let* ((restored
                          (mapcar
                           (lambda (workspace)
                             (neomacs-treemacs-test-workspace-state
                              workspace test-root))
                           (treemacs-workspaces)))
                         (restored-deploy
                          (treemacs-find-workspace-by-name "Deploy"))
                         (restored-file-project
                          (treemacs-with-workspace restored-deploy
                            (treemacs-is-path
                             (expand-file-name "application/deploy.el" test-root)
                             :in-workspace))))
                    (list
                     :persisted persisted-display
                     :validation validation
                     :restored restored
                     :restored-file-project
                     (neomacs-treemacs-test-project-state
                      restored-file-project test-root))))))))
      (put 'treemacs :state-is-restored original-restored))))
"##;
    let expected = expect![[
        r####"OK (:persisted "* Deploy\n** Application\n - path :: $ROOT/application\n** Docs\n - path :: $ROOT/docs\n* Review\n** Notes\n - path :: $ROOT/review-notes\n" :validation success :restored ((:name "Deploy" :disabled nil :projects ((:name "Application" :path "application" :status local-readable :disabled nil) (:name "Docs" :path "docs" :status local-readable :disabled nil))) (:name "Review" :disabled nil :projects ((:name "Notes" :path "review-notes" :status local-readable :disabled nil)))) :restored-file-project (:name "Application" :path "application" :status local-readable :disabled nil))"####
    ]];
    ParityBatchCase::value(
        "persisted_workspaces_round_trip_real_project_paths_and_order",
        elisp_form,
        expected,
    )
}

fn workspace_editor_rejects_malformed_missing_and_overlapping_records() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let* ((application (expand-file-name "application" test-root))
         (nested (expand-file-name "src" application))
         (docs (expand-file-name "docs" test-root))
         (missing (expand-file-name "missing" test-root)))
    (make-directory nested t)
    (make-directory docs t)
    (with-temp-buffer
      (rename-buffer treemacs--org-edit-buffer-name t)
      (let ((valid
             (list "# edited by a user"
                   ""
                   "* Deploy"
                   "** Application"
                   (concat " - path :: " application)
                   "** Docs"
                   (concat " - path :: " docs)))
            (overlap
             (list "* Deploy"
                   "** Application"
                   (concat " - path :: " application)
                   "** Generated"
                   (concat " - path :: " nested)))
            (missing-property
             (list "* Deploy" "** Application" "* Review"))
            (orphan-record
             (list "* Deploy" "** Application"
                   (concat " - path :: " application)
                   "unexpected"))
            (all-projects-disabled
             (list "* Deploy" "** COMMENT Application"
                   (concat " - path :: " application)))
            (all-workspaces-disabled
             (list "* COMMENT Deploy" "** Application"
                   (concat " - path :: " application)))
            (missing-on-disk
             (list "* Deploy" "** Missing"
                   (concat " - path :: " missing))))
        (list
         :valid
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines
           (treemacs--read-persist-lines (mapconcat #'identity valid "\n")))
          test-root)
         :overlap
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines overlap) test-root)
         :missing-property
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines missing-property) test-root)
         :orphan-record
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines orphan-record) test-root)
         :all-projects-disabled
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines all-projects-disabled) test-root)
         :all-workspaces-disabled
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines all-workspaces-disabled) test-root)
         :missing-on-disk
         (neomacs-treemacs-test-validation-state
          (treemacs--validate-persist-lines missing-on-disk) test-root))))))
"##;
    let expected = expect![[
        r####"OK (:valid success :overlap (error " - path :: $ROOT/application/src" "Path '$ROOT/application/src' appears in the workspace more than once.") :missing-property (error "** Application" "Project name must be followed by path declaration") :orphan-record (error " - path :: $ROOT/application" "Path property must be followed by the next workspace or project") :all-projects-disabled (error " - path :: $ROOT/application" "Workspace must contain at least 1 project that is not disabled.") :all-workspaces-disabled (error " - path :: $ROOT/application" "There must be at least 1 worspace that is not disabled.") :missing-on-disk (error " - path :: $ROOT/missing" "File '$ROOT/missing' does not exist"))"####
    ]];
    ParityBatchCase::value(
        "workspace_editor_rejects_malformed_missing_and_overlapping_records",
        elisp_form,
        expected,
    )
}

fn visiting_files_selects_their_workspace_and_uses_the_fallback_for_unowned_files()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let* ((application (expand-file-name "application" test-root))
         (docs (expand-file-name "docs" test-root))
         (misc (expand-file-name "misc" test-root))
         (application-file (expand-file-name "src/deploy.el" application))
         (docs-file (expand-file-name "guide.md" docs))
         (misc-file (expand-file-name "notes.txt" misc)))
    (make-directory (file-name-directory application-file) t)
    (make-directory docs t)
    (make-directory misc t)
    (with-temp-file application-file (insert "(provide 'deploy)\n"))
    (with-temp-file docs-file (insert "# Deployment Guide\n"))
    (with-temp-file misc-file (insert "unowned notes\n"))
    (let* ((deploy (cadr (treemacs-do-create-workspace "Deploy")))
           (review (cadr (treemacs-do-create-workspace "Review"))))
      (treemacs-do-switch-workspace deploy)
      (treemacs-do-add-project-to-workspace application "Application")
      (treemacs-do-switch-workspace review)
      (treemacs-do-add-project-to-workspace docs "Docs")
      (let (application-state docs-state fallback-state)
        (setq treemacs--scope-storage nil)
        (with-temp-buffer
          (setq buffer-file-name application-file)
          (let* ((workspace (treemacs-current-workspace))
                 (project (treemacs-is-path application-file :in-workspace)))
            (setq application-state
                  (list :workspace (treemacs-workspace->name workspace)
                        :project (treemacs-project->name project)))))
        (setq treemacs--scope-storage nil)
        (with-temp-buffer
          (setq buffer-file-name docs-file)
          (let* ((workspace (treemacs-current-workspace))
                 (project (treemacs-is-path docs-file :in-workspace)))
            (setq docs-state
                  (list :workspace (treemacs-workspace->name workspace)
                        :project (treemacs-project->name project)))))
        (setq treemacs--scope-storage nil)
        (with-temp-buffer
          (setq buffer-file-name misc-file)
          (let ((workspace (treemacs-current-workspace)))
            (setq fallback-state
                  (list :workspace (treemacs-workspace->name workspace)
                        :project (treemacs-is-path misc-file :in-workspace)))))
        (list :application application-state
              :docs docs-state
              :fallback fallback-state)))))
"##;
    let expected = expect![[
        r####"OK (:application (:workspace "Deploy" :project "Application") :docs (:workspace "Review" :project "Docs") :fallback (:workspace "Default" :project nil))"####
    ]];
    ParityBatchCase::value(
        "visiting_files_selects_their_workspace_and_uses_the_fallback_for_unowned_files",
        elisp_form,
        expected,
    )
}

fn configured_first_found_hook_surfaces_treemacs_hook_dispatch_bug() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let* ((application (expand-file-name "application" test-root))
         (application-file (expand-file-name "deploy.el" application)))
    (make-directory application t)
    (with-temp-file application-file (insert "(provide 'deploy)\n"))
    (let ((deploy (cadr (treemacs-do-create-workspace "Deploy"))))
      (treemacs-do-switch-workspace deploy)
      (treemacs-do-add-project-to-workspace application "Application")
      (setq treemacs--scope-storage nil)
      (let ((treemacs-workspace-first-found-functions '(ignore)))
        (with-temp-buffer
          (setq buffer-file-name application-file)
          (treemacs-current-workspace))))))
"##;
    let expected = expect![[r####"ERR (wrong-type-argument symbolp (ignore))"####]];
    ParityBatchCase::signal(
        "configured_first_found_hook_surfaces_treemacs_hook_dispatch_bug",
        elisp_form,
        expected,
    )
}

fn terminal_tree_refreshes_sorting_hidden_files_and_expanded_directories() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-treemacs-test-with-state
  (let* ((application (expand-file-name "application" test-root))
         (src (expand-file-name "src" application))
         (assets (expand-file-name "assets" application))
         (hidden (expand-file-name ".env" application))
         (readme (expand-file-name "README.md" application))
         (release (expand-file-name "release.txt" application))
         (main (expand-file-name "main.rs" src))
         (util (expand-file-name "util.rs" src))
         treemacs-buffer treemacs-window)
    (make-directory src t)
    (make-directory assets t)
    (dolist (file (list hidden readme release main util))
      (with-temp-file file (insert (file-name-nondirectory file) "\n")))
    (treemacs-do-add-project-to-workspace application "Application")
    (let ((treemacs-collapse-dirs 0)
          (treemacs-expand-after-init t)
          (treemacs-follow-after-init nil)
          (treemacs-show-hidden-files nil)
          (treemacs-sorting 'alphabetic-asc)
          (treemacs-filewatch-mode nil)
          (treemacs-git-mode nil)
          (treemacs-space-between-root-nodes nil))
      (unwind-protect
          (progn
            (treemacs)
            (setq treemacs-buffer (treemacs-get-local-buffer)
                  treemacs-window (treemacs-get-local-window))
            (with-current-buffer treemacs-buffer
              (let ((initial
                     (neomacs-treemacs-test-visible-nodes application)))
                (goto-char (treemacs-find-visible-node src))
                (treemacs-toggle-node)
                (let ((expanded-src
                       (neomacs-treemacs-test-visible-nodes application)))
                  (treemacs-toggle-show-dotfiles)
                  (let ((with-hidden
                         (neomacs-treemacs-test-visible-nodes application)))
                    (setq treemacs-sorting 'alphabetic-desc)
                    (goto-char (treemacs-find-visible-node application))
                    (treemacs-refresh)
                    (list
                     :mode major-mode
                     :terminal (not (display-graphic-p))
                     :initial initial
                     :expanded-src expanded-src
                     :with-hidden with-hidden
                     :descending
                     (neomacs-treemacs-test-visible-nodes application)))))))
        (when (window-live-p treemacs-window)
          (delete-window treemacs-window))
        (when (buffer-live-p treemacs-buffer)
          (kill-buffer treemacs-buffer))))))
"##;
    let expected = expect![[
        r####"OK (:mode treemacs-mode :terminal t :initial ((:label "Application" :path "." :state root-node-open :depth 0) (:label "assets" :path "assets" :state dir-node-closed :depth 1) (:label "src" :path "src" :state dir-node-closed :depth 1) (:label "README.md" :path "README.md" :state file-node-closed :depth 1) (:label "release.txt" :path "release.txt" :state file-node-closed :depth 1)) :expanded-src ((:label "Application" :path "." :state root-node-open :depth 0) (:label "assets" :path "assets" :state dir-node-closed :depth 1) (:label "src" :path "src" :state dir-node-open :depth 1) (:label "main.rs" :path "src/main.rs" :state file-node-closed :depth 2) (:label "util.rs" :path "src/util.rs" :state file-node-closed :depth 2) (:label "README.md" :path "README.md" :state file-node-closed :depth 1) (:label "release.txt" :path "release.txt" :state file-node-closed :depth 1)) :with-hidden ((:label "Application" :path "." :state root-node-open :depth 0) (:label "assets" :path "assets" :state dir-node-closed :depth 1) (:label "src" :path "src" :state dir-node-open :depth 1) (:label "main.rs" :path "src/main.rs" :state file-node-closed :depth 2) (:label "util.rs" :path "src/util.rs" :state file-node-closed :depth 2) (:label ".env" :path ".env" :state file-node-closed :depth 1) (:label "README.md" :path "README.md" :state file-node-closed :depth 1) (:label "release.txt" :path "release.txt" :state file-node-closed :depth 1)) :descending ((:label "Application" :path "." :state root-node-open :depth 0) (:label "src" :path "src" :state dir-node-open :depth 1) (:label "util.rs" :path "src/util.rs" :state file-node-closed :depth 2) (:label "main.rs" :path "src/main.rs" :state file-node-closed :depth 2) (:label "assets" :path "assets" :state dir-node-closed :depth 1) (:label "release.txt" :path "release.txt" :state file-node-closed :depth 1) (:label "README.md" :path "README.md" :state file-node-closed :depth 1) (:label ".env" :path ".env" :state file-node-closed :depth 1)))"####
    ]];
    ParityBatchCase::value(
        "terminal_tree_refreshes_sorting_hidden_files_and_expanded_directories",
        elisp_form,
        expected,
    )
    .fresh_process()
}

#[test]
fn treemacs_package_batch() {
    assert_oracle_batch_cases(
        treemacs_oracle(),
        "treemacs-package-batch",
        "Treemacs",
        &[
            project_admission_rejects_overlaps_and_preserves_workspace_order(),
            workspace_lifecycle_reports_validation_switching_and_hook_order(),
            persisted_workspaces_round_trip_real_project_paths_and_order(),
            workspace_editor_rejects_malformed_missing_and_overlapping_records(),
            visiting_files_selects_their_workspace_and_uses_the_fallback_for_unowned_files(),
            configured_first_found_hook_surfaces_treemacs_hook_dispatch_bug(),
            terminal_tree_refreshes_sorting_hidden_files_and_expanded_directories(),
        ],
    );
}
