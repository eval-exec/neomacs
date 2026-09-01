use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_chunk_ports_complete_upstream_mode_and_suffix_boundary_matrix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_ports_complete_upstream_mode_and_suffix_boundary_matrix",
        r##"(mapcar
                           (lambda (case)
                             (apply
                              #'auto-complete-chunk-test-beginning
                              case))
                           '((fundamental-mode "\na.b")
                             (emacs-lisp-mode "\na.b")
                             (python-mode "\na.b")
                             (fundamental-mode "a.b")
                             (emacs-lisp-mode "a.b")
                             (python-mode "a.b")
                             (fundamental-mode "a.")
                             (emacs-lisp-mode "a.")
                             (python-mode "a.")
                             (fundamental-mode "a")
                             (emacs-lisp-mode "a")
                             (python-mode "a")
                             (fundamental-mode "a..")
                             (emacs-lisp-mode "a..")
                             (python-mode "a..")))"##,
        expect![[
            r#"OK ((fundamental-mode "\na.b" 5 2 "a.b") (emacs-lisp-mode "\na.b" 5 2 "a.b") (python-mode "\na.b" 5 2 "a.b") (fundamental-mode "a.b" 4 1 "a.b") (emacs-lisp-mode "a.b" 4 1 "a.b") (python-mode "a.b" 4 1 "a.b") (fundamental-mode "a." 3 1 "a.") (emacs-lisp-mode "a." 3 1 "a.") (python-mode "a." 3 1 "a.") (fundamental-mode "a" 2 1 "a") (emacs-lisp-mode "a" 2 1 "a") (python-mode "a" 2 1 "a") (fundamental-mode "a.." 4 nil nil) (emacs-lisp-mode "a.." 4 1 "a..") (python-mode "a.." 4 nil nil))"#
        ]],
    )
}

fn auto_complete_chunk_recognizes_bol_whitespace_and_parenthesis_boundaries_without_leaking_context()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_recognizes_bol_whitespace_and_parenthesis_boundaries_without_leaking_context",
        r##"(mapcar
                           (lambda (text)
                             (auto-complete-chunk-test-beginning
                              'emacs-lisp-mode
                              text))
                           '("alpha.beta"
                             "prefix alpha.beta"
                             "prefix\talpha.beta"
                             "prefix\nalpha.beta"
                             "(alpha.beta"
                             "[alpha.beta"
                             "{alpha.beta"
                             "prefix)alpha.beta"
                             "prefix]alpha.beta"
                             "prefix}alpha.beta"))"##,
        expect![[
            r#"OK ((emacs-lisp-mode "alpha.beta" 11 1 "alpha.beta") (emacs-lisp-mode "prefix alpha.beta" 18 8 "alpha.beta") (emacs-lisp-mode "prefix\11alpha.beta" 18 8 "alpha.beta") (emacs-lisp-mode "prefix\nalpha.beta" 18 8 "alpha.beta") (emacs-lisp-mode "(alpha.beta" 12 2 "alpha.beta") (emacs-lisp-mode "[alpha.beta" 12 2 "alpha.beta") (emacs-lisp-mode "{alpha.beta" 12 1 "{alpha.beta") (emacs-lisp-mode "prefix)alpha.beta" 18 8 "alpha.beta") (emacs-lisp-mode "prefix]alpha.beta" 18 8 "alpha.beta") (emacs-lisp-mode "prefix}alpha.beta" 18 1 "prefix}alpha.beta"))"#
        ]],
    )
}

fn auto_complete_chunk_point_positions_expose_each_incremental_prefix_boundary() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_point_positions_expose_each_incremental_prefix_boundary",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert
                            "alpha.beta.gamma tail")
                           (mapcar
                            (lambda (position)
                              (goto-char position)
                              (let ((beginning
                                     (ac-chunk-beginning)))
                                (list
                                 position
                                 beginning
                                 (and beginning
                                      (buffer-substring-no-properties
                                       beginning
                                       (point))))))
                            '(1 2 6 7 8 11 12 13 17 18 19 22)))"##,
        expect![[
            r#"OK ((1 nil nil) (2 1 "a") (6 1 "alpha") (7 1 "alpha.") (8 1 "alpha.b") (11 1 "alpha.beta") (12 1 "alpha.beta.") (13 1 "alpha.beta.g") (17 1 "alpha.beta.gamma") (18 nil nil) (19 18 "t") (22 18 "tail"))"#
        ]],
    )
}

