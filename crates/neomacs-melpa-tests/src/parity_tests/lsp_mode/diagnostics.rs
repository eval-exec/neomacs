use expect_test::expect;

use super::ParityBatchCase;

fn publishing_replacing_and_cleaning_workspace_diagnostics_updates_project_totals()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "lsp-diagnostics-project"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (service-directory (expand-file-name "services/" root))
       (payments (expand-file-name "payments.el" service-directory))
       (ledger (expand-file-name "ledger.el" service-directory))
       (workspace (make-lsp--workspace))
       (lsp-diagnostic-stats (ht))
       (updates 0))
  (add-hook 'lsp-diagnostics-updated-hook
            (lambda () (setq updates (1+ updates))))
  (lsp--on-diagnostics
   workspace
   (lsp-make-publish-diagnostics-params
    :uri (lsp--path-to-uri payments)
    :diagnostics
    (vector
     (lsp-make-diagnostic
      :severity? lsp/diagnostic-severity-error
      :code? "E101"
      :message "Undefined payment gateway")
     (lsp-make-diagnostic
      :severity? lsp/diagnostic-severity-warning
      :code? "W204"
      :message "Retry budget is unused")
     (lsp-make-diagnostic
      :severity? lsp/diagnostic-severity-hint
      :message "Extract provider selection"))))
  (lsp--on-diagnostics
   workspace
   (lsp-make-publish-diagnostics-params
    :uri (lsp--path-to-uri ledger)
    :diagnostics
    (vector
     (lsp-make-diagnostic
      :severity? lsp/diagnostic-severity-information
      :code? "I301"
      :message "Ledger schema is current"))))
  (let ((published
         (list
          :project (neomacs-lsp-test-copy-stats (directory-file-name root))
          :services
          (neomacs-lsp-test-copy-stats
           (directory-file-name service-directory))
          :payments (neomacs-lsp-test-copy-stats payments)
          :ledger (neomacs-lsp-test-copy-stats ledger)
          :tracked-files
          (sort
           (mapcar (lambda (path) (file-relative-name path root))
                   (hash-table-keys (lsp--workspace-diagnostics workspace)))
           #'string<))))
    (lsp--on-diagnostics
     workspace
     (lsp-make-publish-diagnostics-params
      :uri (lsp--path-to-uri payments)
      :diagnostics
      (vector
       (lsp-make-diagnostic
        :severity? lsp/diagnostic-severity-error
        :code? "E101"
        :message "Undefined payment gateway"))))
    (let* ((payment-diagnostics
            (gethash payments (lsp--workspace-diagnostics workspace)))
           (replaced
           (list
             :project
             (neomacs-lsp-test-copy-stats (directory-file-name root))
             :services
             (neomacs-lsp-test-copy-stats
              (directory-file-name service-directory))
             :payments
             (mapcar
              (lambda (diagnostic)
                (list
                 (lsp:diagnostic-code? diagnostic)
                 (lsp:diagnostic-severity? diagnostic)
                 (lsp:diagnostic-message diagnostic)))
              payment-diagnostics)
             :ledger (neomacs-lsp-test-copy-stats ledger))))
      (lsp-diagnostics--workspace-cleanup workspace)
      (list
       :published published
       :replaced replaced
       :updates updates
       :cleanup
       (list
        :project
        (neomacs-lsp-test-copy-stats (directory-file-name root))
        :services
        (neomacs-lsp-test-copy-stats
         (directory-file-name service-directory))
        :tracked-count (hash-table-count
                        (lsp--workspace-diagnostics workspace)))))))
"##;
    let expected = expect![[
        r##"OK (:published (:project [0 1 1 1 1] :services [0 1 1 1 1] :payments [0 1 1 0 1] :ledger [0 0 0 1 0] :tracked-files ("services/ledger.el" "services/payments.el")) :replaced (:project [0 1 0 1 0] :services [0 1 0 1 0] :payments (("E101" 1 "Undefined payment gateway")) :ledger [0 0 0 1 0]) :updates 3 :cleanup (:project [0 0 0 0 0] :services [0 0 0 0 0] :tracked-count 0))"##
    ]];
    ParityBatchCase::value(
        "publishing_replacing_and_cleaning_workspace_diagnostics_updates_project_totals",
        elisp_form,
        expected,
    )
}

pub(super) fn diagnostics_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![publishing_replacing_and_cleaning_workspace_diagnostics_updates_project_totals()]
}
