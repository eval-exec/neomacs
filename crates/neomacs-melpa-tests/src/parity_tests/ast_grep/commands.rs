use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_build_command_preserves_patterns_rewrites_and_expands_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_build_command_preserves_patterns_rewrites_and_expands_directory",
        r##"(let* ((root (ast-grep-test-path "work"))
               (default-directory (file-name-as-directory root))
               (ast-grep-executable "sg fixture"))
          (make-directory root t)
          (mapcar
           (lambda (command)
             (mapcar
              (lambda (part)
                (if (string-prefix-p root part)
                    (concat "$ROOT/" (file-relative-name part root))
                  part))
              command))
           (list
            (ast-grep--build-command "console.log($A)")
            (ast-grep--build-command "$A && $A()" "src")
            (ast-grep--build-command
             "old($X)" "src/lib" "new($X)"))))"##,
        expect![[
            r#"OK (("sg fixture" "run" "--pattern=console.log($A)" "--json=stream") ("sg fixture" "run" "--pattern=$A && $A()" "--json=stream" "$ROOT/src") ("sg fixture" "run" "--pattern=old($X)" "--rewrite=new($X)" "--json=stream" "$ROOT/src/lib"))"#
        ]],
    )
}

fn ast_grep_command_string_shell_quotes_hostile_arguments_losslessly() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_command_string_shell_quotes_hostile_arguments_losslessly",
        r##"(let* ((command
                '("ast grep"
                  "run"
                  "--pattern=a b;$(touch nope)"
                  "--rewrite=it's \"$X\""
                  "/work/a dir"))
               (quoted (ast-grep--command-string command)))
          (list
           quoted
           (shell-command-to-string
            (concat
             "set -- "
             quoted
             "; printf '<%s>' \"$@\""))
           (file-exists-p "nope")))"##,
        expect![[
            r#"OK ("ast\\ grep run --pattern\\=a\\ b\\;\\$\\(touch\\ nope\\) --rewrite\\=it\\'s\\ \\\"\\$X\\\" /work/a\\ dir" "<ast grep><run><--pattern=a b;$(touch nope)><--rewrite=it's \"$X\"></work/a dir>" nil)"#
        ]],
    )
}

fn ast_grep_read_file_and_executable_probe_cover_present_missing_and_disabled_tools()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_read_file_and_executable_probe_cover_present_missing_and_disabled_tools",
        r##"(let* ((file (ast-grep-test-write-file
                      "work/utf8.txt"
                      "alpha\nβeta\n"))
               (program
                (ast-grep-test-make-executable
                 "sg-ok"
                 "printf '%s\\n' ok"))
               (ast-grep-executable program)
               (available (ast-grep--executable-available-p)))
          (list
           (ast-grep--read-file file)
           (ast-grep--read-file (ast-grep-test-path "missing"))
           (ast-grep--read-file nil)
           (and available
                (equal (file-truename available)
                       (file-truename program)))
           (let ((ast-grep-executable "certainly-no-such-sg"))
             (ast-grep--executable-available-p))))"##,
        expect![[r#"OK ("alpha\nβeta\n" "" "" t nil)"#]],
    )
}

fn ast_grep_call_runs_real_program_with_exact_argv_cwd_stdout_and_stderr() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_call_runs_real_program_with_exact_argv_cwd_stdout_and_stderr",
        r##"(let* ((work (ast-grep-test-path "project"))
               (log (ast-grep-test-path "argv.log"))
               (program
                (ast-grep-test-make-executable
                 "sg-capture"
                 (format
                  "printf 'cwd=%%s\\n' \"$PWD\" > %s\nprintf 'arg=%%s\\n' \"$@\" >> %s\nprintf 'match-one\\nmatch-two\\n'\nprintf 'diagnostic-only\\n' >&2"
                  (shell-quote-argument log)
                  (shell-quote-argument log))))
               (ast-grep-executable program))
          (make-directory work t)
          (let ((stdout
                 (ast-grep--call
                  (list program "run" "--pattern=a b" "--json=stream" ".")
                  work
                  "integration")))
            (list
             stdout
             (replace-regexp-in-string
              (regexp-quote work)
              "$WORK"
              (ast-grep-test-read-file log))
             (directory-files
              temporary-file-directory
              nil
              "\\`ast-grep-stderr-"
              t))))"##,
        expect![[
            r#"OK ("match-one\nmatch-two\n" "cwd=$WORK\narg=run\narg=--pattern=a b\narg=--json=stream\narg=.\n" nil)"#
        ]],
    )
}

