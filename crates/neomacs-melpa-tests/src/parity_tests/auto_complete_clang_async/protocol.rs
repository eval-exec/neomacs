use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_source_protocol_widens_and_uses_unibyte_length_for_unicode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_source_protocol_widens_and_uses_unibyte_length_for_unicode",
        r##"(with-temp-buffer
                           (insert
                            "prefix\n"
                            "int naïve = 1;\n"
                            "suffix")
                           (narrow-to-region
                            8
                            22)
                           (let (chunks)
                             (cl-letf
                                 (((symbol-function
                                    'process-send-string)
                                   (lambda (process string)
                                     (push
                                      (list
                                       process
                                       string
                                       (string-bytes string)
                                       (length string))
                                      chunks))))
                               (ac-clang-send-source-code
                                'fixture-process)
                               (list
                                (point-min)
                                (point-max)
                                (nreverse chunks)))))"##,
        expect![[
            r#"OK (8 22 ((fixture-process "source_length:29\n" 17 17) (fixture-process "prefix\nint naïve = 1;\nsuffix" 29 28) (fixture-process "\n\n" 2 2)))"#
        ]],
    )
}

fn auto_complete_clang_async_reparse_protocol_sends_source_and_command_only_to_running_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_reparse_protocol_sends_source_and_command_only_to_running_process",
        r##"(mapcar
                           (lambda (status)
                             (with-temp-buffer
                               (insert
                                "int value;\n")
                               (let (chunks)
                                 (cl-letf
                                     (((symbol-function
                                        'process-status)
                                       (lambda (_process)
                                         status))
                                      ((symbol-function
                                        'process-send-string)
                                       (lambda (_process string)
                                         (push string chunks))))
                                   (list
                                    status
                                    (ac-clang-send-reparse-request
                                     'fixture-process)
                                    (nreverse chunks))))))
                           '(run
                             stop
                             exit
                             signal))"##,
        expect![[
            r#"OK ((run #1=("REPARSE\n\n") ("SOURCEFILE\n" "source_length:11\n" "int value;\n" "\n\n" . #1#)) (stop nil nil) (exit nil nil) (signal nil nil))"#
        ]],
    )
}

fn auto_complete_clang_async_completion_protocol_sends_position_prefix_adjustment_and_full_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_completion_protocol_sends_position_prefix_adjustment_and_full_source",
        r##"(with-temp-buffer
                           (insert
                            "int main() {\n"
                            "  object.member\n"
                            "}\n")
                           (goto-char
                            (point-min))
                           (search-forward
                            "member")
                           (let ((ac-prefix
                                  "mem")
                                 chunks)
                             (cl-letf
                                 (((symbol-function
                                    'process-send-string)
                                   (lambda (_process string)
                                     (push string chunks))))
                               (ac-clang-send-completion-request
                                'fixture-process)
                               (list
                                (point)
                                ac-prefix
                                (nreverse chunks)))))"##,
        expect![[
            r#"OK (29 "mem" ("COMPLETION\n" "row:2\ncolumn:13\n" "source_length:31\n" "int main() {\n  object.member\n}\n" "\n\n"))"#
        ]],
    )
}

fn auto_complete_clang_async_syntaxcheck_protocol_sends_command_then_exact_full_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_syntaxcheck_protocol_sends_command_then_exact_full_source",
        r##"(with-temp-buffer
                           (insert
                            "int broken = ;\n")
                           (let (chunks)
                             (cl-letf
                                 (((symbol-function
                                    'process-send-string)
                                   (lambda (_process string)
                                     (push string chunks))))
                               (ac-clang-send-syntaxcheck-request
                                'fixture-process)
                               (nreverse chunks))))"##,
        expect![[r#"OK ("SYNTAXCHECK\n" "source_length:15\n" "int broken = ;\n" "\n\n")"#]],
    )
}

fn auto_complete_clang_async_cmdline_protocol_serializes_real_built_arguments_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_cmdline_protocol_serializes_real_built_arguments_in_order",
        r##"(with-temp-buffer
                           (c++-mode)
                           (let ((ac-clang-cflags
                                  '("-Iinclude"
                                    "-DVALUE=two words"
                                    "-Iinclude"))
                                 (ac-clang-prefix-header
                                  "./tmp/headers/prefix.pch")
                                 chunks)
                             (cl-letf
                                 (((symbol-function
                                    'process-send-string)
                                   (lambda (process string)
                                     (push
                                      (cons process string)
                                      chunks))))
                               (ac-clang-send-cmdline-args
                                'fixture-process)
                               (list
                                (ac-clang-build-complete-args)
                                (nreverse chunks)))))"##,
        expect![[
            r#"OK (("-cc1" "-fsyntax-only" "-x" "c++" "-Iinclude" "-DVALUE=two words" "-Iinclude" "-include-pch" "[ORACLE-TMPDIR]/headers/prefix.pch") ((fixture-process . "CMDLINEARGS\n") (fixture-process . "num_args:9\n") (fixture-process . "-cc1 ") (fixture-process . "-fsyntax-only ") (fixture-process . "-x ") (fixture-process . "c++ ") (fixture-process . "-Iinclude ") (fixture-process . "-DVALUE=two words ") (fixture-process . "-Iinclude ") (fixture-process . "-include-pch ") (fixture-process . "[ORACLE-TMPDIR]/headers/prefix.pch ") (fixture-process . "\n")))"#
        ]],
    )
}

