use expect_test::expect;

use super::ParityBatchCase;

fn agent_recall_matches_a_real_claude_session_by_timestamp_and_message_then_backfills_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "agent_recall_matches_a_real_claude_session_by_timestamp_and_message_then_backfills_it",
        r####"(let* ((workspace-tmp
                                 (expand-file-name
                                  "tmp"
                                  (getenv
                                   "NEOMACS_TEST_WORKSPACE_ROOT")))
                                (root
                                 (make-temp-file
                                  (expand-file-name
                                   "agent-recall-match-"
                                   workspace-tmp)
                                  t))
                                (project
                                 (expand-file-name
                                  "projects/parser-service"
                                  root))
                                (transcript-dir
                                 (expand-file-name
                                  ".agent-shell/transcripts"
                                  project))
                                (transcript
                                 (expand-file-name
                                  "2026-07-10-17-07-00.md"
                                  transcript-dir))
                                (claude-root
                                 (expand-file-name
                                  "claude"
                                  root))
                                (mangled-project
                                 (replace-regexp-in-string
                                  "[/. _]"
                                  "-"
                                  (directory-file-name
                                   (expand-file-name project))))
                                (claude-dir
                                 (expand-file-name
                                  (concat
                                   "projects/"
                                   mangled-project)
                                  claude-root))
                                (wrong-session
                                 "11111111-1111-1111-1111-111111111111")
                                (matched-session
                                 "22222222-2222-2222-2222-222222222222")
                                (agent-recall-search-paths
                                 (list
                                  (expand-file-name
                                   "projects"
                                   root)))
                                (agent-recall-claude-config-dir
                                 claude-root)
                                (agent-recall-index-file
                                 (expand-file-name
                                  "state/index.el"
                                  root))
                                (agent-recall-session-match-window
                                 120)
                                (agent-recall--index nil)
                                (agent-recall--index-loaded-p nil)
                                (agent-recall--session-id-cache
                                 (make-hash-table
                                  :test
                                  'equal)))
                           (make-directory
                            transcript-dir
                            t)
                           (make-directory
                            claude-dir
                            t)
                           (with-temp-file transcript
                             (insert
                              "**Started:** 2026-07-10 17:07:00\n"
                              "**Agent:** Claude Code\n"
                              "**Working Directory:** "
                              project
                              "\n\n"
                              "---\n\n"
                              "## User\n"
                              "> Please   explain the failing parser thoroughly.\n\n"
                              "## Agent\n"
                              "I will reproduce the state transition.\n"))
                           (with-temp-file
                               (expand-file-name
                                "sessions-index.json"
                                claude-dir)
                             (insert
                              "{\"entries\":["
                              "{\"sessionId\":\""
                              wrong-session
                              "\",\"created\":\"2026-07-10T17:07:05Z\"},"
                              "{\"sessionId\":\""
                              matched-session
                              "\",\"created\":\"2026-07-10T17:07:45Z\"}"
                              "]}"))
                           (with-temp-file
                               (expand-file-name
                                (concat
                                 wrong-session
                                 ".jsonl")
                                claude-dir)
                             (insert
                              "{\"type\":\"user\","
                              "\"timestamp\":\"2026-07-10T17:07:05Z\","
                              "\"message\":{\"content\":\"Investigate an unrelated timeout.\"}}\n"))
                           (with-temp-file
                               (expand-file-name
                                (concat
                                 matched-session
                                 ".jsonl")
                                claude-dir)
                             (insert
                              "{\"type\":\"user\","
                              "\"timestamp\":\"2026-07-10T17:07:45Z\","
                              "\"message\":{\"content\":["
                              "{\"type\":\"text\","
                              "\"text\":\"Please explain the failing parser thoroughly.\"}"
                              "]}}\n"))
                           (unwind-protect
                               (progn
                                 (agent-recall-reindex)
                                 (let ((indexed-session
                                        (plist-get
                                         (gethash
                                          transcript
                                          agent-recall--index)
                                         :session-id)))
                                   (agent-recall-backfill t)
                                   (let* ((write-report
                                           (replace-regexp-in-string
                                            (regexp-quote root)
                                            "[ROOT]"
                                            (with-current-buffer
                                                "*agent-recall-backfill*"
                                              (buffer-substring-no-properties
                                               (point-min)
                                               (point-max)))
                                            t
                                            t))
                                          (log-file
                                           (expand-file-name
                                            "state/backfill-log.el"
                                            root))
                                          (log
                                           (replace-regexp-in-string
                                            "^;; Written:.*$"
                                            ";; Written: <timestamp>"
                                            (replace-regexp-in-string
                                             (regexp-quote root)
                                             "[ROOT]"
                                             (with-temp-buffer
                                               (insert-file-contents
                                                log-file)
                                               (buffer-string))
                                             t
                                             t))))
                                     (agent-recall-backfill nil)
                                     (list
                                      indexed-session
                                      (agent-recall--read-embedded-session-id
                                       transcript)
                                      (replace-regexp-in-string
                                       (regexp-quote root)
                                       "[ROOT]"
                                       (with-temp-buffer
                                         (insert-file-contents transcript)
                                         (buffer-string))
                                       t
                                       t)
                                      write-report
                                      log
                                      (with-current-buffer
                                          "*agent-recall-backfill*"
                                        (buffer-substring-no-properties
                                         (point-min)
                                         (point-max)))))))
                             (when-let ((buffer
                                         (get-buffer
                                          "*agent-recall-backfill*")))
                               (kill-buffer buffer))
                             (delete-directory
                              root
                              t)))"####,
        expect![[
            r#"OK ("22222222-2222-2222-2222-222222222222" "22222222-2222-2222-2222-222222222222" "**Started:** 2026-07-10 17:07:00\n**Agent:** Claude Code\n**Working Directory:** [ROOT]/projects/parser-service\n\n**Session:** 22222222-2222-2222-2222-222222222222\n\n---\n\n## User\n> Please   explain the failing parser thoroughly.\n\n## Agent\nI will reproduce the state transition.\n" "Agent Recall -- Backfill (WRITING)\n══════════════════════════════════════════════════\n\n  MATCH:    [parser-service] 2026-07-10-17-07-00.md → 22222222\n\n──────────────────────────────────────────────────\nSummary:\n  Total:      1\n  Matched:    1\n  Skipped:    0 (already have session ID)\n  No match:   0\n\n  Wrote session IDs to 1 files.\n  Undo log: [ROOT]/state/backfill-log.el\n" ";; agent-recall backfill undo log\n;; Written: <timestamp>\n;; Files modified: 1\n\n;; To undo, evaluate this buffer (removes **Session:** lines):\n(dolist (file '(\n  \"[ROOT]/projects/parser-service/.agent-shell/transcripts/2026-07-10-17-07-00.md\"\n))\n  (when (file-exists-p file)\n    (with-temp-buffer\n      (insert-file-contents file)\n      (goto-char (point-min))\n      (when (re-search-forward \"^\\\\*\\\\*Session:\\\\*\\\\*.*\\n\\n?\" nil t)\n        (replace-match \"\")\n        (write-region (point-min) (point-max) file nil 'no-message)))))\n" "Agent Recall -- Backfill (DRY RUN)\n══════════════════════════════════════════════════\n\n  SKIP:     [parser-service] 2026-07-10-17-07-00.md (has 22222222)\n\n──────────────────────────────────────────────────\nSummary:\n  Total:      1\n  Matched:    0\n  Skipped:    1 (already have session ID)\n  No match:   0\n\n  To write, run: C-u C-u M-x agent-recall-backfill\n")"#
        ]],
    )
}

pub(super) fn matching_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agent_recall_matches_a_real_claude_session_by_timestamp_and_message_then_backfills_it()]
}