fn auto_complete_chunk_punctuation_classification_changes_with_major_mode_syntax_tables()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_punctuation_classification_changes_with_major_mode_syntax_tables",
        r##"(mapcar
                           (lambda (mode)
                             (mapcar
                              (lambda (text)
                                (auto-complete-chunk-test-beginning
                                 mode
                                 text))
                              '("pkg.module/name"
                                "pkg::member"
                                "pkg->member"
                                "pkg..member"
                                "pkg...member"
                                "pkg/member:"
                                "snake_case.member"
                                "kebab-case.member")))
                           '(fundamental-mode
                             emacs-lisp-mode
                             python-mode))"##,
        expect![[
            r#"OK (((fundamental-mode "pkg.module/name" 16 1 "pkg.module/name") (fundamental-mode "pkg::member" 12 nil nil) (fundamental-mode "pkg->member" 12 1 "pkg->member") (fundamental-mode "pkg..member" 12 nil nil) (fundamental-mode "pkg...member" 13 nil nil) (fundamental-mode "pkg/member:" 12 1 "pkg/member:") (fundamental-mode "snake_case.member" 18 1 "snake_case.member") (fundamental-mode "kebab-case.member" 18 1 "kebab-case.member")) ((emacs-lisp-mode "pkg.module/name" 16 1 "pkg.module/name") (emacs-lisp-mode "pkg::member" 12 1 "pkg::member") (emacs-lisp-mode "pkg->member" 12 1 "pkg->member") (emacs-lisp-mode "pkg..member" 12 1 "pkg..member") (emacs-lisp-mode "pkg...member" 13 1 "pkg...member") (emacs-lisp-mode "pkg/member:" 12 1 "pkg/member:") (emacs-lisp-mode "snake_case.member" 18 1 "snake_case.member") (emacs-lisp-mode "kebab-case.member" 18 1 "kebab-case.member")) ((python-mode "pkg.module/name" 16 1 "pkg.module/name") (python-mode "pkg::member" 12 nil nil) (python-mode "pkg->member" 12 nil nil) (python-mode "pkg..member" 12 nil nil) (python-mode "pkg...member" 13 nil nil) (python-mode "pkg/member:" 12 1 "pkg/member:") (python-mode "snake_case.member" 18 1 "snake_case.member") (python-mode "kebab-case.member" 18 1 "kebab-case.member")))"#
        ]],
    )
}

fn auto_complete_chunk_unicode_words_symbols_and_punctuation_follow_active_syntax()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_unicode_words_symbols_and_punctuation_follow_active_syntax",
        r##"(mapcar
                           (lambda (mode)
                             (mapcar
                              (lambda (text)
                                (auto-complete-chunk-test-beginning
                                 mode
                                 text))
                              '("λ.value"
                                "naïve.module"
                                "東京.駅"
                                "δ-value.part"
                                "emoji.😀"
                                "😀.emoji")))
                           '(fundamental-mode
                             emacs-lisp-mode
                             python-mode))"##,
        expect![[
            r#"OK (((fundamental-mode "λ.value" 8 1 "λ.value") (fundamental-mode "naïve.module" 13 1 "naïve.module") (fundamental-mode "東京.駅" 5 1 "東京.駅") (fundamental-mode "δ-value.part" 13 1 "δ-value.part") (fundamental-mode "emoji.😀" 8 1 "emoji.😀") (fundamental-mode "😀.emoji" 8 1 "😀.emoji")) ((emacs-lisp-mode "λ.value" 8 1 "λ.value") (emacs-lisp-mode "naïve.module" 13 1 "naïve.module") (emacs-lisp-mode "東京.駅" 5 1 "東京.駅") (emacs-lisp-mode "δ-value.part" 13 1 "δ-value.part") (emacs-lisp-mode "emoji.😀" 8 1 "emoji.😀") (emacs-lisp-mode "😀.emoji" 8 1 "😀.emoji")) ((python-mode "λ.value" 8 1 "λ.value") (python-mode "naïve.module" 13 1 "naïve.module") (python-mode "東京.駅" 5 1 "東京.駅") (python-mode "δ-value.part" 13 1 "δ-value.part") (python-mode "emoji.😀" 8 1 "emoji.😀") (python-mode "😀.emoji" 8 1 "😀.emoji")))"#
        ]],
    )
}

fn auto_complete_chunk_custom_dot_syntax_reclassifies_double_separator_edge_case() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_custom_dot_syntax_reclassifies_double_separator_edge_case",
        r##"(mapcar
                           (lambda (syntax)
                             (with-temp-buffer
                               (fundamental-mode)
                               (modify-syntax-entry
                                ?.
                                syntax)
                               (insert "a..")
                               (let ((beginning
                                      (ac-chunk-beginning)))
                                 (list
                                  syntax
                                  beginning
                                  (and beginning
                                       (buffer-substring-no-properties
                                        beginning
                                        (point)))))))
                           '("." "_" "w" " "))"##,
        expect![[r#"OK (("." nil nil) ("_" 1 "a..") ("w" 1 "a..") (" " nil nil))"#]],
    )
}

