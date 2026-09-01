use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_json_match_normalization_and_candidate_format_preserve_structured_data()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_json_match_normalization_and_candidate_format_preserve_structured_data",
        r##"(let* ((json
                '(:file "src/lib:parser.rs"
                  :range (:start (:line 8 :column 13)
                          :end (:line 8 :column 21))
                  :text "  parse(\n value,\r\n other)  "))
               (match (ast-grep--match-from-json json))
               (candidate (ast-grep--format-candidate match)))
          (list
           match
           (substring-no-properties candidate)
           (text-properties-at 0 candidate)
           (ast-grep-test-match-summary candidate)
           (ast-grep-test-match-summary
            (substring-no-properties candidate))
           (hash-table-count ast-grep--candidate-table)))"##,
        expect![[
            r#"OK (#1=(:file "src/lib:parser.rs" :start-line 8 :start-column 13 :text "  parse(\n value,\15\n other)  ") "src/lib:parser.rs:9:13:parse(  value,  other)" (ast-grep-match #1#) ("src/lib:parser.rs" 8 13 nil nil "  parse(\n value,\15\n other)  " nil) ("src/lib:parser.rs" 8 13 nil nil "  parse(\n value,\15\n other)  " nil) 1)"#
        ]],
    )
}

fn ast_grep_candidate_display_text_normalizes_multiline_real_source_snippets() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_candidate_display_text_normalizes_multiline_real_source_snippets",
        r##"(mapcar
          #'ast-grep--candidate-display-text
          '(nil
            ""
            " \n\t "
            "\r\n  function call(\n  alpha,\r\n  beta)\n"
            "single β界 line"))"##,
        expect![[r#"OK ("" "" "" "function call(   alpha,   beta)" "single β界 line")"#]],
    )
}

fn ast_grep_candidate_lookup_supports_plists_properties_registry_and_legacy_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_candidate_lookup_supports_plists_properties_registry_and_legacy_paths",
        r##"(let* ((match
                '(:file "C:/work/lib:thing.ts"
                  :start-line 4
                  :start-column 11
                  :text "target()"))
               (candidate (ast-grep--format-candidate match))
               (plain (substring-no-properties candidate))
               (legacy-unregistered
                "D:/other/path:with:colon.rs:17:9:body"))
          (list
           (ast-grep-test-match-summary match)
           (ast-grep-test-match-summary candidate)
           (ast-grep-test-match-summary plain)
           (progn
             (ast-grep--reset-candidate-table)
             (ast-grep-test-match-summary plain))
           (ast-grep-test-match-summary legacy-unregistered)
           (ast-grep-test-match-summary 42)
           (ast-grep-test-match-summary "not-a-match")))"##,
        expect![[
            r#"OK (("C:/work/lib:thing.ts" 4 11 nil nil "target()" nil) ("C:/work/lib:thing.ts" 4 11 nil nil "target()" nil) ("C:/work/lib:thing.ts" 4 11 nil nil "target()" nil) ("C:/work/lib:thing.ts" 4 11 nil nil nil nil) ("D:/other/path:with:colon.rs" 16 9 nil nil nil nil) nil nil)"#
        ]],
    )
}

fn ast_grep_stream_parser_handles_multiple_files_unicode_and_malformed_records() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_stream_parser_handles_multiple_files_unicode_and_malformed_records",
        r##"(let* ((output
                (concat
                 "{\"file\":\"src/a.js\",\"range\":{\"start\":{\"line\":0,\"column\":0}},\"text\":\"let α = one()\"}\n"
                 "not json\n"
                 "\n"
                 "{\"file\":\"src/b.rs\",\"range\":{\"start\":{\"line\":11,\"column\":7}},\"text\":\"界::call(\\n  x)\"}\n"
                 "{\"file\":\"missing-range\",\"text\":\"fallback\"}\n"))
               (candidates (ast-grep--parse-stream-output output)))
          (list
           (mapcar #'substring-no-properties candidates)
           (mapcar #'ast-grep-test-match-summary candidates)
           (hash-table-count ast-grep--candidate-table)
           (ast-grep--parse-stream-line nil)
           (ast-grep--parse-stream-output "")
           (ast-grep--parse-stream-output nil)))"##,
        expect![[
            r#"OK (("src/a.js:1:0:let α = one()" "src/b.rs:12:7:界::call(   x)") (("src/a.js" 0 0 nil nil "let α = one()" nil) ("src/b.rs" 11 7 nil nil "界::call(\n  x)" nil)) 2 nil nil nil)"#
        ]],
    )
}

