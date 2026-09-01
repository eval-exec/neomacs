use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_completion_parser_preserves_order_duplicates_and_detailed_help()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_completion_parser_preserves_order_duplicates_and_detailed_help",
        r##"(with-temp-buffer
                           (insert
                            "noise before\n"
                            "COMPLETION: alpha : [#int#]alpha(<#int x#>)\n"
                            "COMPLETION: alphabet : [#char *#]alphabet\n"
                            "COMPLETION: alpha : [#double#]alpha(<#double x#>)\n"
                            "COMPLETION: beta : [#void#]beta()\n"
                            "COMPLETION: Pattern : ignored\n")
                           (mapcar
                            #'acclang-test-candidate-summary
                            (ac-clang-parse-output
                             "alp")))"##,
        expect![[
            r#"OK (("alpha" "[#double#]alpha(<#double x#>)" nil) ("alphabet" "[#char *#]alphabet" nil) ("alpha" "[#int#]alpha(<#int x#>)" nil))"#
        ]],
    )
}

fn auto_complete_clang_async_completion_parser_filters_prefix_pattern_colons_and_malformed_lines()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_completion_parser_filters_prefix_pattern_colons_and_malformed_lines",
        r##"(with-temp-buffer
                           (insert
                            "COMPLETION: first\n"
                            "COMPLETION: format : [#int#]format(<#char *fmt#>)\n"
                            "COMPLETION: forward::iterator : detail\n"
                            "COMPLETION: Pattern : pattern\n"
                            " COMPLETION: false-indent : detail\n"
                            "COMPLETION: fork :\n"
                            "garbage\n")
                           (mapcar
                            (lambda (prefix)
                              (goto-char
                               (point-max))
                              (list
                               prefix
                               (mapcar
                                #'acclang-test-candidate-summary
                                (ac-clang-parse-output
                                 prefix))))
                            '(""
                              "f"
                              "for"
                              "format"
                              "missing")))"##,
        expect![[
            r#"OK (("" (("fork" " :" nil) ("forward" "::iterator : detail" nil) ("format" "[#int#]format(<#char *fmt#>)" nil) ("first" "" nil))) ("f" (("fork" " :" nil) ("forward" "::iterator : detail" nil) ("format" "[#int#]format(<#char *fmt#>)" nil) ("first" "" nil))) ("for" (("fork" " :" nil) ("forward" "::iterator : detail" nil) ("format" "[#int#]format(<#char *fmt#>)" nil))) ("format" (("format" "[#int#]format(<#char *fmt#>)" nil))) ("missing" nil))"#
        ]],
    )
}

fn auto_complete_clang_async_document_cleanup_and_candidate_property_lookup_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_document_cleanup_and_candidate_property_lookup_match",
        r##"(let ((candidate
                                (propertize
                                 "format"
                                 'ac-clang-help
                                 "[#int#]format(<#const char *fmt#>, <#...#>)")))
                           (list
                            (ac-clang-clean-document
                             nil)
                            (ac-clang-clean-document
                             "")
                            (ac-clang-clean-document
                             "[#Result#]call(<#one#>, <#two#>)")
                            (ac-clang-document
                             candidate)
                            (ac-clang-document
                             "plain")
                            (ac-clang-document
                             'not-a-string)
                            (get-text-property
                             0
                             'ac-clang-help
                             candidate)))"##,
        expect![[
            r#"OK (nil "" "Result call(one, two)" "int format(const char *fmt, ...)" nil nil "[#int#]format(<#const char *fmt#>, <#...#>)")"#
        ]],
    )
}

fn auto_complete_clang_async_prefix_recognizes_symbols_member_access_and_namespace_access()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_prefix_recognizes_symbols_member_access_and_namespace_access",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (c++-mode)
                               (insert
                                (car fixture))
                               (goto-char
                                (or
                                 (cdr fixture)
                                 (point-max)))
                               (list
                                (car fixture)
                                (point)
                                (ac-clang-prefix)
                                (and
                                 (ac-clang-prefix)
                                 (buffer-substring-no-properties
                                  (ac-clang-prefix)
                                  (point))))))
                           '(("identifier")
                             ("object.")
                             ("pointer->")
                             ("namespace::")
                             ("operator:")
                             ("operator-")
                             ("a..")
                             ("left right")
                             ("member.tail" . 8)))"##,
        expect![[
            r#"OK (("identifier" 11 1 "identifier") ("object." 8 8 "") ("pointer->" 10 10 "") ("namespace::" 12 12 "") ("operator:" 10 nil nil) ("operator-" 10 nil nil) ("a.." 4 4 "") ("left right" 11 6 "right") ("member.tail" 8 8 ""))"#
        ]],
    )
}

