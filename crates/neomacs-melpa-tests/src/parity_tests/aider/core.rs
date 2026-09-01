use expect_test::expect;

use super::ParityBatchCase;

fn aider_multiline_message_protocol_handles_plain_wrapped_and_partial_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_multiline_message_protocol_handles_plain_wrapped_and_partial_inputs",
        r##"(mapcar
         #'aider--process-message-if-multi-line
         '("single line"
           ""
           "line one\nline two"
           "\nleading"
           "trailing\n"
           "{aider\nalready wrapped\naider}"
           "prefix {aider\nalready marked"))"##,
        expect![[
            r#"OK ("single line" "" "{aider\nline one\nline two\naider}" "{aider\n\nleading\naider}" "{aider\ntrailing\n\naider}" "{aider\nalready wrapped\naider}" "prefix {aider\nalready marked")"#
        ]],
    )
}

fn aider_cli_history_parser_preserves_order_multiline_blocks_and_unclosed_input() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aider_cli_history_parser_preserves_order_multiline_blocks_and_unclosed_input",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                (history (expand-file-name "history/.aider.input.history" root)))
         (make-directory (file-name-directory history) t)
         (with-temp-file history
           (insert "- ignored output\n")
           (insert "+ first command\n")
           (insert "+ {aider\n")
           (insert "+ line one\n")
           (insert "+ line two\n")
           (insert "+ aider}\n")
           (insert "+ second command\n")
           (insert "+ {aider\n")
           (insert "+ unfinished\n"))
         (list
          (aider--parse-aider-cli-history history)
          (aider--parse-aider-cli-history
           (expand-file-name "missing" root))
          (file-attribute-size (file-attributes history))))"##,
        expect![[
            r#"OK (("first command" "{aider\nline one\nline two\naider}" "second command" "{aider\nunfinished") nil 112)"#
        ]],
    )
}

