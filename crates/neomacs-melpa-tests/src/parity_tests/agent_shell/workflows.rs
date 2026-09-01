use expect_test::expect;

use super::ParityBatchCase;

fn real_acp_session_boots_submits_markdown_and_cleans_up_its_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_acp_session_boots_submits_markdown_and_cleans_up_its_process",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "agent-shell-real-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (transcript (expand-file-name "conversation.md" root))
       (notifications
        '(((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "answer-1")
              (content
               (type . "text")
               (text
                . "I reviewed **both paths**. Run `cargo nextest` before merging."))))))))
       (messages
        (neomacs-agent-shell-test-session-messages notifications))
       (agent-shell-cwd-function (lambda () root))
       (agent-shell-transcript-file-path-function
        (lambda () transcript))
       (agent-shell-show-welcome-message nil)
       (agent-shell-show-busy-indicator nil)
       (agent-shell-show-usage-at-turn-end nil)
       (shell nil)
       (client nil)
       (turn-events nil)
       (snapshot nil))
  (make-directory (expand-file-name ".git" root) t)
  (unwind-protect
      (progn
        (setq shell (neomacs-agent-shell-test-start messages)
              client neomacs-agent-shell-test-last-client)
        (with-current-buffer shell
          (agent-shell-subscribe-to
           :shell-buffer shell
           :event 'turn-complete
           :on-event
           (lambda (event)
             (push event turn-events)))
          (shell-maker-submit
           :input
           "Review **src/lib.rs** and explain the safe merge path.")
          (setq
           snapshot
           (list
            (buffer-name)
            major-mode
            agent-shell-completion-mode
            (map-nested-elt agent-shell--state '(:session :id))
            (map-nested-elt agent-shell--state '(:session :title))
            (mapcar
             (lambda (request)
               (list
                (map-elt request :method)
                (map-nested-elt request '(:params sessionId))
                (map-nested-elt request '(:params prompt))))
             (nreverse neomacs-agent-shell-test-sent-requests))
            (nreverse turn-events)
            (map-elt agent-shell--state :active-requests)
            (map-elt client :pending-requests)
            (neomacs-agent-shell-test-visible-buffer-string)
            (with-temp-buffer
              (insert-file-contents transcript)
              (neomacs-agent-shell-test-normalize-transcript
               (buffer-string)))
            (and (process-live-p (map-elt client :process)) t)))))
    (neomacs-agent-shell-test-kill shell))
  (list
   snapshot
   (buffer-live-p shell)
   (and (process-live-p (map-elt client :process)) t)
   (file-exists-p transcript)))
"##,
        expect![[
            r##"OK (("Parity Agent @ agent-shell-real-session" agent-shell-mode t "parity-session" "Review **src/lib.rs** and explain the safe merge path." (("initialize" nil nil) ("session/new" nil nil) ("session/prompt" "parity-session" [((type . "text") (text . "Review **src/lib.rs** and explain the safe merge path."))])) (((:data (:stop-reason . "end_turn") (:usage (:total-tokens . 0) (:input-tokens . 0) (:output-tokens . 0) (:thought-tokens . 0) (:cached-read-tokens . 0) (:cached-write-tokens . 0) (:context-used . 0) (:context-size . 0) (:cost-amount . 0.0) (:cost-currency))) (:event . turn-complete))) nil nil "\n\n\n▶ [✓] Starting agent\n\n▶ Agent capabilities\n\n▶ Available config options\n\n▶ Available models\n\n  Available /commands\n\nParity> Review **src/lib.rs** and explain the safe merge path.\n\nI reviewed both paths. Run cargo nextest before merging.\n\nParity>" "# Agent Shell Transcript\n\n**Agent:** Parity\n**Started:** TIME\n**Working Directory:** [ORACLE-SANDBOX]/agent-shell-real-session/\n**Session ID:** parity-session\n\n---\n\n## User (TIME)\n\nReview **src/lib.rs** and explain the safe merge path.\n\n\n## Agent (TIME)\n\nI reviewed **both paths**. Run `cargo nextest` before merging.\n\n" t) nil nil t)"##
        ]],
    )
    .fresh_process()
}

fn real_provider_turn_renders_thought_tool_diff_and_streamed_markdown() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_provider_turn_renders_thought_tool_diff_and_streamed_markdown",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "agent-shell-real-tool-turn"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "src/lib.rs" root))
       (transcript (expand-file-name "conversation.md" root))
       (notifications
        (list
         '((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_thought_chunk")
              (content
               (type . "text")
               (text
                . "I will inspect the implementation, edit it, and verify the result."))))))
         `((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "tool_call")
              (toolCallId . "edit-1")
              (status . "in_progress")
              (title . "Update src/lib.rs")
              (kind . "edit")
              (rawInput
               (description . "Change the return value and add a test"))
              (locations . [((path . ,source) (line . 2))])
              (content . [])))))
         `((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "tool_call_update")
              (toolCallId . "edit-1")
              (status . "completed")
              (title . "Updated src/lib.rs")
              (kind . "edit")
              (locations . [((path . ,source) (line . 2))])
              (content
               . [((type . "diff")
                   (oldText
                    . "fn value() -> i32 {\n    1\n}\n")
                   (newText
                    . "fn value() -> i32 {\n    2\n}\n\n#[test]\nfn value_is_two() {\n    assert_eq!(value(), 2);\n}\n")
                   (path . ,source))
                  ((type . "content")
                   (content
                    (type . "text")
                    (text
                     . "Changed the return value and added a regression test.")))])))))
         '((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "answer-2")
              (content
               (type . "text")
               (text
                . "Implemented the change.\n\n| Check | Result |\n|---|---|\n| nextest | **passed** |\n\n```rust\nassert_eq!(value(), 2);\n```"))))))
         '((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "answer-2")
              (content
               (type . "text")
               (text . "\nNo unrelated files changed."))))))))
       (messages
        (neomacs-agent-shell-test-session-messages notifications))
       (agent-shell-cwd-function (lambda () root))
       (agent-shell-transcript-file-path-function
        (lambda () transcript))
       (agent-shell-show-welcome-message nil)
       (agent-shell-show-busy-indicator nil)
       (agent-shell-show-usage-at-turn-end nil)
       (agent-shell-thought-process-expand-by-default t)
       (agent-shell-tool-use-expand-by-default t)
       (agent-shell-activity-group-expand-by-default t)
       (shell nil)
       (tool-events nil)
       (snapshot nil))
  (make-directory (file-name-directory source) t)
  (make-directory (expand-file-name ".git" root) t)
  (with-temp-file source
    (insert "fn value() -> i32 {\n    1\n}\n"))
  (unwind-protect
      (progn
        (setq shell (neomacs-agent-shell-test-start messages))
        (with-current-buffer shell
          (agent-shell-subscribe-to
           :shell-buffer shell
           :event 'tool-call-update
           :on-event
           (lambda (event)
             (push (map-elt event :data) tool-events)))
          (shell-maker-submit
           :input
           "Update `value`, add a regression test, and summarize the diff.")
          (let ((rendered
                 (neomacs-agent-shell-test-visible-buffer-string)))
            (setq
             snapshot
             (list
              rendered
              (nreverse tool-events)
              (map-elt agent-shell--state :last-entry-type)
              (map-elt agent-shell--state :activity-group-count)
              (map-elt agent-shell--state :activity-thoughts)
              (map-elt agent-shell--state :chunked-group-count)
              (with-temp-buffer
                (insert-file-contents transcript)
                (neomacs-agent-shell-test-normalize-transcript
                 (buffer-string))))))))
    (neomacs-agent-shell-test-kill shell))
  snapshot)
