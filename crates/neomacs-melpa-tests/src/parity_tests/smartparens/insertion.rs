use expect_test::expect;

use super::ParityBatchCase;

fn typing_a_complete_elisp_command_autopairs_and_skips_every_closer() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (smartparens-mode 1)
    (execute-kbd-macro "(defun deploy (environment")
    (let ((signature (neomacs-smartparens-test-state :signature)))
      (execute-kbd-macro ") (message \"Deploy %s\" environment))")
      (list
       :signature signature
       :completed (neomacs-smartparens-test-state :completed)
       :at-end (eobp)
       :enclosing
       (save-excursion
         (goto-char (1+ (point-min)))
         (neomacs-smartparens-test-sexp-shape
          (sp-get-enclosing-sexp)))))))
"##;
    let expected = expect![[
        r####"OK (:signature (:label :signature :buffer "(defun deploy (environment))" :point 27 :mark nil :depth 2 :string nil :comment nil :balanced t) :completed (:label :completed :buffer "(defun deploy (environment) (message \"Deploy %s\" environment))" :point 63 :mark nil :depth 0 :string nil :comment nil :balanced t) :at-end t :enclosing (:beg 1 :end 63 :open "(" :close ")" :text "(defun deploy (environment) (message \"Deploy %s\" environment))"))"####
    ]];
    ParityBatchCase::value(
        "typing_a_complete_elisp_command_autopairs_and_skips_every_closer",
        elisp_form,
        expected,
    )
}

pub(super) fn insertion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![typing_a_complete_elisp_command_autopairs_and_skips_every_closer()]
}
