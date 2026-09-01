use expect_test::expect;

use super::ParityBatchCase;

fn prompt_suffix_pipeline_preserves_provider_order_and_memoizes_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "prompt_suffix_pipeline_preserves_provider_order_and_memoizes_nil",
        r##"
(progn
  (setq ai-code-test-provider-calls 0)
  (unwind-protect
      (let ((ai-code-prompt-suffix-functions
             (list
              (lambda (context)
                (ai-code-prompt-context-memoize
                 context 'first
                 (lambda ()
                   (cl-incf ai-code-test-provider-calls)
                   "Verify focused tests.")))
              (lambda (_context) nil)
              (lambda (_context) "Report changed files."))))
        (let* ((context (ai-code--prompt-context-for-text
                         "Implement bounded retries."))
               (first (ai-code--collect-prompt-suffixes context))
               (second (ai-code--collect-prompt-suffixes context)))
          (list first second ai-code-test-provider-calls
                (ai-code--apply-prompt-suffixes
                 "Implement bounded retries.")
                ai-code-test-provider-calls)))
    (makunbound 'ai-code-test-provider-calls)))
"##,
        expect![[
            r#"OK (("Verify focused tests." "Report changed files.") ("Verify focused tests." "Report changed files.") 1 "Implement bounded retries.\nVerify focused tests.\nReport changed files." 2)"#
        ]],
    )
}

fn prompt_command_detection_accepts_only_true_single_token_cli_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "prompt_command_detection_accepts_only_true_single_token_cli_commands",
        r##"
(mapcar
 (lambda (text) (list text (and (ai-code--direct-command-p text) t)))
 '("/help" "/resume" "/model opus" " /help" "/help\nnext"
   "Explain /help" "/" "/compact"))
"##,
        expect![[
            r#"OK (("/help" t) ("/resume" t) ("/model opus" nil) (" /help" nil) ("/help\nnext" t) ("Explain /help" nil) ("/" t) ("/compact" t))"#
        ]],
    )
}

fn structured_change_and_question_briefs_preserve_scope_and_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "structured_change_and_question_briefs_preserve_scope_and_boundaries",
        r##"
(list
 (ai-code--compose-code-change-brief
  :goal "Make payment retries idempotent."
  :scope "src/payments/retry.rs::schedule_retry"
  :context "Duplicate delivery occurs after a worker restart."
  :clipboard-context "trace-id=abc; attempts=2"
  :boundaries "Preserve the public API and database schema."
  :code-change-note "Add focused property tests before editing production code.")
 (ai-code--compose-question-brief
  :goal "Explain why the retry can execute twice."
  :scope "schedule_retry and its transaction boundary"
  :context "The worker acknowledges after committing."
  :instruction "Trace the state transitions and identify the race window."))
"##,
        expect![[
            r#"OK ("Goal:\nMake payment retries idempotent.\n\nScope:\nsrc/payments/retry.rs::schedule_retry\n\nContext:\nDuplicate delivery occurs after a worker restart.\n\nClipboard context:\ntrace-id=abc; attempts=2\n\nBoundaries:\nPreserve the public API and database schema.\n\nAgent responsibilities:\nInspect the relevant files before editing. Plan briefly, then edit the code. Run appropriate project verification for the change. Fix failures caused by the change.\n\nVerification evidence:\nReport the exact verification command(s), result, and any remaining risk or blocker.\n\nInstruction:\nAdd focused property tests before editing production code." "Goal:\nExplain why the retry can execute twice.\n\nScope:\nschedule_retry and its transaction boundary\n\nContext:\nThe worker acknowledges after committing.\n\nBoundaries:\nAnswer the question only. Do not make code changes.\n\nInstruction:\nTrace the state transitions and identify the race window.")"#
        ]],
    )
}

