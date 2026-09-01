use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_distel_default_prefix_matrix_covers_erlang_modules_functions_and_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_default_prefix_matrix_covers_erlang_modules_functions_and_boundaries",
        r##"(mapcar
          (lambda (text)
            (with-temp-buffer
              (insert text)
              (let ((start
                     (auto-complete-distel-get-start)))
                (list
                 text
                 (point)
                 start
                 (and
                  start
                  (buffer-substring-no-properties
                   start
                   (point)))))))
          '(""
            "."
            "l"
            "lists"
            "lists:ma"
            "my-module:run"
            "MY_VAR"
            "module2"
            "2module"
            "lists:map()"
            "result = lists:ma"
            "lists.ma"
            "å"))"##,
        expect![[
            r#"OK (("" 1 nil nil) ("." 2 nil nil) ("l" 2 1 "l") ("lists" 6 1 "lists") ("lists:ma" 9 1 "lists:ma") ("my-module:run" 14 1 "my-module:run") ("MY_VAR" 7 1 "MY_VAR") ("module2" 8 nil nil) ("2module" 8 2 "module") ("lists:map()" 12 nil nil) ("result = lists:ma" 18 10 "lists:ma") ("lists.ma" 9 7 "ma") ("å" 2 nil nil))"#
        ]],
    )
}

fn auto_complete_distel_prefix_respects_each_cursor_position_inside_real_erlang_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_prefix_respects_each_cursor_position_inside_real_erlang_code",
        r##"(with-temp-buffer
          (insert
           "Result = lists:map(Fun, Items).")
          (mapcar
           (lambda (position)
             (goto-char position)
             (let ((start
                    (auto-complete-distel-get-start)))
               (list
                position
                (char-before)
                start
                (and
                 start
                 (buffer-substring-no-properties
                  start
                  (point))))))
           '(1 2 7 8 9 13 14 15 17 18 19 20 23 27 33)))"##,
        expect![[
            r#"OK ((1 nil nil nil) (2 82 1 "R") (7 116 1 "Result") (8 32 nil nil) (9 61 nil nil) (13 115 10 "lis") (14 116 10 "list") (15 115 10 "lists") (17 109 10 "lists:m") (18 97 10 "lists:ma") (19 112 10 "lists:map") (20 40 nil nil) (23 110 20 "Fun") (27 116 25 "It") (33 46 nil nil))"#
        ]],
    )
}

fn auto_complete_distel_custom_valid_syntax_controls_digits_dots_slashes_and_unicode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_custom_valid_syntax_controls_digits_dots_slashes_and_unicode",
        r##"(mapcar
          (lambda (syntax)
            (let ((distel-completion-valid-syntax
                   syntax))
              (mapcar
               (lambda (text)
                 (with-temp-buffer
                   (insert text)
                   (let ((start
                          (auto-complete-distel-get-start)))
                     (list
                      text
                      start
                      (and
                       start
                       (buffer-substring-no-properties
                        start
                        (point)))))))
               '("module2"
                 "app.module:fun_2"
                 "path/to:run"
                 "módulo:función"))))
          '("a-zA-Z:_-"
            "a-zA-Z0-9:_.-"
            "^ \t\n"))"##,
        expect![[
            r#"OK ((("module2" nil nil) ("app.module:fun_2" nil nil) ("path/to:run" 6 "to:run") ("módulo:función" 14 "n")) (("module2" 1 "module2") ("app.module:fun_2" 1 "app.module:fun_2") ("path/to:run" 6 "to:run") ("módulo:función" 14 "n")) (("module2" 1 "module2") ("app.module:fun_2" 1 "app.module:fun_2") ("path/to:run" 1 "path/to:run") ("módulo:función" 1 "módulo:función")))"#
        ]],
    )
}

