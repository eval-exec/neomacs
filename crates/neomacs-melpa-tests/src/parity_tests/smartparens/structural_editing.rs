use expect_test::expect;

use super::ParityBatchCase;

fn refactoring_a_pipeline_slurps_barfs_and_transposes_whole_stages() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (smartparens-mode 1)
  (insert "(pipeline (fetch source)) (validate config) (publish result)")
  (goto-char (point-min))
  (search-forward "pipeline")
  (let (states)
    (push (neomacs-smartparens-test-state :initial) states)
    (sp-forward-slurp-sexp)
    (push (neomacs-smartparens-test-state :slurp-validate) states)
    (sp-forward-slurp-sexp)
    (push (neomacs-smartparens-test-state :slurp-publish) states)
    (sp-forward-barf-sexp)
    (push (neomacs-smartparens-test-state :barf-publish) states)
    (goto-char (point-min))
    (search-forward "(fetch source)")
    (sp-transpose-sexp)
    (push (neomacs-smartparens-test-state :transpose-stages) states)
    (nreverse states)))
"##;
    let expected = expect![[
        r####"OK ((:label :initial :buffer "(pipeline (fetch source)) (validate config) (publish result)" :point 10 :mark nil :depth 1 :string nil :comment nil :balanced t) (:label :slurp-validate :buffer "(pipeline (fetch source) (validate config)) (publish result)" :point 10 :mark nil :depth 1 :string nil :comment nil :balanced t) (:label :slurp-publish :buffer "(pipeline (fetch source) (validate config) (publish result))" :point 10 :mark nil :depth 1 :string nil :comment nil :balanced t) (:label :barf-publish :buffer "(pipeline (fetch source) (validate config)) (publish result)" :point 10 :mark nil :depth 1 :string nil :comment nil :balanced t) (:label :transpose-stages :buffer "(pipeline (validate config) (fetch source)) (publish result)" :point 43 :mark nil :depth 1 :string nil :comment nil :balanced t))"####
    ]];
    ParityBatchCase::value(
        "refactoring_a_pipeline_slurps_barfs_and_transposes_whole_stages",
        elisp_form,
        expected,
    )
}

fn splicing_a_nested_branch_preserves_its_comment_and_surrounding_pipeline() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (smartparens-mode 1)
  (insert
   "(pipeline\n"
   "  (when preview\n"
   "    ;; Keep the audit step with the deployment.\n"
   "    (audit preview)\n"
   "    (deploy preview))\n"
   "  (notify owner))")
  (goto-char (point-min))
  (search-forward "when preview")
  (let ((before (neomacs-smartparens-test-state :before-splice)))
    (sp-splice-sexp)
    (list
     :before before
     :after (neomacs-smartparens-test-state :after-splice)
     :comment-position
     (save-excursion
       (goto-char (point-min))
       (search-forward ";; Keep")
       (line-number-at-pos)))))
"##;
    let expected = expect![[
        r####"OK (:before (:label :before-splice :buffer "(pipeline\n  (when preview\n    ;; Keep the audit step with the deployment.\n    (audit preview)\n    (deploy preview))\n  (notify owner))" :point 26 :mark nil :depth 2 :string nil :comment nil :balanced t) :after (:label :after-splice :buffer "(pipeline\n when preview\n ;; Keep the audit step with the deployment.\n (audit preview)\n (deploy preview)\n  (notify owner))" :point 24 :mark nil :depth 1 :string nil :comment nil :balanced t) :comment-position 3)"####
    ]];
    ParityBatchCase::value(
        "splicing_a_nested_branch_preserves_its_comment_and_surrounding_pipeline",
        elisp_form,
        expected,
    )
}

pub(super) fn structural_editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        refactoring_a_pipeline_slurps_barfs_and_transposes_whole_stages(),
        splicing_a_nested_branch_preserves_its_comment_and_surrounding_pipeline(),
    ]
}