fn task_helpers_generate_safe_names_initial_content_and_search_handoff() -> ParityBatchCase {
    ParityBatchCase::value(
        "task_helpers_generate_safe_names_initial_content_and_search_handoff",
        r##"
(let ((timestamp "2026-07-27T12:00:00-0400"))
  (cl-letf (((symbol-function 'format-time-string)
             (lambda (&rest _args) timestamp)))
    (list
     (ai-code--extract-radar-id
      "Fix sync race https://radar.apple.com/12345678")
     (ai-code--normalize-radar-text
      "rdar://problem/12345678 - Sync race")
     (ai-code--generate-task-filename
      "Fix OAuth refresh / retry race")
     (ai-code--initialize-task-file-content
      "Fix OAuth refresh race"
      "https://github.com/acme/app/issues/42")
     (ai-code--build-task-search-prompt
      "/workspace/tasks/"
      "OAuth incidents involving refresh-token reuse"))))
"##,
        expect![[
            r#"OK (nil "rdar://problem/12345678 - Sync race" "task_2026-07-27T12:00:00-0400_fix_oauth_refresh_retry_race.org" nil "Search the content of all .org files recursively under directory: /workspace/tasks/\nSearch target description: OAuth incidents involving refresh-token reuse\nFocus on matching content inside the files, not just file names.\nReturn the relevant file paths, matched excerpts, and a concise summary.")"#
        ]],
    )
}

fn input_symbol_pipeline_flattens_nested_imenu_and_extracts_real_languages() -> ParityBatchCase {
    ParityBatchCase::value(
        "input_symbol_pipeline_flattens_nested_imenu_and_extracts_real_languages",
        r##"
(let* ((index
        '(("Classes"
           ("PaymentService" . 10)
           ("Methods"
            ("charge" . 30)
            ("*" . 40)))
          ("retry_payment" . 80)
          ("Variables" ("MAX_RETRIES" . 100))))
       (flattened (ai-code--flatten-imenu-index index)))
  (list
   flattened
   (mapcar
    #'ai-code--extract-symbol-from-line
    '("def retry_payment(order_id):"
      "class PaymentService:"
      "async function fetchLedger(id) {"
      "export function settleInvoice() {"
      "let total = 0;"))))
"##,
        expect![[
            r#"OK (("retry_payment" "PaymentService" "*" "charge" "MAX_RETRIES") ("retry_payment" "PaymentService" "fetchLedger" nil nil))"#
        ]],
    )
}

fn prompt_path_preprocessing_relativizes_repo_files_and_keeps_external_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "prompt_path_preprocessing_relativizes_repo_files_and_keeps_external_paths",
        r##"
(let* ((root (make-temp-file "ai-code-prompt-paths-" t))
       (outside (make-temp-file "ai-code-external-" nil ".md"))
       (source (expand-file-name "src/payment_retry.el" root)))
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (with-temp-file source (insert "(provide 'payment-retry)"))
        (cl-letf (((symbol-function 'ai-code--git-root)
                   (lambda (&optional _dir) root)))
          (list
           (let ((processed
                  (ai-code--preprocess-prompt-text
                   (format "Compare @%s with @%s and keep spacing."
                           source outside))))
             (string-replace
              outside "$OUTSIDE"
              (string-replace root "$ROOT/" processed)))
           (ai-code--candidate-path source (file-truename root))
           (equal
            (ai-code--candidate-path outside (file-truename root))
            outside))))
    (delete-directory root t)
    (delete-file outside)))
"##,
        expect![[
            r#"OK ("Compare @$ROOT//src/payment_retry.el with @$OUTSIDE and keep spacing." "@src/payment_retry.el" t)"#
        ]],
    )
}

