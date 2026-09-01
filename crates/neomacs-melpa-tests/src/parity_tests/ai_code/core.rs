use expect_test::expect;

use super::ParityBatchCase;

fn package_loads_exact_version_and_complete_core_feature_graph() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_loads_exact_version_and_complete_core_feature_graph",
        r##"
(list
 (bound-and-true-p package-version)
 (mapcar
  (lambda (feature) (and (featurep feature) t))
  '(ai-code ai-code-backends ai-code-backends-infra ai-code-session
    ai-code-input ai-code-task ai-code-prompt-mode ai-code-send
    ai-code-agile ai-code-git ai-code-github ai-code-change
    ai-code-discussion ai-code-file ai-code-doc ai-code-harness
    ai-code-ai ai-code-mcp-server ai-code-notifications
    ai-code-onboarding))
 (length ai-code-backends)
 (ai-code-current-backend-label)
 (commandp 'ai-code-menu))
"##,
        expect![[r#"OK (nil (t t t t t t t t t t t t t t t t t t t t) 19 "Claude Code" t)"#]],
    )
}

fn backend_registry_exposes_complete_actionable_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "backend_registry_exposes_complete_actionable_contracts",
        r##"
(mapcar
 (lambda (entry)
   (let ((key (car entry))
         (spec (cdr entry)))
     (list key
           (plist-get spec :label)
           (plist-get spec :require)
           (plist-get spec :start)
           (plist-get spec :switch)
           (plist-get spec :send)
           (plist-get spec :resume)
           (plist-get spec :cli)
           (plist-get spec :agent-file))))
 ai-code-backends)
"##,
        expect![[
            r#"OK ((claude-code "Claude Code" ai-code-claude-code ai-code-claude-code ai-code-claude-code-switch-to-buffer ai-code-claude-code-send-command ai-code-claude-code-resume "claude" "CLAUDE.md") (gemini "Gemini CLI" ai-code-gemini-cli ai-code-gemini-cli ai-code-gemini-cli-switch-to-buffer ai-code-gemini-cli-send-command ai-code-gemini-cli-resume "gemini" "GEMINI.md") (antigravity "Antigravity CLI" ai-code-antigravity-cli ai-code-antigravity-cli ai-code-antigravity-cli-switch-to-buffer ai-code-antigravity-cli-send-command ai-code-antigravity-cli-resume "agy" "AGENTS.md") (github-copilot-cli "GitHub Copilot CLI" ai-code-github-copilot-cli ai-code-github-copilot-cli ai-code-github-copilot-cli-switch-to-buffer ai-code-github-copilot-cli-send-command ai-code-github-copilot-cli-resume "copilot" nil) (codex "OpenAI Codex CLI" ai-code-codex-cli ai-code-codex-cli ai-code-codex-cli-switch-to-buffer ai-code-codex-cli-send-command ai-code-codex-cli-resume "codex" "AGENTS.md") (pi "Pi" ai-code-pi ai-code-pi-start ai-code-pi-switch-to-buffer ai-code-pi-send-command ai-code-pi-resume "pi" "AGENTS.md") (open-interpreter "Open Interpreter CLI" ai-code-open-interpreter-cli ai-code-open-interpreter-cli ai-code-open-interpreter-cli-switch-to-buffer ai-code-open-interpreter-cli-send-command ai-code-open-interpreter-cli-resume "interpreter" "AGENTS.md") (opencode "Opencode" ai-code-opencode ai-code-opencode ai-code-opencode-switch-to-buffer ai-code-opencode-send-command ai-code-opencode-resume "opencode" nil) (kilo "Kilo" ai-code-kilo ai-code-kilo ai-code-kilo-switch-to-buffer ai-code-kilo-send-command ai-code-kilo-resume "kilo" nil) (grok "Grok CLI" ai-code-grok-cli ai-code-grok-cli ai-code-grok-cli-switch-to-buffer ai-code-grok-cli-send-command ai-code-grok-cli-resume "grok" nil) (cursor "Cursor CLI" ai-code-cursor-cli ai-code-cursor-cli ai-code-cursor-cli-switch-to-buffer ai-code-cursor-cli-send-command ai-code-cursor-cli-resume "cursor-agent" nil) (kiro "Kiro CLI" ai-code-kiro-cli ai-code-kiro-cli ai-code-kiro-cli-switch-to-buffer ai-code-kiro-cli-send-command ai-code-kiro-cli-resume "kiro-cli" nil) (codebuddy "CodeBuddy Code" ai-code-codebuddy-cli ai-code-codebuddy-cli ai-code-codebuddy-cli-switch-to-buffer ai-code-codebuddy-cli-send-command ai-code-codebuddy-cli-resume "codebuddy" nil) (aider "Aider CLI" ai-code-aider-cli ai-code-aider-cli ai-code-aider-cli-switch-to-buffer ai-code-aider-cli-send-command nil "aider" nil) (eca "ECA (Editor Code Assistant)" ai-code-eca ai-code-eca-start ai-code-eca-switch ai-code-eca-send ai-code-eca-resume nil "AGENTS.md") (agent-shell "agent-shell" ai-code-agent-shell ai-code-agent-shell ai-code-agent-shell-switch-to-buffer ai-code-agent-shell-send-command ai-code-agent-shell-resume "agent-shell" nil) (gptel-agent "GPTel Agent" ai-code-gptel-agent ai-code-gptel-agent ai-code-gptel-agent-switch-to-buffer ai-code-gptel-agent-send-command nil nil nil) (claude-code-ide "claude-code-ide.el" claude-code-ide claude-code-ide--start-if-no-session claude-code-ide-switch-to-buffer claude-code-ide-send-prompt claude-code-ide-resume "claude" "CLAUDE.md") (claude-code-el "claude-code.el" claude-code claude-code claude-code-switch-to-buffer ai-code-claude-code-el-send-command claude-code-resume "claude" "CLAUDE.md"))"#
        ]],
    )
}

