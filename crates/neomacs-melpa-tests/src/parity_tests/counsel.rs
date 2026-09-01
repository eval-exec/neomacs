use std::time::Duration;

use expect_test::expect;

use crate::{COUNSEL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const COUNSEL_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const COUNSEL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'counsel)
(require 'compile)
(require 'imenu)

(global-set-key (kbd "C-c f") #'counsel-find-file)
(global-set-key (kbd "C-c g") #'counsel-git)
(global-set-key (kbd "C-c q") #'counsel-git-grep)
(global-set-key (kbd "C-c y") #'counsel-yank-pop)
(global-set-key (kbd "C-c i") #'counsel-imenu)
(global-set-key (kbd "C-c o") #'counsel-outline)
(global-set-key (kbd "C-c c") #'counsel-compile)

(defvar neomacs-counsel-test-command-log nil)
(defvar neomacs-counsel-test-compile-log nil)

(defun neomacs-counsel-test-deploy (arg)
  "Record a deployment command selected through Counsel."
  (interactive "P")
  (push (list arg this-command real-this-command current-prefix-arg)
        neomacs-counsel-test-command-log)
  (insert (format "deploy[%S]" arg)))

(defalias 'neomacs-counsel-test-rollout #'neomacs-counsel-test-deploy)

(defun neomacs-counsel-test-compile (command &optional comint)
  "Record COMMAND, COMINT, directory, and environment without starting a job."
  (push (list command comint
              (file-name-nondirectory
               (directory-file-name default-directory))
              (copy-sequence compilation-environment))
        neomacs-counsel-test-compile-log))

(defun neomacs-counsel-test-in-buffer (name text body)
  "Run BODY in a displayed buffer named NAME containing TEXT."
  (let ((buffer (generate-new-buffer name)))
    (unwind-protect
        (save-window-excursion
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (insert text)
          (goto-char (point-min))
          (funcall body))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-counsel-test-root (name)
  "Create and return a deterministic test directory for NAME."
  (let ((root (expand-file-name
               (format "counsel-%s-fixture/" name)
               temporary-file-directory)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-counsel-test-write-file (root relative contents)
  "Write CONTENTS below ROOT at RELATIVE and return the resulting path."
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-counsel-test-clean-root (root)
  "Kill buffers visiting ROOT and delete ROOT recursively."
  (dolist (buffer (buffer-list))
    (let ((file (buffer-local-value 'buffer-file-name buffer)))
      (when (and file (file-in-directory-p file root))
        (kill-buffer buffer))))
  (when (file-exists-p root)
    (delete-directory root t)))

(defun neomacs-counsel-test-position ()
  "Return a stable summary of the selected buffer position."
  (list :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun neomacs-counsel-test-relative-file (root)
  "Return the current buffer file relative to ROOT."
  (and buffer-file-name (file-relative-name buffer-file-name root)))
"##;

fn counsel_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COUNSEL_MELPA_PIN, "counsel.el")
        .expect("prepare revision-pinned Counsel source below ./tmp")
        .with_prelude(COUNSEL_TEST_PRELUDE)
        .with_timeout(COUNSEL_TEST_TIMEOUT)
}

fn command_palette_remaps_m_x_executes_prefix_commands_and_records_history() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-counsel-test-in-buffer
 "*counsel-command-palette*" "release: "
 (lambda ()
   (let ((counsel-M-x-collection
          (lambda ()
            '(neomacs-counsel-test-deploy
              neomacs-counsel-test-rollout)))
         (counsel-M-x-history nil)
         (command-history nil)
         (neomacs-counsel-test-command-log nil)
         (suggest-key-bindings nil)
         (was-enabled counsel-mode))
     (unwind-protect
         (progn
           (counsel-mode 1)
           (let ((remaps
                  (mapcar #'command-remapping
                          '(execute-extended-command find-file imenu yank-pop))))
             (goto-char (point-max))
             (execute-kbd-macro
              (vconcat (kbd "C-u M-x")
                       "neomacs-counsel-test-deploy"
                       (kbd "RET")))
             (insert " | ")
             (execute-kbd-macro
              (vconcat (kbd "M-x")
                       "neomacs-counsel-test-rollout"
                       (kbd "RET")))
             (list
              :text (buffer-substring-no-properties (point-min) (point-max))
              :remaps remaps
              :log
              (mapcar
               (lambda (entry)
                 (list (prin1-to-string (nth 0 entry))
                       (nth 1 entry)
                       (nth 2 entry)
                       (prin1-to-string (nth 3 entry))))
               (nreverse neomacs-counsel-test-command-log))
              :history (mapcar #'substring-no-properties counsel-M-x-history)
              :commands (mapcar #'prin1-to-string command-history)
              :alias-display
              (substring-no-properties
               (counsel-M-x-transformer
                "neomacs-counsel-test-rollout")))))
       (counsel-mode (if was-enabled 1 -1))))))
"##;
    let expected = expect![[
        r####"OK (:text "release: deploy[(4)] | deploy[nil]" :remaps (counsel-M-x counsel-find-file counsel-imenu counsel-yank-pop) :log (("(4)" neomacs-counsel-test-deploy neomacs-counsel-test-deploy "(4)") ("nil" neomacs-counsel-test-rollout neomacs-counsel-test-rollout "nil")) :history ("^neomacs-counsel-test-rollout" "neomacs-counsel-test-rollout" "^neomacs-counsel-test-deploy" "neomacs-counsel-test-deploy") :commands ("(neomacs-counsel-test-rollout nil)" "(neomacs-counsel-test-deploy '(4))") :alias-display "neomacs-counsel-test-rollout (neomacs-counsel-test-deploy)")"####
    ]];
    ParityBatchCase::value(
        "command_palette_remaps_m_x_executes_prefix_commands_and_records_history",
        elisp_form,
        expected,
    )
}

fn file_picker_opens_hidden_spaced_and_nested_project_files() -> ParityBatchCase {
    let elisp_form = r##"
(let ((root (neomacs-counsel-test-root "files")))
  (unwind-protect
      (progn
        (neomacs-counsel-test-write-file root ".env" "REGION=us-east-1\n")
        (neomacs-counsel-test-write-file
         root "incident notes.txt" "INC-417 requires rollback\n")
        (neomacs-counsel-test-write-file
         root "runbooks/recovery.org" "* Recovery\nRestore the stable release.\n")
        (neomacs-counsel-test-in-buffer
         "*counsel-file-origin*" "choose deployment artifact"
         (lambda ()
           (setq default-directory root)
           (let ((origin (current-buffer))
                 (file-name-history nil)
                 selected)
             (dolist (relative '(".env" "incident notes.txt" "runbooks/recovery.org"))
               (switch-to-buffer origin)
               (execute-kbd-macro
                (vconcat (kbd "C-c f") relative (kbd "RET")))
               (push
                (list
                 :file (neomacs-counsel-test-relative-file root)
                 :directory (file-relative-name default-directory root)
                 :text (buffer-substring-no-properties (point-min) (point-max)))
                selected))
             (list
              :selected (nreverse selected)
              :history
              (mapcar
               (lambda (file)
                 (file-relative-name (expand-file-name file root) root))
               (cl-remove-if-not #'stringp file-name-history)))))))
    (neomacs-counsel-test-clean-root root)))
"##;
    let expected = expect![[
        r####"OK (:selected ((:file ".env" :directory "./" :text "REGION=us-east-1\n") (:file "incident notes.txt" :directory "./" :text "INC-417 requires rollback\n") (:file "runbooks/recovery.org" :directory "runbooks/" :text "* Recovery\nRestore the stable release.\n")) :history ("runbooks/recovery.org" "incident notes.txt" ".env"))"####
    ]];
    ParityBatchCase::value(
        "file_picker_opens_hidden_spaced_and_nested_project_files",
        elisp_form,
        expected,
    )
}

fn git_workflow_lists_only_tracked_files_and_navigates_a_real_repository_search() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((root (neomacs-counsel-test-root "git")))
  (unwind-protect
      (progn
        (neomacs-counsel-test-write-file
         root "src/api.el"
         ";;; api.el\n(defun retry-budget () 3)\n(defun deploy-api () 'ready)\n")
        (neomacs-counsel-test-write-file
         root "docs/runbook.md"
         "# API Runbook\nUse retry-budget before rollback.\n")
        (neomacs-counsel-test-write-file
         root "scratch.log" "untracked diagnostic\n")
        (call-process "git" nil nil nil "init" "-q" root)
        (let ((default-directory root))
          (call-process "git" nil nil nil "add" "src/api.el" "docs/runbook.md"))
        (neomacs-counsel-test-in-buffer
         "*counsel-git-origin*" "repository dashboard"
         (lambda ()
           (setq default-directory (expand-file-name "src/" root))
           (let ((origin (current-buffer))
                 (counsel-git-history nil)
                 (counsel-git-grep-cmd nil)
                 tracked selected grep-command grep-candidates grep-result)
             (setq tracked (sort (counsel-git-cands root) #'string-lessp))
             (execute-kbd-macro
              (vconcat (kbd "C-c g") "docs/runbook.md" (kbd "RET")))
             (setq selected
                   (list :file (neomacs-counsel-test-relative-file root)
                         :position (neomacs-counsel-test-position)))
             (switch-to-buffer origin)
             (setq default-directory root
                   counsel-git-grep-cmd counsel-git-grep-cmd-default)
             (let ((ivy--regex-function #'ivy--regex))
               (ivy-set-text "defun retry-budget")
               (setq grep-command
                     (counsel-git-grep-cmd-function-default ivy-text)))
             (setq grep-candidates
                   (split-string
                    (counsel--command
                     "git" "--no-pager" "grep" "--full-name" "-n"
                     "--no-color" "-I" "-e" "defun retry-budget")
                    "\n" t))
             (let ((ivy-text "defun retry-budget")
                   (ivy-exit 'done))
               (counsel-git-grep-action (car grep-candidates)))
             (setq grep-result
                   (list :file (neomacs-counsel-test-relative-file root)
                         :position (neomacs-counsel-test-position)))
             (list :tracked tracked
                   :untracked-listed (member "scratch.log" tracked)
                   :selected selected
                   :grep-command grep-command
                   :grep-candidates grep-candidates
                   :grep grep-result
                   :git-history counsel-git-history)))))
    (neomacs-counsel-test-clean-root root)))
"##;
    let expected = expect![[
        r####"OK (:tracked ("docs/runbook.md" "src/api.el") :untracked-listed nil :selected (:file "docs/runbook.md" :position (:line 1 :column 0 :text "# API Runbook")) :grep-command "git --no-pager grep -n --no-color -I -e \"\\(defun\\).*\\(retry-budget\\)\"" :grep-candidates ("src/api.el:2:(defun retry-budget () 3)") :grep (:file "src/api.el" :position (:line 2 :column 19 :text "(defun retry-budget () 3)")) :git-history ("docs/runbook.md"))"####
    ]];
    ParityBatchCase::value(
        "git_workflow_lists_only_tracked_files_and_navigates_a_real_repository_search",
        elisp_form,
        expected,
    )
}

fn kill_ring_picker_filters_noise_and_replaces_the_last_yank_with_an_operator_command()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-counsel-test-in-buffer
 "*counsel-kill-ring*" "Recovery command: "
 (lambda ()
   (let* ((restart "kubectl rollout restart deployment/api")
          (undo (propertize "kubectl rollout undo deployment/api"
                           'source 'operations-runbook))
          (kill-ring (list restart "  \n" undo (copy-sequence undo)))
          (kill-ring-yank-pointer kill-ring)
          (interprogram-paste-function nil)
          (interprogram-cut-function nil)
          (select-enable-clipboard nil)
          (counsel-yank-pop-preselect-last t))
     (goto-char (point-max))
     (execute-kbd-macro
      (vconcat (kbd "C-y C-c y") "rollout undo" (kbd "RET")))
     (list
      :text (buffer-substring-no-properties (point-min) (point-max))
      :point (point)
      :mark (mark t)
      :ring (mapcar #'substring-no-properties kill-ring)
      :yank-pointer
      (mapcar #'substring-no-properties kill-ring-yank-pointer)))))
"##;
    let expected = expect![[
        r####"OK (:text "Recovery command: kubectl rollout undo deployment/api" :point 54 :mark 19 :ring ("kubectl rollout restart deployment/api" "kubectl rollout undo deployment/api") :yank-pointer ("kubectl rollout undo deployment/api"))"####
    ]];
    ParityBatchCase::value(
        "kill_ring_picker_filters_noise_and_replaces_the_last_yank_with_an_operator_command",
        elisp_form,
        expected,
    )
}

fn structural_navigation_flattens_imenu_and_follows_nested_operational_headings() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-counsel-test-in-buffer
 "*counsel-structure*"
 ";;; Services\n;;;; Preview\n(defun deploy-preview () 'preview)\n;;;; Production\n(defun deploy-production () 'production)\n;;; Operations\n;;;; Rollback\n(defun rollback-production () 'stable)\n"
 (lambda ()
   (emacs-lisp-mode)
   (let* ((imenu-use-markers nil)
          (imenu-auto-rescan t)
          (counsel-outline-display-style 'path)
          (counsel-outline-face-style nil)
          (imenu-candidates
           (mapcar
            (lambda (candidate)
              (list (substring-no-properties (car candidate))
                    (line-number-at-pos (cddr candidate))))
            (counsel--imenu-candidates)))
          (outline-settings (cdr (assq major-mode counsel-outline-settings)))
          (outline-candidates
           (mapcar
            (lambda (candidate)
              (list (substring-no-properties (car candidate))
                    (line-number-at-pos (cdr candidate))))
            (counsel-outline-candidates outline-settings)))
          imenu-jump outline-jump)
     (goto-char (point-min))
     (execute-kbd-macro
      (vconcat (kbd "C-c i") "rollback-production" (kbd "RET")))
     (setq imenu-jump (neomacs-counsel-test-position))
     (goto-char (point-max))
     (execute-kbd-macro
      (vconcat (kbd "C-c o") "Operations/Rollback" (kbd "RET")))
     (setq outline-jump (neomacs-counsel-test-position))
     (list :imenu imenu-candidates
           :imenu-jump imenu-jump
           :outline outline-candidates
           :outline-jump outline-jump
           :outline-preselect counsel-outline--preselect))))
"##;
    let expected = expect![[
        r####"OK (:imenu (("Functions: deploy-preview" 3) ("Functions: deploy-production" 5) ("Functions: rollback-production" 8)) :imenu-jump (:line 8 :column 0 :text "(defun rollback-production () 'stable)") :outline (("Services" 1) ("Services/Preview" 2) ("Services/Production" 4) ("Operations" 6) ("Operations/Rollback" 7)) :outline-jump (:line 7 :column 0 :text ";;;; Rollback") :outline-preselect 5)"####
    ]];
    ParityBatchCase::value(
        "structural_navigation_flattens_imenu_and_follows_nested_operational_headings",
        elisp_form,
        expected,
    )
}

fn compile_picker_discovers_real_make_targets_and_preserves_project_environment_metadata()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((root (neomacs-counsel-test-root "compile")))
  (unwind-protect
      (progn
        (make-directory (expand-file-name ".git/" root) t)
        (neomacs-counsel-test-write-file
         root "Makefile"
         ".PHONY: build deploy verify\nbuild:\n\t@echo build\ndeploy:\n\t@echo deploy\nverify:\n\t@echo verify\n")
        (neomacs-counsel-test-in-buffer
         "*counsel-compile-origin*" "release project"
         (lambda ()
           (setq default-directory root)
           (let* ((counsel-compile-root-functions '(counsel--git-root))
                  (counsel-compile-local-builds
                   '(counsel-compile-get-make-invocation))
                  (counsel-compile-make-args "-k -j2")
                  (counsel-compile-env '("DEPLOY_ENV=staging" "REGION=us-east-1"))
                  (counsel-compile-history nil)
                  (neomacs-counsel-test-compile-log nil)
                  (candidates (counsel--get-compile-candidates root))
                  (summary
                   (mapcar
                    (lambda (candidate)
                      (list
                       (replace-regexp-in-string
                        (regexp-quote root) "[PROJECT]/"
                        (substring-no-properties candidate) t t)
                       (get-text-property 0 'cmd candidate)
                       (file-relative-name
                        (get-text-property 0 'srcdir candidate) root)
                       (file-relative-name
                        (get-text-property 0 'blddir candidate) root)
                       (copy-sequence
                        (get-text-property 0 'bldenv candidate))))
                    candidates)))
             (cl-letf (((symbol-function 'compile)
                        #'neomacs-counsel-test-compile))
               (execute-kbd-macro
                (kbd "C-c c C-n RET")))
             (list :candidates summary
                   :selected (nreverse neomacs-counsel-test-compile-log))))))
    (neomacs-counsel-test-clean-root root)))
"##;
    let expected = expect![[
        r####"OK (:candidates (("make -k -j2 build in [PROJECT]/ with DEPLOY_ENV=staging REGION=us-east-1" t "./" "./" ("DEPLOY_ENV=staging" "REGION=us-east-1")) ("make -k -j2 deploy in [PROJECT]/ with DEPLOY_ENV=staging REGION=us-east-1" t "./" "./" ("DEPLOY_ENV=staging" "REGION=us-east-1")) ("make -k -j2 verify in [PROJECT]/ with DEPLOY_ENV=staging REGION=us-east-1" t "./" "./" ("DEPLOY_ENV=staging" "REGION=us-east-1"))) :selected (("make -k -j2 deploy" nil "counsel-compile-fixture" ("DEPLOY_ENV=staging" "REGION=us-east-1"))))"####
    ]];
    ParityBatchCase::value(
        "compile_picker_discovers_real_make_targets_and_preserves_project_environment_metadata",
        elisp_form,
        expected,
    )
}

#[test]
fn counsel_package_batch() {
    assert_oracle_batch_cases(
        counsel_oracle(),
        "counsel-package-batch",
        "Counsel",
        &[
            command_palette_remaps_m_x_executes_prefix_commands_and_records_history(),
            file_picker_opens_hidden_spaced_and_nested_project_files(),
            git_workflow_lists_only_tracked_files_and_navigates_a_real_repository_search(),
            kill_ring_picker_filters_noise_and_replaces_the_last_yank_with_an_operator_command(),
            structural_navigation_flattens_imenu_and_follows_nested_operational_headings(),
            compile_picker_discovers_real_make_targets_and_preserves_project_environment_metadata(),
        ],
    );
}
