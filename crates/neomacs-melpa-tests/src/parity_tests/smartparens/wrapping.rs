use expect_test::expect;

use super::ParityBatchCase;

fn selected_scalar_evolves_into_a_vector_and_then_a_list_wrapped_vector() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (smartparens-mode 1)
  (insert "(deploy :environment \"preview\")")
  (goto-char (point-min))
  (search-forward "\"preview\"")
  (set-mark (match-beginning 0))
  (goto-char (match-end 0))
  (activate-mark)
  (let (states)
    (push (neomacs-smartparens-test-state :selected-scalar) states)
    (sp-wrap-with-pair "(")
    (push (neomacs-smartparens-test-state :list-scalar) states)
    (goto-char (point-min))
    (search-forward "(\"preview\")")
    (goto-char (1+ (match-beginning 0)))
    (sp-rewrap-sexp '("[" . "]"))
    (push (neomacs-smartparens-test-state :vector-scalar) states)
    (goto-char (point-min))
    (search-forward "[\"preview\"]")
    (goto-char (1+ (match-beginning 0)))
    (sp-rewrap-sexp '("(" . ")") t)
    (push (neomacs-smartparens-test-state :list-wrapped-vector) states)
    (nreverse states)))
"##;
    let expected = expect![[
        r####"OK ((:label :selected-scalar :buffer "(deploy :environment \"preview\")" :point 31 :mark 22 :depth 1 :string nil :comment nil :balanced t) (:label :list-scalar :buffer "(deploy :environment (\"preview\"))" :point 23 :mark 22 :depth 2 :string nil :comment nil :balanced t) (:label :vector-scalar :buffer "(deploy :environment [\"preview\"])" :point 23 :mark 22 :depth 2 :string nil :comment nil :balanced t) (:label :list-wrapped-vector :buffer "(deploy :environment ([\"preview\"]))" :point 24 :mark 22 :depth 3 :string nil :comment nil :balanced t))"####
    ]];
    ParityBatchCase::value(
        "selected_scalar_evolves_into_a_vector_and_then_a_list_wrapped_vector",
        elisp_form,
        expected,
    )
}

pub(super) fn wrapping_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![selected_scalar_evolves_into_a_vector_and_then_a_list_wrapped_vector()]
}
