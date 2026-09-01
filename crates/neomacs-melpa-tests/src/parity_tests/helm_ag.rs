use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_AG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_AG_TEST_TIMEOUT: Duration = Duration::from_secs(90);
const HELM_AG_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'helm-ag)

(defun neomacs-helm-ag-test-root (name)
  "Create and return a deterministic sandbox directory for NAME."
  (let ((root (expand-file-name
               (concat "helm-ag-" name "/")
               (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-helm-ag-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-ag-test-read (path)
  "Read PATH without visiting it."
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-helm-ag-test-location (root)
  "Describe the current visited location relative to ROOT."
  (list :file (and buffer-file-name
                   (file-relative-name buffer-file-name root))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :line-text (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position))))

(defun neomacs-helm-ag-test-face-spans (string)
  "Describe every non-nil face run in STRING."
  (let ((position 0)
        spans)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (or (next-single-property-change
                        position 'face string (length string))
                       (length string))))
        (when face
          (push (list :range (list position next)
                      :text (substring-no-properties string position next)
                      :face face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun neomacs-helm-ag-test-cleanup (root)
  "Kill test buffers and remove ROOT."
  (dolist (buffer (buffer-list))
    (when (or (member (buffer-name buffer)
                      '("*helm ag results*" "*helm-ag-edit*"
                        " *helm-ag persistent*"))
              (and (buffer-file-name buffer)
                   (string-prefix-p root (buffer-file-name buffer))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when (file-exists-p root)
    (delete-directory root t)))
"###;

fn helm_ag_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_AG_MELPA_PIN, "helm-ag.el")
        .expect("prepare revision-pinned Helm Ag source below ./tmp")
        .with_prelude(HELM_AG_TEST_PRELUDE)
        .with_timeout(HELM_AG_TEST_TIMEOUT)
}

fn ripgrep_project_search_builds_argument_safe_command_and_returns_sorted_matches()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-ag-test-root "project-search"))
       (default-directory root)
       (helm-ag-base-command
        "rg --line-number --no-heading --color=never --sort=path")
       (helm-ag-command-option nil)
       (helm-ag-ignore-patterns nil)
       (helm-ag-use-grep-ignore-list nil)
       (helm-ag-use-agignore nil)
       (helm-ag--buffer-search nil)
       (helm-ag--default-directory root)
       (helm-ag--default-target
        (list (expand-file-name "src" root)))
       (helm-ag--last-query "--ignore-case deploy_(canary|stable)"))
  (unwind-protect
      (progn
        (neomacs-helm-ag-test-write
         (expand-file-name "src/deploy.el" root)
         "(defun deploy-canary (release)\n  (message \"deploy_canary %s\" release))\n")
        (neomacs-helm-ag-test-write
         (expand-file-name "src/stable.el" root)
         "(defun deploy-stable (release)\n  (message \"DEPLOY_STABLE %s\" release))\n")
        (neomacs-helm-ag-test-write
         (expand-file-name "vendor/generated.el" root)
         "(message \"deploy_canary generated\")\n")
        (helm-ag--set-command-features)
        (let ((command (helm-ag--construct-command nil)))
          (with-temp-buffer
            (let ((exit-status
                   (apply #'process-file
                          (car command) nil t nil (cdr command))))
              (list :features helm-ag--command-features
                    :command command
                    :exit exit-status
                    :query helm-ag--last-query
                    :elisp-query helm-ag--elisp-regexp-query
                    :valid-regexp helm-ag--valid-regexp-for-emacs
                    :output (buffer-string))))))
    (neomacs-helm-ag-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:features (re2 rg) :command ("rg" "--line-number" "--no-heading" "--color=never" "--sort=path" "--ignore-case" "deploy_(canary|stable)" "src") :exit 0 :query "deploy_(canary|stable)" :elisp-query "deploy_\\(canary\\|stable\\)" :valid-regexp t :output "src/deploy.el:2:  (message \"deploy_canary %s\" release))\nsrc/stable.el:2:  (message \"DEPLOY_STABLE %s\" release))\n")"#
    ]];
    ParityBatchCase::value(
        "ripgrep_project_search_builds_argument_safe_command_and_returns_sorted_matches",
        elisp_form,
        expected,
    )
}

fn query_dialects_and_candidate_highlighting_preserve_search_intent() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((candidate
        "src/deploy.el:42:deploy_canary release to us-east-1")
       (helm-ag--last-command '("rg" "--line-number"))
       (helm-ag--last-query "deploy_(canary|stable)")
       (helm-ag--elisp-regexp-query
        (helm-ag--convert-to-elisp-regexp helm-ag--last-query))
       (helm-ag--valid-regexp-for-emacs
        (helm-ag--validate-regexp helm-ag--elisp-regexp-query))
       (helm-ag--ignore-case nil)
       (display (helm-ag--candidate-transform-for-files candidate))
       pcre re2 fixed)
  (let ((helm-ag-base-command "rg --pcre2 --line-number")
        (helm-ag-command-option "--hidden")
        (helm-ag--extra-options nil)
        (helm-ag-ignore-patterns nil)
        (helm-ag-use-grep-ignore-list nil)
        (helm-ag-use-agignore nil)
        (helm-ag--buffer-search nil)
        (helm-ag--default-target nil)
        (helm-do-ag--extensions nil))
    (helm-ag--set-command-features)
    (helm-ag--do-ag-set-command)
    (setq pcre
          (list :features helm-ag--command-features
                :command
                (helm-ag--construct-do-ag-command
                 "deploy !rollback"))))
  (let ((helm-ag-base-command "rg --line-number"))
    (helm-ag--set-command-features)
    (setq re2
          (list :features helm-ag--command-features
                :joined (helm-ag--join-patterns "release canary"))))
  (let ((helm-ag-base-command "rg --fixed-strings"))
    (helm-ag--set-command-features)
    (setq fixed
          (list :features helm-ag--command-features
                :joined (helm-ag--join-patterns "release canary"))))
  (list :pcre pcre
        :re2 re2
        :fixed fixed
        :converted
        (helm-ag--convert-to-elisp-regexp
         "(deploy|rollback)_[a-z]{2,4}\\s")
        :roundtrip
        (helm-ag--elisp-regexp-to-pcre
         "\\(deploy\\|rollback\\)_[a-z]\\{2,4\\}")
        :candidate (substring-no-properties display)
        :faces (neomacs-helm-ag-test-face-spans display)))
"###;
    let expected = expect![[
        r#"OK (:pcre (:features (pcre rg) :command ("rg" "--pcre2" "--line-number" "--hidden" "(?=.*deploy.*)(?=^(?!.*rollback).+$)")) :re2 (:features (re2 rg) :joined "release.*canary") :fixed (:features (fixed rg) :joined "release canary") :converted "\\(deploy\\|rollback\\)_[a-z]\\{2,4\\}\\s-" :roundtrip "(deploy|rollback)_[a-z]{2,4}" :candidate "src/deploy.el:42:deploy_canary release to us-east-1" :faces ((:range (0 13) :text "src/deploy.el" :face helm-moccur-buffer) (:range (14 16) :text "42" :face helm-grep-lineno) (:range (17 30) :text "deploy_canary" :face helm-match)))"#
    ]];
    ParityBatchCase::value(
        "query_dialects_and_candidate_highlighting_preserve_search_intent",
        elisp_form,
        expected,
    )
}

fn saved_results_jump_to_match_and_context_stack_returns_to_runbook() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-ag-test-root "saved-navigation"))
       (runbook (expand-file-name "notes/runbook.org" root))
       (deployment (expand-file-name "src/deploy.el" root))
       (helm-ag--context-stack nil)
       (helm-ag--last-query "deploy_canary")
       (helm-ag--last-command '("rg" "--line-number"))
       (helm-ag--default-directory root)
       saved-source jumped returned)
  (unwind-protect
      (progn
        (neomacs-helm-ag-test-write
         runbook
         "* REL-2048\nInvestigate deployment before promotion.\n")
        (neomacs-helm-ag-test-write
         deployment
         "(defun release-flow (release)\n  (deploy_canary release)\n  (notify-team release))\n")
        (let ((origin (find-file-noselect runbook)))
          (with-current-buffer origin
            (goto-char (point-min))
            (search-forward "REL-2048")
            (let ((helm-current-buffer origin))
              (helm-ag--save-current-context))))
        (with-current-buffer (get-buffer-create "*helm ag results*")
          (setq default-directory root)
          (helm-ag--put-result-in-save-buffer
           "src/deploy.el:2:  (deploy_canary release)\n"
           nil)
          (setq saved-source
                (buffer-substring-no-properties (point-min) (point-max)))
          (goto-char (point-min))
          (search-forward "src/deploy.el:")
          (beginning-of-line)
          (helm-ag-mode-jump)
          (setq jumped (neomacs-helm-ag-test-location root)))
        (helm-ag-pop-stack)
        (setq returned (neomacs-helm-ag-test-location root))
        (list :saved-buffer saved-source
              :jumped jumped
              :returned returned
              :stack helm-ag--context-stack))
    (neomacs-helm-ag-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:saved-buffer "-*- mode: helm-ag -*-\n\nAg Results for `deploy_canary':\n\nsrc/deploy.el:2:  (deploy_canary release)\n" :jumped (:file "src/deploy.el" :point 34 :line 2 :column 3 :line-text "  (deploy_canary release)") :returned (:file "notes/runbook.org" :point 11 :line 1 :column 10 :line-text "* REL-2048") :stack nil)"#
    ]];
    ParityBatchCase::value(
        "saved_results_jump_to_match_and_context_stack_returns_to_runbook",
        elisp_form,
        expected,
    )
}

