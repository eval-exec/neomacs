use expect_test::expect;

use super::ParityBatchCase;

fn helm_source_formats_metadata_and_preserves_the_exact_company_candidates() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((company-frontends nil)
        (company-format-margin-function
         (lambda (candidate _selected)
           (if (equal candidate "deploy-production") "[P] " "[C] "))))
    (neomacs-helm-company-test-start "deploy-pr")
    (let* ((helm-buffer (current-buffer))
           (helm-current-source helm-source-company)
           (helm--in-fuzzy nil)
           (display-strings (funcall (helm-get-attr 'data helm-source-company)))
           (formatted
            (helm-company-get-formatted-display-strings display-strings)))
      (prog1
          (list
           :company-prefix company-prefix
           :company-candidates
           (mapcar #'neomacs-helm-company-test-candidate-shape
                   company-candidates)
           :display formatted
           :real-candidates
           (mapcar
            (lambda (display)
              (neomacs-helm-company-test-candidate-shape
               (helm-company-get-real-candidate display)))
            display-strings))
        (helm-company-cleanup-post-command)
        (company-abort)))))
"##;
    let expected = expect![[
        r#"OK (:company-prefix "deploy-pr" :company-candidates (("deploy-preview" canary) ("deploy-production" primary) ("deploy-preproduction" staging)) :display (#("[C] deploy-preproduction   staging" 25 34 (font-lock-face company-tooltip-annotation)) #("[C] deploy-preview   canary" 19 27 (font-lock-face company-tooltip-annotation)) #("[P] deploy-production   primary" 22 31 (font-lock-face company-tooltip-annotation))) :real-candidates (("deploy-preproduction" staging) ("deploy-preview" canary) ("deploy-production" primary)))"#
    ]];
    ParityBatchCase::value(
        "helm_source_formats_metadata_and_preserves_the_exact_company_candidates",
        elisp_form,
        expected,
    )
}

fn selected_candidate_replaces_the_prefix_and_runs_both_completion_contracts() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((company-frontends nil)
        (helm-company-after-completion-hooks
         '(neomacs-helm-company-test-after-completion))
        (company-completion-finished-hook
         '(neomacs-helm-company-test-company-finished)))
    (setq neomacs-helm-company-test-events nil)
    (neomacs-helm-company-test-start "deploy-pr")
    (let ((selected
           (cl-find-if
            (lambda (candidate)
              (eq (get-text-property 0 'neomacs-environment candidate)
                  'primary))
            company-candidates)))
      (helm-company-action-insert selected)
      (list
       :buffer (buffer-substring-no-properties (point-min) (point-max))
       :point (point)
       :company-active (and company-candidates t)
       :events (nreverse neomacs-helm-company-test-events)))))
"##;
    let expected = expect![[
        r#"OK (:buffer "deploy-production" :point 18 :company-active nil :events ((:company-finished "deploy-production") (:completed "deploy-production" primary) :helm-company-hook))"#
    ]];
    ParityBatchCase::value(
        "selected_candidate_replaces_the_prefix_and_runs_both_completion_contracts",
        elisp_form,
        expected,
    )
}

fn multi_candidate_command_passes_the_live_company_session_to_helm_and_aborts_cleanly()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((company-frontends nil)
        (helm-company-initialize-pattern-with-prefix t)
        (helm-company-candidate-number-limit 17)
        captured)
    (neomacs-helm-company-test-start "deploy-")
    (company-complete)
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest plist)
                 (setq captured
                       (list
                        :sources (plist-get plist :sources)
                        :buffer (plist-get plist :buffer)
                        :input (plist-get plist :input)
                        :candidate-number-limit
                        (plist-get plist :candidate-number-limit)
                        :company-prefix company-prefix
                        :company-candidates
                        (mapcar #'neomacs-helm-company-test-candidate-shape
                                company-candidates)
                        :quit-aborts-company
                        (and (memq 'company-abort helm-quit-hook) t)))
                 (run-hooks 'helm-quit-hook))))
      (helm-company))
    (list
     :before-quit captured
     :after-quit
     (list :buffer (buffer-substring-no-properties (point-min) (point-max))
           :company-prefix company-prefix
           :company-candidates company-candidates))))
"##;
    let expected = expect![[
        r#"OK (:before-quit (:sources helm-source-company :buffer "*helm company*" :input #("deploy-pr" 0 9 (neomacs-environment canary)) :candidate-number-limit 17 :company-prefix #("deploy-pr" 0 9 (neomacs-environment canary)) :company-candidates (("deploy-preview" canary) ("deploy-production" primary) ("deploy-preproduction" staging)) :quit-aborts-company t) :after-quit (:buffer "deploy-pr" :company-prefix nil :company-candidates nil))"#
    ]];
    ParityBatchCase::value(
        "multi_candidate_command_passes_the_live_company_session_to_helm_and_aborts_cleanly",
        elisp_form,
        expected,
    )
}

fn single_candidate_command_completes_directly_without_opening_helm() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((company-frontends nil)
        (helm-company-after-completion-hooks
         '(neomacs-helm-company-test-after-completion))
        (company-completion-finished-hook
         '(neomacs-helm-company-test-company-finished))
        (helm-opened nil))
    (setq neomacs-helm-company-test-events nil)
    (neomacs-helm-company-test-start "deploy-prev")
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest _)
                 (setq helm-opened t)
                 (error "Helm must not open for one candidate"))))
      (helm-company))
    (list
     :buffer (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :helm-opened helm-opened
     :company-active (and company-candidates t)
     :events (nreverse neomacs-helm-company-test-events))))