fn auto_complete_distel_prefix_uses_the_narrowed_buffer_boundary_as_its_start() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_prefix_uses_the_narrowed_buffer_boundary_as_its_start",
        r##"(with-temp-buffer
          (insert
           "ignored-prefixlists:map trailing")
          (let ((start
                 (progn
                   (goto-char
                    (point-min))
                   (search-forward
                    "prefix")
                   (point)))
                end)
            (search-forward
             "map")
            (setq end (point))
            (narrow-to-region
             start end)
            (goto-char
             (point-max))
            (let ((prefix
                   (auto-complete-distel-get-start)))
              (list
               (point-min)
               (point-max)
               prefix
               (buffer-substring-no-properties
                prefix
                (point))))))"##,
        expect![[r#"OK (15 24 15 "lists:map")"#]],
    )
}

fn auto_complete_distel_prefix_preserves_point_mark_and_existing_match_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_prefix_preserves_point_mark_and_existing_match_data",
        r##"(with-temp-buffer
          (insert
           "prefix lists:map suffix")
          (goto-char 17)
          (set-mark 3)
          (string-match
           "\\(sent\\)\\(inel\\)"
           "sentinel")
          (let ((before
                 (list
                  (point)
                  (mark)
                  (match-data)))
                (start
                 (auto-complete-distel-get-start)))
            (list
             before
             start
             (buffer-substring-no-properties
              start
              (point))
             (point)
             (mark)
             (match-data))))"##,
        expect![[r#"OK ((17 3 (0 8 0 4 4 8)) 8 "lists:map" 17 3 (0 8 0 4 4 8))"#]],
    )
}

fn auto_complete_distel_prefix_follows_dynamic_option_changes_without_reloading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_prefix_follows_dynamic_option_changes_without_reloading_source",
        r##"(with-temp-buffer
          (insert "app.module_2:run")
          (mapcar
           (lambda (syntax)
             (let ((distel-completion-valid-syntax
                    syntax))
               (let ((start
                      (auto-complete-distel-get-start)))
                 (list
                  syntax
                  start
                  (and
                   start
                   (buffer-substring-no-properties
                    start
                    (point)))))))
           '("a-zA-Z:_-"
             "a-zA-Z0-9:_.-"
             "a-z"
             "")))"##,
        expect![[
            r#"OK (("a-zA-Z:_-" 13 ":run") ("a-zA-Z0-9:_.-" 1 "app.module_2:run") ("a-z" 14 "run") ("" nil nil))"#
        ]],
    )
}

fn auto_complete_distel_invalid_valid_syntax_values_signal_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_invalid_valid_syntax_values_signal_exactly",
        r##"(mapcar
          (lambda (syntax)
            (with-temp-buffer
              (insert "lists:map")
              (let ((distel-completion-valid-syntax
                     syntax))
                (list
                 syntax
                 (auto-complete-distel-test-error
                  (lambda ()
                    (auto-complete-distel-get-start)))))))
          '(nil 42 valid-syntax-symbol ("a-z")))"##,
        expect![[
            r#"OK ((nil (:signal wrong-type-argument (stringp nil))) (42 (:signal wrong-type-argument (stringp 42))) (valid-syntax-symbol (:signal wrong-type-argument (stringp valid-syntax-symbol))) (#1=("a-z") (:signal wrong-type-argument (stringp #1#))))"#
        ]],
    )
}

pub(super) fn prefixes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_distel_default_prefix_matrix_covers_erlang_modules_functions_and_boundaries(),
        auto_complete_distel_prefix_respects_each_cursor_position_inside_real_erlang_code(),
        auto_complete_distel_custom_valid_syntax_controls_digits_dots_slashes_and_unicode(),
        auto_complete_distel_prefix_uses_the_narrowed_buffer_boundary_as_its_start(),
        auto_complete_distel_prefix_preserves_point_mark_and_existing_match_data(),
        auto_complete_distel_prefix_follows_dynamic_option_changes_without_reloading_source(),
        auto_complete_distel_invalid_valid_syntax_values_signal_exactly(),
    ]
}