"##,
        expect![[
            r##"OK ("\n\n\n▶ [✓] Starting agent\n\n▶ Agent capabilities\n\n▶ Available config options\n\n▶ Available models\n\n  Available /commands\n\nParity> Update `value`, add a regression test, and summarize the diff.\n\n▼ Thought, edited a file\n\n▼ 💡 Thinking\n\nI will inspect the implementation, edit it, and verify the result.\n\n▼ [✓][edit] Updated src/lib.rs Change the return value and add a test +6 -1\n\nChanged the return value and added a regression test.\n\n\n\n╭────────────╮\n│ src/lib.rs │\n╰────────────╯\n\n@@ -1,3 +1,8 @@\n fn value() -> i32 {\n-    1\n+    2\n+}\n+\n+#[test]\n+fn value_is_two() {\n+    assert_eq!(value(), 2);\n }\n\nImplemented the change.\n\n│ Check   │ Result │\n├─────────┼────────┤\n│ nextest │ passed │\n\n\nrust ⧉\n\nassert_eq!(value(), 2);\n\n\nNo unrelated files changed.\n\nParity>" (((:tool-call-id . "edit-1") (:tool-call (:title . "Update src/lib.rs") (:status . "in_progress") (:kind . "edit") (:description . "Change the return value and add a test") (:content . []) (:raw-input . #1=((description . "Change the return value and add a test"))))) ((:tool-call-id . "edit-1") (:tool-call (:title . "Updated src/lib.rs") (:status . "completed") (:kind . "edit") (:description . "Change the return value and add a test") (:content . [((type . "diff") (oldText . "fn value() -> i32 {\n    1\n}\n") (newText . "fn value() -> i32 {\n    2\n}\n\n#[test]\nfn value_is_two() {\n    assert_eq!(value(), 2);\n}\n") (path . "[ORACLE-SANDBOX]/agent-shell-real-tool-turn/src/lib.rs")) ((type . "content") (content (type . "text") (text . "Changed the return value and added a regression test.")))]) (:raw-input . #1#) (:group-id . "activity-1") (:locations . [((path . "[ORACLE-SANDBOX]/agent-shell-real-tool-turn/src/lib.rs") (line . 2))]) (:diffs ((:old . "fn value() -> i32 {\n    1\n}\n") (:new . "fn value() -> i32 {\n    2\n}\n\n#[test]\nfn value_is_two() {\n    assert_eq!(value(), 2);\n}\n") (:file . "[ORACLE-SANDBOX]/agent-shell-real-tool-turn/src/lib.rs") (:line . 2)))))) "agent_message_chunk" 1 (("activity-1" . 1)) 2 "# Agent Shell Transcript\n\n**Agent:** Parity\n**Started:** TIME\n**Working Directory:** [ORACLE-SANDBOX]/agent-shell-real-tool-turn/\n**Session ID:** parity-session\n\n---\n\n## User (TIME)\n\nUpdate `value`, add a regression test, and summarize the diff.\n\n## Agent's Thoughts (TIME)\n\nI will inspect the implementation, edit it, and verify the result.\n\n### Tool Call [completed]: Updated src/lib.rs\n\n**Tool:** edit\n**Timestamp:** TIME\n**Description:** Change the return value and add a test\n\n```\nChanged the return value and added a regression test.\n\n\n\n╭────────────╮\n│ src/lib.rs │\n╰────────────╯\n\n@@ -1,3 +1,8 @@\n fn value() -> i32 {\n-    1\n+    2\n+}\n+\n+#[test]\n+fn value_is_two() {\n+    assert_eq!(value(), 2);\n }\n```\n\n## Agent (TIME)\n\nImplemented the change.\n\n| Check | Result |\n|---|---|\n| nextest | **passed** |\n\n```rust\nassert_eq!(value(), 2);\n```\nNo unrelated files changed.\n\n")"##
        ]],
    )
    .fresh_process()
}