"##;
    let expected = expect![[
        r#"OK (:buffer "deploy-preview" :point 15 :helm-opened nil :company-active nil :events ((:company-finished "deploy-preview") (:completed "deploy-preview" canary)))"#
    ]];
    ParityBatchCase::value(
        "single_candidate_command_completes_directly_without_opening_helm",
        elisp_form,
        expected,
    )
}

fn documentation_actions_display_real_backend_buffers_and_reuse_the_help_window() -> ParityBatchCase
{
    let elisp_form = r##"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (let ((company-frontends nil))
      (setq neomacs-helm-company-test-events nil)
      (neomacs-helm-company-test-start "deploy-pr")
      (let ((helm-buffer (current-buffer))
            (helm-current-source helm-source-company))
        (funcall (helm-get-attr 'data helm-source-company))
        (unwind-protect
            (progn
            (helm-company-action-show-document "deploy-production")
            (let* ((action-buffer (get-buffer "*deployment primary help*"))
                   (action-window (get-buffer-window action-buffer))
                   (action-state
                    (list
                     :live (window-live-p action-window)
                     :buffer (buffer-name (window-buffer action-window))
                     :contents
                     (with-current-buffer action-buffer
                       (buffer-substring-no-properties (point-min) (point-max))))))
              (helm-company-show-doc-buffer "deploy-preview")
              (let ((persistent-window helm-company-help-window))
                (list
                 :action action-state
                 :persistent
                 (list
                  :live (window-live-p persistent-window)
                  :buffer (buffer-name (window-buffer persistent-window))
                  :side (window-parameter persistent-window 'window-side)
                  :point (window-point persistent-window)
                  :contents
                  (with-current-buffer (window-buffer persistent-window)
                    (buffer-substring-no-properties (point-min) (point-max))))
                 :events (nreverse neomacs-helm-company-test-events)))))
          (helm-company-cleanup-post-command)
          (company-abort)
          (dolist (name '("*deployment primary help*"
                          "*deployment canary help*"))
            (when-let ((buffer (get-buffer name))) (kill-buffer buffer))))))))