fn auto_complete_clang_async_balanced_delimiter_counter_covers_nested_template_and_call_fragments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_balanced_delimiter_counter_covers_nested_template_and_call_fragments",
        r##"(mapcar
                           (lambda (fixture)
                             (list
                              fixture
                              (ac-clang-same-count-in-string
                               ?<
                               ?>
                               fixture)
                              (ac-clang-same-count-in-string
                               ?\(
                               ?\)
                               fixture)))
                           '("plain"
                             "std::vector<int>"
                             "std::map<Key, std::vector<Value>>"
                             "std::vector<int"
                             "call(a, nested(b, c))"
                             "call(a, nested(b, c)"
                             "operator<<"
                             ")(<>"))"##,
        expect![[
            r#"OK (("plain" t t) ("std::vector<int>" t t) ("std::map<Key, std::vector<Value>>" t t) ("std::vector<int" nil t) ("call(a, nested(b, c))" t t) ("call(a, nested(b, c)" t nil) ("operator<<" nil t) (")(<>" t t))"#
        ]],
    )
}

fn auto_complete_clang_async_argument_splitter_keeps_nested_templates_calls_and_function_pointers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_argument_splitter_keeps_nested_templates_calls_and_function_pointers",
        r##"(mapcar
                           (lambda (arguments)
                             (list
                              arguments
                              (ac-clang-split-args
                               arguments)))
                           '("a, b, c"
                             "std::vector<int>, callback(x, y), tail"
                             "std::map<Key, std::vector<Value>>, std::function<void(int, char)>, flags"
                             "(void (*)(int, char)), value"
                             "unbalanced(a, b, tail"
                             ""))"##,
        expect![[
            r#"OK (("a, b, c" ("a" "b" "c")) ("std::vector<int>, callback(x, y), tail" ("std::vector<int>" "callback(x, y)" "tail")) ("std::map<Key, std::vector<Value>>, std::function<void(int, char)>, flags" ("std::map<Key, std::vector<Value>>" "std::function<void(int, char)>" "flags")) ("(void (*)(int, char)), value" ("(void (*)(int, char))" "value")) ("unbalanced(a, b, tail" nil) ("" ("")))"#
        ]],
    )
}

fn auto_complete_clang_async_position_string_tracks_real_lines_columns_and_unicode_characters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_position_string_tracks_real_lines_columns_and_unicode_characters",
        r##"(with-temp-buffer
                           (insert
                            "first\n"
                            "  naïve.member\n"
                            "最後")
                           (mapcar
                            (lambda (position)
                              (list
                               position
                               (ac-clang-create-position-string
                                position)))
                            (list
                             (point-min)
                             6
                             7
                             9
                             14
                             (point-max))))"##,
        expect![[
            r#"OK ((1 "row:1\ncolumn:1\n") (6 "row:1\ncolumn:6\n") (7 "row:2\ncolumn:1\n") (9 "row:2\ncolumn:3\n") (14 "row:2\ncolumn:8\n") (24 "row:3\ncolumn:3\n"))"#
        ]],
    )
}

fn auto_complete_clang_async_string_comment_detection_uses_real_c_and_cpp_syntax_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_string_comment_detection_uses_real_c_and_cpp_syntax_state",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (funcall
                                (car fixture))
                               (insert
                                (cadr fixture))
                               (goto-char
                                (point-min))
                               (search-forward
                                (nth 2 fixture))
                               (list
                                (car fixture)
                                (cadr fixture)
                                (nth 2 fixture)
                                (ac-clang-in-string/comment))))
                           '((c-mode
                              "int value; // comment token"
                              "token")
                             (c-mode
                              "const char *s = \"string token\";"
                              "token")
                             (c++-mode
                              "auto value = object.member;"
                              "member")
                             (c++-mode
                              "/* block token */ int token;"
                              "block")
                             (c++-mode
                              "/* block token */ int token;"
                              "int token")))"##,
        expect![[
            r#"OK ((c-mode "int value; // comment token" "token" 12) (c-mode "const char *s = \"string token\";" "token" 17) (c++-mode "auto value = object.member;" "member" nil) (c++-mode "/* block token */ int token;" "block" 1) (c++-mode "/* block token */ int token;" "int token" nil))"#
        ]],
    )
}

