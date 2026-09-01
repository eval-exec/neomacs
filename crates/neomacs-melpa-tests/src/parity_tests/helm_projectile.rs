use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_MELPA_PIN, HELM_PROJECTILE_MELPA_PIN, PROJECTILE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_PROJECTILE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HELM_PROJECTILE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'dired)
(require 'nadvice)
(require 'helm-projectile)

(defvar neomacs-helm-projectile-test-switch-record nil)

(defun neomacs-helm-projectile-test-switch-action ()
  "Record the editor state delivered to a custom project-switch action."
  (setq neomacs-helm-projectile-test-switch-record
        (list :directory default-directory
              :completion projectile-completion-system
              :project-name (projectile-project-name))))

(defmacro neomacs-helm-projectile-test-with-project (files &rest body)
  "Create a real temporary Projectile project containing FILES for BODY."
  (declare (indent 1) (debug t))
  `(let* ((root (file-name-as-directory
                 (file-truename
                  (make-temp-file "helm-projectile-parity-" t))))
          (default-directory root)
          (projectile-project-root-cache (make-hash-table :test 'equal))
          (projectile-enable-caching nil)
          (projectile-indexing-method 'native))
     (unwind-protect
         (progn
           (with-temp-file (expand-file-name ".projectile" root))
           (dolist (entry ,files)
             (let ((file (expand-file-name (car entry) root)))
               (make-directory (file-name-directory file) t)
               (with-temp-file file (insert (cdr entry)))))
           ,@body)
       (dolist (buffer (buffer-list))
         (when-let* ((file (buffer-file-name buffer))
                     ((string-prefix-p root (file-truename file))))
           (kill-buffer buffer)))
       (delete-directory root t))))
"##;

fn helm_projectile_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_PROJECTILE_MELPA_PIN, "helm-projectile.el")
        .expect("prepare revision-pinned Helm-Projectile source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare revision-pinned Helm dependency below ./tmp")
        .with_melpa_dependency(PROJECTILE_MELPA_PIN)
        .expect("prepare revision-pinned Projectile dependency below ./tmp")
        .with_prelude(HELM_PROJECTILE_TEST_PRELUDE)
        .with_timeout(HELM_PROJECTILE_TEST_TIMEOUT)
}

fn global_mode_installs_and_removes_the_complete_user_integration() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (helm-projectile-mode -1)
  (let ((original-action projectile-switch-project-action))
    (unwind-protect
        (progn
          (setq projectile-switch-project-action #'projectile-find-file)
          (helm-projectile-mode 1)
          (let ((enabled
                 (list
                  :mode helm-projectile-mode
                  :switch-action projectile-switch-project-action
                  :file-remap
                  (lookup-key projectile-mode-map [remap projectile-find-file])
                  :buffer-remap
                  (lookup-key projectile-mode-map
                              [remap projectile-switch-to-buffer])
                  :grep-remap
                  (lookup-key projectile-mode-map [remap projectile-grep])
                  :ripgrep-remap
                  (lookup-key projectile-mode-map [remap projectile-ripgrep])
                  :file-advice
                  (and (advice-member-p
                        #'helm-projectile-run-projectile-hooks-after-find-file
                        'helm-find-file-or-marked)
                       t)
                  :search-advice
                  (and (advice-member-p #'helm-projectile--ag-automatic-input
                                        'helm-grep-ag-1)
                       t)
                  :etags-binding
                  (commandp (lookup-key helm-etags-map (kbd "C-c p f"))))))
            (helm-projectile-mode -1)
            (list :enabled enabled
                  :disabled
                  (list :mode helm-projectile-mode
                        :switch-action projectile-switch-project-action
                        :file-remap
                        (lookup-key projectile-mode-map
                                    [remap projectile-find-file])
                        :file-advice
                        (advice-member-p
                         #'helm-projectile-run-projectile-hooks-after-find-file
                         'helm-find-file-or-marked)
                        :search-advice
                        (advice-member-p #'helm-projectile--ag-automatic-input
                                         'helm-grep-ag-1)
                        :etags-binding
                        (lookup-key helm-etags-map (kbd "C-c p f"))))))
      (helm-projectile-mode -1)
      (setq projectile-switch-project-action original-action))))
"##;
    let expected = expect![[
        r####"OK (:enabled (:mode t :switch-action helm-projectile-find-file :file-remap helm-projectile-find-file :buffer-remap helm-projectile-switch-to-buffer :grep-remap helm-projectile-grep :ripgrep-remap helm-projectile-rg :file-advice t :search-advice t :etags-binding t) :disabled (:mode nil :switch-action projectile-find-file :file-remap nil :file-advice nil :search-advice nil :etags-binding nil))"####
    ]];
    ParityBatchCase::value(
        "global_mode_installs_and_removes_the_complete_user_integration",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn project_file_source_indexes_real_files_and_preserves_nested_display_paths() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("README.md" . "Release notes\n")
      ("src/main.el" . "(message \"ship\")\n")
      ("test/main-test.el" . "(ert-deftest release ())\n")
      ("vendor/generated.el" . "generated\n"))
  (with-temp-file (expand-file-name ".projectile" root)
    (insert "-/vendor\n"))
  (let* ((projectile-globally-ignored-directories '(".git"))
         (projectile-globally-ignored-files nil)
         (projectile-globally-ignored-file-suffixes nil)
         (candidate-function
          (helm-get-attr 'candidates helm-source-projectile-files-list))
         (candidates (funcall candidate-function)))
    (sort
     (mapcar (lambda (candidate)
               (list :display (substring-no-properties (car candidate))
                     :real (file-relative-name (cdr candidate) root)))
             candidates)
     (lambda (left right)
       (string< (plist-get left :real) (plist-get right :real))))))
"##;
    let expected = expect![[
        r####"OK ((:display ".projectile" :real ".projectile") (:display "README.md" :real "README.md") (:display "src/main.el" :real "src/main.el") (:display "test/main-test.el" :real "test/main-test.el"))"####
    ]];
    ParityBatchCase::value(
        "project_file_source_indexes_real_files_and_preserves_nested_display_paths",
        elisp_form,
        expected,
    )
}

fn other_file_workflow_opens_the_unique_pair_and_reports_missing_pairs() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("src/widget.c" . "int widget(void) { return 42; }\n")
      ("src/widget.h" . "int widget(void);\n")
      ("README.md" . "Widget release\n"))
  (save-window-excursion
    (switch-to-buffer (find-file-noselect (expand-file-name "src/widget.c" root)))
    (helm-projectile-find-other-file)
    (let ((opened
           (list :file (file-relative-name (buffer-file-name) root)
                 :contents (buffer-substring-no-properties
                            (point-min) (point-max))
                 :project-name-matches-root
                 (equal (projectile-project-name)
                        (file-name-nondirectory (directory-file-name root))))))
      (switch-to-buffer (find-file-noselect (expand-file-name "README.md" root)))
      (list :opened opened
            :missing-pair
            (condition-case err
                (progn
                  (helm-projectile-find-other-file)
                  :unexpected-success)
              (user-error (error-message-string err)))))))
"##;
    let expected = expect![[
        r####"OK (:opened (:file "src/widget.h" :contents "int widget(void);\n" :project-name-matches-root t) :missing-pair "No other file found")"####
    ]];
    ParityBatchCase::value(
        "other_file_workflow_opens_the_unique_pair_and_reports_missing_pairs",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn virtual_dired_round_trips_the_selected_release_files() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("dist/neomacs" . "binary\n")
      ("dist/neomacs.pdump" . "dump\n")
      ("notes/internal.txt" . "not released\n"))
  (let ((buffer
         (dired (cons "helm-projectile-release-files"
                      (list (expand-file-name "dist/neomacs" root)
                            (expand-file-name "dist/neomacs.pdump" root))))))
    (unwind-protect
        (with-current-buffer buffer
          (list :mode major-mode
                :registered
                (and (member (buffer-name)
                             (helm-projectile-all-dired-buffers))
                     t)
                :files
                (sort
                 (mapcar (lambda (file) (file-relative-name file root))
                         (helm-projectile-files-in-current-dired-buffer))
                 #'string<)))
      (kill-buffer buffer))))
"##;
    let expected = expect![[
        r####"OK (:mode dired-mode :registered t :files ("dist/neomacs" "dist/neomacs.pdump"))"####
    ]];
    ParityBatchCase::value(
        "virtual_dired_round_trips_the_selected_release_files",
        elisp_form,
        expected,
    )
}

fn switching_projects_runs_the_configured_action_in_the_selected_root() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("README.md" . "Selected project\n"))
  (let ((projectile-switch-project-action
         #'neomacs-helm-projectile-test-switch-action)
        (neomacs-helm-projectile-test-switch-record nil))
    (helm-projectile-switch-project-by-name root)
    (list :selected-root
          (equal (plist-get neomacs-helm-projectile-test-switch-record
                            :directory)
                 root)
          :completion
          (plist-get neomacs-helm-projectile-test-switch-record :completion)
          :project-name-matches-root
          (equal
           (plist-get neomacs-helm-projectile-test-switch-record :project-name)
           (file-name-nondirectory (directory-file-name root))))))
"##;
    let expected =
        expect![[r####"OK (:selected-root t :completion helm :project-name-matches-root t)"####]];
    ParityBatchCase::value(
        "switching_projects_runs_the_configured_action_in_the_selected_root",
        elisp_form,
        expected,
    )
}

fn project_search_builds_a_scoped_command_with_input_and_ignore_rules() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("src/release.el" . "(defun deploy-release ())\n")
      ("build/generated.el" . "generated\n")
      ("TAGS" . "tags\n")
      ("trace.log" . "trace\n"))
  (with-temp-file (expand-file-name ".projectile" root)
    (insert "-/build\n-/TAGS\n-/*.log\n"))
  (with-temp-buffer
    (insert "deploy_release")
    (goto-char (point-min))
    (let ((projectile-use-git-grep nil)
          (projectile-globally-ignored-directories nil)
          (projectile-globally-ignored-files nil)
          (projectile-globally-ignored-file-suffixes nil)
          (grep-find-ignored-files '("*.o"))
          (grep-find-ignored-directories '(".git"))
          (helm-projectile-ignore-strategy 'projectile)
          capture)
      (cl-letf (((symbol-function 'helm)
                 (lambda (&rest arguments)
                   (setq capture
                         (list
                          :command helm-grep-default-command
                          :include helm-grep-include-files
                          :ignored-files
                          (sort (copy-sequence helm-grep-ignored-files) #'string<)
                          :ignored-directories
                          (sort (copy-sequence helm-grep-ignored-directories)
                                #'string<)
                          :input (plist-get arguments :input)
                          :root
                          (equal (plist-get arguments :default-directory)
                                 root))))))
        (helm-projectile-grep-or-ack root nil nil nil "*.el"))
      capture)))
"##;
    let expected = expect![[
        r####"OK (:command "grep -a -r %e -n%cH -e %p %f" :include "*.el" :ignored-files ("*.log" "*.o" "TAGS" "build") :ignored-directories (".git") :input "deploy_release" :root t)"####
    ]];
    ParityBatchCase::value(
        "project_search_builds_a_scoped_command_with_input_and_ignore_rules",
        elisp_form,
        expected,
    )
}

fn project_buffer_source_orders_the_current_buffer_last_or_removes_it() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-helm-projectile-test-with-project
    '(("hp-main.el" . "(message \"main\")\n")
      ("hp-test.el" . "(message \"test\")\n"))
  (let* ((main (find-file-noselect (expand-file-name "hp-main.el" root)))
         (test (find-file-noselect (expand-file-name "hp-test.el" root)))
         (source (helm-source-projectile-buffer :name "Release buffers"))
         (initialize (slot-value source 'init)))
    (with-current-buffer main
      (let ((helm-projectile-remove-current-buffer nil))
        (funcall initialize)
        (let ((current-last (copy-sequence helm-projectile-buffers-list-cache)))
          (let ((helm-projectile-remove-current-buffer t))
            (funcall initialize)
            (list :opened (sort (list (buffer-name main) (buffer-name test))
                                #'string<)
                  :current-last current-last
                  :current-removed
                  (copy-sequence helm-projectile-buffers-list-cache))))))))
"##;
    let expected = expect![[
        r####"OK (:opened ("hp-main.el" "hp-test.el") :current-last ("hp-test.el" "hp-main.el") :current-removed ("hp-test.el"))"####
    ]];
    ParityBatchCase::value(
        "project_buffer_source_orders_the_current_buffer_last_or_removes_it",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_projectile_package_batch() {
    assert_oracle_batch_cases(
        helm_projectile_oracle(),
        "helm-projectile-package-batch",
        "Helm-Projectile",
        &[
            global_mode_installs_and_removes_the_complete_user_integration(),
            project_file_source_indexes_real_files_and_preserves_nested_display_paths(),
            other_file_workflow_opens_the_unique_pair_and_reports_missing_pairs(),
            virtual_dired_round_trips_the_selected_release_files(),
            switching_projects_runs_the_configured_action_in_the_selected_root(),
            project_search_builds_a_scoped_command_with_input_and_ignore_rules(),
            project_buffer_source_orders_the_current_buffer_last_or_removes_it(),
        ],
    );
}