"##;
    let expected = expect![[
        r#"OK (:action (:live t :buffer "*deployment primary help*" :contents "deploy-production deploys to primary.\n") :persistent (:live t :buffer "*deployment canary help*" :side bottom :point 1 :contents "deploy-preview deploys to canary.\n") :events ((:document "deploy-production" primary) (:document "deploy-preview" canary)))"#
    ]];
    ParityBatchCase::value(
        "documentation_actions_display_real_backend_buffers_and_reuse_the_help_window",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn location_action_navigates_to_buffer_positions_and_file_line_numbers() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (let* ((company-frontends nil)
           (location-file
            (expand-file-name "deployment-targets.el"
                              (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
      (with-temp-file location-file
        (insert ";;; Deployment targets\n"
                "(defconst preview-target \"canary\")\n"
                "(defconst preproduction-target \"staging\")\n"
                "(defconst production-target \"primary\")\n"))
      (setq neomacs-helm-company-test-events nil)
      (neomacs-helm-company-test-start "deploy-pr")
      (let ((helm-buffer (current-buffer))
            (helm-current-source helm-source-company))
        (funcall (helm-get-attr 'data helm-source-company))
        (unwind-protect
            (progn
              (helm-company-find-location "deploy-production")
              (let* ((definition-buffer (get-buffer "*deployment definitions*"))
                     (definition-window (get-buffer-window definition-buffer))
                     (buffer-location
                      (list
                       :buffer (buffer-name (window-buffer definition-window))
                       :point (window-point definition-window)
                       :start (window-start definition-window)
                       :line
                       (with-current-buffer definition-buffer
                         (save-excursion
                           (goto-char (window-point definition-window))
                           (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position)))))))
                (helm-company-find-location "deploy-preproduction")
                (let* ((file-buffer (get-file-buffer location-file))
                       (file-window (get-buffer-window file-buffer)))
                  (list
                   :buffer-location buffer-location
                   :file-location
                   (list
                    :file (file-name-nondirectory
                           (buffer-file-name (window-buffer file-window)))
                    :point (window-point file-window)
                    :start (window-start file-window)
                    :line-number
                    (with-current-buffer file-buffer
                      (line-number-at-pos (window-point file-window)))
                    :line
                    (with-current-buffer file-buffer
                      (save-excursion
                        (goto-char (window-point file-window))
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))))
                   :events (nreverse neomacs-helm-company-test-events)))))
          (helm-company-cleanup-post-command)
          (company-abort)
          (dolist (buffer (list (get-buffer "*deployment definitions*")
                                (get-file-buffer location-file)))
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (when (file-exists-p location-file) (delete-file location-file)))))))
"##;
    let expected = expect![[
        r#"OK (:buffer-location (:buffer "*deployment definitions*" :point 21 :start 21 :line "Production target") :file-location (:file "deployment-targets.el" :point 59 :start 59 :line-number 3 :line "(defconst preproduction-target \"staging\")") :events ((:location "deploy-production" primary) (:location "deploy-preproduction" staging)))"#
    ]];
    ParityBatchCase::value(
        "location_action_navigates_to_buffer_positions_and_file_line_numbers",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn read_only_buffer_without_an_eligible_completion_never_opens_helm() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (company-mode 1)
  (insert "(defu")
  (goto-char (point-max))
  (read-only-mode 1)
  (let ((company-minimum-prefix-length 10)
        (helm-opened nil)
        result)
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest _)
                 (setq helm-opened t)
                 (error "Helm must not open without candidates"))))
      (setq result (helm-company)))
    (list
     :result result
     :buffer (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :read-only buffer-read-only
     :helm-opened helm-opened
     :company-prefix company-prefix
     :company-candidates company-candidates)))
"##;
    let expected = expect![[
        r#"OK (:result nil :buffer "(defu" :point 6 :read-only t :helm-opened nil :company-prefix nil :company-candidates nil)"#
    ]];
    ParityBatchCase::value(
        "read_only_buffer_without_an_eligible_completion_never_opens_helm",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        helm_source_formats_metadata_and_preserves_the_exact_company_candidates(),
        selected_candidate_replaces_the_prefix_and_runs_both_completion_contracts(),
        multi_candidate_command_passes_the_live_company_session_to_helm_and_aborts_cleanly(),
        single_candidate_command_completes_directly_without_opening_helm(),
        documentation_actions_display_real_backend_buffers_and_reuse_the_help_window(),
        location_action_navigates_to_buffer_positions_and_file_line_numbers(),
        read_only_buffer_without_an_eligible_completion_never_opens_helm(),
    ]
}