fn git_and_github_prompt_builders_create_review_ready_handoffs() -> ParityBatchCase {
    ParityBatchCase::value(
        "git_and_github_prompt_builders_create_review_ready_handoffs",
        r##"
(list
 (ai-code--normalize-branch-name "  refs/heads/feature/oauth-race  ")
 (ai-code--default-pr-target-branch "feature/oauth-race")
 (ai-code--build-log-prompt
  "payment-service"
  "Find the commit that introduced duplicate settlement.")
 (ai-code--build-pr-review-init-prompt
  'github
  "https://github.com/acme/payment-service/pull/77")
 (ai-code--build-issue-investigation-init-prompt
  'github
  "https://github.com/acme/payment-service/issues/91"
  t))
"##,
        expect![[
            r#"OK ("  refs/heads/feature/oauth-race  " nil "Analyze the Git commit history for the entire repository 'payment-service'.\n\nRepository: payment-service\n\nThe detailed Git log content is in the 'git.log' file (which has been added to the chat).\nPlease use its content for your analysis, following these instructions:\nFind the commit that introduced duplicate settlement." "Review pull request: https://github.com/acme/payment-service/pull/77\n\nReview this pull request.\n\nReview Steps:\n1. Requirement Fit: Verify the PR implementation against requirements.\n2. Code Quality: Check code quality, security, and performance concerns.\n3. Findings: For each issue include location, issue, fix suggestion, and priority.\n\nProvide an overall assessment at the end." "Investigate issue: https://github.com/acme/payment-service/issues/91\n\nInvestigate this GitHub issue using the repository as context.\n\nIssue Investigation Steps:\n1. Understand the issue description, reproduction details, and expected behavior.\n2. Analyze relevant code in this repository as context and identify likely root causes.\n3. Provide concrete insights on how to fix it, including likely files or areas to change.\n4. No need to make code change. Provide analysis only.")"#
        ]],
    )
}

fn discussion_and_note_prompts_capture_locations_and_search_scope() -> ParityBatchCase {
    ParityBatchCase::value(
        "discussion_and_note_prompts_capture_locations_and_search_scope",
        r##"
(list
 (ai-code--format-code-change-explanation-outline
  "Summarize the behavior change."
  "Trace data flow and state transitions."
  "Identify risks, compatibility impact, and verification evidence.")
 (ai-code--build-note-insert-prompt
  "/workspace/notes/payments.org" 42
  "Record the retry invariants and operational warning.")
 (ai-code--build-note-create-prompt
  "/workspace/notes/incidents/"
  "Create a postmortem note for duplicate settlement.")
 (cl-letf (((symbol-function 'ai-code--git-root) (lambda () nil)))
   (ai-code--build-note-search-prompt
    '("/workspace/tasks/" "/workspace/notes/incidents/")
    "refresh-token reuse incidents")))
"##,
        expect![[
            r#"OK ("Explanation Steps:\n1. Summarize the behavior change.\n2. Trace data flow and state transitions.\n3. Identify risks, compatibility impact, and verification evidence.\n4. Focus on understanding the change. Do not make code changes." "Insert the note into the current Org file.\nTarget file: /workspace/notes/payments.org\nInsert location: around line 42 (current cursor position)\n\nNote request:\nRecord the retry invariants and operational warning.\n\nOnly update the requested insertion location. Do not change unrelated sections. Go ahead and start do the work." "Create a new Org note file under directory: /workspace/notes/incidents/\nAutomatically determine a concise filename from the note title/content you identified. Use lowercase letters, numbers, and underscores for the filename, with .org extension.\n\nNote request:\nCreate a postmortem note for duplicate settlement.\n\nDo not modify unrelated files. Go ahead and start the work." "Search my notes and related files for: refresh-token reuse incidents\nSearch scope paths:\n- /workspace/tasks/\n- /workspace/notes/incidents/\nUse the available search tools to inspect the selected paths.\nFocus on relevant information inside files, not just file names.\nReturn the most relevant paths, matched excerpts, and a concise answer.")"#
        ]],
    )
}

pub(super) fn prompts_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        prompt_suffix_pipeline_preserves_provider_order_and_memoizes_nil(),
        prompt_command_detection_accepts_only_true_single_token_cli_commands(),
        structured_change_and_question_briefs_preserve_scope_and_boundaries(),
        task_helpers_generate_safe_names_initial_content_and_search_handoff(),
        input_symbol_pipeline_flattens_nested_imenu_and_extracts_real_languages(),
        prompt_path_preprocessing_relativizes_repo_files_and_keeps_external_paths(),
        git_and_github_prompt_builders_create_review_ready_handoffs(),
        discussion_and_note_prompts_capture_locations_and_search_scope(),
    ]
}
