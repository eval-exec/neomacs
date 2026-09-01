use expect_test::expect;

use super::ParityBatchCase;

fn agent_recall_browses_opens_navigates_and_cleans_a_real_transcript() -> ParityBatchCase {
    ParityBatchCase::value(
        "agent_recall_browses_opens_navigates_and_cleans_a_real_transcript",
        r####"(let* ((root
                                 (expand-file-name
                                  "interaction"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (project
                                 (expand-file-name
                                  "interaction-project"
                                  root))
                                (transcript-dir
                                 (expand-file-name
                                  ".agent-shell/transcripts"
                                  project))
                                (transcript
                                 (expand-file-name
                                  "2026-07-12-08-15-00.md"
                                  transcript-dir))
                                (session-id
                                 "33333333-3333-3333-3333-333333333333")
                                (agent-recall-search-paths
                                 (list root))
                                (agent-recall-index-file
                                 (expand-file-name
                                  "state/index.el"
                                  root))
                                (agent-recall-browse-preview nil)
                                (agent-recall-auto-transcript-mode t)
                                (agent-recall--index nil)
                                (agent-recall--index-loaded-p nil)
                                (agent-recall--session-id-cache
                                 (make-hash-table
                                  :test
                                  'equal))
                                selected-display
                                transcript-buffer
                                clean-buffer)
                           (make-directory
                            transcript-dir
                            t)
                           (with-temp-file transcript
                             (insert
                              "**Started:** 2026-07-12 08:15:00\n"
                              "**Agent:** Claude Code\n"
                              "**Working Directory:** "
                              project
                              "\n"
                              "**Session:** "
                              session-id
                              "\n\n"
                              "---\n\n"
                              "## User Message\n"
                              "> Diagnose the parser state leak.\n\n"
                              "## Agent Response\n"
                              "The lookahead token survives recovery.\n\n"
                              "### Tool Call\n"
                              "Internal trace output that should be removed.\n\n"
                              "## User Message\n"
                              "> Apply the smallest safe correction.\n\n"
                              "## Agent Response\n"
                              "Reset lookahead after the recovery branch.\n"))
                           (unwind-protect
                               (progn
                                 (agent-recall-reindex)
                                 (setq selected-display
                                       (caar
                                        (agent-recall--list-transcripts)))
                                 (let* ((completing-read-function
                                         (lambda (&rest _arguments)
                                           (caar
                                            (agent-recall--list-transcripts)))))
                                   (agent-recall-browse))
                                 (setq transcript-buffer
                                       (current-buffer))
                                 (with-current-buffer transcript-buffer
                                   (goto-char
                                    (point-min))
                                   (agent-recall-next-user-message)
                                   (let ((first-user-line
                                          (line-number-at-pos)))
                                     (agent-recall-next-user-message)
                                     (let ((second-user-line
                                            (line-number-at-pos)))
                                       (agent-recall-prev-user-message)
                                       (let ((previous-user-line
                                              (line-number-at-pos))
                                             (mode-state
                                              (list
                                               (file-name-nondirectory
                                                buffer-file-name)
                                               agent-recall-transcript-mode
                                               buffer-read-only
                                               agent-recall--transcript-session-id
                                               (substring-no-properties
                                                (agent-recall--header-line
                                                 agent-recall--transcript-session-id)))))
                                         (agent-recall-clean-view)
                                         (setq clean-buffer
                                               (current-buffer))
                                         (list
                                          selected-display
                                          mode-state
                                          (list
                                           first-user-line
                                           second-user-line
                                           previous-user-line)
                                          (with-current-buffer clean-buffer
                                            (list
                                             (file-name-nondirectory
                                              buffer-file-name)
                                             (file-exists-p
                                              buffer-file-name)
                                             (buffer-substring-no-properties
                                              (point-min)
                                              (point-max))))))))))
                             (dolist (buffer
                                      (list
                                       clean-buffer
                                       transcript-buffer))
                               (when
                                   (buffer-live-p buffer)
                                 (with-current-buffer buffer
                                   (set-buffer-modified-p nil))
                                 (kill-buffer buffer)))
                             (delete-directory
                              root
                              t)))"####,
        expect![[
            r#"OK ("[interaction-project] 2026-07-12-08-15-00" ("2026-07-12-08-15-00.md" t t "33333333-3333-3333-3333-333333333333" "  r Resume (33333333)  c Clean  b Browse  C-j/C-k Navigate  q Quit") (8 17 8) ("2026-07-12-08-15-00-clean.md" t "**Started:** 2026-07-12 08:15:00\n**Agent:** Claude Code\n**Working Directory:** [ORACLE-SANDBOX]/interaction/interaction-project\n**Session:** 33333333-3333-3333-3333-333333333333\n\n---\n\n## ## User Message\n> Diagnose the parser state leak.\n\n## Agent Response\nThe lookahead token survives recovery.\n\n## User Message\n> Apply the smallest safe correction.\n\n## Agent Response\nReset lookahead after the recovery branch.\n"))"#
        ]],
    )
}

pub(super) fn interaction_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agent_recall_browses_opens_navigates_and_cleans_a_real_transcript()]
}
