use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_template_source_returns_buffer_local_candidates_and_start_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_template_source_returns_buffer_local_candidates_and_start_point",
        r##"(with-temp-buffer
                           (insert "call")
                           (let ((ac-clang-template-candidates
                                  '("(<#int x#>)"
                                    "(<#double x#>)"))
                                 (ac-clang-template-start-point
                                  3))
                             (list
                              (ac-clang-template-candidate)
                              (ac-clang-template-prefix)
                              ac-source-clang-template
                              (funcall
                               (cdr
                                (assq
                                 'candidates
                                 ac-source-clang-template)))
                              (funcall
                               (cdr
                                (assq
                                 'prefix
                                 ac-source-clang-template))))))"##,
        expect![[
            r#"OK (#1=("(<#int x#>)" "(<#double x#>)") 3 ((candidates . ac-clang-template-candidate) (prefix . ac-clang-template-prefix) (requires . 0) (action . ac-clang-template-action) (document . ac-clang-document) (cache) (symbol . "t")) #1# 3)"#
        ]],
    )
}

fn auto_complete_clang_async_action_builds_overload_template_candidates_with_help_and_raw_args()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_action_builds_overload_template_candidates_with_help_and_raw_args",
        r##"(with-temp-buffer
                           (insert "obj.method")
                           (goto-char
                            (point-max))
                           (let* ((item
                                   (propertize
                                    "method"
                                    'ac-clang-help
                                    (concat
                                     "[#int#]method(<#int x#>)\n"
                                     "[#double#]method(<#double x#>)")))
                                  (ac-last-completion
                                   (cons nil item))
                                  captured)
                             (cl-letf
                                 (((symbol-function
                                    'ac-complete-clang-template)
                                   (lambda ()
                                     (setq captured
                                           (list
                                            ac-clang-template-start-point
                                            (mapcar
                                             #'acclang-test-candidate-summary
                                             ac-clang-template-candidates)))
                                     :started)))
                               (list
                                (ac-clang-action)
                                captured
                                (point)
                                (buffer-string)))))"##,
        expect![[
            r#"OK (nil (11 (("(int x)" "int" "(<#int x#>)") ("(double x)" "double" "(<#double x#>)"))) 11 "obj.method")"#
        ]],
    )
}

fn auto_complete_clang_async_action_adds_optional_and_variadic_short_forms_without_duplicates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_action_adds_optional_and_variadic_short_forms_without_duplicates",
        r##"(with-temp-buffer
                           (insert "format")
                           (goto-char
                            (point-max))
                           (let* ((item
                                   (propertize
                                    "format"
                                    'ac-clang-help
                                    (concat
                                     "[#int#]format(<#const char *fmt#>, <#...#>)\n"
                                     "[#void#]configure(<#int required#>{#, <#int optional#>#})\n"
                                     "[#int#]format(<#const char *fmt#>, <#...#>)")))
                                  (ac-last-completion
                                   (cons nil item))
                                  captured)
                             (cl-letf
                                 (((symbol-function
                                    'ac-complete-clang-template)
                                   (lambda ()
                                     (setq captured
                                           (mapcar
                                            #'acclang-test-candidate-summary
                                            ac-clang-template-candidates))
                                     :started)))
                               (list
                                (ac-clang-action)
                                captured
                                (length captured)))))"##,
        expect![[
            r#"OK (nil (("(int required{#, int optional#})" "void" "(<#int required#>{#, <#int optional#>#})") ("(int required)" "void" "(<#int required#>)") ("(const char *fmt, ...)" "int" "(<#const char *fmt#>, <#...#>)")) 3)"#
        ]],
    )
}

fn auto_complete_clang_async_action_single_template_starts_completion_and_reports_clean_help()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_action_single_template_starts_completion_and_reports_clean_help",
        r##"(with-temp-buffer
                           (insert "value")
                           (goto-char
                            (point-max))
                           (let* ((item
                                   (propertize
                                    "value"
                                    'ac-clang-help
                                    "[#Widget#]value : const Widget"))
                                  (ac-last-completion
                                   (cons nil item))
                                  calls
                                  messages)
                             (cl-letf
                                 (((symbol-function
                                    'ac-complete-clang-template)
                                   (lambda ()
                                     (push
                                      (mapcar
                                       #'acclang-test-candidate-summary
                                       ac-clang-template-candidates)
                                      calls)
                                     :started))
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
                                (ac-clang-action)
                                (nreverse calls)
                                (nreverse messages)))))"##,
        expect![[
            r#"OK (#1=("Widget value : const Widget") (((": const Widget" "Widget" ": const Widget"))) #1#)"#
        ]],
    )
}

fn auto_complete_clang_async_action_without_template_candidates_only_reports_documentation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_action_without_template_candidates_only_reports_documentation",
        r##"(let* ((item
                                 (propertize
                                  "constant"
                                  'ac-clang-help
                                  "[#int#]constant"))
                                (ac-last-completion
                                 (cons nil item))
                                starts
                                messages)
                           (cl-letf
                               (((symbol-function
                                  'ac-complete-clang-template)
                                 (lambda ()
                                   (setq starts
                                         (1+ (or starts
                                                0)))))
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
                              (ac-clang-action)
                              starts
                              ac-clang-template-candidates
                              (nreverse messages))))"##,
        expect![[r#"OK (#1=("int constant") nil ("ok" "no" "yes:)") #1#)"#]],
    )
    .fresh_process()
}