fn auto_complete_clang_async_update_cmdline_accepts_lists_and_reports_non_lists_without_sending()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_update_cmdline_accepts_lists_and_reports_non_lists_without_sending",
        r##"(mapcar
                           (lambda (value)
                             (with-temp-buffer
                               (let ((ac-clang-cflags
                                      value)
                                     (ac-clang-completion-process
                                      'fixture-process)
                                     chunks
                                     messages)
                                 (cl-letf
                                     (((symbol-function
                                        'process-send-string)
                                       (lambda (_process string)
                                         (push string chunks)))
                                      ((symbol-function
                                        'message)
                                       (lambda (format-string
                                                &rest arguments)
                                         (push
                                          (apply
                                           #'format
                                           format-string
                                           arguments)
                                          messages))))
                                   (list
                                    value
                                    (ac-clang-update-cmdlineargs)
                                    (nreverse chunks)
                                    (nreverse messages))))))
                           '(nil
                             ("-Wall")
                             "-Wall"
                             42))"##,
        expect![[
            r#"OK ((nil #1=("\n") ("CMDLINEARGS\n" "num_args:4\n" "-cc1 " "-fsyntax-only " "-x " "c++ " . #1#) nil) (("-Wall") #2=("\n") ("CMDLINEARGS\n" "num_args:5\n" "-cc1 " "-fsyntax-only " "-x " "c++ " "-Wall " . #2#) nil) ("-Wall" #3=("`ac-clang-cflags' should be a list of strings") nil #3#) (42 #4=("`ac-clang-cflags' should be a list of strings") nil #4#))"#
        ]],
    )
}

fn auto_complete_clang_async_shutdown_protocol_sends_only_for_running_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_shutdown_protocol_sends_only_for_running_process",
        r##"(mapcar
                           (lambda (status)
                             (let (chunks)
                               (cl-letf
                                   (((symbol-function
                                      'process-status)
                                     (lambda (_process)
                                       status))
                                    ((symbol-function
                                      'process-send-string)
                                     (lambda (process string)
                                       (push
                                        (list process string)
                                        chunks))))
                                 (list
                                  status
                                  (ac-clang-send-shutdown-command
                                   'fixture-process)
                                  (nreverse chunks)))))
                           '(run
                             stop
                             exit
                             signal))"##,
        expect![[
            r#"OK ((run #1=((fixture-process "SHUTDOWN\n")) #1#) (stop nil nil) (exit nil nil) (signal nil nil))"#
        ]],
    )
}

fn auto_complete_clang_async_append_output_advances_real_process_marker_without_moving_user_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_append_output_advances_real_process_marker_without_moving_user_point",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-append"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair)))
                           (unwind-protect
                               (with-current-buffer buffer
                                 (insert "before|after")
                                 (set-marker
                                  (process-mark process)
                                  8)
                                 (goto-char
                                  (point-min))
                                 (let ((before-point
                                        (point)))
                                   (ac-clang-append-process-output-to-process-buffer
                                    process
                                    "OUTPUT")
                                   (list
                                    before-point
                                    (point)
                                    (marker-position
                                     (process-mark process))
                                    (buffer-string))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[r#"OK (1 14 14 "before|OUTPUTafter")"#]],
    )
}

fn auto_complete_clang_async_parse_completion_results_reads_real_process_buffer_with_saved_prefix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_parse_completion_results_reads_real_process_buffer_with_saved_prefix",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-parse-buffer"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair)))
                           (unwind-protect
                               (progn
                                 (with-current-buffer buffer
                                   (insert
                                    "COMPLETION: object : [#Type#]object\n"
                                    "COMPLETION: observe : [#void#]observe()\n"
                                    "COMPLETION: other : [#int#]other\n"))
                                 (let ((ac-clang-saved-prefix
                                        "ob"))
                                   (mapcar
                                    #'acclang-test-candidate-summary
                                    (ac-clang-parse-completion-results
                                     process))))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[r#"OK (("observe" "[#void#]observe()" nil) ("object" "[#Type#]object" nil))"#]],
    )
}

pub(super) fn protocol_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_source_protocol_widens_and_uses_unibyte_length_for_unicode(),
        auto_complete_clang_async_reparse_protocol_sends_source_and_command_only_to_running_process(),
        auto_complete_clang_async_completion_protocol_sends_position_prefix_adjustment_and_full_source(),
        auto_complete_clang_async_syntaxcheck_protocol_sends_command_then_exact_full_source(),
        auto_complete_clang_async_cmdline_protocol_serializes_real_built_arguments_in_order(),
        auto_complete_clang_async_update_cmdline_accepts_lists_and_reports_non_lists_without_sending(),
        auto_complete_clang_async_shutdown_protocol_sends_only_for_running_process(),
        auto_complete_clang_async_append_output_advances_real_process_marker_without_moving_user_point(),
        auto_complete_clang_async_parse_completion_results_reads_real_process_buffer_with_saved_prefix(),
    ]
}
