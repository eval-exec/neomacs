use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_mix_test_suite_file_line_stale_and_rerun_build_exact_practical_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_mix_test_suite_file_line_stale_and_rerun_build_exact_practical_commands",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (test-file
                           (expand-file-name
                            "test/accounts/user_test.exs" sandbox))
                          (buffer-file-name test-file)
                          (alchemist-mix-command "custom-mix")
                          (alchemist-mix-test-task "espec")
                          (alchemist-mix-test-default-options
                           '("--exclude" "pending"))
                          (alchemist-last-run-test nil)
                          events)
                      (make-directory (file-name-directory test-file) t)
                      (with-temp-file test-file (insert "test"))
                      (cl-letf
                          (((symbol-function 'alchemist-test-execute)
                            (lambda (command)
                              (push command events)
                              command))
                           ((symbol-function
                             'alchemist-utils-elixir-version-check-p)
                            (lambda (&rest _) t)))
                        (list
                         (alchemist-mix-test)
                         alchemist-last-run-test
                         (alchemist-mix-test-file test-file)
                         alchemist-last-run-test
                         (with-temp-buffer
                           (setq buffer-file-name test-file)
                           (insert "one\ntwo\nthree\n")
                           (goto-char (point-min))
                           (forward-line 1)
                           (alchemist-mix-test-at-point))
                         alchemist-last-run-test
                         (alchemist-mix-test-stale)
                         alchemist-last-run-test
                         (alchemist-mix-rerun-last-test)
                         alchemist-last-run-test
                         (nreverse events))))"##,
        expect![[
            r#"OK (#2=("custom-mix" "espec" nil #1=("--exclude" "pending")) "" #3=("custom-mix" "espec" "[ORACLE-SANDBOX]/test/accounts/user_test.exs" #1#) "[ORACLE-SANDBOX]/test/accounts/user_test.exs" #4=("custom-mix" "espec" "[ORACLE-SANDBOX]/test/accounts/user_test.exs:2" #1#) "[ORACLE-SANDBOX]/test/accounts/user_test.exs:2" #5=("custom-mix" "espec" "--stale" #1#) "--stale" #6=("custom-mix" "espec" "--stale" #1#) "--stale" (#2# #3# #4# #5# #6#))"#
        ]],
    )
}