fn auto_complete_clang_async_template_action_expands_real_nested_arguments_through_yasnippet_api()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_template_action_expands_real_nested_arguments_through_yasnippet_api",
        r##"(with-temp-buffer
                           (insert "method")
                           (let* ((candidate
                                   (propertize
                                    "(<#std::vector<int> values#>, <#callback(int, char)#>)"
                                    'raw-args
                                    "(<#std::vector<int> values#>, <#callback(int, char)#>)"))
                                  (ac-last-completion
                                   (cons nil candidate))
                                  (ac-clang-template-start-point
                                   (point-min))
                                  calls)
                             (provide 'yasnippet)
                             (fset
                              'yas/expand-snippet
                              (lambda (&rest arguments)
                                (push arguments calls)
                                :expanded))
                             (goto-char
                              (point-max))
                             (list
                              (ac-clang-template-action)
                              (nreverse calls)
                              (buffer-string)
                              (point))))"##,
        expect![[
            r#"OK (:expanded (("(${std::vector<int> values}, ${callback(int, char)})" 1 7)) "method" 7)"#
        ]],
    )
}

fn auto_complete_clang_async_template_action_retries_legacy_yasnippet_argument_order_on_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_template_action_retries_legacy_yasnippet_argument_order_on_error",
        r##"(with-temp-buffer
                           (insert "call")
                           (let* ((candidate
                                   (propertize
                                    "(<#int x#>)"
                                    'raw-args
                                    "(<#int x#>)"))
                                  (ac-last-completion
                                   (cons nil candidate))
                                  (ac-clang-template-start-point
                                   (point-min))
                                  calls)
                             (provide 'yasnippet)
                             (fset
                              'yas/expand-snippet
                              (lambda (&rest arguments)
                                (push arguments calls)
                                (if
                                    (stringp
                                     (car arguments))
                                    (error
                                     "new signature unavailable")
                                  :legacy-expanded)))
                             (goto-char
                              (point-max))
                             (list
                              (ac-clang-template-action)
                              (nreverse calls)
                              (buffer-string))))"##,
        expect![[r#"OK (:legacy-expanded (("(${int x})" 1 5) (1 5 "(${int x})")) "call")"#]],
    )
}

fn auto_complete_clang_async_template_action_uses_legacy_snippet_package_when_available()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_template_action_uses_legacy_snippet_package_when_available",
        r##"(with-temp-buffer
                           (insert "invoke")
                           (let* ((candidate
                                   (propertize
                                    "(<#int x#>, <#char y#>)"
                                    'raw-args
                                    "(<#int x#>, <#char y#>)"))
                                  (ac-last-completion
                                   (cons nil candidate))
                                  (ac-clang-template-start-point
                                   (point-min))
                                  calls)
                             (provide 'snippet)
                             (fset
                              'snippet-insert
                              (lambda (snippet)
                                (push
                                 (list
                                  snippet
                                  (buffer-string)
                                  (point))
                                 calls)
                                (insert snippet)
                                :inserted))
                             (goto-char
                              (point-max))
                             (list
                              (ac-clang-template-action)
                              (nreverse calls)
                              (buffer-string))))"##,
        expect![[r#"OK (:inserted (("($${int x}, $${char y})" "" 1)) "($${int x}, $${char y})")"#]],
    )
    .fresh_process()
}

fn auto_complete_clang_async_template_action_without_snippet_backend_preserves_text_and_warns()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_template_action_without_snippet_backend_preserves_text_and_warns",
        r##"(with-temp-buffer
                           (insert "call")
                           (let* ((candidate
                                   (propertize
                                    "(<#int x#>)"
                                    'raw-args
                                    "(<#int x#>)"))
                                  (ac-last-completion
                                   (cons nil candidate))
                                  (ac-clang-template-start-point
                                   (point-min))
                                  messages)
                             (goto-char
                              (point-max))
                             (cl-letf
                                 (((symbol-function
                                    'featurep)
                                   (lambda (feature)
                                     (and
                                      (not
                                       (memq
                                        feature
                                        '(yasnippet
                                          snippet)))
                                      (memq
                                       feature
                                       features))))
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
                                (ac-clang-template-action)
                                (buffer-string)
                                (point)
                                (nreverse messages)))))"##,
        expect![[
            r#"OK (#1=("Dude! You are too out! Please install a yasnippet or a snippet script:)") "call" 5 #1#)"#
        ]],
    )
}

pub(super) fn templates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_template_source_returns_buffer_local_candidates_and_start_point(),
        auto_complete_clang_async_action_builds_overload_template_candidates_with_help_and_raw_args(),
        auto_complete_clang_async_action_adds_optional_and_variadic_short_forms_without_duplicates(),
        auto_complete_clang_async_action_single_template_starts_completion_and_reports_clean_help(),
        auto_complete_clang_async_action_without_template_candidates_only_reports_documentation(),
        auto_complete_clang_async_template_action_expands_real_nested_arguments_through_yasnippet_api(),
        auto_complete_clang_async_template_action_retries_legacy_yasnippet_argument_order_on_error(),
        auto_complete_clang_async_template_action_uses_legacy_snippet_package_when_available(),
        auto_complete_clang_async_template_action_without_snippet_backend_preserves_text_and_warns(),
    ]
}
