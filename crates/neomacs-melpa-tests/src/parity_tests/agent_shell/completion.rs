use expect_test::expect;

use super::ParityBatchCase;

fn live_session_completes_real_project_files_and_advertised_agent_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "live_session_completes_real_project_files_and_advertised_agent_commands",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "agent-shell-completion-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
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
              (sessionUpdate . "available_commands_update")
              (availableCommands
               . [((name . "review")
                   (description . "Review current changes"))
                  ((name . "resume")
                   (description . "Resume the previous session"))
                  ((name . "release")
                   (description . "Prepare a release candidate"))])))))
         '((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "completion-answer")
              (content
               (type . "text")
               (text . "Completion data is ready."))))))))
       (messages
        (neomacs-agent-shell-test-session-messages notifications))
       (agent-shell-cwd-function (lambda () root))
       (agent-shell-transcript-file-path-function (lambda () transcript))
       (agent-shell-show-welcome-message nil)
       (agent-shell-show-busy-indicator nil)
       (agent-shell-show-usage-at-turn-end nil)
       (shell nil)
       (snapshot nil))
  (make-directory (expand-file-name "src/parity" root) t)
  (make-directory (expand-file-name "docs" root) t)
  (with-temp-file (expand-file-name "README.md" root)
    (insert "# Completion fixture\n"))
  (with-temp-file (expand-file-name "src/lib.rs" root)
    (insert "pub fn answer() -> i32 { 42 }\n"))
  (with-temp-file (expand-file-name "src/parity/session.rs" root)
    (insert "pub fn compare() {}\n"))
  (with-temp-file (expand-file-name "docs/usage.md" root)
    (insert "# Usage\n"))
  (call-process "git" nil nil nil "init" "-q" root)
  (unwind-protect
      (progn
        (setq shell (neomacs-agent-shell-test-start messages))
        (with-current-buffer shell
          (shell-maker-submit :input "Load commands for this workspace.")
          (let ((commands (map-elt agent-shell--state :available-commands))
                (completion-enabled agent-shell-completion-mode)
                (cached-before agent-shell--project-files-cache))
            (with-temp-buffer
              (setq-local default-directory root)
              (setq-local agent-shell-completion--shell-buffer shell)
              (insert "@src/")
              (let* ((file-capf (agent-shell--file-completion-at-point))
                     (file-candidates (nth 2 file-capf))
                     (file-kind (plist-get (nthcdr 3 file-capf)
                                           :company-kind))
                     (cache-after-first
                      (buffer-local-value
                       'agent-shell--project-files-cache shell))
                     (second-file-capf
                      (agent-shell--file-completion-at-point)))
                (erase-buffer)
                (insert "/re")
                (let* ((command-capf
                        (agent-shell--command-completion-at-point))
                       (command-candidates (nth 2 command-capf))
                       (annotate
                        (plist-get (nthcdr 3 command-capf)
                                   :annotation-function)))
                  (setq
                   snapshot
                   (list
                    completion-enabled
                    commands
                    (seq-take file-capf 2)
                    file-candidates
                    (mapcar file-kind file-candidates)
                    (eq cache-after-first
                        (nth 2 second-file-capf))
                    cached-before
                    (seq-take command-capf 2)
                    command-candidates
                    (mapcar annotate command-candidates)
                    (plist-get (nthcdr 3 command-capf) :exclusive)
                    (try-completion "re" command-candidates)
                    (neomacs-agent-shell-test-visible-buffer-string)))))))))
    (neomacs-agent-shell-test-kill shell))
  snapshot)
"##,
        expect![[
            r#"OK (t [((name . "review") (description . "Review current changes")) ((name . "resume") (description . "Resume the previous session")) ((name . "release") (description . "Prepare a release candidate"))] (2 6) ("README.md" "conversation.md" "docs/usage.md" "src/lib.rs" "src/parity/session.rs") (file file file file file) t nil (2 4) ("review" "resume" "release") ("  Review current changes" "  Resume the previous session" "  Prepare a release candidate") t "re" "/re")"#
        ]],
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![live_session_completes_real_project_files_and_advertised_agent_commands()]
}
