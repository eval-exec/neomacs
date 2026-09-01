use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_major_and_minor_modes_install_real_read_only_keymap_and_buffer_local_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_major_and_minor_modes_install_real_read_only_keymap_and_buffer_local_contracts",
        r##"(mapcar
                      (lambda (mode)
                        (with-temp-buffer
                          (funcall mode)
                          (list
                           mode
                           major-mode
                           mode-name
                           buffer-read-only
                           truncate-lines
                           electric-indent-chars
                           (key-binding "q")
                           (key-binding "r")
                           (key-binding "i")
                           (key-binding "M-n")
                           (key-binding "C-c C-k"))))
                      '(alchemist-compile-mode
                        alchemist-execute-mode
                        alchemist-mix-mode
                        alchemist-test-report-mode
                        alchemist-iex-mode))"##,
        expect![[
            r#"OK ((alchemist-compile-mode alchemist-compile-mode "Elixir Compile Mode" t t nil quit-window self-insert-command self-insert-command nil nil) (alchemist-execute-mode alchemist-execute-mode "Elixir Execute Mode" t t nil quit-window self-insert-command self-insert-command nil nil) (alchemist-mix-mode alchemist-mix-mode "Mix Mode" t t nil quit-window alchemist-mix-rerun-last-task alchemist-mix-send-input-to-mix-process nil nil) (alchemist-test-report-mode alchemist-test-report-mode "Alchemist Test Report" t t nil quit-window alchemist-mix-rerun-last-test self-insert-command nil nil) (alchemist-iex-mode alchemist-iex-mode "Alchemist-IEx" nil nil (10) self-insert-command self-insert-command self-insert-command nil nil))"#
        ]],
    )
}

fn alchemist_test_help_eval_macroexpand_info_hex_and_phoenix_minor_modes_toggle_exact_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_test_help_eval_macroexpand_info_hex_and_phoenix_minor_modes_toggle_exact_state",
        r##"(mapcar
                      (lambda (mode)
                        (with-temp-buffer
                          (let ((before
                                 (list major-mode mode-name
                                       buffer-read-only)))
                            (funcall mode 1)
                            (let ((enabled
                                   (list
                                    (symbol-value mode)
                                    major-mode mode-name
                                    buffer-read-only
                                    (key-binding "q")
                                    (key-binding "C-c , s")
                                    (key-binding "C-c a n r"))))
                              (funcall mode -1)
                              (list
                               mode before enabled
                               (symbol-value mode)
                               buffer-read-only)))))
                      '(alchemist-test-mode
                        alchemist-help-minor-mode
                        alchemist-eval-mode
                        alchemist-macroexpand-mode
                        alchemist-info-mode
                        alchemist-hex-mode
                        alchemist-phoenix-mode))"##,
        expect![[
            r#"OK ((alchemist-test-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" nil self-insert-command nil nil) nil nil) (alchemist-help-minor-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" t quit-window nil nil) nil nil) (alchemist-eval-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" nil quit-window nil nil) nil nil) (alchemist-macroexpand-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" nil quit-window nil nil) nil nil) (alchemist-info-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" nil quit-window nil nil) nil nil) (alchemist-hex-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" t quit-window nil nil) nil t) (alchemist-phoenix-mode (fundamental-mode "Fundamental" nil) (t fundamental-mode "Fundamental" nil self-insert-command nil nil) nil nil))"#
        ]],
    )
}

fn alchemist_save_hooks_only_run_hidden_project_test_and_compile_reports_when_enabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_save_hooks_only_run_hidden_project_test_and_compile_reports_when_enabled",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "hook_app" sandbox)))
                          (default-directory project)
                          (alchemist-project-root-path-cache nil)
                          (alchemist-hooks-test-on-save t)
                          (alchemist-hooks-compile-on-save t)
                          events)
                      (make-directory project t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (cl-letf
                          (((symbol-function 'alchemist-report-run)
                            (lambda (&rest arguments)
                              (push arguments events)
                              'reported)))
                        (list
                         (alchemist-hooks-test-on-save)
                         (alchemist-hooks-compile-on-save)
                         (let ((alchemist-hooks-test-on-save nil))
                           (alchemist-hooks-test-on-save))
                         (let ((alchemist-hooks-compile-on-save nil))
                           (alchemist-hooks-compile-on-save))
                         (nreverse events))))"##,
        expect![[
            r#"OK (reported reported nil nil (("mix test" "alchemist-test-process" "*alchemist test report*" alchemist-test-report-mode alchemist-test--handle-exit t) ("mix compile" "alchemist-mix-report" "*alchemist mix*" alchemist-mix-mode nil t)))"#
        ]],
    )
}

fn alchemist_refcard_resolves_real_multiple_bindings_and_builds_strict_tabulated_rows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_refcard_resolves_real_multiple_bindings_and_builds_strict_tabulated_rows",
        r##"(let ((alchemist-server-processes nil))
                      (cl-letf
                          (((symbol-function
                             'alchemist-server-start-if-not-running)
                            (lambda () nil)))
                        (alchemist-mode 1)
                        (alchemist-phoenix-mode 1)
                        (let ((functions
                               '("alchemist-mix-test"
                                 "alchemist-goto-definition-at-point"
                                 "alchemist-eval-current-line"
                                 "alchemist-phoenix-router"
                                 "alchemist-refcard")))
                          (list
                           (mapcar
                            (lambda (function)
                              (list
                               function
                               (alchemist-refcard--get-keybinding
                                function)
                               (alchemist-refcard--build-tabulated-row
                                function)))
                            functions)
                           (alchemist-refcard--build-empty-tabulated-row)
                           (alchemist-refcard--build-tabulated-title-row
                            "Navigation")
                           (alchemist-refcard--build-tabulated-refcard-title-row
                            "Alchemist Refcard v1.8.2")
                           (length
                            (alchemist-refcard--tabulated-list-entries))))))"##,
        expect![[
            r#"OK ((("alchemist-mix-test" "C-c a t" ("alchemist-mix-test" ["alchemist-mix-test" #("C-c a t" 0 7 (face font-lock-builtin-face))])) ("alchemist-goto-definition-at-point" "M-." ("alchemist-goto-definition-at-point" ["alchemist-goto-definition-at-point" #("M-." 0 3 (face font-lock-builtin-face))])) ("alchemist-eval-current-line" "C-c a v l" ("alchemist-eval-current-line" ["alchemist-eval-current-line" #("C-c a v l" 0 9 (face font-lock-builtin-face))])) ("alchemist-phoenix-router" "C-c a n r" ("alchemist-phoenix-router" ["alchemist-phoenix-router" #("C-c a n r" 0 9 (face font-lock-builtin-face))])) ("alchemist-refcard" "C-c a h r" ("alchemist-refcard" ["alchemist-refcard" #("C-c a h r" 0 9 (face font-lock-builtin-face))]))) ("" ["" ""]) ("" [#("Navigation" 0 10 (face font-lock-constant-face)) ""]) ("" [#("Alchemist Refcard v1.8.2" 0 24 (face font-lock-variable-name-face)) ""]) 99)"#
        ]],
    )
}