fn editable_results_apply_multi_file_refactor_and_delete_obsolete_match() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-ag-test-root "edit-results"))
       (deploy (expand-file-name "src/deploy.el" root))
       (obsolete (expand-file-name "src/obsolete.el" root))
       (helm-ag--last-command '("rg" "--line-number"))
       (helm-ag--original-window (selected-window))
       (helm-ag-edit-save t)
       (inhibit-message t)
       deletion-marked)
  (unwind-protect
      (progn
        (neomacs-helm-ag-test-write
         deploy
         "(defun release-flow (release)\n  (deploy-canary release)\n  (notify-team release))\n")
        (neomacs-helm-ag-test-write
         obsolete
         "  (legacy-deploy release)\n  (keep-audit-record release)\n")
        (with-current-buffer (get-buffer-create "*helm-ag-edit*")
          (erase-buffer)
          (setq default-directory root)
          (setq-local helm-ag--search-this-file-p nil)
          (insert
           "src/deploy.el:2:  (deploy-canary-with-audit release)\n"
           "src/deploy.el:3:  (notify-release-team release))\n"
           "src/obsolete.el:1:  (legacy-deploy release)\n")
          (goto-char (point-min))
          (forward-line 2)
          (helm-ag--mark-line-deleted)
          (setq deletion-marked
                (and (cl-some
                      (lambda (overlay)
                        (overlay-get overlay 'helm-ag-deleted))
                      (overlays-at (line-beginning-position)))
                     t))
          (helm-ag--edit-commit))
        (list :deletion-marked deletion-marked
              :deploy (neomacs-helm-ag-test-read deploy)
              :obsolete (neomacs-helm-ag-test-read obsolete)
              :edit-buffer-live
              (and (get-buffer "*helm-ag-edit*") t)))
    (neomacs-helm-ag-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:deletion-marked t :deploy "(defun release-flow (release)\n  (deploy-canary-with-audit release)\n  (notify-release-team release))\n" :obsolete "  (keep-audit-record release)\n" :edit-buffer-live nil)"#
    ]];
    ParityBatchCase::value(
        "editable_results_apply_multi_file_refactor_and_delete_obsolete_match",
        elisp_form,
        expected,
    )
}