fn aider_buffer_names_cover_repo_branch_file_and_invalid_contexts() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_buffer_names_cover_repo_branch_file_and_invalid_contexts",
        r##"(let (messages)
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest args)
                      (push (apply #'format format-string args) messages))))
           (list
            (let ((aider-use-branch-specific-buffers nil))
              (cl-letf (((symbol-function 'aider--get-git-repo-root)
                         (lambda () "/repo/project/")))
                (aider-buffer-name)))
            (let ((aider-use-branch-specific-buffers t))
              (cl-letf (((symbol-function 'aider--get-git-repo-root)
                         (lambda () "/repo/project/"))
                        ((symbol-function 'aider--get-current-git-branch)
                         (lambda (_root) "feature/a")))
                (aider-buffer-name)))
            (let ((aider-use-branch-specific-buffers t))
              (cl-letf (((symbol-function 'aider--get-current-git-branch)
                         (lambda (_root) nil)))
                (aider--buffer-name-for-git-repo "/repo/project/")))
            (with-temp-buffer
              (setq buffer-file-name "/work/loose/file.el")
              (cl-letf (((symbol-function 'aider--get-git-repo-root)
                         (lambda () nil)))
                (aider-buffer-name)))
            (with-temp-buffer
              (cl-letf (((symbol-function 'aider--get-git-repo-root)
                         (lambda () nil)))
                (condition-case error-data
                    (aider-buffer-name)
                  (error (list (car error-data) (cadr error-data))))))
            (nreverse messages))))"##,
        expect![[
            r#"OK ("*aider:/repo/project/*" "*aider:/repo/project/:feature/a*" "*aider:/repo/project/*" "*aider:/work/loose/*" (error "Aider: Not in a git repository and current buffer is not associated with a file") ("Aider: Could not determine git branch for '/repo/project/', or branch name is empty. Using default git repo buffer name."))"#
        ]],
    )
}

fn aider_prepare_args_adds_architect_guard_and_subtree_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_prepare_args_adds_architect_guard_and_subtree_once",
        r##"(let ((aider-args '("--model" "sonnet"))
               messages)
         (cl-letf (((symbol-function 'aider--maybe-prompt-subtree-only-for-special-modes)
                    (lambda (args) (append args '("--from-mode"))))
                   ((symbol-function 'message)
                    (lambda (format-string &rest args)
                      (push (apply #'format format-string args) messages))))
           (list
            (aider--prepare-aider-args nil nil)
            (aider--prepare-aider-args nil t)
            (let ((aider-args
                   '("--auto-accept-architect" "--subtree-only")))
              (aider--prepare-aider-args nil t))
            (nreverse messages))))"##,
        expect![[
            r#"OK (("--model" "sonnet" "--no-auto-accept-architect" . #1=("--from-mode")) ("--model" "sonnet" "--no-auto-accept-architect" "--from-mode" "--subtree-only") ("--auto-accept-architect" "--subtree-only" . #1#) ("Adding --subtree-only argument as requested."))"#
        ]],
    )
}

fn aider_command_completion_reports_exact_bounds_candidates_and_exclusivity() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_command_completion_reports_exact_bounds_candidates_and_exclusivity",
        r##"(mapcar
         (lambda (input)
           (with-temp-buffer
             (insert input)
             (let ((completion (aider-core--command-completion)))
               (and completion
                    (list
                     (nth 0 completion)
                     (nth 1 completion)
                     (nth 2 completion)
                     (nth 3 completion)
                     (nth 4 completion))))))
         '("/" "/co" "prefix /co" "  /rea" "/unknown" "/model suffix"))"##,
        expect![[
            r#"OK ((1 2 ("/add" "/architect" "/ask" "/code" "/reset" "/undo" "/lint" "/read-only" "/drop" "/copy" "/copy-context" "/clear" "/commit" "/exit" "/quit" "/paste" "/help" "/chat-mode" "/diff" "/editor" "/git" "/load" "/ls" "/map" "/map-refresh" "/think-tokens" "/tokens" "/model" "/editor-model" "/weak-model" "/models" "/reasoning-effort" "/multiline-mode" "/report" "/run" "/save" "/settings" "/test" "/voice" "/web") :exclusive no) (1 4 ("/code" "/copy" "/copy-context" "/commit") :exclusive no) nil nil nil (1 7 ("/model" "/models") :exclusive no))"#
        ]],
    )
}

fn aider_comint_mode_installs_buffer_local_hooks_history_and_input_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_comint_mode_installs_buffer_local_hooks_history_and_input_navigation",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                (history (expand-file-name ".aider.input.history" root))
                (aider-enable-markdown-highlighting nil))
         (with-temp-file history
           (insert "+ oldest\n+ newest\n"))
         (with-temp-buffer
           (setq default-directory (file-name-as-directory root))
           (cl-letf (((symbol-function 'aider--generate-history-file-name)
                      (lambda () history))
                     ((symbol-function 'aider--ensure-highlight-timer)
                      (lambda () 'timer-ready)))
             (aider-comint-mode)
             (insert "draft")
             (let ((before
                    (list major-mode
                          comint-input-sender
                          (local-variable-p 'aider--history-index)
                          (memq #'aider-core--command-completion
                                completion-at-point-functions)
                          (ring-elements comint-input-ring))))
               (aider-history-prev)
               (let ((first (buffer-string)))
                 (aider-history-prev)
                 (let ((second (buffer-string)))
                   (aider-history-next)
                   (let ((third (buffer-string)))
                     (aider-history-next)
                     (list before first second third
                           (buffer-string)
                           aider--history-index
                           aider--original-input))))))))"##,
        expect![[
            r#"OK ((aider-comint-mode aider-input-sender t (aider-core--command-completion comint-completion-at-point t) ("newest" "oldest")) "newest" "oldest" "newest" "draft" nil nil)"#
        ]],
    )
}

fn aider_current_added_file_parser_reads_latest_prompt_block_and_read_only_suffixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aider_current_added_file_parser_reads_latest_prompt_block_and_read_only_suffixes",
        r##"(let ((buffer (generate-new-buffer " *aider-added-files*")))
         (unwind-protect
             (with-current-buffer buffer
               (insert "old.py\n\n> old prompt\n")
               (insert "src/main.py\n")
               (insert "docs/guide.md (read only)\n")
               (insert "tests/test_main.py\n")
               (insert "> current prompt\n")
               (cl-letf (((symbol-function 'aider-buffer-name)
                          (lambda () (buffer-name buffer))))
                 (list
                  (aider-core--parse-added-file-list)
                  (progn
                    (erase-buffer)
                    (insert "no prompt here\n")
                    (aider-core--parse-added-file-list)))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("> old prompt" "src/main.py" "docs/guide.md" "tests/test_main.py") nil)"#
        ]],
    )
}

fn aider_auto_trigger_hooks_route_only_exact_commands_at_end_of_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "aider_auto_trigger_hooks_route_only_exact_commands_at_end_of_line",
        r##"(let (calls)
         (cl-letf (((symbol-function 'completion-at-point)
                    (lambda () (push 'completion calls)))
                   ((symbol-function 'aider-prompt-insert-add-file-path)
                    (lambda () (push 'add-path calls)))
                   ((symbol-function 'aider-prompt-insert-drop-file-path)
                    (lambda () (push 'drop-path calls)))
                   ((symbol-function 'aider-core-insert-prompt)
                    (lambda () (push 'prompt calls))))
           (dolist (input '("/" "/add " "/drop " "/ask " "/architect  " "x /"))
             (with-temp-buffer
               (insert input)
               (let ((aider-auto-trigger-command-completion t)
                     (aider-auto-trigger-file-path-insertion t)
                     (aider-auto-trigger-prompt t))
                 (aider-core--auto-trigger-command-completion)
                 (aider-core--auto-trigger-file-path-insertion)
                 (aider-core--auto-trigger-insert-prompt))))
           (nreverse calls)))"##,
        expect!["OK (completion add-path drop-path prompt completion)"],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aider_multiline_message_protocol_handles_plain_wrapped_and_partial_inputs(),
        aider_cli_history_parser_preserves_order_multiline_blocks_and_unclosed_input(),
        aider_buffer_names_cover_repo_branch_file_and_invalid_contexts(),
        aider_prepare_args_adds_architect_guard_and_subtree_once(),
        aider_command_completion_reports_exact_bounds_candidates_and_exclusivity(),
        aider_comint_mode_installs_buffer_local_hooks_history_and_input_navigation(),
        aider_current_added_file_parser_reads_latest_prompt_block_and_read_only_suffixes(),
        aider_auto_trigger_hooks_route_only_exact_commands_at_end_of_line(),
    ]
}