fn repository_backend_affinity_is_normalized_isolated_and_replaceable() -> ParityBatchCase {
    ParityBatchCase::value(
        "repository_backend_affinity_is_normalized_isolated_and_replaceable",
        r##"
(let* ((root (make-temp-file "ai-code-affinity-" t))
       (nested (expand-file-name "src/" root))
       (ai-code--repo-backend-alist nil)
       (ai-code-selected-backend 'claude-code))
  (unwind-protect
      (progn
        (make-directory nested t)
        (let ((normalized (ai-code--normalize-git-root root)))
          (ai-code--remember-repo-backend normalized 'codex)
          (ai-code--remember-repo-backend normalized 'gemini)
          (cl-letf (((symbol-function 'ai-code--current-git-root)
                     (lambda () normalized)))
            (list
             (file-name-absolute-p normalized)
             (= 1 (length ai-code--repo-backend-alist))
             (ai-code--repo-backend-for-root normalized)
             (ai-code--effective-backend)
             (ai-code-current-backend-label)))
          ))
    (delete-directory root t)))
"##,
        expect![[r#"OK (t t gemini gemini "Gemini CLI")"#]],
    )
}

fn selecting_backend_rebinds_all_dispatch_points_and_cli_identity() -> ParityBatchCase {
    ParityBatchCase::value(
        "selecting_backend_rebinds_all_dispatch_points_and_cli_identity",
        r##"
(let ((ai-code-selected-backend 'claude-code)
      (ai-code--repo-backend-alist nil))
  (cl-letf (((symbol-function 'ai-code--current-git-root) (lambda () nil)))
    (ai-code-set-backend 'codex)
    (let ((codex-state
           (list ai-code-selected-backend ai-code-cli
                 (eq ai-code--cli-start-fn 'ai-code-codex-cli)
                 (functionp ai-code--cli-resume-fn)
                 (eq ai-code--cli-switch-fn
                     'ai-code-codex-cli-switch-to-buffer)
                 (eq ai-code--cli-send-fn
                     'ai-code-codex-cli-send-command))))
      (ai-code-set-backend 'aider)
      (list codex-state
            (list ai-code-selected-backend ai-code-cli
                  (eq ai-code--cli-start-fn 'ai-code-aider-cli)
                  (eq ai-code--cli-resume-fn
                      'ai-code--unsupported-resume)
                  (eq ai-code--cli-switch-fn
                      'ai-code-aider-cli-switch-to-buffer)
                  (eq ai-code--cli-send-fn
                      'ai-code-aider-cli-send-command))
            (condition-case err
                (ai-code-set-backend 'missing-backend)
              (user-error (error-message-string err)))))))
"##,
        expect![[
            r#"OK ((codex "codex" t t t t) (aider "aider" t t t t) "Unknown backend: missing-backend")"#
        ]],
    )
}

fn terminal_output_cleanup_preserves_content_and_rejects_control_noise() -> ParityBatchCase {
    ParityBatchCase::value(
        "terminal_output_cleanup_preserves_content_and_rejects_control_noise",
        r##"
(let ((enter (concat "\e[?1049h" "dashboard"))
      (leave (concat "done" "\e[?1049l"))
      (clear (concat "before" "\e[3J" "after")))
  (list
   (ai-code-backends-infra--strip-alternate-screen-transitions enter)
   (ai-code-backends-infra--strip-alternate-screen-transitions leave)
   (ai-code-backends-infra--strip-scrollback-clear clear)
   (mapcar #'ai-code-backends-infra--output-meaningful-p
           (list "" "\r\n" "\e[2K\r" "\e[31m\e[0m" "assistant: fixed it\n"
                 "\e[?25lprogress 42%\e[?25h"))))
"##,
        expect![[r#"OK ("dashboard" "done" "beforeafter" (nil nil nil nil 0 0))"#]],
    )
}

fn safe_terminal_paste_routes_single_and_multiline_input_correctly() -> ParityBatchCase {
    ParityBatchCase::value(
        "safe_terminal_paste_routes_single_and_multiline_input_correctly",
        r##"
(let (events)
  (let ((send (lambda (text) (push (list 'send text) events)))
        (paste (lambda (text) (push (list 'paste text) events))))
    (ai-code-backends-infra--send-string-with-paste
     "status" nil send paste (lambda () t) "demo")
    (ai-code-backends-infra--send-string-with-paste
     "line one\nline two" t send paste (lambda () t) "demo")
    (list
     (nreverse events)
     (condition-case err
         (ai-code-backends-infra--send-string-with-paste
          "unsafe\npaste" t send paste (lambda () nil) "demo")
       (user-error (error-message-string err))))))
"##,
        expect![[
            r#"OK (((send "status") (paste "line one\nline two")) "This demo session cannot paste multiline input without submitting")"#
        ]],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_loads_exact_version_and_complete_core_feature_graph(),
        backend_registry_exposes_complete_actionable_contracts(),
        repository_backend_affinity_is_normalized_isolated_and_replaceable(),
        selecting_backend_rebinds_all_dispatch_points_and_cli_identity(),
        terminal_output_cleanup_preserves_content_and_rejects_control_noise(),
        safe_terminal_paste_routes_single_and_multiline_input_correctly(),
    ]
}
