use expect_test::expect;

use super::ParityBatchCase;

fn workspace_symbol_without_an_active_server_reports_the_public_error() -> ParityBatchCase {
    let elisp_form = r####"
(let ((lsp--cur-workspace nil)
      (lsp--buffer-workspaces nil))
  (neomacs-helm-lsp-test-capture
   (lambda () (helm-lsp-workspace-symbol nil))))
"####;
    let expected = expect![
        "OK (:signal user-error :data (\"No LSP workspace active\") :message \"No LSP workspace active\")"
    ];
    ParityBatchCase::value(
        "workspace_symbol_without_an_active_server_reports_the_public_error",
        elisp_form,
        expected,
    )
}

fn workspace_symbol_search_renders_server_results_and_jumps_to_the_selected_definition()
-> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (let* ((root (neomacs-helm-lsp-test-root "workspace-symbol"))
         (enable-dir-local-variables nil)
         (runbook (expand-file-name "notes/runbook.el" root))
         (controller (expand-file-name "src/controller.el" root))
         (service (expand-file-name "src/release-service.el" root))
         (workspace
          (make-lsp--workspace :root root :client (make-lsp--client)))
         (symbols
          (list
           (neomacs-helm-lsp-test-symbol
            "CreateRelease" 12 "ReleaseController" controller 1 9)
           (neomacs-helm-lsp-test-symbol
            "PromoteRelease" 12 "ReleaseService" service 2 9)
           (neomacs-helm-lsp-test-symbol
            "RollbackRelease" 12 "ReleaseService" service 5 9)))
         (lsp--cur-workspace nil)
         (lsp--buffer-workspaces (list workspace))
         (helm-lsp-treemacs-icons nil)
         (helm-candidate-number-limit 20)
         (helm-map (copy-keymap helm-map))
         (executing-kbd-macro t)
         (unread-command-events
          (listify-key-sequence (kbd "<f8> RET")))
         (helm-after-update-hook
          (cons #'neomacs-helm-lsp-test-record-display
                helm-after-update-hook))
         (helm-move-selection-after-hook
          (cons #'neomacs-helm-lsp-test-record-selection
                helm-move-selection-after-hook)))
    (unwind-protect
        (progn
          (neomacs-helm-lsp-test-write
           runbook
           ";; Release runbook\n(PromoteRelease \"candidate-42\")\n")
          (neomacs-helm-lsp-test-write
           controller
           ";; Controller\n(defun CreateRelease (release)\n  (list :create release))\n")
          (neomacs-helm-lsp-test-write
           service
           ";; Release service\n\n(defun PromoteRelease (release)\n  (list :promote release))\n\n(defun RollbackRelease (release)\n  (list :rollback release))\n")
          (switch-to-buffer (find-file-noselect runbook))
          (goto-char (point-min))
          (search-forward "PromoteRelease")
          (goto-char (match-beginning 0))
          (define-key helm-map (kbd "<f8>")
                      #'neomacs-helm-lsp-test-deliver-symbol-response)
          (setq neomacs-helm-lsp-test-request-log nil
                neomacs-helm-lsp-test-display nil
                neomacs-helm-lsp-test-selection-log nil
                neomacs-helm-lsp-test-pending-symbol-response nil
                neomacs-helm-lsp-test-symbol-request-state-log nil)
          (cl-letf (((symbol-function 'lsp-request-async)
                     (lambda (method params callback &rest options)
                       (push (list :method method
                                   :params params
                                   :options options)
                             neomacs-helm-lsp-test-request-log)
                       (let ((query (plist-get params :query)))
                         (setq neomacs-helm-lsp-test-pending-symbol-response
                               (cons
                                callback
                                (seq-filter
                                 (lambda (symbol)
                                   (string-match-p
                                    (regexp-quote (downcase query))
                                    (downcase
                                     (lsp:symbol-information-name symbol))))
                                 symbols))))
                       '(:id 731))))
            (helm-lsp-workspace-symbol t))
          (list :requests (nreverse neomacs-helm-lsp-test-request-log)
                :request-states
                (nreverse neomacs-helm-lsp-test-symbol-request-state-log)
                :display neomacs-helm-lsp-test-display
                :selections (nreverse neomacs-helm-lsp-test-selection-log)
                :destination (neomacs-helm-lsp-test-selected-location root)
                :request-id helm-lsp-symbols-request-id))
      (neomacs-helm-lsp-test-cleanup root))))
"####;
    let expected = expect![[
        r#"OK (:requests ((:method "workspace/symbol" :params (:query "PromoteRelease") :options (:mode detached :cancel-token :workspace-symbols))) :request-states ((:before-delivery nil :after-delivery nil)) :display (:text "Workspace symbol\nPromoteRelease ReleaseService - (Function) · release-service.el\n" :faces ((:text "Workspace symbol\n" :face helm-source-header) (:text "PromoteRelease" :face helm-match) (:text "ReleaseService" :face helm-lsp-container-face) (:text "(Function)" :face font-lock-type-face) (:text " · " :face success) (:text "release-service.el" :face helm-lsp-container-face))) :selections ((:text "PromoteRelease ReleaseService - (Function) · release-service.el" :faces ((:text "PromoteRelease" :face helm-match) (:text "ReleaseService" :face helm-lsp-container-face) (:text "(Function)" :face font-lock-type-face) (:text " · " :face success) (:text "release-service.el" :face helm-lsp-container-face)))) :destination (:file "src/release-service.el" :line 3 :column 9 :text "(defun PromoteRelease (release)") :request-id nil)"#
    ]];
    ParityBatchCase::value(
        "workspace_symbol_search_renders_server_results_and_jumps_to_the_selected_definition",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn a_single_code_action_applies_the_real_workspace_edit_without_opening_helm() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((root (neomacs-helm-lsp-test-root "single-code-action"))
         (enable-dir-local-variables nil)
         (source (expand-file-name "src/deploy.el" root))
         (workspace
          (make-lsp--workspace
           :root root
           :client (make-lsp--client)
           :server-capabilities (make-hash-table :test 'equal)))
         (lsp--cur-workspace workspace)
         (lsp--buffer-workspaces (list workspace))
         (lsp-auto-execute-action t)
         request action)
    (unwind-protect
        (progn
          (neomacs-helm-lsp-test-write
           source
           "(defun deploy (release)\n  (promote release))\n")
          (switch-to-buffer (find-file-noselect source))
          (goto-char (point-min))
          (search-forward "promote")
          (setq action
                (neomacs-helm-lsp-test-code-action
                 "Add an audit event" source 1 2 "(audit release)\n  "))
          (setq neomacs-helm-lsp-test-display nil)
          (cl-letf (((symbol-function 'lsp-request)
                     (lambda (method params)
                       (setq request
                             (neomacs-helm-lsp-test-code-action-request
                              root method params))
                       (list action))))
            (helm-lsp-code-actions))
          (list :request request
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max))
                :point (point)
                :line (line-number-at-pos)
                :column (current-column)
                :modified (buffer-modified-p)
                :helm-opened neomacs-helm-lsp-test-display))
      (neomacs-helm-lsp-test-cleanup root))))
"####;
    let expected = expect![[
        r#"OK (:request (:method "textDocument/codeAction" :file "src/deploy.el" :range ((1 10) (1 10)) :diagnostic-count 0) :buffer "(defun deploy (release)\n  (audit release)\n  (promote release))\n" :point 53 :line 3 :column 10 :modified t :helm-opened nil)"#
    ]];
    ParityBatchCase::value(
        "a_single_code_action_applies_the_real_workspace_edit_without_opening_helm",
        elisp_form,
        expected,
    )
}

fn multiple_code_actions_render_in_helm_and_apply_the_selected_server_edit() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (let* ((root (neomacs-helm-lsp-test-root "multiple-code-actions"))
         (enable-dir-local-variables nil)
         (source (expand-file-name "src/deploy.el" root))
         (workspace
          (make-lsp--workspace
           :root root
           :client (make-lsp--client)
           :server-capabilities (make-hash-table :test 'equal)))
         (lsp--cur-workspace workspace)
         (lsp--buffer-workspaces (list workspace))
         (lsp-auto-execute-action nil)
         (executing-kbd-macro t)
         (unread-command-events
          (listify-key-sequence (kbd "C-n RET")))
         (helm-after-update-hook
          (cons #'neomacs-helm-lsp-test-record-display
                helm-after-update-hook))
         (helm-move-selection-after-hook
          (cons #'neomacs-helm-lsp-test-record-selection
                helm-move-selection-after-hook))
         request actions)
    (unwind-protect
        (progn
          (neomacs-helm-lsp-test-write
           source
           "(defun deploy (release)\n  (promote release))\n")
          (switch-to-buffer (find-file-noselect source))
          (goto-char (point-min))
          (search-forward "promote")
          (setq actions
                (list
                 (neomacs-helm-lsp-test-code-action
                  "Add an audit event" source 1 2 "(audit release)\n  ")
                 (neomacs-helm-lsp-test-code-action
                  "Notify the release team" source 1 2
                  "(notify-team release)\n  "))
                neomacs-helm-lsp-test-display nil
                neomacs-helm-lsp-test-selection-log nil)
          (cl-letf (((symbol-function 'lsp-request)
                     (lambda (method params)
                       (setq request
                             (neomacs-helm-lsp-test-code-action-request
                              root method params))
                       actions)))
            (helm-lsp-code-actions))
          (list :request request
                :display neomacs-helm-lsp-test-display
                :selections (nreverse neomacs-helm-lsp-test-selection-log)
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max))
                :point (point)
                :line (line-number-at-pos)
                :column (current-column)
                :modified (buffer-modified-p)))
      (neomacs-helm-lsp-test-cleanup root))))
"####;
    let expected = expect![[
        r#"OK (:request (:method "textDocument/codeAction" :file "src/deploy.el" :range ((1 10) (1 10)) :diagnostic-count 0) :display (:text "Code Actions\nAdd an audit event\nNotify the release team\n" :faces ((:text "Code Actions\n" :face helm-source-header))) :selections ((:text "Add an audit event" :faces nil) (:text "Notify the release team" :faces nil)) :buffer "(defun deploy (release)\n  (notify-team release)\n  (promote release))\n" :point 59 :line 3 :column 10 :modified t)"#
    ]];
    ParityBatchCase::value(
        "multiple_code_actions_render_in_helm_and_apply_the_selected_server_edit",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn no_code_actions_preserves_the_source_and_signals_the_lsp_condition() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((root (neomacs-helm-lsp-test-root "no-code-actions"))
         (enable-dir-local-variables nil)
         (source (expand-file-name "src/deploy.el" root))
         (workspace
          (make-lsp--workspace
           :root root
           :client (make-lsp--client)
           :server-capabilities (make-hash-table :test 'equal)))
         (lsp--cur-workspace workspace)
         (lsp--buffer-workspaces (list workspace))
         request outcome)
    (unwind-protect
        (progn
          (neomacs-helm-lsp-test-write
           source
           "(defun deploy (release)\n  (promote release))\n")
          (switch-to-buffer (find-file-noselect source))
          (goto-char (point-min))
          (search-forward "promote")
          (cl-letf (((symbol-function 'lsp-request)
                     (lambda (method params)
                       (setq request
                             (neomacs-helm-lsp-test-code-action-request
                              root method params))
                       nil)))
            (setq outcome
                  (neomacs-helm-lsp-test-capture
                   #'helm-lsp-code-actions)))
          (list :request request
                :outcome outcome
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max))
                :point (point)
                :modified (buffer-modified-p)))
      (neomacs-helm-lsp-test-cleanup root))))
"####;
    let expected = expect![[
        r#"OK (:request (:method "textDocument/codeAction" :file "src/deploy.el" :range ((1 10) (1 10)) :diagnostic-count 0) :outcome (:signal lsp-no-code-actions :data nil :message "No code actions") :buffer "(defun deploy (release)\n  (promote release))\n" :point 35 :modified nil)"#
    ]];
    ParityBatchCase::value(
        "no_code_actions_preserves_the_source_and_signals_the_lsp_condition",
        elisp_form,
        expected,
    )
}

fn diagnostic_query_filters_real_session_data_and_jumps_to_the_exact_problem() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (let* ((root (neomacs-helm-lsp-test-root "diagnostic-query"))
         (enable-dir-local-variables nil)
         (runbook (expand-file-name "notes/runbook.el" root))
         (payments (expand-file-name "src/payments.el" root))
         (ledger (expand-file-name "src/ledger.el" root))
         (workspace
          (make-lsp--workspace
           :root root
           :client (make-lsp--client)
           :server-capabilities (make-hash-table :test 'equal)))
         (folder-servers (make-hash-table :test 'equal))
         (session
          (make-lsp-session
           :folders (list root)
           :folder->servers folder-servers))
         (lsp--session session)
         (lsp--cur-workspace nil)
         (lsp--buffer-workspaces nil)
         (helm-input-idle-delay 0)
         (helm-map (copy-keymap helm-map))
         (executing-kbd-macro t)
         (unread-command-events
          (append
           (string-to-list "*error #payments Retry checkout")
           (listify-key-sequence (kbd "<f8> RET"))))
         (helm-after-update-hook
          (cons #'neomacs-helm-lsp-test-record-display
                helm-after-update-hook))
         (helm-move-selection-after-hook
          (cons #'neomacs-helm-lsp-test-record-selection
                helm-move-selection-after-hook)))
    (unwind-protect
        (progn
          (neomacs-helm-lsp-test-write
           runbook
           ";; Investigate release diagnostics before promotion.\n")
          (neomacs-helm-lsp-test-write
           payments
           "(defun checkout (release)\n  (retry-budget checkout)\n\n  (promote release))\n")
          (neomacs-helm-lsp-test-write
           ledger
           "(defun reconcile-ledger ()\n  (retry-ledger))\n")
          (puthash root (list workspace) folder-servers)
          (puthash
           payments
           (list
            (neomacs-helm-lsp-test-diagnostic
             "Retry budget exhausted for checkout" "compiler" 1 1 2)
            (neomacs-helm-lsp-test-diagnostic
             "Retry budget below release threshold" "policy" 2 3 2))
           (lsp--workspace-diagnostics workspace))
          (puthash
           ledger
           (list
            (neomacs-helm-lsp-test-diagnostic
             "Retry reconciliation failed for ledger" "compiler" 1 1 2))
           (lsp--workspace-diagnostics workspace))
          (switch-to-buffer (find-file-noselect runbook))
          (define-key helm-map (kbd "<f8>")
                      #'neomacs-helm-lsp-test-process-input)
          (setq neomacs-helm-lsp-test-display nil
                neomacs-helm-lsp-test-display-log nil
                neomacs-helm-lsp-test-pattern nil
                neomacs-helm-lsp-test-selection-log nil)
          (helm-lsp-diagnostics nil)
          (list
           :query neomacs-helm-lsp-test-pattern
           :unfiltered
           (cl-find-if
            (lambda (display)
              (string-match-p "ledger" (plist-get display :text)))
            (reverse neomacs-helm-lsp-test-display-log))
           :filtered neomacs-helm-lsp-test-display
           :selections (nreverse neomacs-helm-lsp-test-selection-log)
           :destination (neomacs-helm-lsp-test-selected-location root)))
      (neomacs-helm-lsp-test-cleanup root))))
"####;
    let expected = expect![[
        r#"OK (:query "*error #payments Retry checkout" :unfiltered (:text "Diagnostics\n[error] [compiler] compiler Retry budget exhausted for checkout src/payments.el:1:2\n[error] [compiler] compiler Retry reconciliation failed for ledger src/ledger.el:1:2\n[warning] [policy] policy Retry budget below release threshold src/payments.el:3:2\n" :faces ((:text "Diagnostics\n" :face helm-source-header) (:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "src/payments.el:1:2" :face lsp-details-face) (:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "src/ledger.el:1:2" :face lsp-details-face) (:text "[warning] " :face helm-lsp-diag-warning) (:text "[policy]" :face lsp-details-face) (:text "src/payments.el:3:2" :face lsp-details-face))) :filtered (:text "Diagnostics\n[error] [compiler] compiler Retry budget exhausted for checkout src/payments.el:1:2\n" :faces ((:text "Diagnostics\n" :face helm-source-header) (:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "Retry" :face helm-match) (:text "checkout" :face helm-match) (:text "src/payments.el:1:2" :face lsp-details-face))) :selections ((:text "[error] [compiler] compiler Retry budget exhausted for checkout src/payments.el:1:2" :faces ((:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "src/payments.el:1:2" :face lsp-details-face))) (:text "[error] [compiler] compiler Retry budget exhausted for checkout src/payments.el:1:2" :faces ((:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "Retry" :face helm-match) (:text "checkout" :face helm-match) (:text "src/payments.el:1:2" :face lsp-details-face))) (:text "[error] [compiler] compiler Retry budget exhausted for checkout src/payments.el:1:2" :faces ((:text "[error] " :face helm-lsp-diag-error) (:text "[compiler]" :face lsp-details-face) (:text "Retry" :face helm-match) (:text "checkout" :face helm-match) (:text "src/payments.el:1:2" :face lsp-details-face)))) :destination (:file "src/payments.el" :line 2 :column 2 :text "  (retry-budget checkout)"))"#
    ]];
    ParityBatchCase::value(
        "diagnostic_query_filters_real_session_data_and_jumps_to_the_exact_problem",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        workspace_symbol_without_an_active_server_reports_the_public_error(),
        workspace_symbol_search_renders_server_results_and_jumps_to_the_selected_definition(),
        a_single_code_action_applies_the_real_workspace_edit_without_opening_helm(),
        multiple_code_actions_render_in_helm_and_apply_the_selected_server_edit(),
        no_code_actions_preserves_the_source_and_signals_the_lsp_condition(),
        diagnostic_query_filters_real_session_data_and_jumps_to_the_exact_problem(),
    ]
}
