use expect_test::expect;

use super::ParityBatchCase;

fn aidev_mode_refactors_one_function_in_a_saved_project_file_via_openai() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidev_mode_refactors_one_function_in_a_saved_project_file_via_openai",
        r##"(let* ((root
                                 (expand-file-name
                                  "openai-project"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (file
                                 (expand-file-name
                                  "greeting.el"
                                  root))
                                request
                                response-buffer
                                project-buffer)
                           (make-directory root t)
                           (with-temp-file file
                             (insert
                              "(defun greet (name)\n"
                              "  (concat \"Hello \" name))\n"
                              "\n"
                              "(defun untouched (value)\n"
                              "  (* value 2))\n"))
                           (setq project-buffer
                                 (find-file-noselect file))
                           (unwind-protect
                               (with-current-buffer project-buffer
                                 (emacs-lisp-mode)
                                 (aidev-mode 1)
                                 (goto-char
                                  (point-min))
                                 (let ((start
                                        (point))
                                       (end
                                        (progn
                                          (search-forward
                                           "\n\n")
                                          (point))))
                                   (goto-char end)
                                   (set-mark start)
                                   (setq transient-mark-mode t)
                                   (activate-mark))
                                 (let ((aidev-provider
                                        'openai)
                                       (aidev-default-model
                                        "gpt-4"))
                                   (setenv
                                    "OPENAI_API_KEY"
                                    "test-key")
                                   (cl-letf
                                       (((symbol-function
                                          'url-retrieve-synchronously)
                                         (lambda (url &rest _arguments)
                                           (let* ((payload
                                                   (json-read-from-string
                                                    url-request-data))
                                                  (messages
                                                   (append
                                                    (alist-get
                                                     'messages
                                                     payload)
                                                    nil)))
                                             (setq request
                                                   (list
                                                    url
                                                    url-request-method
                                                    (alist-get
                                                     'model
                                                     payload)
                                                    (mapcar
                                                     (lambda (message)
                                                       (alist-get
                                                        'role
                                                        message))
                                                     messages)
                                                    (mapcar
                                                     (lambda (message)
                                                       (alist-get
                                                        'content
                                                        message))
                                                     (cdr messages)))))
                                           (setq response-buffer
                                                 (generate-new-buffer
                                                  " *aidev-openai-response*"))
                                           (with-current-buffer response-buffer
                                             (insert
                                              "HTTP/1.1 200 OK\n"
                                              "Content-Type: application/json\n"
                                              "\n"
                                              "{\"choices\":[{\"message\":{\"content\":\"```elisp\\n(defun greet (name)\\n  (format \\\"Hello, %s!\\\" name))\\n\\n```\"}}]}"))
                                           response-buffer)))
                                     (aidev-refactor-region-with-chat
                                      "Use format and punctuation")))
                                 (save-buffer)
                                 (list
                                  aidev-mode
                                  (buffer-substring-no-properties
                                   (point-min)
                                   (point-max))
                                  (with-temp-buffer
                                    (insert-file-contents file)
                                    (buffer-string))
                                  request
                                  (buffer-live-p
                                   response-buffer)))
                             (when
                                 (buffer-live-p project-buffer)
                               (with-current-buffer project-buffer
                                 (set-buffer-modified-p nil))
                               (kill-buffer project-buffer))))"##,
        expect![[
            r#"OK (t "(defun greet (name)\n  (format \"Hello, %s!\" name))\n(defun untouched (value)\n  (* value 2))\n" "(defun greet (name)\n  (format \"Hello, %s!\" name))\n(defun untouched (value)\n  (* value 2))\n" ("https://api.openai.com/v1/chat/completions" "POST" "gpt-4" ("system" "user" "user") ("(defun greet (name)\n  (concat \"Hello \" name))\n\n" "Use format and punctuation")) nil)"#
        ]],
    )
}