fn ast_grep_completion_table_filters_real_candidates_and_exposes_affixation_metadata()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_completion_table_filters_real_candidates_and_exposes_affixation_metadata",
        r##"(let* ((ast-grep-use-nerd-icons nil)
               (candidates
                (mapcar
                 #'ast-grep--format-candidate
                 '((:file "src/apple.rs" :start-line 0 :start-column 1
                    :text "apple()")
                   (:file "src/banana.rs" :start-line 2 :start-column 3
                    :text "banana()")
                   (:file "test/apple_test.rs" :start-line 4 :start-column 5
                    :text "apple_test()"))))
               (table (ast-grep--completion-table candidates)))
          (list
           (funcall table "" nil 'metadata)
           (mapcar
            #'substring-no-properties
            (all-completions "src/a" table))
           (mapcar
            #'substring-no-properties
            (all-completions "test/" table))
           (test-completion
            (substring-no-properties (car candidates))
            table)
           (ast-grep--affixation candidates)))"##,
        expect![[
            r#"OK ((metadata (affixation-function . ast-grep--affixation)) ("src/apple.rs:1:1:apple()") ("test/apple_test.rs:5:5:apple_test()") t ((#("src/apple.rs:1:1:apple()" 0 24 (ast-grep-match (:file "src/apple.rs" :start-line 0 :start-column 1 :text "apple()"))) "" "") (#("src/banana.rs:3:3:banana()" 0 26 (ast-grep-match (:file "src/banana.rs" :start-line 2 :start-column 3 :text "banana()"))) "" "") (#("test/apple_test.rs:5:5:apple_test()" 0 35 (ast-grep-match (:file "test/apple_test.rs" :start-line 4 :start-column 5 :text "apple_test()"))) "" "")))"#
        ]],
    )
}

fn ast_grep_nerd_icon_probe_caches_by_setting_and_affixes_without_mutating_candidate()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_nerd_icon_probe_caches_by_setting_and_affixes_without_mutating_candidate",
        r##"(let* ((match
                '(:file "src/render.tsx"
                  :start-line 0
                  :start-column 0
                  :text "render()"))
               (candidate (ast-grep--format-candidate match))
               (loads 0)
               (icon-calls nil)
               (ast-grep-use-nerd-icons t)
               (ast-grep--nerd-icons-available-cache nil))
          (cl-letf (((symbol-function 'require)
                     (lambda (feature &optional _filename _noerror)
                       (if (eq feature 'nerd-icons)
                           (progn (setq loads (1+ loads)) t)
                         t)))
                    ((symbol-function 'nerd-icons-icon-for-file)
                     (lambda (file)
                       (push file icon-calls)
                       "ICON")))
            (list
             (ast-grep--candidate-icon-prefix candidate)
             (ast-grep--candidate-icon-prefix candidate)
             loads
             (nreverse icon-calls)
             (substring-no-properties candidate)
             (let ((ast-grep-use-nerd-icons nil))
               (ast-grep--candidate-icon-prefix candidate))
             (let ((ast-grep-use-nerd-icons t))
               (ast-grep--candidate-icon-prefix candidate))
             loads)))"##,
        expect![[
            r#"OK ("ICON " "ICON " 1 ("src/render.tsx" "src/render.tsx") "src/render.tsx:1:0:render()" "" "ICON " 2)"#
        ]],
    )
}