fn project_discovery_ignore_file_and_extension_filters_follow_nested_workspace() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-helm-ag-test-root "project-discovery"))
       (nested (expand-file-name "services/api/" root))
       (source (expand-file-name "services/api/release.el" root))
       (generated (expand-file-name "services/api/release.generated.el" root))
       (notes (expand-file-name "notes/release.md" root))
       (default-directory nested)
       (helm-ag-ignore-buffer-patterns
        '("generated" "\\.md\\'"))
       (helm-do-ag--extensions '("*.el" "*.rs" "*"))
       buffers)
  (unwind-protect
      (progn
        (make-directory (expand-file-name ".git" root) t)
        (neomacs-helm-ag-test-write
         (expand-file-name ".agignore" root)
         "vendor/\n*.min.js\n")
        (neomacs-helm-ag-test-write source "(provide 'release)\n")
        (neomacs-helm-ag-test-write generated "(provide 'generated)\n")
        (neomacs-helm-ag-test-write notes "# Release notes\n")
        (find-file-noselect source)
        (find-file-noselect generated)
        (find-file-noselect notes)
        (setq buffers
              (sort
               (cl-remove-if-not
                (lambda (path) (string-prefix-p root path))
                (helm-ag--file-visited-buffers))
               #'string<))
        (list :root
              (file-relative-name (helm-ag--project-root) root)
              :agignore
              (file-relative-name (helm-ag--root-agignore) root)
              :search-buffers
              (mapcar (lambda (path) (file-relative-name path root))
                      buffers)
              :targets
              (let ((helm-ag--default-directory root))
                (helm-ag--construct-targets buffers))
              :extensions (helm-ag--construct-extension-options)))
    (neomacs-helm-ag-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:root "./" :agignore ".agignore" :search-buffers ("services/api/release.el") :targets ("services/api/release.el") :extensions ("-G\\.el" "-G\\.rs"))"#
    ]];
    ParityBatchCase::value(
        "project_discovery_ignore_file_and_extension_filters_follow_nested_workspace",
        elisp_form,
        expected,
    )
}

fn custom_success_codes_and_invalid_queries_keep_precise_failure_contracts() -> ParityBatchCase {
    let elisp_form = r###"
(let ((helm-ag-success-exit-status nil)
      exact-status
      listed-status)
  (let ((default-status
         (mapcar (lambda (status)
                   (list status
                         (and (helm-ag--command-succeeded-p status) t)))
                 '(0 1 2))))
    (let ((helm-ag-success-exit-status 1))
      (setq exact-status
            (mapcar (lambda (status)
                      (list status
                            (and (helm-ag--command-succeeded-p status) t)))
                    '(0 1 2))))
    (let ((helm-ag-success-exit-status '(0 2)))
      (setq listed-status
            (mapcar (lambda (status)
                      (list status
                            (and (helm-ag--command-succeeded-p status) t)))
                    '(0 1 2))))
    (list :default default-status
          :exact exact-status
          :listed listed-status
          :regexp-validation
          (list (list "(release|rollback)"
                      (helm-ag--validate-regexp
                       (helm-ag--convert-to-elisp-regexp
                        "(release|rollback)")))
                (list "\\(" (helm-ag--validate-regexp "\\(")))
          :empty-query
          (condition-case error-data
              (list :value (helm-ag--query ""))
            (error
             (list :signal (car error-data)
                   :data (cdr error-data)
                   :message (error-message-string error-data)))))))
"###;
    let expected = expect![[
        r#"OK (:default ((0 t) (1 nil) (2 nil)) :exact ((0 nil) (1 t) (2 nil)) :listed ((0 t) (1 nil) (2 t)) :regexp-validation (("(release|rollback)" t) ("\\(" nil)) :empty-query (:signal error :data ("Input is empty!!") :message "Input is empty!!"))"#
    ]];
    ParityBatchCase::value(
        "custom_success_codes_and_invalid_queries_keep_precise_failure_contracts",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_ag_package_batch() {
    assert_oracle_batch_cases(
        helm_ag_oracle(),
        "helm-ag-package-batch",
        "helm-ag",
        &[
            ripgrep_project_search_builds_argument_safe_command_and_returns_sorted_matches(),
            query_dialects_and_candidate_highlighting_preserve_search_intent(),
            saved_results_jump_to_match_and_context_stack_returns_to_runbook(),
            editable_results_apply_multi_file_refactor_and_delete_obsolete_match(),
            project_discovery_ignore_file_and_extension_filters_follow_nested_workspace(),
            custom_success_codes_and_invalid_queries_keep_precise_failure_contracts(),
        ],
    );
}