fn aidev_mode_inserts_generated_python_using_selected_context_via_ollama() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidev_mode_inserts_generated_python_using_selected_context_via_ollama",
        r##"(let (request response-buffer)
                           (with-temp-buffer
                             (python-mode)
                             (insert
                              "def normalize(value):\n"
                              "    return value.strip().lower()\n"
                              "\n"
                              "# add slug helper below\n")
                             (goto-char
                              (point-min))
                             (let ((start
                                    (point))
                                   (end
                                    (progn
                                      (search-forward
                                       "\n\n")
                                      (point))))
                               (goto-char end)
                               (set-mark start)
                               (setq transient-mark-mode t)
                               (activate-mark))
                             (let ((aidev-provider
                                    'ollama)
                                   (aidev-default-model
                                    "qwen2.5-coder:7b")
                                   (aidev---ollama-default-url
                                    "http://ollama.test:11434"))
                               (cl-letf
                                   (((symbol-function
                                      'url-retrieve-synchronously)
                                     (lambda (url &rest _arguments)
                                       (let ((payload
                                              (json-read-from-string
                                               url-request-data)))
                                         (setq request
                                               (list
                                                url
                                                url-request-method
                                                (alist-get
                                                 'model
                                                 payload)
                                                (and
                                                 (string-match-p
                                                  "def normalize"
                                                  (alist-get
                                                   'prompt
                                                   payload))
                                                 t)
                                                (and
                                                 (string-match-p
                                                  "Add a slugify helper"
                                                  (alist-get
                                                   'prompt
                                                   payload))
                                                 t))))
                                       (setq response-buffer
                                             (generate-new-buffer
                                              " *aidev-ollama-response*"))
                                       (with-current-buffer response-buffer
                                         (insert
                                          "HTTP/1.1 200 OK\n"
                                          "Content-Type: application/json\n"
                                          "\n"
                                          "{\"response\":\"```python\\ndef slugify(value):\\n    return normalize(value).replace(\\\" \\\", \\\"-\\\")\\n\\n\\n```\"}"))
                                       response-buffer)))
                                 (aidev-insert-chat
                                  "Add a slugify helper")))
                             (list
                              (buffer-substring-no-properties
                               (point-min)
                               (point-max))
                              request
                              (buffer-live-p
                               response-buffer))))"##,
        expect![[
            r#"OK ("def normalize(value):\n    return value.strip().lower()\n\ndef slugify(value):\n    return normalize(value).replace(\" \", \"-\")\n\n# add slug helper below\n" ("http://ollama.test:11434/api/generate" "POST" "qwen2.5-coder:7b" t t) nil)"#
        ]],
    )
}

fn aidev_mode_runs_a_two_turn_claude_chat_with_complete_conversation_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidev_mode_runs_a_two_turn_claude_chat_with_complete_conversation_history",
        r##"(let ((aidev-provider
                                'claude)
                               (aidev-default-model
                                "claude-3-5-sonnet-20240620")
                               (aidev-chat-buffer-name
                                "*AIdev migration chat*")
                               requests
                               response-buffers
                               (call-count 0)
                               chat-buffer)
                           (setenv
                            "ANTHROPIC_API_KEY"
                            "test-key")
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'url-retrieve-synchronously)
                                     (lambda (url &rest _arguments)
                                       (setq call-count
                                             (1+ call-count))
                                       (let* ((payload
                                               (json-read-from-string
                                                url-request-data))
                                              (messages
                                               (append
                                                (alist-get
                                                 'messages
                                                 payload)
                                                nil)))
                                         (push
                                          (list
                                           url
                                           (alist-get
                                            'model
                                            payload)
                                           (mapcar
                                            (lambda (message)
                                              (list
                                               (alist-get
                                                'role
                                                message)
                                               (alist-get
                                                'content
                                                message)))
                                            messages))
                                          requests))
                                       (let ((buffer
                                              (generate-new-buffer
                                               " *aidev-claude-response*")))
                                         (push buffer
                                               response-buffers)
                                         (with-current-buffer buffer
                                           (insert
                                            "HTTP/1.1 200 OK\n"
                                            "Content-Type: application/json\n"
                                            "\n"
                                            (if
                                                (= call-count 1)
                                                "{\"content\":[{\"text\":\"Inventory dependencies first.\"}]}"
                                              "{\"content\":[{\"text\":\"Add a rollback checkpoint before each migration.\"}]}")))
                                         buffer))))
                                 (aidev-start-chat
                                  "Plan a safe migration")
                                 (setq chat-buffer
                                       (get-buffer
                                        aidev-chat-buffer-name))
                                 (with-current-buffer chat-buffer
                                   (insert
                                    aidev-chat-user-prompt-prefix
                                    "Add rollback steps"
                                    aidev-chat-separator)
                                   (aidev-chat-send-message
                                    "Add rollback steps")
                                   (list
                                    (buffer-substring-no-properties
                                     (point-min)
                                     (point-max))
                                    (reverse
                                     (copy-tree
                                      aidev-chat-messages))
                                    (nreverse requests)
                                    (mapcar
                                     #'buffer-live-p
                                     response-buffers))))
                             (when
                                 (buffer-live-p chat-buffer)
                               (kill-buffer chat-buffer))))"##,
        expect![[
            r#"OK ("User: Plan a safe migration\n\nAI: Inventory dependencies first.\n\nUser: Add rollback steps\n\nAI: Add a rollback checkpoint before each migration.\n\n" ((("role" . "user") ("content" . "Plan a safe migration")) (("role" . "assistant") ("content" . "Inventory dependencies first.")) (("role" . "user") ("content" . "Add rollback steps")) (("role" . "assistant") ("content" . "Add a rollback checkpoint before each migration."))) (("https://api.anthropic.com/v1/messages" "claude-3-5-sonnet-20240620" (("user" "Plan a safe migration"))) ("https://api.anthropic.com/v1/messages" "claude-3-5-sonnet-20240620" (("user" "Plan a safe migration") ("assistant" "Inventory dependencies first.") ("user" "Add rollback steps")))) (nil nil))"#
        ]],
    )
}

