use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_multiline_and_edit_classification_cover_real_chat_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_multiline_and_edit_classification_cover_real_chat_modes",
        r##"(list
                      (mapcar #'aidermacs--process-message-if-multi-line
                              '("one line"
                                "first\nsecond"
                                "{aidermacs\nfirst\nsecond\naidermacs}"))
                      (mapcar
                       (lambda (mode)
                         (let ((aidermacs--current-mode mode))
                           (mapcar #'aidermacs--command-may-edit-files
                                   '("refactor this" "/ask explain"
                                     "/code fix" "/architect plan"
                                     "/help commands"))))
                       '(code architect ask help nil)))"##,
        expect![[
            r#"OK (("one line" "{aidermacs\nfirst\nsecond\naidermacs}" "{aidermacs\n{aidermacs\nfirst\nsecond\naidermacs}\naidermacs}") ((t nil 0 0 nil) (t nil 0 0 nil) (nil nil 0 0 nil) (nil nil 0 0 nil) (nil nil 0 0 nil)))"#
        ]],
    )
}

fn aidermacs_prompt_builder_combines_active_region_user_input_and_deduplicated_history()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_prompt_builder_combines_active_region_user_input_and_deduplicated_history",
        r##"(with-temp-buffer
                      (rename-buffer "practical.py" t)
                      (insert "before\n")
                      (let ((start (point)))
                        (insert "def add(a, b):\n    return a + b\n")
                        (set-mark start)
                        (activate-mark)
                        (goto-char (point-max))
                        (let ((aidermacs--read-string-history
                               '("old request" "duplicate"))
                              answers)
                          (cl-letf
                              (((symbol-function 'read-string)
                                (lambda (prompt &rest _)
                                  (push prompt answers)
                                  "duplicate")))
                            (list
                             (aidermacs--form-prompt
                              "/architect" "Improve this"
                              "confirm before edit")
                             (aidermacs--form-prompt
                              "/ask" nil "general" t)
                             aidermacs--read-string-history
                             (nreverse answers))))))"##,
        expect![[
            r#"OK ("/architect Improve this in practical.py regarding this section:\n```\ndef add(a, b):\n    return a + b\n\n```\n: duplicate" "/ask : duplicate" ("duplicate" "old request") ("/architect Improve this in practical.py regarding this section:\n```\ndef add(a, b):\n    return a + b\n\n```\n (confirm before edit): " "/ask  (general): "))"#
        ]],
    )
}

fn aidermacs_context_region_detection_and_todo_comment_logic_use_real_buffer_syntax()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_context_region_detection_and_todo_comment_logic_use_real_buffer_syntax",
        r##"(with-temp-buffer
                      (emacs-lisp-mode)
                      (insert
                       ";; first paragraph\n"
                       ";; more context\n\n"
                       "(defun demo ()\n"
                       "  ;; TODO: implement branch\n"
                       "  nil)\n\n"
                       "(message \"tail\")\n")
                      (goto-char (point-min))
                      (search-forward "TODO")
                      (let ((region (aidermacs--detect-code-change-region)))
                        (list
                         (buffer-substring-no-properties
                          (car region) (cdr region))
                         (mapcar #'aidermacs--is-comment-line
                                 '(";; TODO work" "  ; note"
                                   "(message \"x\")" ""))
                         (progn
                           (deactivate-mark)
                           (aidermacs--set-code-change-region)
                           (list
                            (use-region-p)
                            (buffer-substring-no-properties
                             (region-beginning) (region-end)))))))"##,
        expect![[
            r#"OK ("\n(defun demo ()\n  ;; TODO: implement branch\n  nil)\n" (0 0 nil nil) (nil "\n(defun demo ()\n  ;; TODO: implement branch\n  nil)\n"))"#
        ]],
    )
}

fn aidermacs_prompt_file_creation_is_repeatable_and_auto_enables_minor_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_prompt_file_creation_is_repeatable_and_auto_enables_minor_mode",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (root (file-name-as-directory
                                 (expand-file-name "repo" sandbox)))
                          (default-directory root)
                          (aidermacs-prompt-file-name ".aider.prompt.org")
                          opened)
                      (make-directory (expand-file-name ".git" root) t)
                      (cl-letf
                          (((symbol-function 'find-file-other-window)
                            (lambda (file)
                              (setq opened file)
                              (find-file file))))
                        (save-window-excursion
                          (aidermacs-open-prompt-file)
                          (let ((first
                                 (buffer-substring-no-properties
                                  (point-min) (point-max))))
                            (aidermacs-minor-mode -1)
                            (aidermacs--maybe-enable-minor-mode)
                            (save-buffer)
                            (kill-buffer)
                            (aidermacs-open-prompt-file)
                            (list
                             (file-relative-name opened root)
                             first
                             (buffer-substring-no-properties
                              (point-min) (point-max))
                             aidermacs-minor-mode
                             (lookup-key
                              aidermacs-minor-mode-map
                              (kbd "C-c C-c")))))))"##,
        expect![[
            r##"OK (".aider.prompt.org" "# aidermacs Prompt File - Command Reference:\n# C-c C-n or C-<return>: Send current line or selected region line by line\n# C-c C-c: Send current block or selected region as a whole\n# C-c C-z: Switch to aidermacs buffer\n\n* Sample task:\n\n/ask what this repo is about?\n" "# aidermacs Prompt File - Command Reference:\n# C-c C-n or C-<return>: Send current line or selected region line by line\n# C-c C-c: Send current block or selected region as a whole\n# C-c C-z: Switch to aidermacs buffer\n\n* Sample task:\n\n/ask what this repo is about?\n" nil aidermacs-send-block-or-region)"##
        ]],
    )
}

