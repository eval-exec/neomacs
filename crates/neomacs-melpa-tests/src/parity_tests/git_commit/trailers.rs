use expect_test::expect;

use super::ParityBatchCase;

fn git_commit_public_trailer_commands_insert_exact_labels_and_identity_format() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_public_trailer_commands_insert_exact_labels_and_identity_format",
        r##"(with-temp-buffer
               (setq-local comment-start "#")
               (insert "Summary\n\nBody\n")
               (git-commit-ack "Ack" "ack@example.test")
               (git-commit-modified "Mod" "mod@example.test")
               (git-commit-review "Rev" "rev@example.test")
               (git-commit-signoff "Sign" "sign@example.test")
               (git-commit-test "Test" "test@example.test")
               (git-commit-cc "Cc" "cc@example.test")
               (git-commit-reported "Report" "report@example.test")
               (git-commit-suggested "Suggest" "suggest@example.test")
               (git-commit-co-authored "Author" "author@example.test")
               (git-commit-co-developed "Dev" "dev@example.test")
               (buffer-string))"##,
        expect![[
            r#"OK "Summary\n\nBody\n\nAcked-by: Ack <ack@example.test>\nModified-by: Mod <mod@example.test>\nReviewed-by: Rev <rev@example.test>\nSigned-off-by: Sign <sign@example.test>\nTested-by: Test <test@example.test>\nCc: Cc <cc@example.test>\nReported-by: Report <report@example.test>\nSuggested-by: Suggest <suggest@example.test>\nCo-authored-by: Author <author@example.test>\nCo-developed-by: Dev <dev@example.test>\n\n""#
        ]],
    )
}

fn git_commit_trailers_stay_above_comments_and_verbose_diff() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_trailers_stay_above_comments_and_verbose_diff",
        r##"(list
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "Summary\n\n# status\n")
                 (git-commit-signoff "A" "a@example.test")
                 (buffer-string))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "Summary\n\nBody\n# ---------------- >8 ----------------\ndiff --git a/a b/a\n")
                 (git-commit-signoff "A" "a@example.test")
                 (buffer-string))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert "# instructions\n")
                 (git-commit-signoff "A" "a@example.test")
                 (buffer-string)))"##,
        expect![[
            r##"OK ("Summary\n\nSigned-off-by: A <a@example.test>\n\n# status\n" "Summary\n\nBody\n\nSigned-off-by: A <a@example.test>\n\n# ---------------- >8 ----------------\ndiff --git a/a b/a\n" "\n\nSigned-off-by: A <a@example.test>\n\n# instructions\n")"##
        ]],
    )
}

fn git_commit_trailers_append_to_recognized_blocks_without_reordering_existing_lines()
-> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_trailers_append_to_recognized_blocks_without_reordering_existing_lines",
        r##"(let ((git-commit-trailers
                    '("Signed-off-by" "Reviewed-by")))
               (with-temp-buffer
                 (setq-local comment-start "#")
                 (insert
                  "Summary\n\nBody\n\n"
                  "Signed-off-by: First <first@example.test>\n"
                  "Custom-field: untouched\n"
                  "# status\n")
                 (git-commit-review "Second" "second@example.test")
                 (git-commit-signoff "Third" "third@example.test")
                 (buffer-string)))"##,
        expect![[
            r##"OK "Summary\n\nBody\n\nSigned-off-by: First <first@example.test>\nReviewed-by: Second <second@example.test>\nSigned-off-by: Third <third@example.test>\n\nCustom-field: untouched\n# status\n""##
        ]],
    )
}

fn git_commit_get_ident_obeys_author_committer_email_and_user_fallback_precedence()
-> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_get_ident_obeys_author_committer_email_and_user_fallback_precedence",
        r##"(let ((process-environment
                    (copy-sequence process-environment))
                   (user-full-name "Fallback User"))
               (dolist (name
                        '("GIT_AUTHOR_NAME" "GIT_COMMITTER_NAME"
                          "GIT_AUTHOR_EMAIL" "GIT_COMMITTER_EMAIL"
                          "EMAIL"))
                 (setenv name nil))
               (cl-letf (((symbol-function 'magit-get)
                          (lambda (&rest _keys) nil))
                         ((symbol-function 'read-string)
                          (lambda (prompt)
                            (if (string-prefix-p "Name" prompt)
                                "Prompt Name"
                              "prompt@example.test"))))
                 (let ((fallback (git-commit-get-ident)))
                   (setenv "EMAIL" "email@example.test")
                   (setenv "GIT_COMMITTER_NAME" "Committer")
                   (let ((committer (git-commit-get-ident)))
                     (setenv "GIT_AUTHOR_NAME" "Author")
                     (setenv "GIT_AUTHOR_EMAIL" "author@example.test")
                     (list
                      fallback
                      committer
                      (git-commit-get-ident))))))"##,
        expect![[
            r#"OK (("Fallback User" "prompt@example.test") ("Committer" "email@example.test") ("Author" "author@example.test"))"#
        ]],
    )
}

fn git_commit_read_ident_trims_valid_input_and_preserves_match_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_commit_read_ident_trims_valid_input_and_preserves_match_data",
        r##"(let ((git-commit-read-ident-history nil))
               (string-match "keep-\\([0-9]+\\)" "keep-42")
               (let ((before (match-data)))
                 (cl-letf (((symbol-function 'magit-completing-read)
                            (lambda (&rest _arguments)
                              "Example Person   < person@example.test >")))
                   (list
                    (git-commit-read-ident "Reviewed-by")
                    (equal before (match-data))
                    git-commit-read-ident-history))))"##,
        expect![[r#"OK (("Example Person" "person@example.test") t nil)"#]],
    )
}

fn git_commit_read_ident_rejects_text_without_a_name_email_pair() -> ParityBatchCase {
    ParityBatchCase::signal(
        "git_commit_read_ident_rejects_text_without_a_name_email_pair",
        r##"(cl-letf (((symbol-function 'magit-completing-read)
                          (lambda (&rest _arguments)
                            "not an identity")))
               (git-commit-read-ident "Reviewed-by"))"##,
        expect![[r#"ERR (user-error "Invalid input")"#]],
    )
}

pub(super) fn trailers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        git_commit_public_trailer_commands_insert_exact_labels_and_identity_format(),
        git_commit_trailers_stay_above_comments_and_verbose_diff(),
        git_commit_trailers_append_to_recognized_blocks_without_reordering_existing_lines(),
        git_commit_get_ident_obeys_author_committer_email_and_user_fallback_precedence(),
        git_commit_read_ident_trims_valid_input_and_preserves_match_data(),
        git_commit_read_ident_rejects_text_without_a_name_email_pair(),
    ]
}