fn ast_grep_call_reports_real_exit_failures_with_both_output_streams() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_call_reports_real_exit_failures_with_both_output_streams",
        r##"(let* ((program
                (ast-grep-test-make-executable
                 "sg-fail"
                 "printf 'partial result\\n'\nprintf 'invalid pattern: $BAD\\n' >&2\nexit 7"))
               (ast-grep-executable program))
          (ast-grep-test-error-data
           (lambda ()
             (ast-grep--call
              (list program "run" "--pattern=$BAD")
              nil
              "search"))))"##,
        expect![[
            r#"OK (:error error ("The ast-grep failed with exit code 7: partial result\n\ninvalid pattern: $BAD"))"#
        ]],
    )
}

fn ast_grep_call_debug_mode_emits_command_directory_streams_and_status() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_call_debug_mode_emits_command_directory_streams_and_status",
        r##"(let* ((work (ast-grep-test-path "debug-work"))
               (program
                (ast-grep-test-make-executable
                 "sg-debug"
                 "printf 'json-output\\n'\nprintf 'warning-output\\n' >&2"))
               (ast-grep-executable program)
               (ast-grep-debug t)
               messages)
          (make-directory work t)
          (cl-letf (((symbol-function 'message)
                     (lambda (format-string &rest args)
                       (push
                        (replace-regexp-in-string
                         (regexp-quote work) "$WORK"
                         (apply #'format format-string args))
                        messages))))
            (list
             (ast-grep--call
              (list program "run" "--pattern=x y")
              work
              "fixture")
             (nreverse messages))))"##,
        expect![[
            r#"OK ("json-output\n" ("Debug: fixture command: [ORACLE-SANDBOX]/bin/sg-debug run --pattern\\=x\\ y" "Debug: Working directory: $WORK" "Debug: fixture stdout: json-output\n" "Debug: fixture stderr: warning-output\n" "Debug: fixture exit code: 0"))"#
        ]],
    )
}

fn ast_grep_run_command_composes_real_search_command_and_parses_stream_workflow() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_run_command_composes_real_search_command_and_parses_stream_workflow",
        r##"(let* ((work (ast-grep-test-path "search-root"))
               (log (ast-grep-test-path "search-argv.log"))
               (program
                (ast-grep-test-make-executable
                 "sg-stream"
                 (format
                  "printf '%%s\\n' \"$@\" > %s\nprintf '%%s\\n' '{\"file\":\"src/app.js\",\"range\":{\"start\":{\"line\":2,\"column\":4}},\"text\":\"console.log(value)\"}'"
                  (shell-quote-argument log))))
               (ast-grep-executable program))
          (make-directory work t)
          (ast-grep--reset-candidate-table)
          (let* ((output (ast-grep--run-command "console.log($A)" work))
                 (candidates (ast-grep--parse-stream-output output)))
            (list
             (ast-grep-test-read-file log)
             (mapcar #'substring-no-properties candidates)
             (mapcar #'ast-grep-test-match-summary candidates)
             (hash-table-count ast-grep--candidate-table))))"##,
        expect![[
            r#"OK ("run\n--pattern=console.log($A)\n--json=stream\n[ORACLE-SANDBOX]/search-root\n" ("src/app.js:3:4:console.log(value)") (("src/app.js" 2 4 nil nil "console.log(value)" nil)) 1)"#
        ]],
    )
}