fn alchemist_report_callbacks_status_mode_activation_and_cleanup_follow_process_lifecycle_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_report_callbacks_status_mode_activation_and_cleanup_follow_process_lifecycle_contract",
        r##"(let ((alchemist-report-on-render-function
                           (lambda (buffer)
                             (with-current-buffer buffer
                               (goto-char (point-max))
                               (insert "|rendered"))))
                          (alchemist-report-on-exit-function
                           (lambda (status buffer)
                             (with-current-buffer buffer
                               (goto-char (point-max))
                               (insert (format "|exit:%s" status)))))
                          (alchemist-report--last-run-status nil)
                          (buffer
                           (get-buffer-create
                            " *alchemist-report-parity*")))
                      (unwind-protect
                          (progn
                            (with-current-buffer buffer
                              (insert "old output"))
                            (alchemist-report--render-report buffer)
                            (alchemist-report--handle-exit
                             "finished\n" buffer)
                            (let ((after-callbacks
                                   (with-current-buffer buffer
                                     (buffer-string))))
                              (alchemist-report-activate-mode
                               #'alchemist-test-report-mode buffer)
                              (let ((mode-state
                                     (with-current-buffer buffer
                                       (list
                                        major-mode mode-name
                                        buffer-read-only
                                        truncate-lines
                                        window-point-insertion-type))))
                                (alchemist-report-cleanup-process-buffer
                                 buffer)
                                (list
                                 after-callbacks
                                 alchemist-report--last-run-status
                                 (alchemist-report--last-run-successful-p)
                                 mode-state
                                 (with-current-buffer buffer
                                   (buffer-string))
                                 (progn
                                   (alchemist-report--store-process-status
                                    "exited abnormally\n")
                                   (alchemist-report--last-run-successful-p))))))
                        (when (buffer-live-p buffer)
                          (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("old output|rendered|exit:finished\n" "finished\n" t (alchemist-test-report-mode "Alchemist Test Report" t t t) "" nil)"#
        ]],
    )
}

fn alchemist_message_replaces_real_buffer_applies_ansi_read_only_mode_and_display_boundary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_message_replaces_real_buffer_applies_ansi_read_only_mode_and_display_boundary",
        r##"(let (displayed)
                      (cl-letf
                          (((symbol-function 'display-buffer)
                            (lambda (buffer)
                              (setq displayed (buffer-name buffer))
                              buffer)))
                        (unwind-protect
                            (progn
                              (with-current-buffer
                                  (get-buffer-create
                                   alchemist-message--buffer-name)
                                (insert "stale"))
                              (list
                               (alchemist-message
                                "\e[31mCompilation failed\e[0m\nline 2")
                               displayed
                               (with-current-buffer
                                   alchemist-message--buffer-name
                                 (list
                                  (buffer-string)
                                  buffer-read-only
                                  alchemist-message-mode
                                  mode-name
                                  (key-binding "q")))))
                          (when
                              (get-buffer
                               alchemist-message--buffer-name)
                            (kill-buffer
                             alchemist-message--buffer-name)))))"##,
        expect![[
            r#"OK (t "*alchemist message*" ("Compilation failed\nline 2" t t "Fundamental" quit-window))"#
        ]],
    )
}

pub(super) fn modes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_major_and_minor_modes_install_real_read_only_keymap_and_buffer_local_contracts(),
        alchemist_test_help_eval_macroexpand_info_hex_and_phoenix_minor_modes_toggle_exact_state(),
        alchemist_save_hooks_only_run_hidden_project_test_and_compile_reports_when_enabled(),
        alchemist_refcard_resolves_real_multiple_bindings_and_builds_strict_tabulated_rows(),
        alchemist_report_callbacks_status_mode_activation_and_cleanup_follow_process_lifecycle_contract(),
        alchemist_message_replaces_real_buffer_applies_ansi_read_only_mode_and_display_boundary(),
    ]
}