fn alchemist_mix_execute_compile_run_environment_and_rerun_use_report_boundary_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_mix_execute_compile_run_environment_and_rerun_use_report_boundary_exactly",
        r##"(let ((alchemist-mix-command "/tools/mix")
                          (alchemist-mix-env "test")
                          (alchemist-mix-last-task-command nil)
                          events)
                      (cl-letf
                          (((symbol-function 'alchemist-report-run)
                            (lambda (&rest arguments)
                              (push arguments events)
                              'reported))
                           ((symbol-function 'completing-read)
                            (lambda (&rest _) "prod")))
                        (list
                         (alchemist-mix-execute
                          '("deps.get" "--only" "test"))
                         alchemist-mix-last-task-command
                         (alchemist-mix-compile "--warnings-as-errors")
                         alchemist-mix-last-task-command
                         (alchemist-mix-run
                          "-e \"App.seed()\"" '(4))
                         alchemist-mix-last-task-command
                         (alchemist-mix-rerun-last-task)
                         (nreverse events))))"##,
        expect![[
            r#"OK (reported "MIX_ENV=test /tools/mix deps.get --only test" reported "MIX_ENV=test /tools/mix compile --warnings-as-errors" reported "MIX_ENV=prod /tools/mix run -e \"App.seed()\"" reported (("MIX_ENV=test /tools/mix deps.get --only test" "alchemist-mix-report" "*alchemist mix*" alchemist-mix-mode) ("MIX_ENV=test /tools/mix compile --warnings-as-errors" "alchemist-mix-report" "*alchemist mix*" alchemist-mix-mode) ("MIX_ENV=prod /tools/mix run -e \"App.seed()\"" "alchemist-mix-report" "*alchemist mix*" alchemist-mix-mode) ("MIX_ENV=prod /tools/mix run -e \"App.seed()\"" "alchemist-mix-report" "*alchemist mix*" alchemist-mix-mode)))"#
        ]],
    )
}

fn alchemist_mix_task_filter_accumulates_server_chunks_prompts_and_runs_selected_task()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_mix_task_filter_accumulates_server_chunks_prompts_and_runs_selected_task",
        r##"(let ((alchemist-mix-filter-output nil)
                          (current-prefix-arg '(4))
                          events)
                      (cl-letf
                          (((symbol-function
                             'alchemist-mix--completing-read)
                            (lambda (prompt tasks)
                              (push (list 'select prompt tasks) events)
                              "ecto.migrate"))
                           ((symbol-function 'read-shell-command)
                            (lambda (prompt initial)
                              (push (list 'command prompt initial) events)
                              "ecto.migrate --quiet"))
                           ((symbol-function 'alchemist-mix-execute)
                            (lambda (command prefix)
                              (push
                               (list 'execute command prefix)
                               events)
                              'executed)))
                        (list
                         (alchemist-mix-filter
                          'process "compile\necto.migrate\n")
                         alchemist-mix-filter-output
                         (alchemist-mix-filter
                          'process
                          "ecto.migrate\ntest\nEND-OF-INFO\n")
                         alchemist-mix-filter-output
                         (nreverse events))))"##,
        expect![[
            r#"OK (nil ("compile\necto.migrate\n") executed nil ((select "mix: " ("compile" "ecto.migrate" "test")) (command "mix " "ecto.migrate ") (execute ("ecto.migrate --quiet") (4))))"#
        ]],
    )
}

fn alchemist_compile_and_execute_real_files_validate_types_and_emit_exact_report_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_compile_and_execute_real_files_validate_types_and_emit_exact_report_commands",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (source
                           (expand-file-name "lib/demo.ex" sandbox))
                          (script
                           (expand-file-name "scripts/demo.exs" sandbox))
                          (alchemist-compile-command "/tools/elixirc")
                          (alchemist-execute-command "/tools/elixir")
                          events
                          compile-script-error)
                      (dolist (file (list source script))
                        (make-directory (file-name-directory file) t)
                        (with-temp-file file (insert "IO.puts(:ok)")))
                      (cl-letf
                          (((symbol-function 'alchemist-report-run)
                            (lambda (&rest arguments)
                              (push arguments events)
                              'reported)))
                        (condition-case error
                            (alchemist-compile-file script)
                          (error
                           (setq compile-script-error
                                 (prin1-to-string error))))
                        (list
                         (alchemist-compile-file source)
                         (alchemist-execute-file source)
                         (alchemist-execute-file script)
                         (alchemist-compile
                          '("/tools/elixirc" "--ignore-module-conflict"
                            "lib/demo.ex"))
                         (alchemist-execute
                          '("/tools/elixir" "-e" "IO.puts(:ok)"))
                         compile-script-error
                         (nreverse events))))"##,
        expect![[
            r#"OK (reported reported reported reported reported "(error \"The given file is an Elixir Script\")" (("/tools/elixirc [ORACLE-SANDBOX]/lib/demo.ex" "alchemist-compile-report" "*alchemist elixirc*" alchemist-compile-mode) ("/tools/elixir [ORACLE-SANDBOX]/lib/demo.ex" "alchemist-execute-report" "*alchemist elixir*" alchemist-execute-mode) ("/tools/elixir [ORACLE-SANDBOX]/scripts/demo.exs" "alchemist-execute-report" "*alchemist elixir*" alchemist-execute-mode) ("/tools/elixirc --ignore-module-conflict lib/demo.ex" "alchemist-compile-report" "*alchemist elixirc*" alchemist-compile-mode) ("/tools/elixir -e IO.puts(:ok)" "alchemist-execute-report" "*alchemist elixir*" alchemist-execute-mode)))"#
        ]],
    )
}

fn alchemist_test_mode_parses_navigates_cleans_and_renders_real_exunit_report_links()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_test_mode_parses_navigates_cleans_and_renders_real_exunit_report_links",
        r##"(with-temp-buffer
                      (insert
                       "defmodule AccountTest do\n"
                       "  test \"creates an account\" do\n"
                       "    assert true\n"
                       "  end\n\n"
                       "  test :deletes_account do\n"
                       "    refute false\n"
                       "  end\n"
                       "end\n")
                      (let* ((tests
                              (alchemist-test-mode--tests-in-buffer))
                             (first-position (point)))
                        (alchemist-test-mode-jump-to-next-test)
                        (let ((next-line (line-number-at-pos)))
                          (alchemist-test-mode-jump-to-next-test)
                          (let ((wrapped-line
                                 (line-number-at-pos)))
                            (list
                             (mapcar
                              (lambda (entry)
                                (cons
                                 (substring-no-properties (car entry))
                                 (cdr entry)))
                              tests)
                             (alchemist-test-mode--buffer-contains-tests-p)
                             first-position
                             next-line
                             wrapped-line
                             (let ((alchemist-test-display-compilation-output
                                    nil))
                               (alchemist-test-clean-compilation-output
                                "Compiled lib/a.ex\nGenerated app\n\n2 tests, 0 failures\n"))
                             (let ((alchemist-test-display-compilation-output
                                    t))
                               (alchemist-test-clean-compilation-output
                                "Compiled lib/a.ex\n2 tests, 0 failures\n")))))))"##,
        expect![[
            r#"OK ((("\"creates an account\"" :marker nil nil) (":deletes_account" :marker nil nil)) 56 134 2 6 "\n2 tests, 0 failures\n" "Compiled lib/a.ex\n2 tests, 0 failures\n")"#
        ]],
    )
}

fn alchemist_test_report_turns_failures_and_stacktraces_into_real_source_buttons() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alchemist_test_report_turns_failures_and_stacktraces_into_real_source_buttons",
        r##"(with-temp-buffer
                      (insert
                       "  1) test creates an account\n"
                       "     test/accounts_test.exs:14\n"
                       "     test/accounts_test.exs:15: (test)\n")
                      (alchemist-test--render-files)
                      (let (buttons button)
                        (goto-char (point-min))
                        (while (setq button (next-button (point)))
                          (goto-char (button-start button))
                          (push
                           (list
                            (button-label button)
                            (button-get button 'file)
                            (button-get button 'face)
                            (button-get button 'follow-link)
                            (button-get button 'help-echo)
                            (button-get button 'action))
                           buttons)
                          (goto-char (button-end button)))
                        (list
                         (buffer-string)
                         (nreverse buttons))))"##,
        expect![[
            r#"OK (#("  1) test creates an account\n     test/accounts_test.exs:14\n     test/accounts_test.exs:15: (test)\n" 34 59 (help-echo #1="visit the source location" action alchemist-test--open-file follow-link t file "test/accounts_test.exs:14" face alchemist-test--test-file-and-location-face category default-button button (t)) 65 90 (help-echo #1# action alchemist-test--open-file follow-link t file "test/accounts_test.exs:15" face alchemist-test--stacktrace-file-and-location-face category default-button button (t))) (("test/accounts_test.exs:14" "test/accounts_test.exs:14" alchemist-test--test-file-and-location-face t "visit the source location" alchemist-test--open-file) ("test/accounts_test.exs:15" "test/accounts_test.exs:15" alchemist-test--stacktrace-file-and-location-face t "visit the source location" alchemist-test--open-file)))"#
        ]],
    )
}

fn alchemist_test_execute_saves_buffers_flattens_options_and_registers_exit_renderer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_test_execute_saves_buffers_flattens_options_and_registers_exit_renderer",
        r##"(let ((alchemist-test-ask-about-save nil)
                          events)
                      (cl-letf
                          (((symbol-function 'save-some-buffers)
                            (lambda (&rest arguments)
                              (push (list 'save arguments) events)
                              'saved))
                           ((symbol-function 'alchemist-report-run)
                            (lambda (&rest arguments)
                              (push (list 'report arguments) events)
                              'reported))
                           ((symbol-function 'message)
                            (lambda (&rest arguments)
                              (push (list 'message arguments) events)
                              "Testing...")))
                        (list
                         (alchemist-test-execute
                          '("mix" "test"
                            ("/workspace/user_test.exs:14")
                            ("--exclude" "pending")))
                         (nreverse events))))"##,
        expect![[
            r#"OK (reported ((message ("Testing...")) (save (t nil)) (report ("mix test /workspace/user_test.exs:14 --exclude pending" "alchemist-test-process" "*alchemist test report*" alchemist-test-report-mode alchemist-test--handle-exit))))"#
        ]],
    )
}

pub(super) fn mix_test_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_mix_test_suite_file_line_stale_and_rerun_build_exact_practical_commands(),
        alchemist_mix_execute_compile_run_environment_and_rerun_use_report_boundary_exactly(),
        alchemist_mix_task_filter_accumulates_server_chunks_prompts_and_runs_selected_task(),
        alchemist_compile_and_execute_real_files_validate_types_and_emit_exact_report_commands(),
        alchemist_test_mode_parses_navigates_cleans_and_renders_real_exunit_report_links(),
        alchemist_test_report_turns_failures_and_stacktraces_into_real_source_buttons(),
        alchemist_test_execute_saves_buffers_flattens_options_and_registers_exit_renderer(),
    ]
}
