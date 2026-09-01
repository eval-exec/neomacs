use expect_test::expect;

use super::ParityBatchCase;

fn multi_server_completion_and_resolve_results_preserve_actionable_metadata() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((primary
        (lsp-make-completion-item
         :label "deploy-preview"
         :kind? 3
         :detail? "workspace command"))
       (secondary
        (lsp-make-completion-item
         :label "deploy-production"
         :kind? 3
         :detail? "guarded command"))
       (completion
        (lsp--merge-results
         (list
          (vector primary)
          (lsp-make-completion-list
           :is-incomplete t
           :items (vector secondary)))
         "textDocument/completion"))
       (resolved
        (lsp--merge-results
         (list
          (lsp-make-completion-item
           :label "deploy-preview"
           :detail? "workspace"
           :documentation? "Open a deployment target"
           :additional-text-edits?
           (lsp-make-text-edit
            :range (neomacs-lsp-test-range 0 0 0 0)
            :new-text "(require 'deployment)\n"))
          (lsp-make-completion-item
           :label "deploy-preview"
           :detail? "command"
           :documentation?
           (lsp-make-markup-content
            :kind lsp/markup-kind-markdown
            :value "Runs against **preview**.")
           :additional-text-edits?
           (lsp-make-text-edit
            :range (neomacs-lsp-test-range 2 0 2 0)
            :new-text ";; audited\n")))
         "completionItem/resolve"))
       (documentation (lsp:completion-item-documentation? resolved))
       (resolved-edits (lsp:completion-item-additional-text-edits? resolved)))
  (list
   :completion
   (list
    :incomplete (lsp:completion-list-is-incomplete completion)
    :items
    (mapcar
     (lambda (item)
       (list (lsp:completion-item-label item)
             (lsp:completion-item-kind? item)
             (lsp:completion-item-detail? item)))
     (append (lsp:completion-list-items completion) nil)))
   :resolved
   (list
    :label (lsp:completion-item-label resolved)
    :detail (lsp:completion-item-detail? resolved)
    :documentation
    (list (lsp:markup-content-kind documentation)
          (lsp:markup-content-value documentation))
    :additional-edits
    (mapcar
     (lambda (edit)
       (list
        (lsp:text-edit-new-text edit)
        (neomacs-lsp-test-position-shape
         (lsp:range-start (lsp:text-edit-range edit)))))
     resolved-edits))))
"##;
    let expected = expect![[
        r##"OK (:completion (:incomplete t :items (("deploy-preview" 3 "workspace command") ("deploy-production" 3 "guarded command"))) :resolved (:label "deploy-preview" :detail "workspace command" :documentation ("markdown" "Open a deployment target\nRuns against **preview**.") :additional-edits (("(require 'deployment)\n" (0 0)) (";; audited\n" (2 0)))))"##
    ]];
    ParityBatchCase::value(
        "multi_server_completion_and_resolve_results_preserve_actionable_metadata",
        elisp_form,
        expected,
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![multi_server_completion_and_resolve_results_preserve_actionable_metadata()]
}