fn aidermacs_line_region_and_block_senders_preserve_practical_prompt_units() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_line_region_and_block_senders_preserve_practical_prompt_units",
        r##"(with-temp-buffer
                      (insert
                       "first task\n"
                       "\n"
                       "second task\n"
                       "  third task  \n\n"
                       "paragraph two\ncontinues\n")
                      (let (sent)
                        (cl-letf
                            (((symbol-function 'aidermacs--send-command)
                              (lambda (command &rest _)
                                (push command sent))))
                          (goto-char (point-min))
                          (aidermacs-send-line-or-region)
                          (aidermacs-send-region-by-line
                           (point-min)
                           (save-excursion
                             (goto-char (point-min))
                             (forward-line 4)
                             (point)))
                          (goto-char (point-max))
                          (forward-line -1)
                          (aidermacs-send-block-or-region)
                          (nreverse sent))))"##,
        expect![[
            r#"OK ("first task" "first task" "second task" "third task" "\nparagraph two\ncontinues\n")"#
        ]],
    )
}

fn aidermacs_mode_and_utility_commands_form_a_persistent_session_sequence() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_mode_and_utility_commands_form_a_persistent_session_sequence",
        r##"(let ((session (get-buffer-create "*aidermacs:modes*"))
                          sent messages)
                      (unwind-protect
                          (cl-letf
                              (((symbol-function 'aidermacs-get-buffer-name)
                                (lambda (&rest _) (buffer-name session)))
                               ((symbol-function 'aidermacs--send-command)
                                (lambda (command &rest _)
                                  (push command sent)))
                               ((symbol-function 'message)
                                (lambda (format-string &rest arguments)
                                  (push
                                   (apply #'format
                                          format-string arguments)
                                   messages))))
                            (dolist
                                (command
                                 '(aidermacs-switch-to-code-mode
                                   aidermacs-switch-to-ask-mode
                                   aidermacs-switch-to-architect-mode
                                   aidermacs-switch-to-help-mode
                                   aidermacs-clear-chat-history
                                   aidermacs-reset
                                   aidermacs-accept-change
                                   aidermacs-undo-last-commit
                                   aidermacs-commit-with-auto-message
                                   aidermacs-refresh-repo-map
                                   aidermacs-send-voice))
                              (funcall command))
                            (aidermacs-web "https://example.test/docs?q=1")
                            (with-current-buffer session
                              (list
                               aidermacs--current-mode
                               aidermacs--tracked-files
                               (nreverse sent)
                               (nreverse messages))))
                        (when (buffer-live-p session)
                          (kill-buffer session))))"##,
        expect![[
            r#"OK (help nil ("/chat-mode code" "/chat-mode ask" "/chat-mode architect" "/chat-mode help" "/clear" "/reset" "/code ok" "/undo" "/commit" "/map-refresh" "/voice" "/web https://example.test/docs?q=1") ("Switched to code mode <default> - aider will make changes to your code" "Switched to ask mode - you can chat freely, aider will not edit your code" "Switched to architect mode - aider will propose solutions before making changes" "Switched to help mode - aider will answer questions about using aider" "Refreshing repository map..." "aidermacs awaiting speech" "Fetching content from https://example.test/docs?q=1..."))"#
        ]],
    )
}

fn aidermacs_common_code_actions_build_practical_commands_at_external_session_boundary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_common_code_actions_build_practical_commands_at_external_session_boundary",
        r##"(with-temp-buffer
                      (rename-buffer "service.el" t)
                      (emacs-lisp-mode)
                      (insert
                       "(defun serve (request)\n"
                       "  ;; TODO validate REQUEST\n"
                       "  (list :ok request))\n")
                      (goto-char (point-min))
                      (search-forward "request")
                      (let (sent tracked prompts)
                        (cl-letf
                            (((symbol-function 'aidermacs--send-command)
                              (lambda (command &rest _)
                                (push command sent)))
                             ((symbol-function
                               'aidermacs--ensure-current-file-tracked)
                              (lambda () (setq tracked (1+ (or tracked 0)))))
                             ((symbol-function 'read-string)
                              (lambda (prompt &rest _)
                                (push prompt prompts)
                                "handle nil and malformed values")))
                          (aidermacs-direct-change)
                          (deactivate-mark)
                          (aidermacs-question-code)
                          (deactivate-mark)
                          (aidermacs-architect-this-code)
                          (aidermacs-question-general)
                          (aidermacs-help)
                          (list
                           tracked
                           (nreverse sent)
                           (nreverse prompts)
                           aidermacs--read-string-history))))"##,
        expect![[
            r#"OK (3 ("/code Make this change: handle nil and malformed values" "/ask Propose a solution: handle nil and malformed values" "/architect Design a solution: handle nil and malformed values" "/ask : handle nil and malformed values" "/help : handle nil and malformed values") ("/code Make this change (will edit file): " "/ask Propose a solution (won't edit file): " "/architect Design a solution (confirm before edit): " "/ask  (empty for ask mode): " "/help  (question how to use aider, empty for all commands): ") ("handle nil and malformed values"))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_multiline_and_edit_classification_cover_real_chat_modes(),
        aidermacs_prompt_builder_combines_active_region_user_input_and_deduplicated_history(),
        aidermacs_context_region_detection_and_todo_comment_logic_use_real_buffer_syntax(),
        aidermacs_prompt_file_creation_is_repeatable_and_auto_enables_minor_mode(),
        aidermacs_line_region_and_block_senders_preserve_practical_prompt_units(),
        aidermacs_mode_and_utility_commands_form_a_persistent_session_sequence(),
        aidermacs_common_code_actions_build_practical_commands_at_external_session_boundary(),
    ]
}