fn auto_complete_chunk_narrowed_buffer_start_behaves_as_real_bol_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_narrowed_buffer_start_behaves_as_real_bol_boundary",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert
                            "ignored-prefix alpha.beta ignored-suffix")
                           (let ((start
                                  (progn
                                    (goto-char
                                     (point-min))
                                    (search-forward
                                     "alpha")
                                    (match-beginning 0)))
                                 (end
                                  (progn
                                    (search-forward
                                     "beta")
                                    (match-end 0))))
                             (narrow-to-region
                              start
                              end)
                             (goto-char
                              (point-max))
                             (let ((beginning
                                    (ac-chunk-beginning)))
                               (list
                                (point-min)
                                (point-max)
                                beginning
                                (buffer-substring-no-properties
                                 beginning
                                 (point))))))"##,
        expect![[r#"OK (16 26 16 "alpha.beta")"#]],
    )
}

fn auto_complete_chunk_invalid_or_nonmatching_regex_returns_nil_via_documented_error_shield()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_invalid_or_nonmatching_regex_returns_nil_via_documented_error_shield",
        r##"(mapcar
                           (lambda (regex)
                             (with-temp-buffer
                               (insert "alpha.beta")
                               (let ((ac-chunk-regex
                                      regex))
                                 (list
                                  regex
                                  (auto-complete-chunk-test-error
                                   #'ac-chunk-beginning)))))
                           '("["
                             "\\`never-matches\\'"
                             nil
                             42))"##,
        expect![[
            r#"OK (("[" (:value nil)) ("\\`never-matches\\'" (:value nil)) (nil (:value nil)) (42 (:value nil)))"#
        ]],
    )
}

fn auto_complete_chunk_success_and_failure_have_exact_match_data_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_success_and_failure_have_exact_match_data_side_effects",
        r##"(mapcar
                           (lambda (text)
                             (with-temp-buffer
                               (emacs-lisp-mode)
                               (insert text)
                               (string-match
                                "\\(sent\\)inel"
                                "sentinel")
                               (let ((before
                                      (match-data)))
                                 (list
                                  text
                                  before
                                  (ac-chunk-beginning)
                                  (match-data)))))
                           '("alpha.beta"
                             "alpha.."
                             "two words"))"##,
        expect![[
            r#"OK (("alpha.beta" (0 8 0 4) 1 ((:marker nil nil) (:marker nil nil) (:marker nil nil) (:marker nil nil))) ("alpha.." (0 8 0 4) 1 ((:marker nil nil) (:marker nil nil) (:marker nil nil) (:marker nil nil))) ("two words" (0 8 0 4) 5 ((:marker nil nil) (:marker nil nil) (:marker nil nil) (:marker nil nil))))"#
        ]],
    )
}

fn auto_complete_chunk_quotes_operators_and_trailing_delimiters_define_exact_failure_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_quotes_operators_and_trailing_delimiters_define_exact_failure_edges",
        r##"(mapcar
                           (lambda (text)
                             (auto-complete-chunk-test-beginning
                              'emacs-lisp-mode
                              text))
                           '("'alpha.beta"
                             "`alpha.beta"
                             ",alpha.beta"
                             "\"alpha.beta"
                             "x=alpha.beta"
                             "x+alpha.beta"
                             "alpha.beta)"
                             "alpha.beta]"
                             "alpha.beta}"
                             "alpha.beta,"
                             "alpha.beta;"))"##,
        expect![[
            r#"OK ((emacs-lisp-mode "'alpha.beta" 12 nil nil) (emacs-lisp-mode "`alpha.beta" 12 nil nil) (emacs-lisp-mode ",alpha.beta" 12 nil nil) (emacs-lisp-mode "\"alpha.beta" 12 nil nil) (emacs-lisp-mode "x=alpha.beta" 13 1 "x=alpha.beta") (emacs-lisp-mode "x+alpha.beta" 13 1 "x+alpha.beta") (emacs-lisp-mode "alpha.beta)" 12 nil nil) (emacs-lisp-mode "alpha.beta]" 12 nil nil) (emacs-lisp-mode "alpha.beta}" 12 1 "alpha.beta}") (emacs-lisp-mode "alpha.beta," 12 nil nil) (emacs-lisp-mode "alpha.beta;" 12 nil nil))"#
        ]],
    )
}

pub(super) fn boundaries_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_chunk_ports_complete_upstream_mode_and_suffix_boundary_matrix(),
        auto_complete_chunk_recognizes_bol_whitespace_and_parenthesis_boundaries_without_leaking_context(),
        auto_complete_chunk_point_positions_expose_each_incremental_prefix_boundary(),
        auto_complete_chunk_punctuation_classification_changes_with_major_mode_syntax_tables(),
        auto_complete_chunk_unicode_words_symbols_and_punctuation_follow_active_syntax(),
        auto_complete_chunk_custom_dot_syntax_reclassifies_double_separator_edge_case(),
        auto_complete_chunk_narrowed_buffer_start_behaves_as_real_bol_boundary(),
        auto_complete_chunk_invalid_or_nonmatching_regex_returns_nil_via_documented_error_shield(),
        auto_complete_chunk_success_and_failure_have_exact_match_data_side_effects(),
        auto_complete_chunk_quotes_operators_and_trailing_delimiters_define_exact_failure_edges(),
    ]
}
