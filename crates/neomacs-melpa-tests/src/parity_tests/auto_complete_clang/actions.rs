use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_action_builds_template_candidate_and_starts_template_completion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_builds_template_candidate_and_starts_template_completion",
        r##"(with-temp-buffer
         (insert "foo")
         (let* ((candidate
                 (propertize
                  "foo"
                  'ac-clang-help
                  "int foo(<#int value#>)"))
                (ac-last-completion
                 (cons "foo" candidate))
                (starts 0)
                (messages nil))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))
                   'started))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-action)
             (list
              ac-template-start-point
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)
              starts
              (nreverse messages)))))"##,
        expect![[r#"OK (4 (("(int value)" "" "(<#int value#>)")) 1 ("int foo(int value)"))"#]],
    )
}

fn auto_complete_clang_action_preserves_multiple_overloads_as_distinct_template_choices()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_preserves_multiple_overloads_as_distinct_template_choices",
        r##"(with-temp-buffer
         (insert "draw")
         (let* ((candidate
                 (propertize
                  "draw"
                  'ac-clang-help
                  (concat
                   "void draw(<#int x#>)\n"
                   "void draw(<#double x#>, <#double y#>)\n"
                   "void draw()")))
                (ac-last-completion
                 (cons "draw" candidate))
                (starts 0)
                (messages nil))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-action)
             (list
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)
              starts
              messages))))"##,
        expect![[
            r#"OK ((("(int x)" "" "(<#int x#>)") ("(double x, double y)" "" "(<#double x#>, <#double y#>)") ("()" "" "()")) 1 nil)"#
        ]],
    )
}

fn auto_complete_clang_action_expands_optional_argument_variant_and_variadic_variant()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_expands_optional_argument_variant_and_variadic_variant",
        r##"(with-temp-buffer
         (insert "log")
         (let* ((candidate
                 (propertize
                  "log"
                  'ac-clang-help
                  "void log(<#const char *fmt#>, {#int level#}, ...)"))
                (ac-last-completion
                 (cons "log" candidate))
                (starts 0))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))))
                ((symbol-function 'message)
                 (lambda (&rest _arguments)
                   nil)))
             (ac-clang-action)
             (list
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)
              starts))))"##,
        expect![[
            r#"OK ((("(const char *fmt, {#int level#}, ...)" "" "(<#const char *fmt#>, {#int level#}, ...)") ("(const char *fmt, , ...)" "" "(<#const char *fmt#>, , ...)") ("(const char *fmt, )" "" "(<#const char *fmt#>, )")) 1)"#
        ]],
    )
}

fn auto_complete_clang_action_extracts_function_pointer_return_signature() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_extracts_function_pointer_return_signature",
        r##"(with-temp-buffer
         (insert "callback")
         (let* ((candidate
                 (propertize
                  "callback"
                  'ac-clang-help
                  "[#int (*)(double, const char *, ...)#] callback"))
                (ac-last-completion
                 (cons "callback" candidate))
                (starts 0))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))))
                ((symbol-function 'message)
                 (lambda (&rest _arguments)
                   nil)))
             (ac-clang-action)
             (list
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)
              starts))))"##,
        expect![[
            r#"OK ((("(double, const char *, ...)" "int " "") ("(double, const char *)" "int " "")) 1)"#
        ]],
    )
}

fn auto_complete_clang_action_without_callable_signature_only_displays_clean_help()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_without_callable_signature_only_displays_clean_help",
        r##"(with-temp-buffer
         (insert "constant")
         (let* ((candidate
                 (propertize
                  "constant"
                  'ac-clang-help
                  "const int constant\nsecond detail"))
                (ac-last-completion
                 (cons "constant" candidate))
                (starts 0)
                (messages nil)
                (ac-template-candidates
                 '("stale")))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-action)
             (list
              ac-template-candidates
              starts
              (nreverse messages)))))"##,
        expect![[r#"OK (("stale") 0 ("const int constant   ;    second detail"))"#]],
    )
}

fn auto_complete_clang_action_deduplicates_identical_overload_templates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_deduplicates_identical_overload_templates",
        r##"(with-temp-buffer
         (insert "same")
         (let* ((candidate
                 (propertize
                  "same"
                  'ac-clang-help
                  (concat
                   "void same(<#int x#>)\n"
                   "void same(<#int x#>)\n"
                   "void same(<#double x#>)")))
                (ac-last-completion
                 (cons "same" candidate)))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda () nil))
                ((symbol-function 'message)
                 (lambda (&rest _arguments)
                   nil)))
             (ac-clang-action)
             (mapcar
              #'ac-clang-test-candidate-state
              ac-template-candidates))))"##,
        expect![[r#"OK (("(int x)" "" "(<#int x#>)") ("(double x)" "" "(<#double x#>)"))"#]],
    )
}

fn auto_complete_clang_action_single_template_flattens_multiline_message_after_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_single_template_flattens_multiline_message_after_start",
        r##"(with-temp-buffer
         (insert "only")
         (let* ((candidate
                 (propertize
                  "only"
                  'ac-clang-help
                  "int only(<#int x#>)\nadditional note"))
                (ac-last-completion
                 (cons "only" candidate))
                (events nil))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (push 'complete events)))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (list
                     'message
                     (apply #'format
                            format-string
                            arguments))
                    events))))
             (ac-clang-action)
             (list
              (nreverse events)
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)))))"##,
        expect![[
            r#"OK ((complete (message "int only(int x)   ;    additional note")) (("(int x)" "" "(<#int x#>)")))"#
        ]],
    )
}

fn auto_complete_clang_action_signals_for_missing_or_unpropertized_completion_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_action_signals_for_missing_or_unpropertized_completion_state",
        r##"(list
         (let ((ac-last-completion nil))
           (ac-clang-test-error
            #'ac-clang-action))
         (let ((ac-last-completion
                '("plain" . "plain")))
           (cl-letf
               (((symbol-function 'message)
                 (lambda (&rest _arguments)
                   nil)))
             (ac-clang-test-error
              #'ac-clang-action))))"##,
        expect![
            "OK ((:signal args-out-of-range (0 0)) (:signal wrong-type-argument (stringp nil)))"
        ],
    )
}

pub(super) fn actions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_action_builds_template_candidate_and_starts_template_completion(),
        auto_complete_clang_action_preserves_multiple_overloads_as_distinct_template_choices(),
        auto_complete_clang_action_expands_optional_argument_variant_and_variadic_variant(),
        auto_complete_clang_action_extracts_function_pointer_return_signature(),
        auto_complete_clang_action_without_callable_signature_only_displays_clean_help(),
        auto_complete_clang_action_deduplicates_identical_overload_templates(),
        auto_complete_clang_action_single_template_flattens_multiline_message_after_start(),
        auto_complete_clang_action_signals_for_missing_or_unpropertized_completion_state(),
    ]
}
