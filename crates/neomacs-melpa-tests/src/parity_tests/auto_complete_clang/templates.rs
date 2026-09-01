use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_template_candidate_returns_shared_global_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_candidate_returns_shared_global_list",
        r##"(let ((ac-template-candidates
                '("first" "second")))
         (let ((result
                (ac-template-candidate)))
           (setcar result "changed")
           (list
            (eq result
                ac-template-candidates)
            result
            ac-template-candidates)))"##,
        expect![[r#"OK (t #1=("changed" "second") #1#)"#]],
    )
}

fn auto_complete_clang_template_prefix_returns_exact_start_point_or_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_prefix_returns_exact_start_point_or_nil",
        r##"(list
         (let ((ac-template-start-point
                nil))
           (ac-template-prefix))
         (let ((ac-template-start-point
                1))
           (ac-template-prefix))
         (let ((ac-template-start-point
                42))
           (ac-template-prefix))
         (let ((ac-template-start-point
                (copy-marker 7)))
           (marker-position
            (ac-template-prefix))))"##,
        expect!["OK (nil 1 42 1)"],
    )
}

fn auto_complete_clang_template_action_noops_when_start_point_is_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_action_noops_when_start_point_is_nil",
        r##"(with-temp-buffer
         (insert "candidate")
         (let ((ac-template-start-point nil)
               (ac-last-completion
                '("candidate"
                  . "candidate"))
               (calls nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (push feature calls)
                   t)))
             (list
              (ac-template-action)
              (buffer-string)
              calls))))"##,
        expect![[r#"OK (nil "candidate" nil)"#]],
    )
}

fn auto_complete_clang_template_action_without_snippet_backend_keeps_text_and_messages()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_action_without_snippet_backend_keeps_text_and_messages",
        r##"(with-temp-buffer
         (insert "(int value)")
         (let* ((selected
                 (propertize
                  "(int value)"
                  'raw-args
                  "(<#int value#>)"))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (messages nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (_feature) nil))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (list
              (ac-template-action)
              (buffer-string)
              (nreverse messages)))))"##,
        expect![[
            r#"OK (#1=("Dude! You are too out! Please install a yasnippet or a snippet script:)") "(int value)" #1#)"#
        ]],
    )
}

fn auto_complete_clang_template_action_yasnippet_converts_required_optional_and_variadic_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_action_yasnippet_converts_required_optional_and_variadic_arguments",
        r##"(with-temp-buffer
         (insert "(int value, int level, ...)")
         (let* ((selected
                 (propertize
                  "(int value, int level, ...)"
                  'raw-args
                  "(<#int value#>, {#int level#}, ...)"))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (calls nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (eq feature
                       'yasnippet)))
                ((symbol-function
                  'yas/expand-snippet)
                 (lambda (&rest arguments)
                   (push arguments calls)
                   'expanded)))
             (list
              (ac-template-action)
              (buffer-string)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (expanded "(int value, int level, ...)" (("(${int value}, int level}, ${...)" 1 28)))"#
        ]],
    )
}

fn auto_complete_clang_template_action_yasnippet_falls_back_to_legacy_argument_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_action_yasnippet_falls_back_to_legacy_argument_order",
        r##"(with-temp-buffer
         (insert "(int value)")
         (let* ((selected
                 (propertize
                  "(int value)"
                  'raw-args
                  "(<#int value#>)"))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (calls nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (eq feature
                       'yasnippet)))
                ((symbol-function
                  'yas/expand-snippet)
                 (lambda (&rest arguments)
                   (push arguments calls)
                   (when
                       (stringp
                        (car arguments))
                     (error
                      "new API unavailable"))
                   'legacy-expanded)))
             (list
              (ac-template-action)
              (nreverse calls)))))"##,
        expect![[r#"OK (legacy-expanded (("(${int value})" 1 12) (1 12 "(${int value})")))"#]],
    )
}

fn auto_complete_clang_template_action_snippet_backend_replaces_selected_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_template_action_snippet_backend_replaces_selected_text",
        r##"(with-temp-buffer
         (insert "(int value, int level)")
         (let* ((selected
                 (propertize
                  "(int value, int level)"
                  'raw-args
                  "(<#int value#>, {#int level#})"))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (insertions nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (eq feature 'snippet)))
                ((symbol-function
                  'snippet-insert)
                 (lambda (snippet)
                   (push snippet insertions)
                   (insert snippet)
                   'inserted)))
             (list
              (ac-template-action)
              (buffer-string)
              (nreverse insertions)))))"##,
        expect![[r#"OK (inserted "($${int value}, int level)" ("($${int value}, int level)"))"#]],
    )
}

fn auto_complete_clang_function_pointer_template_builds_yasnippet_from_nested_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_function_pointer_template_builds_yasnippet_from_nested_arguments",
        r##"(with-temp-buffer
         (insert
          "(int, std::pair<long, long>, void (*)(char, double))")
         (let* ((selected
                 (propertize
                  (buffer-string)
                  'raw-args
                  ""))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (calls nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (eq feature
                       'yasnippet)))
                ((symbol-function
                  'yas/expand-snippet)
                 (lambda (&rest arguments)
                   (push arguments calls)
                   'expanded)))
             (list
              (ac-template-action)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (expanded ((#("(${int}, ${std::pair<long, long>}, ${void (*)(char, double)})" 3 6 (raw-args "") 11 25 (raw-args "") 27 32 (raw-args "") 37 50 (raw-args "") 52 59 (raw-args "")) 1 53)))"#
        ]],
    )
}

fn auto_complete_clang_function_pointer_template_builds_snippet_backend_placeholders()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_function_pointer_template_builds_snippet_backend_placeholders",
        r##"(with-temp-buffer
         (insert "(int, const char *, ...)")
         (let* ((selected
                 (propertize
                  (buffer-string)
                  'raw-args
                  ""))
                (ac-last-completion
                 (cons "choice" selected))
                (ac-template-start-point
                 (point-min))
                (insertions nil))
           (cl-letf
               (((symbol-function
                  'featurep)
                 (lambda (feature)
                   (eq feature 'snippet)))
                ((symbol-function
                  'snippet-insert)
                 (lambda (snippet)
                   (push snippet insertions)
                   (insert snippet)
                   'inserted)))
             (list
              (ac-template-action)
              (buffer-string)
              (nreverse insertions)))))"##,
        expect![[
            r#"OK (inserted #("($${int}, $${const char *}, $${...})" 4 7 (raw-args "") 13 25 (raw-args "") 31 34 (raw-args "")) (#("($${int}, $${const char *}, $${...})" 4 7 (raw-args "") 13 25 (raw-args "") 31 34 (raw-args ""))))"#
        ]],
    )
}

pub(super) fn templates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_template_candidate_returns_shared_global_list(),
        auto_complete_clang_template_prefix_returns_exact_start_point_or_nil(),
        auto_complete_clang_template_action_noops_when_start_point_is_nil(),
        auto_complete_clang_template_action_without_snippet_backend_keeps_text_and_messages(),
        auto_complete_clang_template_action_yasnippet_converts_required_optional_and_variadic_arguments(),
        auto_complete_clang_template_action_yasnippet_falls_back_to_legacy_argument_order(),
        auto_complete_clang_template_action_snippet_backend_replaces_selected_text(),
        auto_complete_clang_function_pointer_template_builds_yasnippet_from_nested_arguments(),
        auto_complete_clang_function_pointer_template_builds_snippet_backend_placeholders(),
    ]
}
