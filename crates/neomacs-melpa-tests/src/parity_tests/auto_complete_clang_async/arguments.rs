use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_language_option_covers_c_cpp_objc_extensions_and_fallback_modes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_language_option_covers_c_cpp_objc_extensions_and_fallback_modes",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (funcall
                                (car fixture))
                               (setq
                                buffer-file-name
                                (cadr fixture))
                               (list
                                (car fixture)
                                (cadr fixture)
                                (acclang-test-error
                                 (lambda ()
                                   (ac-clang-lang-option))))))
                           '((c-mode "fixture.c")
                             (c++-mode "fixture.cc")
                             (objc-mode "fixture.m")
                             (objc-mode "fixture.mm")
                             (objc-mode nil)
                             (fundamental-mode "fixture.txt")))"##,
        expect![[
            r#"OK ((c-mode "fixture.c" (:value "c")) (c++-mode "fixture.cc" (:value "c++")) (objc-mode "fixture.m" (:value "objective-c")) (objc-mode "fixture.mm" (:value "objective-c++")) (objc-mode nil (:signal wrong-type-argument (stringp nil))) (fundamental-mode "fixture.txt" (:value "c++")))"#
        ]],
    )
}

fn auto_complete_clang_async_custom_language_function_overrides_mode_and_runs_once_per_query()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_custom_language_function_overrides_mode_and_runs_once_per_query",
        r##"(with-temp-buffer
                           (c-mode)
                           (let* ((calls 0)
                                  (ac-clang-lang-option-function
                                   (lambda ()
                                     (setq calls
                                           (1+ calls))
                                     "cuda")))
                             (list
                              (ac-clang-lang-option)
                              calls
                              (ac-clang-build-complete-args)
                              calls)))"##,
        expect![[r#"OK ("cuda" 1 ("-cc1" "-fsyntax-only" "-x" "cuda") 2)"#]],
    )
}

fn auto_complete_clang_async_complete_arguments_preserve_flags_duplicates_spaces_and_prefix_header()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_complete_arguments_preserve_flags_duplicates_spaces_and_prefix_header",
        r##"(with-temp-buffer
                           (c++-mode)
                           (let ((default-directory
                                  (expand-file-name
                                   "./tmp/auto-complete-clang-async/args/"))
                                 (ac-clang-cflags
                                  '("-Iinclude"
                                    "-DNAME=value with space"
                                    "-Wall"
                                    "-Iinclude"))
                                 (ac-clang-prefix-header
                                  "../headers/prefix.pch"))
                             (list
                              (ac-clang-build-complete-args)
                              ac-clang-cflags
                              ac-clang-prefix-header)))"##,
        expect![[
            r#"OK (("-cc1" "-fsyntax-only" "-x" "c++" "-Iinclude" "-DNAME=value with space" "-Wall" "-Iinclude" "-include-pch" "[ORACLE-TMPDIR]/auto-complete-clang-async/headers/prefix.pch") ("-Iinclude" "-DNAME=value with space" "-Wall" "-Iinclude") "../headers/prefix.pch")"#
        ]],
    )
}

fn auto_complete_clang_async_complete_arguments_omit_non_string_prefix_header_and_handle_nil_flags()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_complete_arguments_omit_non_string_prefix_header_and_handle_nil_flags",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (c-mode)
                               (let ((ac-clang-cflags
                                      (car fixture))
                                     (ac-clang-prefix-header
                                      (cdr fixture)))
                                 (list
                                  fixture
                                  (ac-clang-build-complete-args)))))
                           '((nil)
                             (nil . 42)
                             (("-pedantic") . nil)
                             (("-std=c11") . "")))"##,
        expect![[
            r#"OK (((nil) ("-cc1" "-fsyntax-only" "-x" "c")) ((nil . 42) ("-cc1" "-fsyntax-only" "-x" "c")) ((("-pedantic")) ("-cc1" "-fsyntax-only" "-x" "c" "-pedantic")) ((("-std=c11") . "") ("-cc1" "-fsyntax-only" "-x" "c" "-std=c11" "-include-pch" "[ORACLE-SANDBOX]")))"#
        ]],
    )
}

fn auto_complete_clang_async_prefix_header_setter_distinguishes_empty_whitespace_and_real_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_prefix_header_setter_distinguishes_empty_whitespace_and_real_paths",
        r##"(with-temp-buffer
                           (let ((ac-clang-prefix-header
                                  "before.pch"))
                             (mapcar
                              (lambda (value)
                                (list
                                 value
                                 (ac-clang-set-prefix-header
                                  value)
                                 ac-clang-prefix-header))
                              '(""
                                " "
                                "\t"
                                "  \t  "
                                "./prefix.pch"
                                "with space.pch"))))"##,
        expect![[
            r#"OK (("" nil nil) (" " nil nil) ("\11" nil nil) ("  \11  " nil nil) ("./prefix.pch" "./prefix.pch" "./prefix.pch") ("with space.pch" "with space.pch" "with space.pch"))"#
        ]],
    )
}

