use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, PROJECTILE_MELPA_PIN, TREEMACS_MELPA_PIN, TREEMACS_PROJECTILE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'treemacs)
(require 'projectile)
(require 'treemacs-projectile)
(require 'treemacs-mouse-interface)

(defun neomacs-treemacs-projectile-test-normalize (root value)
  "Replace deterministic ROOT in VALUE with <ROOT>."
  (cond
   ((stringp value)
    (replace-regexp-in-string
     (regexp-quote (directory-file-name root)) "<ROOT>" value t t))
   ((consp value)
    (cons (neomacs-treemacs-projectile-test-normalize root (car value))
          (neomacs-treemacs-projectile-test-normalize root (cdr value))))
   ((vectorp value)
    (apply #'vector
           (mapcar
            (lambda (item)
              (neomacs-treemacs-projectile-test-normalize root item))
            value)))
   (t value)))

(defun neomacs-treemacs-projectile-test-project (project root)
  "Return PROJECT's stable name and path relative to ROOT."
  (list :name (treemacs-project->name project)
        :path (file-relative-name (treemacs-project->path project) root)
        :status (treemacs-project->path-status project)))

(defun neomacs-treemacs-projectile-test-call-with-state (function)
  "Call FUNCTION with an isolated filesystem, workspace, and Projectile state."
  (let* ((test-root (make-temp-file "neomacs-treemacs-projectile-" t))
         (state-directory (expand-file-name "state" test-root))
         (treemacs-persist-file
          (expand-file-name "treemacs-persist" state-directory))
         (treemacs-last-error-persist-file
          (expand-file-name "treemacs-persist-error" state-directory))
         (workspace (treemacs-workspace->create! :name "Development"))
         (treemacs--workspaces (list workspace))
         (treemacs--disabled-workspaces nil)
         (treemacs--scope-storage nil)
         (treemacs-create-project-functions nil)
         (treemacs-delete-project-functions nil)
         (projectile-known-projects nil)
         (projectile-known-projects-file
          (expand-file-name "projectile-known.eld" state-directory))
         (projectile-projects-cache (make-hash-table :test 'equal))
         (projectile-projects-cache-time (make-hash-table :test 'equal))
         (projectile-project-root-cache (make-hash-table :test 'equal))
         (projectile-enable-caching nil)
         (projectile-auto-update-cache t)
         (projectile-dynamic-mode-line nil)
         (sandbox-parent
          (file-name-directory (directory-file-name test-root)))
         (locate-dominating-stop-dir-regexp
          (concat "\\`" (regexp-quote sandbox-parent) "\\'"))
         (default-directory (file-name-as-directory test-root)))
    (make-directory state-directory t)
    (setf (treemacs-current-workspace) workspace)
    (unwind-protect
        (funcall function test-root)
      (dolist (buffer (buffer-list))
        (when-let* ((file (buffer-file-name buffer))
                    ((string-prefix-p test-root file)))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))
      (delete-directory test-root t))))

(defun neomacs-treemacs-projectile-test-make-project (root name)
  "Create and return a real project named NAME below ROOT."
  (let ((project (expand-file-name name root)))
    (make-directory (expand-file-name ".git" project) t)
    (make-directory (expand-file-name "src" project) t)
    project))
"###;

fn package_registration_wires_the_command_key_hooks_and_mouse_provider() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((descriptor (cadr (assq 'treemacs-projectile package-alist)))
       (history-entry
        (cl-find-if
         (lambda (entry)
           (member '(provide . treemacs-projectile) (cdr entry)))
         load-history)))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'treemacs-projectile) t))
   :command
   (list :interactive (commandp 'treemacs-projectile)
         :project-map (lookup-key treemacs-project-map (kbd "p")))
   :discovery
   (list :registered
         (and (memq #'treemacs--projectile-current-user-project-function
                    treemacs--find-user-project-functions)
              t)
         :position
         (cl-position #'treemacs--projectile-current-user-project-function
                      treemacs--find-user-project-functions))
   :hooks
   (mapcar
    (lambda (entry)
      (list (car entry)
            (and (memq (cdr entry) (symbol-value (car entry))) t)))
    '((treemacs-create-file-functions
       . treemacs-projectile--add-file-to-projectile-cache)
      (treemacs-delete-file-functions
       . treemacs-projectile--remove-from-cache)
      (treemacs-rename-file-functions
       . treemacs-projectile--rename-cache-entry)
      (treemacs-move-file-functions
       . treemacs-projectile--rename-cache-entry)
      (treemacs-copy-file-functions
       . treemacs-projectile--add-copied-file-to-cache)))
   :mouse
   (member '("Add Projectile project"
             . treemacs--projectile-project-mouse-selection-menu)
           treemacs--mouse-project-list-functions)
   :history
   (list :file (file-name-nondirectory (car history-entry))
         :requires
         (mapcar #'cdr
                 (seq-filter
                  (lambda (entry) (eq (car-safe entry) 'require))
                  (cdr history-entry)))
         :provided
         (and (member '(provide . treemacs-projectile) (cdr history-entry)) t))))
"###;
    let expected = expect![[
        r#"OK (:package (:name treemacs-projectile :version "20250320.2206" :requirements ((emacs (26 1)) (projectile (0 14 0)) (treemacs (0 0))) :feature t) :command (:interactive t :project-map treemacs-projectile) :discovery (:registered t :position 0) :hooks ((treemacs-create-file-functions t) (treemacs-delete-file-functions t) (treemacs-rename-file-functions t) (treemacs-move-file-functions t) (treemacs-copy-file-functions t)) :mouse (("Add Projectile project" . treemacs--projectile-project-mouse-selection-menu)) :history (:file "treemacs-projectile.el" :requires (treemacs projectile treemacs-macros) :provided t))"#
    ]];
    ParityBatchCase::value(
        "package_registration_wires_the_command_key_hooks_and_mouse_provider",
        elisp_form,
        expected,
    )
}

fn interactive_project_admission_filters_the_workspace_and_preserves_prefix_failure()
-> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((application
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (deployment
           (neomacs-treemacs-projectile-test-make-project
            test-root "deployment"))
          (documentation
           (neomacs-treemacs-projectile-test-make-project
            test-root "documentation"))
          (projectile-known-projects
           (mapcar #'file-name-as-directory
                   (list application deployment documentation)))
          prompts messages selections)
     (treemacs-do-add-project-to-workspace application "Application")
     (setq selections (list deployment documentation))
     (cl-letf (((symbol-function 'completing-read)
                (lambda (prompt candidates &rest _)
                  (push (list prompt
                              (mapcar
                               (lambda (path)
                                 (file-relative-name path test-root))
                               candidates))
                        prompts)
                  (prog1 (car selections)
                    (setq selections (cdr selections)))))
               ((symbol-function 'treemacs-select-window) #'selected-window)
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push (substring-no-properties
                         (apply #'format-message format-string arguments))
                        messages))))
       (treemacs-projectile nil)
       ;; The package's prefix path passes a nil name into current Treemacs.
       ;; Preserve this real integration outcome instead of hiding it.
       (treemacs-projectile t))
     (list
      :prompts (nreverse prompts)
      :projects
      (mapcar
       (lambda (project)
         (neomacs-treemacs-projectile-test-project project test-root))
       (treemacs-workspace->projects (treemacs-current-workspace)))
      :messages (nreverse messages)
      :prefix-project-added
      (and (treemacs-is-path documentation :in-workspace) t)))))
"###;
    let expected = expect![[
        r#"OK (:prompts (("Project: " ("deployment" "documentation")) ("Project: " ("documentation"))) :projects ((:name "Application" :path "application" :status local-readable) (:name "deployment" :path "deployment" :status local-readable)) :messages ("[Treemacs] Added project deployment to the workspace.") :prefix-project-added nil)"#
    ]];
    ParityBatchCase::value(
        "interactive_project_admission_filters_the_workspace_and_preserves_prefix_failure",
        elisp_form,
        expected,
    )
}

fn empty_projectile_registry_reports_one_failure_without_prompting() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (_test-root)
   (let (prompts messages)
     (cl-letf (((symbol-function 'completing-read)
                (lambda (&rest _)
                  (push 'unexpected prompts)
                  (error "unexpected prompt")))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push (substring-no-properties
                         (apply #'format-message format-string arguments))
                        messages))))
       (let ((projectile-known-projects nil))
         (treemacs-projectile))
       (let ((projectile-known-projects 'not-a-list))
         (treemacs-projectile)))
     (list :prompts prompts :messages (nreverse messages)))))
"###;
    let expected = expect![[
        r#"OK (:prompts nil :messages ("[Treemacs] It looks like projectile does not know any projects." "[Treemacs] It looks like projectile does not know any projects."))"#
    ]];
    ParityBatchCase::value(
        "empty_projectile_registry_reports_one_failure_without_prompting",
        elisp_form,
        expected,
    )
}

fn first_workspace_project_defaults_to_the_current_projectile_root() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((application
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (nested (expand-file-name "src/service/api" application))
          (outside (expand-file-name "scratch" test-root))
          calls from-project from-outside)
     (make-directory nested t)
     (make-directory outside t)
     (cl-letf (((symbol-function 'read-directory-name)
                (lambda (prompt &optional directory &rest _)
                  (push (list prompt directory) calls)
                  (if directory application outside))))
       (let ((default-directory (file-name-as-directory nested)))
         (setq from-project (treemacs--read-first-project-path)))
       (clrhash projectile-project-root-cache)
       (let ((default-directory (file-name-as-directory outside)))
         (setq from-outside (treemacs--read-first-project-path))))
     (neomacs-treemacs-projectile-test-normalize
      test-root
      (list :calls (nreverse calls)
            :project-result from-project
            :outside-result from-outside)))))
"###;
    let expected = expect![[
        r#"OK (:calls (("Project root: " "<ROOT>/application/") ("Project root: " nil)) :project-result "<ROOT>/application" :outside-result "<ROOT>/scratch")"#
    ]];
    ParityBatchCase::value(
        "first_workspace_project_defaults_to_the_current_projectile_root",
        elisp_form,
        expected,
    )
}

fn current_project_discovery_canonicalizes_nested_and_symlinked_worktrees() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((application
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (nested (expand-file-name "src/service" application))
          (alias (expand-file-name "application-link" test-root))
          (outside (expand-file-name "outside" test-root))
          direct through-chain through-alias missing)
     (make-directory nested t)
     (make-directory outside t)
     (make-symbolic-link application alias t)
     (let ((default-directory (file-name-as-directory nested)))
       (setq direct (treemacs--projectile-current-user-project-function)
             through-chain (treemacs--find-current-user-project)))
     (clrhash projectile-project-root-cache)
     (let ((default-directory
            (file-name-as-directory (expand-file-name "src" alias))))
       (setq through-alias
             (treemacs--projectile-current-user-project-function)))
     (clrhash projectile-project-root-cache)
     (let ((default-directory (file-name-as-directory outside)))
       (setq missing (treemacs--projectile-current-user-project-function)))
     (neomacs-treemacs-projectile-test-normalize
      test-root
      (list :direct direct
            :through-chain through-chain
            :through-alias through-alias
            :missing missing)))))
"###;
    let expected = expect![[
        r#"OK (:direct "<ROOT>/application" :through-chain "<ROOT>/application" :through-alias "<ROOT>/application" :missing nil)"#
    ]];
    ParityBatchCase::value(
        "current_project_discovery_canonicalizes_nested_and_symlinked_worktrees",
        elisp_form,
        expected,
    )
}

fn created_and_copied_file_adapters_reuse_visiting_buffers_and_clean_up_temporary_ones()
-> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((project
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (created (expand-file-name "src/created.el" project))
          (copied (expand-file-name "src/copied.el" project))
          (open-buffer nil)
          calls)
     (with-temp-file created (insert "(provide 'created)\n"))
     (with-temp-file copied (insert "(provide 'copied)\n"))
     (cl-letf (((symbol-function 'projectile-find-file-hook-function)
                (lambda ()
                  (push (list :file
                              (file-relative-name buffer-file-name project)
                              :buffer (buffer-name)
                              :directory
                              (file-relative-name default-directory project))
                        calls))))
       (treemacs-projectile--add-file-to-projectile-cache created)
       (setq open-buffer (find-file-noselect copied))
       (treemacs-projectile--add-copied-file-to-cache created copied))
     (prog1
         (list
          :calls (nreverse calls)
          :temporary-buffer-alive (and (get-file-buffer created) t)
          :existing-buffer-alive (and (buffer-live-p open-buffer) t)
          :existing-buffer-file
          (file-relative-name (buffer-file-name open-buffer) project))
       (when (buffer-live-p open-buffer)
         (kill-buffer open-buffer))))))
"###;
    let expected = expect![[
        r#"OK (:calls ((:file "src/created.el" :buffer "created.el" :directory "src/") (:file "src/copied.el" :buffer "copied.el" :directory "src/")) :temporary-buffer-alive nil :existing-buffer-alive t :existing-buffer-file "src/copied.el")"#
    ]];
    ParityBatchCase::value(
        "created_and_copied_file_adapters_reuse_visiting_buffers_and_clean_up_temporary_ones",
        elisp_form,
        expected,
    )
}

fn cache_hooks_add_ignore_rename_and_remove_real_project_files() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((project
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (source (expand-file-name "src/deploy.el" project))
          (renamed (expand-file-name "src/deploy-prod.el" project))
          (secret (expand-file-name "src/token.secret" project))
          (generated (expand-file-name "build/generated.el" project))
          (project-root (file-name-as-directory (file-truename project)))
          (projectile-globally-ignored-directories '("build"))
          (projectile-global-ignore-file-patterns '("\\.secret\\'"))
          after-add after-rename wrong-context-removal final-removal serialized)
     (make-directory (file-name-directory generated) t)
     (dolist (file (list source secret generated))
       (with-temp-file file (insert "fixture\n")))
     (let ((default-directory project-root))
       (treemacs-projectile--add-to-cache source)
       (treemacs-projectile--add-to-cache source)
       (treemacs-projectile--add-to-cache secret)
       (treemacs-projectile--add-to-cache generated)
       (setq after-add (copy-sequence (gethash project-root
                                               projectile-projects-cache)))
       (rename-file source renamed)
       (treemacs-projectile--rename-cache-entry source renamed)
       (setq after-rename (copy-sequence (gethash project-root
                                                  projectile-projects-cache))))
     ;; Treemacs invokes delete hooks from its own buffer, so preserve how the
     ;; adapter behaves when `default-directory' is outside the target project.
     (let ((default-directory (file-name-as-directory test-root)))
       (treemacs-projectile--remove-from-cache renamed)
       (setq wrong-context-removal
             (copy-sequence (gethash project-root projectile-projects-cache))))
     (let ((default-directory project-root))
       (treemacs-projectile--remove-from-cache renamed)
       (setq final-removal
             (copy-sequence (gethash project-root projectile-projects-cache))))
     (let ((cache-file (projectile-project-cache-file project-root)))
       (setq serialized
             (and (file-exists-p cache-file)
                  (with-temp-buffer
                    (insert-file-contents cache-file)
                    (buffer-string)))))
     (list :after-add after-add
           :after-rename after-rename
           :wrong-context-removal wrong-context-removal
           :final-removal final-removal
           :serialized serialized))))
"###;
    let expected = expect![[
        r#"OK (:after-add ("src/deploy.el") :after-rename ("src/deploy-prod.el") :wrong-context-removal ("src/deploy-prod.el") :final-removal nil :serialized nil)"#
    ]];
    ParityBatchCase::value(
        "cache_hooks_add_ignore_rename_and_remove_real_project_files",
        elisp_form,
        expected,
    )
}

fn mouse_project_menu_sorts_available_roots_and_its_closures_add_the_selected_project()
-> ParityBatchCase {
    let elisp_form = r###"
(neomacs-treemacs-projectile-test-call-with-state
 (lambda (test-root)
   (let* ((application
           (neomacs-treemacs-projectile-test-make-project
            test-root "application"))
          (beta
           (neomacs-treemacs-projectile-test-make-project test-root "beta"))
          (zeta
           (neomacs-treemacs-projectile-test-make-project test-root "zeta"))
          (projectile-known-projects
           (list (file-name-as-directory zeta)
                 (file-name-as-directory application)
                 (file-name-as-directory beta)))
          menu selected empty all-known)
     (treemacs-do-add-project-to-workspace application "Application")
     (setq menu (treemacs--projectile-project-mouse-selection-menu))
     (cl-letf (((symbol-function 'treemacs-add-project-to-workspace)
                (lambda (path &optional name)
                  (setq selected (list path name)))))
       (funcall (aref (car menu) 1)))
     (let ((projectile-known-projects nil))
       (setq empty (treemacs--projectile-project-mouse-selection-menu)))
     (let ((projectile-known-projects (list application)))
       (setq all-known
             (treemacs--projectile-project-mouse-selection-menu)))
     (neomacs-treemacs-projectile-test-normalize
      test-root
      (list
       :menu (mapcar (lambda (entry) (aref entry 0)) menu)
       :selected selected
       :empty (mapcar (lambda (entry) (aref entry 0)) empty)
       :all-known (mapcar (lambda (entry) (aref entry 0)) all-known))))))
"###;
    let expected = expect![[
        r#"OK (:menu ("<ROOT>/beta" "<ROOT>/zeta") :selected ("<ROOT>/beta" nil) :empty ("Projectile list is empty") :all-known ("All Projectile projects are already in the workspace"))"#
    ]];
    ParityBatchCase::value(
        "mouse_project_menu_sorts_available_roots_and_its_closures_add_the_selected_project",
        elisp_form,
        expected,
    )
}

#[test]
fn treemacs_projectile_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(TREEMACS_PROJECTILE_MELPA_PIN, "treemacs-projectile.el")
            .expect("prepare revision-pinned Treemacs-Projectile source below ./tmp")
            .with_melpa_dependency(PROJECTILE_MELPA_PIN)
            .expect("prepare revision-pinned Projectile dependency below ./tmp")
            .with_melpa_dependency(TREEMACS_MELPA_PIN)
            .expect("prepare revision-pinned Treemacs dependency below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "treemacs-projectile-package-batch",
        "Treemacs-Projectile",
        &[
            package_registration_wires_the_command_key_hooks_and_mouse_provider(),
            interactive_project_admission_filters_the_workspace_and_preserves_prefix_failure(),
            empty_projectile_registry_reports_one_failure_without_prompting(),
            first_workspace_project_defaults_to_the_current_projectile_root(),
            current_project_discovery_canonicalizes_nested_and_symlinked_worktrees(),
            created_and_copied_file_adapters_reuse_visiting_buffers_and_clean_up_temporary_ones(),
            cache_hooks_add_ignore_rename_and_remove_real_project_files(),
            mouse_project_menu_sorts_available_roots_and_its_closures_add_the_selected_project(),
        ],
    );
}
