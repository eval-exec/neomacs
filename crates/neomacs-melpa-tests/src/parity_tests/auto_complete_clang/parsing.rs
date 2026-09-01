use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_parse_output_filters_prefix_and_returns_reverse_clang_order_with_help()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_filters_prefix_and_returns_reverse_clang_order_with_help",
        r##"(with-temp-buffer
         (insert
          "noise\n"
          "COMPLETION: alpha : int alpha\n"
          "COMPLETION: alphabet : void alphabet(<#int n#>)\n"
          "COMPLETION: beta : double beta\n"
          "COMPLETION: alpha_value : long alpha_value\n")
         (mapcar
          #'ac-clang-test-candidate-state
          (ac-clang-parse-output
           "alpha")))"##,
        expect![[
            r#"OK (("alpha_value" "long alpha_value" nil) ("alphabet" "void alphabet(<#int n#>)" nil) ("alpha" "int alpha" nil))"#
        ]],
    )
}

fn auto_complete_clang_parse_output_merges_adjacent_duplicate_overloads_into_one_candidate()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_merges_adjacent_duplicate_overloads_into_one_candidate",
        r##"(with-temp-buffer
         (insert
          "COMPLETION: draw : void draw(<#int x#>)\n"
          "COMPLETION: draw : void draw(<#double x#>)\n"
          "COMPLETION: draw : void draw(<#const char *x#>)\n"
          "COMPLETION: drop : void drop()\n")
         (mapcar
          #'ac-clang-test-candidate-state
          (ac-clang-parse-output
           "dr")))"##,
        expect![[
            r#"OK (("drop" "void drop()" nil) ("draw" "void draw(<#int x#>)\nvoid draw(<#double x#>)\nvoid draw(<#const char *x#>)" nil))"#
        ]],
    )
}

fn auto_complete_clang_parse_output_nonadjacent_duplicates_remain_separate() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_nonadjacent_duplicates_remain_separate",
        r##"(with-temp-buffer
         (insert
          "COMPLETION: same : int same(int)\n"
          "COMPLETION: middle : int middle\n"
          "COMPLETION: same : int same(double)\n")
         (mapcar
          #'ac-clang-test-candidate-state
          (ac-clang-parse-output
           "")))"##,
        expect![[
            r#"OK (("same" "int same(double)" nil) ("middle" "int middle" nil) ("same" "int same(int)" nil))"#
        ]],
    )
}

fn auto_complete_clang_parse_output_excludes_pattern_pseudo_candidate_only_by_exact_name()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_excludes_pattern_pseudo_candidate_only_by_exact_name",
        r##"(with-temp-buffer
         (insert
          "COMPLETION: Pattern : placeholder\n"
          "COMPLETION: PatternValue : real value\n"
          "COMPLETION: pattern : lowercase\n")
         (mapcar
          #'ac-clang-test-candidate-state
          (ac-clang-parse-output
           "")))"##,
        expect![[r#"OK (("pattern" "lowercase" nil) ("PatternValue" "real value" nil))"#]],
    )
}

fn auto_complete_clang_parse_output_quotes_regexp_metacharacters_in_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_quotes_regexp_metacharacters_in_prefix",
        r##"(with-temp-buffer
         (insert
          "COMPLETION: operator[] : index\n"
          "COMPLETION: operator() : call\n"
          "COMPLETION: foo.bar : member\n"
          "COMPLETION: fooXbar : other\n")
         (list
          (mapcar
           #'ac-clang-test-candidate-state
           (ac-clang-parse-output
            "operator["))
          (progn
            (goto-char (point-min))
            (mapcar
             #'ac-clang-test-candidate-state
             (ac-clang-parse-output
              "foo.")))))"##,
        expect![[r#"OK ((("operator[]" "index" nil)) (("foo.bar" "member" nil)))"#]],
    )
}

fn auto_complete_clang_parse_output_ignores_malformed_and_colonless_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_output_ignores_malformed_and_colonless_lines",
        r##"(with-temp-buffer
         (insert
          "COMPLETION alpha : missing marker\n"
          "COMPLETION: alpha\n"
          " COMPLETION: alpha : leading space\n"
          "COMPLETION: alpha: compact\n"
          "COMPLETION: alpha : spaced\n"
          "COMPLETION: alpha_more : \n")
         (mapcar
          #'ac-clang-test-candidate-state
          (ac-clang-parse-output
           "alpha")))"##,
        expect![[r#"OK (("alpha_more" "" nil) ("alpha" "\n: compact\nspaced" nil))"#]],
    )
}

