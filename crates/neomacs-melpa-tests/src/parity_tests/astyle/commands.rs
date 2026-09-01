use super::ParityBatchCase;
use expect_test::expect;

fn region_command_delegates_exact_program_arguments_io_policy_and_success_predicate()
-> ParityBatchCase {
    ParityBatchCase::value(
        "region_command_delegates_exact_program_arguments_io_policy_and_success_predicate",
        r##"
(let* ((input
        (astyle-test-path
         "delegation/input.c"))
       captured)
  (make-directory
   (file-name-directory input)
   t)
  (with-temp-file input
    (insert "stale"))
  (with-temp-buffer
    (insert
     "before\nint main(){\nreturn 0;\n}\nafter\n")
    (setq buffer-file-name
          (astyle-test-path
           "delegation/source.c")
          c-basic-offset 2
          astyle-style "google"
          astyle-indent 4
          astyle-custom-args
          '("--suffix=none"))
    (cl-letf
        (((symbol-function
           'reformatter--make-temp-file)
          (lambda (name)
            (setq captured
                  (list
                   :temporary-name
                   name))
            input))
         ((symbol-function
           'reformatter--do-region)
          (lambda
              (name beg end program args
                    stdin stdout input-file
                    exit-code-success-p
                    display-errors
                    &optional working-directory)
            (setq captured
                  (append
                   captured
                   (list
                    :call
                    name beg end
                    program args
                    stdin stdout
                    (file-name-nondirectory
                     input-file)
                    (funcall
                     exit-code-success-p
                     0)
                    (funcall
                     exit-code-success-p
                     1)
                    display-errors
                    working-directory)))
            :formatted)))
      (let ((result
             (astyle-region
              8 33 t)))
        (list
         result
         captured
         (file-exists-p input)
         (buffer-string))))))
"##,
        expect![[
            r#"OK (:formatted (:temporary-name astyle :call astyle 8 33 "astyle" ("--style=google" "--indent=spaces=4" "--suffix=none") t t "input.c" t nil t nil) nil "before\nint main(){\nreturn 0;\n}\nafter\n")"#
        ]],
    )
}

fn buffer_command_reports_progress_and_delegates_accessible_bounds_and_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_command_reports_progress_and_delegates_accessible_bounds_and_prefix",
        r##"
(with-temp-buffer
  (insert
   "hidden\nvisible one\nvisible two\nhidden end")
  (narrow-to-region 8 31)
  (let (calls)
    (cl-letf
        (((symbol-function
           'astyle-region)
          (lambda (beg end
                   &optional display-errors)
            (push
             (list
              beg end
              display-errors
              (buffer-substring-no-properties
               beg end))
             calls)
            :region-result)))
      (let ((result
             (astyle-buffer 9))
            (message
             (current-message)))
        (list
         result
         message
         calls
         (point-min)
         (point-max))))))
"##,
        expect![[r#"OK (:region-result nil ((8 31 9 "visible one\nvisible two")) 8 31)"#]],
    )
}

fn real_buffer_formatting_runs_sandbox_executable_replaces_content_and_records_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_buffer_formatting_runs_sandbox_executable_replaces_content_and_records_arguments",
        r##"
(let* ((installation
        (astyle-test-install-formatter))
       (argument-log
        (cadr installation)))
  (unwind-protect
      (with-temp-buffer
        (insert
         "int main(){\nreturn 0;\n}\n")
        (setq buffer-file-name
              (astyle-test-path
               "buffer/source.c")
              default-directory
              (file-name-as-directory
               (astyle-test-path
                "buffer"))
              c-basic-offset 4
              astyle-style "google"
              astyle-custom-args
              '("--suffix=none"))
        (make-directory
         default-directory t)
        (let ((marker
               (copy-marker 12)))
          (list
           (astyle-buffer)
           (buffer-string)
           (marker-position marker)
           (astyle-test-read-file
            argument-log)
           (with-current-buffer
               (get-buffer
                "*astyle errors*")
             (list
              major-mode
              (buffer-string))))))
    (astyle-test-kill-error-buffer)))
"##,
        expect![[
            r#"OK (nil "int main() {\n    return 0;\n}\n" 11 "--style=google\n--indent=spaces=4\n--suffix=none\n" (special-mode ""))"#
        ]],
    )
}

fn real_region_formatting_changes_only_selected_c_function_and_preserves_surroundings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_region_formatting_changes_only_selected_c_function_and_preserves_surroundings",
        r##"
(let* ((installation
        (astyle-test-install-formatter))
       (argument-log
        (cadr installation)))
  (unwind-protect
      (with-temp-buffer
        (insert
         "prefix\nint main(){\nreturn 0;\n}\nsuffix\n")
        (setq buffer-file-name
              (astyle-test-path
               "region/source.cpp")
              default-directory
              (file-name-as-directory
               (astyle-test-path
                "region"))
              c-basic-offset 2
              astyle-style "linux"
              astyle-indent 2
              astyle-custom-args nil)
        (make-directory
         default-directory t)
        (goto-char (point-min))
        (search-forward
         "int main(){")
        (let ((beg
               (line-beginning-position)))
          (search-forward
           "}\n")
          (let ((end (point)))
            (list
             (astyle-region
              beg end)
             (buffer-string)
             (astyle-test-read-file
              argument-log)
             (list beg end)))))
    (astyle-test-kill-error-buffer)))
"##,
        expect![[
            r#"OK (nil "prefix\nint main() {\n    return 0;\n}\nsuffix\n" "--style=linux\n--indent=spaces=2\n--pad-oper\n--pad-header\n--break-blocks\n--delete-empty-lines\n--align-pointer=type\n--align-reference=name\n" (8 32))"#
        ]],
    )
}

fn project_rc_argument_is_passed_to_real_formatter_without_default_flags() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_rc_argument_is_passed_to_real_formatter_without_default_flags",
        r##"
(let* ((installation
        (astyle-test-install-formatter))
       (argument-log
        (cadr installation))
       (project
        (file-name-as-directory
         (astyle-test-path
          "rc-project")))
       (source-directory
        (expand-file-name
         "src/"
         project))
       (configuration
        (expand-file-name
         ".astylerc"
         project)))
  (make-directory source-directory t)
  (with-temp-file configuration
    (insert
     "style=allman\n"))
  (unwind-protect
      (with-temp-buffer
        (insert
         "int main(){\nreturn 0;\n}\n")
        (setq buffer-file-name
              (expand-file-name
               "main.c"
               source-directory)
              default-directory
              source-directory
              c-basic-offset 4
              astyle-style "google"
              astyle-custom-args
              '("--should-not-appear"))
        (list
         (astyle-buffer)
         (buffer-string)
         (astyle-test-read-file
          argument-log)
         (file-truename
          configuration)))
    (astyle-test-kill-error-buffer)))
"##,
        expect![[
            r#"OK (nil "int main() {\n    return 0;\n}\n" "--options=[ORACLE-SANDBOX]/rc-project/.astylerc\n" "[ORACLE-SANDBOX]/rc-project/.astylerc")"#
        ]],
    )
    .fresh_process()
}