fn real_prompt_and_usage_notifications_accumulate_into_visible_session_totals() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_prompt_and_usage_notifications_accumulate_into_visible_session_totals",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "agent-shell-real-usage-turn"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (transcript (expand-file-name "conversation.md" root))
       (notifications
        '(((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "usage_update")
              (used . 62500)
              (size . 200000)
              (cost
               (amount . 1.375)
               (currency . "USD"))))))
          ((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "usage-answer")
              (content
               (type . "text")
               (text
                . "The review consumed cached context but stayed inside budget."))))))))
       (messages
        (neomacs-agent-shell-test-session-messages
         notifications
         '((stopReason . "end_turn")
           (usage
            (totalTokens . 15342)
            (inputTokens . 12000)
            (outputTokens . 2410)
            (thoughtTokens . 932)
            (cachedReadTokens . 8000)
            (cachedWriteTokens . 400)))))
       (agent-shell-cwd-function (lambda () root))
       (agent-shell-transcript-file-path-function
        (lambda () transcript))
       (agent-shell-show-welcome-message nil)
       (agent-shell-show-busy-indicator nil)
       (agent-shell-show-usage-at-turn-end t)
       (shell nil)
       (snapshot nil))
  (make-directory (expand-file-name ".git" root) t)
  (unwind-protect
      (progn
        (setq shell (neomacs-agent-shell-test-start messages))
        (with-current-buffer shell
          (shell-maker-submit
           :input
           "Audit this change and report token and cost usage.")
          (setq
           snapshot
           (list
            (map-elt agent-shell--state :usage)
            (neomacs-agent-shell-test-visible-buffer-string)
            (with-temp-buffer
              (insert-file-contents transcript)
              (neomacs-agent-shell-test-normalize-transcript
               (buffer-string)))))))
    (neomacs-agent-shell-test-kill shell))
  snapshot)
"##,
        expect![[
            r##"OK (((:total-tokens . 15342) (:input-tokens . 12000) (:output-tokens . 2410) (:thought-tokens . 932) (:cached-read-tokens . 8000) (:cached-write-tokens . 400) (:context-used . 62500) (:context-size . 200000) (:cost-amount . 1.375) (:cost-currency . "USD")) "\n\n\n▶ [✓] Starting agent\n\n▶ Agent capabilities\n\n▶ Available config options\n\n▶ Available models\n\n  Available /commands\n\nParity> Audit this change and report token and cost usage.\n\nThe review consumed cached context but stayed inside budget.\n\n▶ Usage\n\nParity>" "# Agent Shell Transcript\n\n**Agent:** Parity\n**Started:** TIME\n**Working Directory:** [ORACLE-SANDBOX]/agent-shell-real-usage-turn/\n**Session ID:** parity-session\n\n---\n\n## User (TIME)\n\nAudit this change and report token and cost usage.\n\n\n## Agent (TIME)\n\nThe review consumed cached context but stayed inside budget.\n\n")"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_acp_session_boots_submits_markdown_and_cleans_up_its_process(),
        real_provider_turn_renders_thought_tool_diff_and_streamed_markdown(),
        real_prompt_and_usage_notifications_accumulate_into_visible_session_totals(),
    ]
}
