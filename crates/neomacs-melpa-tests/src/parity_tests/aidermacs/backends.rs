use expect_test::expect;

use super::ParityBatchCase;

fn aidermacs_backend_dispatch_preserves_environment_isolation_and_routes_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_backend_dispatch_preserves_environment_isolation_and_routes_commands",
        r##"(let (calls)
                      (cl-letf (((symbol-function 'aidermacs-run-comint)
                                (lambda (&rest args)
                                  (push (cons 'comint args) calls)))
                               ((symbol-function 'aidermacs-run-vterm)
                                (lambda (&rest args)
                                  (push (cons 'vterm args) calls)))
                               ((symbol-function 'aidermacs--send-command-comint)
                                (lambda (&rest args)
                                  (push (cons 'send-comint args) calls)))
                               ((symbol-function 'aidermacs--send-command-redirect-comint)
                                (lambda (&rest args)
                                  (push (cons 'redirect args) calls)))
                               ((symbol-function 'aidermacs--send-command-vterm)
                                (lambda (&rest args)
                                  (push (cons 'send-vterm args) calls))))
                        (let ((aidermacs-before-run-backend-hook
                               (list (lambda () (setenv "AIDERMACS_SECRET" "scoped"))))
                              (aidermacs-backend 'comint))
                          (setenv "AIDERMACS_SECRET" nil)
                          (aidermacs-run-backend "aider" '("--model" "x") "*a*")
                          (push (getenv "AIDERMACS_SECRET") calls)
                          (aidermacs--send-command-backend "*a*" "/ls")
                          (aidermacs--send-command-backend "*a*" "/models" t))
                        (let ((aidermacs-backend 'vterm))
                          (aidermacs-run-backend "aider-ce" nil "*v*")
                          (aidermacs--send-command-backend "*v*" "/ask hi" t))
                        (nreverse calls)))"##,
        expect![[
            r#"OK ((comint "aider" ("--model" "x") "*a*") "scoped" (send-comint "*a*" "/ls") (redirect "*a*" "/models") (vterm "aider-ce" nil "*v*") (send-vterm "*v*" "/ask hi"))"#
        ]],
    )
}

fn aidermacs_vterm_text_filter_and_theme_argument_builder_cover_color_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_vterm_text_filter_and_theme_argument_builder_cover_color_modes",
        r##"(let ((aidermacs-vterm-theme-foreground-colors-plist
                           '("--user-input-color" "#112233"
                             "--tool-error-color" "#AABBCC"))
                          (aidermacs-vterm-theme-background-colors-plist
                           '("--completion-menu-bg-color" "#010203")))
                      (list
                       (aidermacs--vterm-filter-buffer-substring
                        (lambda (&rest _)
                          "one   two   \nthree\t\t\tfour  ")
                        1 2)
                       (let ((aidermacs-vterm-use-theme-colors t))
                         (aidermacs--vterm-theme-args))
                       (let ((aidermacs-vterm-use-theme-colors nil))
                         (cl-letf
                             (((symbol-function 'frame-parameter)
                               (lambda (&rest _) 'dark)))
                           (aidermacs--vterm-theme-args)))
                       (let ((aidermacs-vterm-use-theme-colors nil))
                         (cl-letf
                             (((symbol-function 'frame-parameter)
                               (lambda (&rest _) 'light)))
                           (aidermacs--vterm-theme-args)))
                       (condition-case err
                           (aidermacs--vterm-convert-color-arg
                            :foreground 42)
                         (error (error-message-string err)))))"##,
        expect![[
            r#"OK ("one\ntwo\n\nthree\nfour" ("--user-input-color" "\\#112233" "--tool-error-color" "\\#AABBCC" "--completion-menu-bg-color" "\\#010203") ("--dark-mode") ("--light-mode") "Invalid face or colour value: 42")"#
        ]],
    )
}

fn aidermacs_comint_major_mode_guessing_handles_fences_aliases_files_and_fallbacks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_comint_major_mode_guessing_handles_fences_aliases_files_and_fallbacks",
        r##"(let ((auto-mode-alist
                           '(("\\.py\\'" . python-mode)
                             ("\\.el\\'" . emacs-lisp-mode))))
                      (mapcar
                       (lambda (text)
                         (with-temp-buffer
                           (insert text)
                           (goto-char (point-max))
                           (aidermacs--guess-major-mode)))
                       '("```elisp\n(message \"x\")\n"
                         "```bash\necho ok\n"
                         "File: src/tool.py\n```\nprint(1)\n"
                         "lib/setup.el\n```\n(message \"x\")\n"
                         "```\nplain text\n")))"##,
        expect!["OK (emacs-lisp-mode sh-mode python-mode emacs-lisp-mode fundamental-mode)"],
    )
}

fn aidermacs_comint_output_filter_accumulates_chunks_stores_history_and_runs_callback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_comint_output_filter_accumulates_chunks_stores_history_and_runs_callback",
        r##"(with-temp-buffer
                      (rename-buffer (aidermacs-get-buffer-name) t)
                      (aidermacs-comint-mode)
                      (let ((aidermacs-enable-notifications nil)
                            (aidermacs-show-diff-after-change nil)
                            (aidermacs--output-history nil)
                            (aidermacs--tracked-files nil)
                            (aidermacs--comint-output-temp "")
                            (aidermacs--current-output "")
                            (aidermacs--ready nil)
                            callback-output)
                        (setq-local aidermacs--current-callback
                                    (lambda ()
                                      (setq callback-output
                                            aidermacs--current-output)))
                        (aidermacs--comint-output-filter
                         "\e[31mAdded ./src/main.el")
                        (let ((mid
                               (list aidermacs--ready
                                     aidermacs--comint-output-temp
                                     aidermacs--output-history)))
                          (aidermacs--comint-output-filter
                           " to the chat.\e[0m\nfixture> ")
                          (list
                           mid
                           aidermacs--ready
                           aidermacs--current-output
                           callback-output
                           aidermacs--tracked-files
                           (mapcar #'cdr aidermacs--output-history)
                           aidermacs--current-callback))))"##,
        expect![[
            r#"OK ((nil "Added ./src/main.el" nil) t "Added ./src/main.el to the chat.\nfixture> " "Added ./src/main.el to the chat.\nfixture> " nil ("Added ./src/main.el to the chat.\nfixture> ") nil)"#
        ]],
    )
}

fn aidermacs_real_comint_process_runs_a_multistep_fake_aider_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "aidermacs_real_comint_process_runs_a_multistep_fake_aider_session",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                          (default-directory
                           (file-name-as-directory sandbox))
                          (program
                           (expand-file-name "bin/aider-session" sandbox))
                          (buffer-name (aidermacs-get-buffer-name)))
                      (make-directory (file-name-directory program) t)
                      (with-temp-file program
                        (insert
                         "#!/bin/sh\n"
                         "printf 'fixture> '\n"
                         "while IFS= read -r line; do\n"
                         "  printf 'received:%s\\nfixture> ' \"$line\"\n"
                         "done\n"))
                      (set-file-modes program #o755)
                      (unwind-protect
                          (progn
                            (aidermacs-run-comint
                             program '("--model" "fixture") buffer-name)
                            (with-current-buffer buffer-name
                              (let ((aidermacs-enable-notifications nil)
                                    (aidermacs-show-diff-after-change nil)
                                    (attempts 0))
                                (while (and (not aidermacs--ready)
                                            (< attempts 100))
                                  (accept-process-output nil 0.02)
                                  (setq attempts (1+ attempts)))
                                (let ((initial
                                       (list
                                        aidermacs--ready
                                        major-mode
                                        (process-live-p
                                         (get-buffer-process
                                          (current-buffer))))))
                                  (setq-local aidermacs--ready nil)
                                  (aidermacs--send-command-comint
                                   (current-buffer)
                                   "explain src/main.el")
                                  (setq attempts 0)
                                  (while (and (not aidermacs--ready)
                                              (< attempts 100))
                                    (accept-process-output nil 0.02)
                                    (setq attempts (1+ attempts)))
                                  (list
                                   initial
                                   aidermacs--ready
                                   aidermacs--last-command
                                   (string-match-p
                                    "received:explain src/main.el"
                                    (buffer-string))
                                   (mapcar
                                    (lambda (entry)
                                      (string-match-p
                                       "received:explain src/main.el"
                                       (cdr entry)))
                                    aidermacs--output-history))))))
                        (when-let ((buffer (get-buffer buffer-name)))
                          (when-let ((process (get-buffer-process buffer)))
                            (delete-process process))
                          (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((t aidermacs-comint-mode (run open listen connect stop)) t "explain src/main.el" 28 (0 nil))"#
        ]],
    )
}

pub(super) fn backends_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aidermacs_backend_dispatch_preserves_environment_isolation_and_routes_commands(),
        aidermacs_vterm_text_filter_and_theme_argument_builder_cover_color_modes(),
        aidermacs_comint_major_mode_guessing_handles_fences_aliases_files_and_fallbacks(),
        aidermacs_comint_output_filter_accumulates_chunks_stores_history_and_runs_callback(),
        aidermacs_real_comint_process_runs_a_multistep_fake_aider_session(),
    ]
}