fn formatter_failure_preserves_buffer_decodes_ansi_stderr_and_honors_display_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "formatter_failure_preserves_buffer_decodes_ansi_stderr_and_honors_display_errors",
        r##"
(let ((installation
       (astyle-test-install-formatter))
      displays)
  (setenv
   "ASTYLE_TEST_FAIL"
   "1")
  (unwind-protect
      (with-temp-buffer
        (insert
         "int main(){\nreturn 0;\n}\n")
        (setq buffer-file-name
              (astyle-test-path
               "failure/source.c")
              default-directory
              (file-name-as-directory
               (astyle-test-path
                "failure"))
              c-basic-offset 4)
        (make-directory
         default-directory t)
        (cl-letf
            (((symbol-function
               'display-buffer)
              (lambda (buffer &rest args)
                (push
                 (list
                  (buffer-name buffer)
                  args)
                 displays)
                nil)))
          (let ((result
                 (astyle-buffer t))
                (message
                 (current-message)))
            (list
             result
             (buffer-string)
             message
             (nreverse displays)
             (with-current-buffer
                 (get-buffer
                  "*astyle errors*")
               (list
                major-mode
                buffer-read-only
                (substring-no-properties
                 (buffer-string))))
             (mapcar
              #'file-exists-p
              installation)))))
    (setenv
     "ASTYLE_TEST_FAIL"
     nil)
    (astyle-test-kill-error-buffer)))
"##,
        expect![[
            r#"OK (nil "int main(){\nreturn 0;\n}\n" nil (("*astyle errors*" nil)) (special-mode t "fixture formatter failed\n") (t t))"#
        ]],
    )
}

fn missing_formatter_program_preserves_content_and_reports_launch_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_formatter_program_preserves_content_and_reports_launch_failure",
        r##"
(unwind-protect
    (with-temp-buffer
      (insert
       "int main(){\nreturn 0;\n}\n")
      (setq buffer-file-name
            (astyle-test-path
             "missing/source.c")
            default-directory
            (file-name-as-directory
             (astyle-test-path
              "missing"))
            c-basic-offset 4
            exec-path nil)
      (make-directory
       default-directory t)
      (let ((result
             (astyle-buffer)))
        (list
         result
         (buffer-string)
         (current-message)
         (with-current-buffer
             (get-buffer
              "*astyle errors*")
           (let ((text
                  (substring-no-properties
                   (buffer-string))))
             (list
              major-mode
              (string-match-p
               "Searching for program"
               text)
              (string-match-p
               "astyle"
               text)))))))
  (astyle-test-kill-error-buffer))
"##,
        expect![[
            r#"OK ("astyle failed: see *astyle errors*" "int main(){\nreturn 0;\n}\n" nil (special-mode 0 50))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        region_command_delegates_exact_program_arguments_io_policy_and_success_predicate(),
        buffer_command_reports_progress_and_delegates_accessible_bounds_and_prefix(),
        real_buffer_formatting_runs_sandbox_executable_replaces_content_and_records_arguments(),
        real_region_formatting_changes_only_selected_c_function_and_preserves_surroundings(),
        project_rc_argument_is_passed_to_real_formatter_without_default_flags(),
        formatter_failure_preserves_buffer_decodes_ansi_stderr_and_honors_display_errors(),
        missing_formatter_program_preserves_content_and_reports_launch_failure(),
    ]
}
