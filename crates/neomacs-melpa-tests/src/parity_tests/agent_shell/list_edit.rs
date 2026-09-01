use expect_test::expect;

use super::ParityBatchCase;

fn user_edits_nested_bullets_multi_digit_steps_and_breaks_out_for_notes() -> ParityBatchCase {
    ParityBatchCase::value(
        "user_edits_nested_bullets_multi_digit_steps_and_breaks_out_for_notes",
        r##"
(with-temp-buffer
  (insert
   "- prepare release\n"
   "  98. audit GNU Emacs\n"
   "  99. audit Neomacs\n"
   "- publish\n")
  (agent-shell-list-edit-mode 1)

  (goto-char (point-min))
  (end-of-line)
  (call-interactively (key-binding (kbd "RET")))
  (insert "run cargo nextest")
  (call-interactively (key-binding (kbd "TAB")))

  (search-forward "99. audit Neomacs")
  (end-of-line)
  (call-interactively (key-binding (kbd "RET")))
  (insert "record every divergence")
  (call-interactively (key-binding (kbd "RET")))
  (insert "attach minimized reproductions")
  (call-interactively (key-binding (kbd "RET")))
  (call-interactively (key-binding (kbd "RET")))
  (insert "Notes: failures remain actionable.")

  (search-forward "- publish")
  (end-of-line)
  (call-interactively (key-binding (kbd "RET")))
  (insert "tag candidate")
  (call-interactively (key-binding (kbd "TAB")))
  (call-interactively (key-binding (kbd "<backtab>")))

  (list
   (buffer-string)
   (line-number-at-pos)
   (current-column)
   (agent-shell-list-edit--at-item)
   agent-shell-list-edit-mode))
"##,
        expect![[
            r#"OK ("- prepare release\n  - run cargo nextest\n  98. audit GNU Emacs\n  99. audit Neomacs\n  100. record every divergence\n  101. attach minimized reproductions\n\nNotes: failures remain actionable.\n- publish\n- tag candidate\n" 10 15 ((:type . bullet) (:indent . "") (:marker . "-") (:content . "tag candidate")) t)"#
        ]],
    )
}

pub(super) fn list_edit_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![user_edits_nested_bullets_multi_digit_steps_and_breaks_out_for_notes()]
}