fn auto_complete_clang_async_error_handler_keeps_diagnostics_before_completion_and_normalizes_report()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_error_handler_keeps_diagnostics_before_completion_and_normalizes_report",
        r##"(let ((ac-clang-complete-executable
                                "./clang-complete")
                               (arguments
                                '("-cc1"
                                  "-x"
                                  "c++"
                                  "fixture.cpp")))
                           (when
                               (get-buffer
                                ac-clang-error-buffer-name)
                             (kill-buffer
                              ac-clang-error-buffer-name))
                           (with-temp-buffer
                             (insert
                              "fixture.cpp:3:5: error: missing member\n"
                              "fixture.cpp:4:2: note: candidate\n"
                              "COMPLETION: member : [#int#]member\n")
                             (ac-clang-handle-error
                              9
                              arguments))
                           (unwind-protect
                               (with-current-buffer
                                   ac-clang-error-buffer-name
                                 (list
                                  buffer-read-only
                                  (point)
                                  (cdr
                                   (split-string
                                    (buffer-string)
                                    "\n"))
                                  (string-match-p
                                   "COMPLETION:"
                                   (buffer-string))))
                             (kill-buffer
                              ac-clang-error-buffer-name)))"##,
        expect![[
            r#"OK (t 1 ("clang failed with error 9:" "./clang-complete -cc1 -x c++ fixture.cpp" "" "fixture.cpp:3:5: error: missing member" "fixture.cpp:4:2: note: candidate") nil)"#
        ]],
    )
}

fn auto_complete_clang_async_real_synchronous_process_parses_useful_candidates_on_success_and_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_real_synchronous_process_parses_useful_candidates_on_success_and_failure",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (insert
                                "int main(void) { return 0; }\n")
                               (let ((ac-clang-complete-executable
                                      (or
                                       (executable-find "sh")
                                       shell-file-name)))
                                 (when
                                     (get-buffer
                                      ac-clang-error-buffer-name)
                                   (kill-buffer
                                    ac-clang-error-buffer-name))
                                 (let ((result
                                        (ac-clang-call-process
                                         "fo"
                                         "-c"
                                         (cadr fixture))))
                                   (prog1
                                       (list
                                        (car fixture)
                                        (mapcar
                                         #'acclang-test-candidate-summary
                                         result)
                                        (and
                                         (get-buffer
                                          ac-clang-error-buffer-name)
                                         (with-current-buffer
                                             ac-clang-error-buffer-name
                                           (list
                                            buffer-read-only
                                            (string-match-p
                                             "clang failed with error"
                                             (buffer-string))
                                            (string-match-p
                                             "diagnostic before completion"
                                             (buffer-string))))))
                                     (when
                                         (get-buffer
                                          ac-clang-error-buffer-name)
                                       (kill-buffer
                                        ac-clang-error-buffer-name)))))))
                           '((success
                              "cat >/dev/null; printf 'COMPLETION: format : [#int#]format(<#const char *fmt#>)\\nCOMPLETION: fork : [#void#]fork()\\n'")
                             (failure
                              "cat >/dev/null; printf 'diagnostic before completion\\nCOMPLETION: format : [#int#]format(<#const char *fmt#>)\\n'; exit 7")))"##,
        expect![[
            r#"OK ((success (("fork" "[#void#]fork()" nil) ("format" "[#int#]format(<#const char *fmt#>)" nil)) nil) (failure (("format" "[#int#]format(<#const char *fmt#>)" nil)) (t 25 141)))"#
        ]],
    )
}

pub(super) fn parsing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_completion_parser_preserves_order_duplicates_and_detailed_help(),
        auto_complete_clang_async_completion_parser_filters_prefix_pattern_colons_and_malformed_lines(),
        auto_complete_clang_async_document_cleanup_and_candidate_property_lookup_match(),
        auto_complete_clang_async_prefix_recognizes_symbols_member_access_and_namespace_access(),
        auto_complete_clang_async_balanced_delimiter_counter_covers_nested_template_and_call_fragments(),
        auto_complete_clang_async_argument_splitter_keeps_nested_templates_calls_and_function_pointers(),
        auto_complete_clang_async_position_string_tracks_real_lines_columns_and_unicode_characters(),
        auto_complete_clang_async_string_comment_detection_uses_real_c_and_cpp_syntax_state(),
        auto_complete_clang_async_error_handler_keeps_diagnostics_before_completion_and_normalizes_report(),
        auto_complete_clang_async_real_synchronous_process_parses_useful_candidates_on_success_and_failure(),
    ]
}