fn ast_grep_project_directory_and_search_dispatch_real_user_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_project_directory_and_search_dispatch_real_user_workflow",
        r##"(let ((program
                (ast-grep-test-make-executable
                 "sg-present"
                 "exit 0"))
               calls)
          (let ((ast-grep-executable program))
            (cl-letf (((symbol-function 'ast-grep--project-root)
                       (lambda () "/fixture/project/"))
                      ((symbol-function 'ast-grep--select-backend)
                       (lambda () 'sync))
                      ((symbol-function 'ast-grep--run-search-backend)
                       (lambda (backend directory)
                         (push (list backend directory) calls)
                         :searched)))
              (list
               (ast-grep-search "/explicit/")
               (ast-grep-project)
               (ast-grep-directory "~/source")
               (nreverse calls)))))"##,
        expect![[
            r#"OK (:searched :searched :searched ((sync "/explicit/") (sync "/fixture/project/") (sync "~/source")))"#
        ]],
    )
}

fn ast_grep_project_root_uses_project_current_and_project_root_protocol() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_project_root_uses_project_current_and_project_root_protocol",
        r##"(let (calls current)
          (cl-letf (((symbol-function 'project-current)
                     (lambda (&rest args)
                       (push (list :current args) calls)
                       current))
                    ((symbol-function 'project-root)
                     (lambda (project)
                       (push (list :root project) calls)
                       (cdr project))))
            (setq current nil)
            (let ((outside
                   (list
                    (ast-grep--project-root)
                    (nreverse calls))))
              (setq current '(fixture-project . "/fixture/root/")
                    calls nil)
              (list
               outside
               (ast-grep--project-root)
               (nreverse calls)))))"##,
        expect![[
            r#"OK ((nil ((:current nil))) "/fixture/root/" ((:current nil) (:root (fixture-project . "/fixture/root/"))))"#
        ]],
    )
}

fn ast_grep_search_rejects_missing_executable_before_backend_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_search_rejects_missing_executable_before_backend_selection",
        r##"(let ((ast-grep-executable
                "ast-grep-certainly-not-installed")
               selected
               dispatched)
          (cl-letf (((symbol-function 'ast-grep--select-backend)
                     (lambda ()
                       (setq selected t)
                       'sync))
                    ((symbol-function 'ast-grep--run-search-backend)
                     (lambda (&rest _)
                       (setq dispatched t))))
            (list
             (ast-grep-test-error-data
              (lambda ()
                (ast-grep-search "/fixture/")))
             selected
             dispatched)))"##,
        expect![[
            r#"OK ((:error error ("The ast-grep executable not found. Please install ast-grep")) nil nil)"#
        ]],
    )
}

fn ast_grep_project_commands_signal_useful_errors_outside_projects() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_project_commands_signal_useful_errors_outside_projects",
        r##"(cl-letf (((symbol-function 'ast-grep--project-root)
                    (lambda () nil)))
          (list
           (ast-grep-test-error-data #'ast-grep-project)
           (ast-grep-test-error-data #'ast-grep-rewrite-project)))"##,
        expect![[
            r#"OK ((:error error ("Not in a project")) (:error error ("Not in a project")))"#
        ]],
    )
}

fn ast_grep_minor_mode_toggles_buffer_local_state_and_exact_lighter() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_minor_mode_toggles_buffer_local_state_and_exact_lighter",
        r##"(with-temp-buffer
          (let ((initial
                 (list ast-grep-mode
                       (local-variable-p 'ast-grep-mode))))
            (ast-grep-mode 1)
            (let ((enabled
                   (list
                    ast-grep-mode
                    (local-variable-p 'ast-grep-mode)
                    (assq 'ast-grep-mode minor-mode-alist))))
              (ast-grep-mode -1)
              (list
               initial
               enabled
               (list
                ast-grep-mode
                (local-variable-p 'ast-grep-mode))))))"##,
        expect![[r#"OK ((nil nil) (t t (ast-grep-mode " ast-grep")) (nil t))"#]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_build_command_preserves_patterns_rewrites_and_expands_directory(),
        ast_grep_command_string_shell_quotes_hostile_arguments_losslessly(),
        ast_grep_read_file_and_executable_probe_cover_present_missing_and_disabled_tools(),
        ast_grep_call_runs_real_program_with_exact_argv_cwd_stdout_and_stderr(),
        ast_grep_call_reports_real_exit_failures_with_both_output_streams(),
        ast_grep_call_debug_mode_emits_command_directory_streams_and_status(),
        ast_grep_run_command_composes_real_search_command_and_parses_stream_workflow(),
        ast_grep_project_directory_and_search_dispatch_real_user_workflow(),
        ast_grep_project_root_uses_project_current_and_project_root_protocol(),
        ast_grep_search_rejects_missing_executable_before_backend_selection(),
        ast_grep_project_commands_signal_useful_errors_outside_projects(),
        ast_grep_minor_mode_toggles_buffer_local_state_and_exact_lighter(),
    ]
}
