use expect_test::expect;

use super::ParityBatchCase;

fn aggressive_fill_paragraph_real_space_command_reflows_and_undo_restores_prose() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aggressive_fill_paragraph_real_space_command_reflows_and_undo_restores_prose",
        r##"(with-temp-buffer
                           (text-mode)
                           (buffer-enable-undo)
                           (setq
                            fill-column
                            38)
                           (insert
                            "Release notes explain how parser recovery prevents data loss while preserving request order across every retry.")
                           (let ((original
                                  (buffer-string)))
                             (setq
                              buffer-undo-list
                              nil)
                             (aggressive-fill-paragraph-mode
                              1)
                             (undo-boundary)
                             (let ((last-command-event
                                    ?\s)
                                   (this-command
                                    'self-insert-command)
                                   (real-this-command
                                    'self-insert-command))
                               (self-insert-command
                                1))
                             (undo-boundary)
                             (let ((filled
                                    (buffer-string))
                                   (filled-point
                                    (point))
                                   (filled-line
                                    (line-number-at-pos))
                                   (filled-column
                                    (current-column))
                                   (filled-character
                                    (char-before)))
                               (setq
                                buffer-undo-list
                                (primitive-undo
                                 2
                                 buffer-undo-list))
                               (list
                                aggressive-fill-paragraph-mode
                                filled
                                filled-point
                                filled-line
                                filled-column
                                filled-character
                                (buffer-string)
                                (point)
                                (equal
                                 original
                                 (buffer-string))
                                buffer-undo-list))))"##,
        expect![[
            r#"OK (t "Release notes explain how parser\nrecovery prevents data loss while\npreserving request order across every\nretry. " 113 4 7 32 "Release notes explain how parser recovery prevents data loss while preserving request order across every retry." 112 t nil)"#
        ]],
    )
}

fn aggressive_fill_paragraph_real_commands_fill_comments_without_reformatting_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aggressive_fill_paragraph_real_commands_fill_comments_without_reformatting_code",
        r####"(with-temp-buffer
                            (emacs-lisp-mode)
                            (setq
                             fill-column
                             46)
                            (aggressive-fill-paragraph-mode
                             1)
                            (insert
                             "(defun publish-result (result)\n"
                             "  ;; Explain why the parser retries the request while preserving the original ordering guarantee.\n"
                             "  (message \"result=%S and this source form must remain on one line\" result))")
                            (goto-char
                             (point-min))
                            (forward-line
                             1)
                            (end-of-line)
                            (let ((last-command-event
                                   ?\s)
                                  (this-command
                                   'self-insert-command)
                                  (real-this-command
                                   'self-insert-command))
                              (self-insert-command
                               1))
                            (let ((comment-point
                                   (point))
                                  (comment-line
                                   (line-number-at-pos))
                                  (comment-column
                                   (current-column)))
                              (goto-char
                               (point-max))
                              (let ((last-command-event
                                     ?\s)
                                    (this-command
                                     'self-insert-command)
                                    (real-this-command
                                     'self-insert-command))
                                (self-insert-command
                                 1))
                              (list
                               (buffer-string)
                               (list
                                comment-point
                                comment-line
                                comment-column)
                               (list
                                (point)
                                (line-number-at-pos)
                                (current-column))
                               aggressive-fill-paragraph-mode)))"####,
        expect![[
            r#"OK ("(defun publish-result (result)\n  ;; Explain why the parser retries the\n  ;; request while preserving the original\n  ;; ordering guarantee. \n  (message \"result=%S and this source form must remain on one line\" result)) " (140 4 25) (218 5 77) t)"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aggressive_fill_paragraph_real_space_command_reflows_and_undo_restores_prose(),
        aggressive_fill_paragraph_real_commands_fill_comments_without_reformatting_code(),
    ]
}