fn auto_complete_clang_async_interactive_cflags_setter_splits_input_and_sends_exact_update_once()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_interactive_cflags_setter_splits_input_and_sends_exact_update_once",
        r##"(with-temp-buffer
                           (let ((ac-clang-cflags
                                  '("-Iold"))
                                 calls)
                             (cl-letf
                                 (((symbol-function
                                    'read-string)
                                   (lambda (&rest arguments)
                                     (push
                                      (cons
                                       :read
                                       arguments)
                                      calls)
                                     "-Iinclude -DDEBUG=1  -Wall"))
                                  ((symbol-function
                                    'ac-clang-update-cmdlineargs)
                                   (lambda ()
                                     (push
                                      (list
                                       :update
                                       ac-clang-cflags)
                                      calls)
                                     :updated)))
                               (list
                                (call-interactively
                                 #'ac-clang-set-cflags)
                                ac-clang-cflags
                                (nreverse calls)))))"##,
        expect![[
            r#"OK (:updated #1=("-Iinclude" "-DDEBUG=1" "-Wall") ((:read "New cflags: ") (:update #1#)))"#
        ]],
    )
}

fn auto_complete_clang_async_shell_cflags_setter_uses_file_context_splits_output_and_updates_once()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_shell_cflags_setter_uses_file_context_splits_output_and_updates_once",
        r##"(with-temp-buffer
                           (setq
                            buffer-file-name
                            (expand-file-name
                             "./tmp/auto-complete-clang-async/project/source.cpp"))
                           (let ((default-directory
                                  (expand-file-name
                                   "./tmp/auto-complete-clang-async/project/"))
                                 calls)
                             (cl-letf
                                 (((symbol-function
                                    'read-shell-command)
                                   (lambda (&rest arguments)
                                     (push
                                      (cons
                                       :read
                                       arguments)
                                      calls)
                                     "./flags.sh"))
                                  ((symbol-function
                                    'shell-command-to-string)
                                   (lambda (command)
                                     (push
                                      (list
                                       :shell
                                       command)
                                      calls)
                                     "-Iinc\n-DVALUE=7\t-Wextra\n"))
                                  ((symbol-function
                                    'ac-clang-update-cmdlineargs)
                                   (lambda ()
                                     (push
                                      (list
                                       :update
                                       ac-clang-cflags)
                                      calls)
                                     :updated)))
                               (list
                                (call-interactively
                                 #'ac-clang-set-cflags-from-shell-command)
                                ac-clang-cflags
                                (nreverse calls)))))"##,
        expect![[
            r#"OK (:updated #1=("-Iinc" "-DVALUE=7" "-Wextra") ((:read "Shell command: " nil nil "source.cpp") (:shell "./flags.sh") (:update #1#)))"#
        ]],
    )
}

fn auto_complete_clang_async_buffer_local_flags_prefix_header_and_process_state_remain_isolated()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_buffer_local_flags_prefix_header_and_process_state_remain_isolated",
        r##"(let ((first
                                (generate-new-buffer
                                 " *acclang-config-first*"))
                               (second
                                (generate-new-buffer
                                 " *acclang-config-second*")))
                           (unwind-protect
                               (progn
                                 (with-current-buffer first
                                   (setq
                                    ac-clang-cflags
                                    '("-DFIRST")
                                    ac-clang-prefix-header
                                    "first.pch"
                                    ac-clang-status
                                    'wait
                                    ac-clang-current-candidate
                                    '("first")))
                                 (with-current-buffer second
                                   (setq
                                    ac-clang-cflags
                                    '("-DSECOND")
                                    ac-clang-prefix-header
                                    "second.pch"
                                    ac-clang-status
                                    'acknowledged
                                    ac-clang-current-candidate
                                    '("second")))
                                 (mapcar
                                  (lambda (buffer)
                                    (with-current-buffer buffer
                                      (list
                                       (buffer-name)
                                       ac-clang-cflags
                                       ac-clang-prefix-header
                                       ac-clang-status
                                       ac-clang-current-candidate
                                       (mapcar
                                        #'local-variable-p
                                        '(ac-clang-cflags
                                          ac-clang-prefix-header
                                          ac-clang-status
                                          ac-clang-current-candidate
                                          ac-clang-completion-process)))))
                                  (list first second)))
                             (kill-buffer first)
                             (kill-buffer second)))"##,
        expect![[
            r#"OK ((" *acclang-config-first*" ("-DFIRST") "first.pch" wait ("first") (t t t t nil)) (" *acclang-config-second*" ("-DSECOND") "second.pch" acknowledged ("second") (t t t t nil)))"#
        ]],
    )
}

pub(super) fn arguments_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_language_option_covers_c_cpp_objc_extensions_and_fallback_modes(),
        auto_complete_clang_async_custom_language_function_overrides_mode_and_runs_once_per_query(),
        auto_complete_clang_async_complete_arguments_preserve_flags_duplicates_spaces_and_prefix_header(),
        auto_complete_clang_async_complete_arguments_omit_non_string_prefix_header_and_handle_nil_flags(),
        auto_complete_clang_async_prefix_header_setter_distinguishes_empty_whitespace_and_real_paths(),
        auto_complete_clang_async_interactive_cflags_setter_splits_input_and_sends_exact_update_once(),
        auto_complete_clang_async_shell_cflags_setter_uses_file_context_splits_output_and_updates_once(),
        auto_complete_clang_async_buffer_local_flags_prefix_header_and_process_state_remain_isolated(),
    ]
}
