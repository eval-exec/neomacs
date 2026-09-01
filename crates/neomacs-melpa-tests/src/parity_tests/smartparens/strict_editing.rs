use expect_test::expect;

use super::ParityBatchCase;

fn strict_editing_kills_payloads_and_crosses_delimiters_without_unbalancing_code() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (let ((kill-ring nil)
          (kill-ring-yank-pointer nil))
      (emacs-lisp-mode)
      (smartparens-strict-mode 1)
      (insert "(when ready\n  (deploy config)\n  (notify owner))")
      (goto-char (point-min))
      (search-forward "(notify")
      (goto-char (match-beginning 0))
      (let (states)
        (push (neomacs-smartparens-test-state :before-crossing) states)
        (execute-kbd-macro (kbd "C-d"))
        (push (neomacs-smartparens-test-state :inside-notify) states)
        (search-forward "owner")
        (sp-backward-kill-word 1)
        (push (neomacs-smartparens-test-state :owner-removed) states)
        (goto-char (point-min))
        (search-forward "deploy ")
        (execute-kbd-macro (kbd "C-k"))
        (push (neomacs-smartparens-test-state :after-kill) states)
        (list :states (nreverse states)
              :kill-ring kill-ring
              :strict smartparens-strict-mode)))))
"##;
    let expected = expect![[
        r####"OK (:states ((:label :before-crossing :buffer "(when ready\n  (deploy config)\n  (notify owner))" :point 33 :mark nil :depth 1 :string nil :comment nil :balanced t) (:label :inside-notify :buffer "(when ready\n  (deploy config)\n  (notify owner))" :point 34 :mark nil :depth 2 :string nil :comment nil :balanced t) (:label :owner-removed :buffer "(when ready\n  (deploy config)\n  (notify ))" :point 41 :mark nil :depth 2 :string nil :comment nil :balanced t) (:label :after-kill :buffer "(when ready\n  (deploy )\n  (notify ))" :point 23 :mark nil :depth 2 :string nil :comment nil :balanced t)) :kill-ring ("ownerconfig") :strict t)"####
    ]];
    ParityBatchCase::value(
        "strict_editing_kills_payloads_and_crosses_delimiters_without_unbalancing_code",
        elisp_form,
        expected,
    )
}

pub(super) fn strict_editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![strict_editing_kills_payloads_and_crosses_delimiters_without_unbalancing_code()]
}
