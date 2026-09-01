use expect_test::expect;

use super::ParityBatchCase;

fn git_commit_buffer_message_removes_comments_scissors_and_normalizes_blank_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_buffer_message_removes_comments_scissors_and_normalizes_blank_edges",
        r##"(list
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "\n\nSummary\n\nBody\n# status\nignored\n")
                 (git-commit-buffer-message))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "Summary\n\nBody\n# ------------------------ >8 ------------------------\ndiff --git a/a b/a\n")
                 (git-commit-buffer-message))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "Summary without newline")
                 (git-commit-buffer-message)))"##,
        expect![[
            r#"OK ("\nSummary\n\nBody\nignored\n" "Summary\n\nBody\n" "Summary without newline\n")"#
        ]],
    )
}

fn git_commit_buffer_message_rejects_whitespace_and_comment_only_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_buffer_message_rejects_whitespace_and_comment_only_buffers",
        r##"(list
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert " \t\n\r\n")
                 (git-commit-buffer-message))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "# first\n# second\n")
                 (git-commit-buffer-message))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "\n# comment\n \t\n")
                 (git-commit-buffer-message)))"##,
        expect![[r#"OK (nil nil nil)"#]],
    )
}

fn git_commit_ensure_comment_gap_only_expands_an_initial_empty_comment_boundary() -> ParityBatchCase
{
    ParityBatchCase::value(
        "git_commit_ensure_comment_gap_only_expands_an_initial_empty_comment_boundary",
        r##"(mapcar
               (lambda (text)
                 (with-temp-buffer
                   (setq-local comment-start "#")
                   (insert text)
                   (git-commit-ensure-comment-gap)
                   (buffer-string)))
               '("\n# instructions\n"
                 "\n\n# instructions\n"
                 "summary\n# instructions\n"
                 "# instructions\n"))"##,
        expect![[
            r##"OK ("\n\n# instructions\n" "\n\n# instructions\n" "summary\n# instructions\n" "# instructions\n")"##
        ]],
    )
}

fn git_commit_save_message_deduplicates_and_moves_the_latest_message_to_the_front()
-> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_save_message_deduplicates_and_moves_the_latest_message_to_the_front",
        r##"(with-temp-buffer
               (setq-local comment-start "#")
               (setq-local log-edit-comment-ring (make-ring 4))
               (let ((git-commit-use-local-message-ring nil))
                 (dolist (text '("one\n" "two\n" "one\n" "# only\n"))
                   (erase-buffer)
                   (insert text)
                   (git-commit-save-message))
                 (list
                  (ring-length log-edit-comment-ring)
                  (ring-elements log-edit-comment-ring))))"##,
        expect![[r#"OK (2 ("one\n" "two\n"))"#]],
    )
}

fn git_commit_previous_and_next_message_preserve_instruction_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_previous_and_next_message_preserve_instruction_comments",
        r##"(with-temp-buffer
               (setq-local comment-start "#")
               (setq-local log-edit-comment-ring (make-ring 5))
               (setq-local log-edit-comment-ring-index nil)
               (ring-insert log-edit-comment-ring "older\n")
               (ring-insert log-edit-comment-ring "newer\n")
               (insert "draft\n# instructions\n")
               (git-commit-prev-message 1)
               (let ((previous (buffer-string))
                     (previous-index log-edit-comment-ring-index))
                 (git-commit-next-message 1)
                 (list
                  previous
                  previous-index
                  (buffer-string)
                  log-edit-comment-ring-index
                  (ring-elements log-edit-comment-ring))))"##,
        expect![[
            r##"OK ("newer\n\n# instructions\n" 1 "draft\n\n# instructions\n" 0 ("draft\n" "newer\n" "older\n"))"##
        ]],
    )
}

fn git_commit_summary_regexp_captures_limit_overflow_and_nonempty_second_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_summary_regexp_captures_limit_overflow_and_nonempty_second_line",
        r##"(let ((comment-start "#")
                    (git-commit-need-summary-line t)
                    (git-commit-summary-max-length 5))
               (mapcar
                (lambda (text)
                  (save-match-data
                    (string-match (git-commit-summary-regexp) text)
                    (list
                     (match-string 1 text)
                     (match-string 2 text)
                     (match-string 3 text))))
                '("# comment\nabcdef\nbody\n"
                  "\nshort\n\nbody\n"
                  "\n# comment\n\n\n")))"##,
        expect![[r#"OK (("abcde" "f" "body") ("short" "" nil) ("" "" nil))"#]],
    )
}

fn git_commit_style_checks_short_circuit_prompts_and_honor_force() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_style_checks_short_circuit_prompts_and_honor_force",
        r##"(let ((comment-start "#")
                    (git-commit-need-summary-line t)
                    (git-commit-summary-max-length 5)
                    (git-commit-style-convention-checks
                     '(overlong-summary-line non-empty-second-line)))
               (cl-labels
                   ((check
                     (text answers force)
                     (with-temp-buffer
                       (insert text)
                       (let (prompts)
                         (cl-letf
                             (((symbol-function 'y-or-n-p)
                               (lambda (prompt)
                                 (push prompt prompts)
                                 (pop answers))))
                           (list
                            (git-commit-check-style-conventions force)
                            (nreverse prompts)))))))
                 (list
                  (check "short\n\nbody\n" nil nil)
                  (check "abcdef\nbody\n" '(nil t) nil)
                  (check "abcdef\nbody\n" '(t nil) nil)
                  (check "abcdef\nbody\n" nil t))))"##,
        expect![[
            r#"OK ((t nil) (nil ("Summary line is too long.  Commit anyway? ")) (nil ("Summary line is too long.  Commit anyway? " "Second line is not empty.  Commit anyway? ")) (t nil))"#
        ]],
    )
}

fn git_commit_cancel_message_reports_whether_the_message_was_saved() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_cancel_message_reports_whether_the_message_was_saved",
        r##"(let (messages)
               (cl-letf (((symbol-function 'message)
                          (lambda (format-string &rest arguments)
                            (let ((text
                                   (apply #'format format-string arguments)))
                              (push text messages)
                              text))))
                 (let ((with-editor-pre-cancel-hook nil))
                   (git-commit-cancel-message))
                 (let ((with-editor-pre-cancel-hook
                        '(git-commit-save-message)))
                   (git-commit-cancel-message))
                 (nreverse messages)))"##,
        expect![[
            r#"OK ("Commit canceled" "Commit canceled.  Message saved to `log-edit-comment-ring'")"#
        ]],
    )
}

pub(super) fn messages_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        git_commit_buffer_message_removes_comments_scissors_and_normalizes_blank_edges(),
        git_commit_buffer_message_rejects_whitespace_and_comment_only_buffers(),
        git_commit_ensure_comment_gap_only_expands_an_initial_empty_comment_boundary(),
        git_commit_save_message_deduplicates_and_moves_the_latest_message_to_the_front(),
        git_commit_previous_and_next_message_preserve_instruction_comments(),
        git_commit_summary_regexp_captures_limit_overflow_and_nonempty_second_line(),
        git_commit_style_checks_short_circuit_prompts_and_honor_force(),
        git_commit_cancel_message_reports_whether_the_message_was_saved(),
    ]
}