fn aidev_mode_provider_failure_keeps_the_selected_file_and_disk_unchanged() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidev_mode_provider_failure_keeps_the_selected_file_and_disk_unchanged",
        r##"(let* ((root
                                 (expand-file-name
                                  "failed-refactor"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (file
                                 (expand-file-name
                                  "service.el"
                                  root))
                                response-buffer
                                project-buffer)
                           (make-directory root t)
                           (with-temp-file file
                             (insert
                              "(defun deploy ()\n"
                              "  (message \"deploying\"))\n"))
                           (setq project-buffer
                                 (find-file-noselect file))
                           (unwind-protect
                               (with-current-buffer project-buffer
                                 (emacs-lisp-mode)
                                 (goto-char
                                  (point-max))
                                 (set-mark
                                  (point-min))
                                 (setq transient-mark-mode t)
                                 (activate-mark)
                                 (let ((aidev-provider
                                        'openai)
                                       (aidev-default-model
                                        "gpt-4"))
                                   (cl-letf
                                       (((symbol-function
                                          'url-retrieve-synchronously)
                                         (lambda (&rest _arguments)
                                           (setq response-buffer
                                                 (generate-new-buffer
                                                  " *aidev-invalid-response*"))
                                           (with-current-buffer response-buffer
                                             (insert
                                              "HTTP/1.1 200 OK\n"
                                              "Content-Type: application/json\n"
                                              "\n"
                                              "{}"))
                                           response-buffer)))
                                     (let ((problem
                                            (condition-case error
                                                (progn
                                                  (aidev-refactor-region-with-chat
                                                   "Make deployment idempotent")
                                                  nil)
                                              (error
                                               (list
                                                (car error)
                                                (error-message-string
                                                 error))))))
                                       (list
                                        problem
                                        (buffer-substring-no-properties
                                         (point-min)
                                         (point-max))
                                        (with-temp-buffer
                                          (insert-file-contents file)
                                          (buffer-string))
                                        (buffer-modified-p)
                                        (buffer-live-p
                                         response-buffer))))))
                             (when
                                 (buffer-live-p project-buffer)
                               (with-current-buffer project-buffer
                                 (set-buffer-modified-p nil))
                               (kill-buffer project-buffer))))"##,
        expect![[
            r#"OK ((wrong-type-argument "Wrong type argument: arrayp, nil") "(defun deploy ()\n  (message \"deploying\"))\n" "(defun deploy ()\n  (message \"deploying\"))\n" nil nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidev_mode_refactors_one_function_in_a_saved_project_file_via_openai(),
        aidev_mode_inserts_generated_python_using_selected_context_via_ollama(),
        aidev_mode_runs_a_two_turn_claude_chat_with_complete_conversation_history(),
        aidev_mode_provider_failure_keeps_the_selected_file_and_disk_unchanged(),
    ]
}