fn ast_grep_character_column_navigation_handles_tabs_unicode_and_double_width_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_character_column_navigation_handles_tabs_unicode_and_double_width_text",
        r##"(with-temp-buffer
          (insert "\tα界target\nsecond\tβeta\nthird")
          (let (positions)
            (dolist (coordinate '((0 0) (0 1) (0 2) (0 3)
                                  (0 8) (1 7) (2 5)))
              (condition-case error-data
                  (progn
                    (ast-grep--goto-line-column
                     (car coordinate)
                     (cadr coordinate))
                    (push
                     (list
                      coordinate
                      (point)
                      (char-after)
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position)))
                     positions))
                (error
                 (push
                  (list coordinate
                        (car error-data)
                        (cdr error-data))
                  positions))))
            (nreverse positions)))"##,
        expect![[
            r#"OK (((0 0) 1 9 "\11α界target") ((0 1) 2 945 "\11α界target") ((0 2) 3 30028 "\11α界target") ((0 3) 4 116 "\11α界target") ((0 8) 9 116 "\11α界target") ((1 7) 18 946 "second\11βeta") ((2 5) 28 nil "third"))"#
        ]],
    )
}

fn ast_grep_goto_match_visits_real_file_and_lands_on_character_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_goto_match_visits_real_file_and_lands_on_character_index",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "project/src/unicode.rs"
                 "header\n\tα界target();\nfooter\n"))
               (candidate
                (ast-grep--format-candidate
                 (list :file file
                       :start-line 1
                       :start-column 3
                       :text "target()"))))
          (unwind-protect
              (progn
                (ast-grep--goto-match
                 (substring-no-properties candidate))
                (list
                 (equal (file-truename buffer-file-name)
                        (file-truename file))
                 (line-number-at-pos)
                 (- (point) (line-beginning-position))
                 (buffer-substring-no-properties
                  (point)
                  (min (+ (point) 6) (point-max)))))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[r#"OK (t 2 3 "target")"#]],
    )
}

fn ast_grep_reset_candidate_table_removes_only_registered_session_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_reset_candidate_table_removes_only_registered_session_data",
        r##"(let* ((first
                (ast-grep--format-candidate
                 '(:file "a.rs" :start-line 0 :start-column 0 :text "a")))
               (second
                (ast-grep--format-candidate
                 '(:file "b.rs" :start-line 1 :start-column 2 :text "b")))
               (before
                (list
                 (hash-table-count ast-grep--candidate-table)
                 (ast-grep-test-match-summary
                  (substring-no-properties first))
                 (ast-grep-test-match-summary
                  (substring-no-properties second)))))
          (ast-grep--reset-candidate-table)
          (list
           before
           (hash-table-count ast-grep--candidate-table)
           (ast-grep-test-match-summary
            (substring-no-properties first))
           (ast-grep-test-match-summary first)))"##,
        expect![[
            r#"OK ((2 ("a.rs" 0 0 nil nil "a" nil) ("b.rs" 1 2 nil nil "b" nil)) 0 ("a.rs" 0 0 nil nil nil nil) ("a.rs" 0 0 nil nil "a" nil))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_json_match_normalization_and_candidate_format_preserve_structured_data(),
        ast_grep_candidate_display_text_normalizes_multiline_real_source_snippets(),
        ast_grep_candidate_lookup_supports_plists_properties_registry_and_legacy_paths(),
        ast_grep_stream_parser_handles_multiple_files_unicode_and_malformed_records(),
        ast_grep_completion_table_filters_real_candidates_and_exposes_affixation_metadata(),
        ast_grep_nerd_icon_probe_caches_by_setting_and_affixes_without_mutating_candidate(),
        ast_grep_character_column_navigation_handles_tabs_unicode_and_double_width_text(),
        ast_grep_goto_match_visits_real_file_and_lands_on_character_index(),
        ast_grep_reset_candidate_table_removes_only_registered_session_data(),
    ]
}