fn auto_complete_clang_handle_error_keeps_only_diagnostics_before_first_completion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_handle_error_keeps_only_diagnostics_before_first_completion",
        r##"(let ((ac-clang-executable
                "/tool/clang")
               (messages nil))
         (with-temp-buffer
           (insert
            "source.cpp:3:4: error: expected expression\n"
            "note: prior diagnostic\n"
            "COMPLETION: value : int value\n"
            "trailing output\n")
           (cl-letf
               (((symbol-function
                  'current-time-string)
                 (lambda ()
                   "FIXED-TIME"))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-handle-error
              2
              '("-cc1"
                "-DNAME=two words"))
             (with-current-buffer
                 ac-clang-error-buffer-name
               (list
                (buffer-string)
                buffer-read-only
                (point)
                messages)))))"##,
        expect![[
            r#"OK ("FIXED-TIME\nclang failed with error 2:\n/tool/clang -cc1 -DNAME=two words\n\nsource.cpp:3:4: error: expected expression\nnote: prior diagnostic" t 1 nil)"#
        ]],
    )
}

fn auto_complete_clang_handle_error_without_completion_messages_and_keeps_full_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_handle_error_without_completion_messages_and_keeps_full_output",
        r##"(let ((ac-clang-executable "clang")
               (messages nil))
         (with-temp-buffer
           (insert
            "fatal error: input file missing\nsecond line\n")
           (cl-letf
               (((symbol-function
                  'current-time-string)
                 (lambda ()
                   "FIXED-TIME"))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-handle-error
              9
              '("-cc1" "-bad"))
             (with-current-buffer
                 ac-clang-error-buffer-name
               (list
                (buffer-string)
                buffer-read-only
                (nreverse messages))))))"##,
        expect![[
            r#"OK ("FIXED-TIME\nclang failed with error 9:\nclang -cc1 -bad\n\nfatal error: input file missing\nsecond line\n" t ("clang failed with error 9:\nclang -cc1 -bad"))"#
        ]],
    )
}

fn auto_complete_clang_handle_error_reuses_and_replaces_existing_error_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_handle_error_reuses_and_replaces_existing_error_buffer",
        r##"(let ((ac-clang-executable "clang"))
         (with-current-buffer
             (get-buffer-create
              ac-clang-error-buffer-name)
           (let ((inhibit-read-only t))
             (erase-buffer)
             (insert "STALE")
             (setq buffer-read-only t)))
         (with-temp-buffer
           (insert "new diagnostic\n")
           (cl-letf
               (((symbol-function
                  'current-time-string)
                 (lambda ()
                   "NOW"))
                ((symbol-function 'message)
                 (lambda (&rest _arguments)
                   nil)))
             (ac-clang-handle-error
              1 '("-cc1"))
             (with-current-buffer
                 ac-clang-error-buffer-name
               (list
                (buffer-string)
                (= (point) (point-min))
                buffer-read-only)))))"##,
        expect![[r#"OK ("NOW\nclang failed with error 1:\nclang -cc1\n\nnew diagnostic\n" t t)"#]],
    )
}

pub(super) fn parsing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_parse_output_filters_prefix_and_returns_reverse_clang_order_with_help(),
        auto_complete_clang_parse_output_merges_adjacent_duplicate_overloads_into_one_candidate(),
        auto_complete_clang_parse_output_nonadjacent_duplicates_remain_separate(),
        auto_complete_clang_parse_output_excludes_pattern_pseudo_candidate_only_by_exact_name(),
        auto_complete_clang_parse_output_quotes_regexp_metacharacters_in_prefix(),
        auto_complete_clang_parse_output_ignores_malformed_and_colonless_lines(),
        auto_complete_clang_handle_error_keeps_only_diagnostics_before_first_completion(),
        auto_complete_clang_handle_error_without_completion_messages_and_keeps_full_output(),
        auto_complete_clang_handle_error_reuses_and_replaces_existing_error_buffer(),
    ]
}
