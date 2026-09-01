use expect_test::expect;

use super::ParityBatchCase;

fn cli_wrappers_construct_complete_launch_specs_at_external_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "cli_wrappers_construct_complete_launch_specs_at_external_boundary",
        r##"
(progn
  (mapc
   #'require
   '(ai-code-gemini-cli ai-code-antigravity-cli
     ai-code-open-interpreter-cli ai-code-cursor-cli
     ai-code-codebuddy-cli ai-code-aider-cli ai-code-grok-cli
     ai-code-opencode ai-code-kilo ai-code-kiro-cli ai-code-pi))
  (setq ai-code-test-backend-events nil)
  (unwind-protect
      (cl-letf
          (((symbol-function 'ai-code-backends-infra--start-cli-session)
            (lambda (options arg)
              (push
               (list
                (plist-get options :program)
                (plist-get options :switches)
                (plist-get options :label)
                (plist-get options :session-prefix)
                (plist-get options :env-vars)
                (plist-get options :multiline-input-sequence)
                (and (plist-get options :escape-function) t)
                arg)
               ai-code-test-backend-events))))
        (let ((ai-code-gemini-cli-program-switches '("--model" "pro"))
              (ai-code-antigravity-cli-program-switches '("--sandbox"))
              (ai-code-open-interpreter-cli-program-switches '("--local"))
              (ai-code-cursor-cli-program-switches '("--mode" "ask"))
              (ai-code-codebuddy-cli-program-switches '("--verbose"))
              (ai-code-aider-cli-program-switches '("--no-auto-commits"))
              (ai-code-grok-cli-program-switches '("--model" "grok-4"))
              (ai-code-opencode-program-switches '("--port" "4096"))
              (ai-code-kilo-program-switches '("--agent" "review"))
              (ai-code-kiro-cli-program-switches '("--verbose"))
              (ai-code-kiro-cli-trust-all-tools t)
              (ai-code-kiro-cli-agent "architect")
              (ai-code-pi-program-switches '("--provider" "local")))
          (ai-code-gemini-cli nil)
          (ai-code-antigravity-cli '(4))
          (ai-code-open-interpreter-cli nil)
          (ai-code-cursor-cli nil)
          (ai-code-codebuddy-cli nil)
          (ai-code-aider-cli nil)
          (ai-code-grok-cli nil)
          (ai-code-opencode nil)
          (ai-code-kilo nil)
          (ai-code-kiro-cli nil)
          (ai-code-pi-start nil)
          (nreverse ai-code-test-backend-events)))
    (makunbound 'ai-code-test-backend-events)))
"##,
        expect![[
            r#"OK (("gemini" ("--model" "pro") "Gemini" "gemini" nil nil t nil) ("agy" ("--sandbox") "Antigravity" "antigravity" nil nil t (4)) ("interpreter" ("--local") "Open Interpreter" "open-interpreter" nil nil t nil) ("cursor-agent" ("--mode" "ask") "Cursor" "cursor" nil nil t nil) ("codebuddy" ("--verbose") "CodeBuddy" "codebuddy" nil nil t nil) ("aider" ("--no-auto-commits") "Aider" "aider" nil nil t nil) ("grok" ("--model" "grok-4") "Grok" "grok" nil nil nil nil) ("opencode" ("--port" "4096") "Opencode" "opencode" ("OTUI_USE_ALTERNATE_SCREEN=main-screen") nil nil nil) ("kilo" ("--agent" "review") "Kilo" "kilo" ("OTUI_USE_ALTERNATE_SCREEN=main-screen") nil nil nil) ("kiro-cli" ("chat" "--trust-all-tools" "--agent" "architect" "--verbose") "Kiro" "kiro" nil nil t nil) ("pi" ("--provider" "local") "Pi" "pi" nil "\33[13;2u" t nil))"#
        ]],
    )
}

fn mcp_capable_wrappers_forward_environment_multiline_and_prepare_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_capable_wrappers_forward_environment_multiline_and_prepare_hooks",
        r##"
(progn
  (mapc
   #'require
   '(ai-code-codex-cli ai-code-claude-code
     ai-code-github-copilot-cli))
  (setq ai-code-test-backend-events nil)
  (unwind-protect
      (cl-letf
          (((symbol-function 'ai-code-backends-infra--start-cli-session)
            (lambda (options arg)
              (push
               (list
                (plist-get options :program)
                (plist-get options :switches)
                (plist-get options :label)
                (plist-get options :session-prefix)
                (plist-get options :env-vars)
                (plist-get options :multiline-input-sequence)
                (functionp (plist-get options :prepare-launch))
                arg)
               ai-code-test-backend-events))))
        (let ((ai-code-codex-cli-program "/opt/bin/codex")
              (ai-code-codex-cli-program-switches '("--oss"))
              (ai-code-claude-code-program "/opt/bin/claude")
              (ai-code-claude-code-program-switches '("--model" "opus"))
              (ai-code-claude-code-no-flicker t)
              (ai-code-github-copilot-cli-program "/opt/bin/copilot")
              (ai-code-github-copilot-cli-program-switches '("--allow-all"))
              (ai-code-github-copilot-cli-extra-env-vars
               '("TERM_PROGRAM=vscode" "NO_COLOR=1")))
          (ai-code-codex-cli nil)
          (ai-code-claude-code '(4))
          (ai-code-github-copilot-cli nil)
          (nreverse ai-code-test-backend-events)))
    (makunbound 'ai-code-test-backend-events)))
"##,
        expect![[
            r#"OK (("/opt/bin/codex" ("--oss") "Codex" "codex" nil nil t nil) ("/opt/bin/claude" ("--model" "opus") "Claude Code" "claude" ("TERM_PROGRAM=emacs" "FORCE_CODE_TERMINAL=true" "CLAUDE_CODE_NO_FLICKER=1") "\33\15" t (4)) ("/opt/bin/copilot" ("--allow-all") "Copilot" "copilot" ("TERM_PROGRAM=vscode" "NO_COLOR=1") "\15\n" t nil))"#
        ]],
    )
}

fn resume_commands_append_backend_specific_flags_and_open_pickers_when_required() -> ParityBatchCase
{
    ParityBatchCase::value(
        "resume_commands_append_backend_specific_flags_and_open_pickers_when_required",
        r##"
(progn
  (mapc
   #'require
   '(ai-code-codex-cli ai-code-claude-code ai-code-gemini-cli
     ai-code-github-copilot-cli ai-code-opencode ai-code-kilo
     ai-code-grok-cli ai-code-kiro-cli))
  (setq ai-code-test-backend-events nil)
  (unwind-protect
      (cl-letf
          (((symbol-function 'ai-code-backends-infra--start-cli-session)
            (lambda (options arg)
              (push
               (list 'start
                     (plist-get options :session-prefix)
                     (plist-get options :switches)
                     arg)
               ai-code-test-backend-events)))
           ((symbol-function 'ai-code-backends-infra--cli-show-resume-picker)
            (lambda (prefix)
              (push (list 'picker prefix)
                    ai-code-test-backend-events))))
        (let ((ai-code-codex-cli-program-switches '("--profile" "work"))
              (ai-code-claude-code-program-switches '("--model" "sonnet"))
              (ai-code-gemini-cli-program-switches '("--model" "pro"))
              (ai-code-github-copilot-cli-program-switches '("--allow-all"))
              (ai-code-opencode-program-switches '("--port" "4096"))
              (ai-code-kilo-program-switches '("--agent" "review"))
              (ai-code-grok-cli-program-switches '("--model" "grok-4"))
              (ai-code-kiro-cli-program-switches '("--verbose")))
          (ai-code-codex-cli-resume nil)
          (ai-code-claude-code-resume '(4))
          (ai-code-gemini-cli-resume nil)
          (ai-code-github-copilot-cli-resume nil)
          (ai-code-opencode-resume nil)
          (ai-code-kilo-resume nil)
          (ai-code-grok-cli-resume nil)
          (ai-code-kiro-cli-resume nil)
          (nreverse ai-code-test-backend-events)))
    (makunbound 'ai-code-test-backend-events)))
"##,
        expect![[
            r#"OK ((start "codex" ("--profile" "work" "resume") nil) (start "claude" ("--model" "sonnet" "--resume") (4)) (start "gemini" ("--model" "pro" "--resume") nil) (start "copilot" ("--allow-all" "--resume") nil) (picker "copilot") (start "opencode" ("--port" "4096" "--continue") nil) (picker "opencode") (start "kilo" ("--agent" "review" "--continue") nil) (picker "kilo") (start "grok" ("--model" "grok-4" "resume") nil) (picker "grok") (start "kiro" ("chat" "--verbose" "--resume") nil))"#
        ]],
    )
}

fn backend_send_switch_and_escape_commands_route_labels_prefixes_and_payloads() -> ParityBatchCase {
    ParityBatchCase::value(
        "backend_send_switch_and_escape_commands_route_labels_prefixes_and_payloads",
        r##"
(progn
  (mapc
   #'require
   '(ai-code-codex-cli ai-code-claude-code ai-code-gemini-cli
     ai-code-github-copilot-cli ai-code-aider-cli ai-code-opencode
     ai-code-kilo ai-code-grok-cli))
  (setq ai-code-test-backend-events nil)
  (unwind-protect
      (cl-letf
          (((symbol-function 'ai-code-backends-infra--cli-send-command)
            (lambda (label prefix line)
              (push (list 'send label prefix line)
                    ai-code-test-backend-events)))
           ((symbol-function 'ai-code-backends-infra--cli-switch-to-buffer)
            (lambda (label prefix force)
              (push (list 'switch label prefix force)
                    ai-code-test-backend-events)))
           ((symbol-function 'ai-code-backends-infra--terminal-send-escape)
            (lambda ()
              (push '(escape) ai-code-test-backend-events))))
        (ai-code-codex-cli-send-command
         "Inspect the failing ledger test, then propose a minimal fix.")
        (ai-code-claude-code-send-command
         "Review the transaction boundary for duplicate writes.")
        (ai-code-gemini-cli-switch-to-buffer t)
        (ai-code-github-copilot-cli-switch-to-buffer nil)
        (ai-code-aider-cli-send-command "/add src/payment.rs")
        (ai-code-opencode-switch-to-buffer '(4))
        (ai-code-kilo-send-command "Run focused tests.")
        (ai-code-grok-cli-switch-to-buffer nil)
        (ai-code-codex-cli-send-escape)
        (nreverse ai-code-test-backend-events))
    (makunbound 'ai-code-test-backend-events)))
"##,
        expect![[
            r#"OK ((send "Codex" "codex" "Inspect the failing ledger test, then propose a minimal fix.") (send "Claude Code" "claude" "Review the transaction boundary for duplicate writes.") (switch "Gemini" "gemini" t) (switch "Copilot" "copilot" nil) (send "Aider" "aider" "/add src/payment.rs") (switch "Opencode" "opencode" (4)) (send "Kilo" "kilo" "Run focused tests.") (switch "Grok" "grok" nil) (escape))"#
        ]],
    )
}

fn kiro_command_builder_orders_chat_trust_agent_and_user_switches() -> ParityBatchCase {
    ParityBatchCase::value(
        "kiro_command_builder_orders_chat_trust_agent_and_user_switches",
        r##"
(progn
  (require 'ai-code-kiro-cli)
  (let ((ai-code-kiro-cli-program "/opt/kiro cli")
        (ai-code-kiro-cli-program-switches '("--verbose" "--profile" "work"))
        (ai-code-kiro-cli-trust-all-tools t)
        (ai-code-kiro-cli-agent "security-review"))
    (list
     (ai-code-kiro-cli--build-args)
     (ai-code-kiro-cli--build-command)
     (let ((ai-code-kiro-cli-trust-all-tools nil)
           (ai-code-kiro-cli-agent nil)
           (ai-code-kiro-cli-program-switches nil))
       (ai-code-kiro-cli--build-args)))))
"##,
        expect![[
            r#"OK (("chat" "--trust-all-tools" "--agent" "security-review" "--verbose" "--profile" "work") "/opt/kiro cli chat --trust-all-tools --agent security-review --verbose --profile work" ("chat"))"#
        ]],
    )
}

pub(super) fn backends_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        cli_wrappers_construct_complete_launch_specs_at_external_boundary(),
        mcp_capable_wrappers_forward_environment_multiline_and_prepare_hooks(),
        resume_commands_append_backend_specific_flags_and_open_pickers_when_required(),
        backend_send_switch_and_escape_commands_route_labels_prefixes_and_payloads(),
        kiro_command_builder_orders_chat_trust_agent_and_user_switches(),
    ]
}
