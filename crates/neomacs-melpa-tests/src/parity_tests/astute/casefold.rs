use expect_test::expect;

use super::ParityBatchCase;

fn astute_case_insensitize_expands_mixed_ascii_words_without_changing_nonletters() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_case_insensitize_expands_mixed_ascii_words_without_changing_nonletters",
        r##"(mapcar
         (lambda (string)
           (list
            string
            (astute-case-insensitize string)))
         '(""
           "Astute"
           "twas"
           "n'"
           "90s"
           "rock-and-roll"
           "O'Reilly"
           "v2.1"))"##,
        expect![[
            r#"OK (("" "") ("Astute" "[Aa][Ss][Tt][Uu][Tt][Ee]") ("twas" "[Tt][Ww][Aa][Ss]") ("n'" "[Nn]'") ("90s" "90[Ss]") ("rock-and-roll" "[Rr][Oo][Cc][Kk]-[Aa][Nn][Dd]-[Rr][Oo][Ll][Ll]") ("O'Reilly" "[Oo]'[Rr][Ee][Ii][Ll][Ll][Yy]") ("v2.1" "[Vv]2.1"))"#
        ]],
    )
}

fn astute_case_insensitive_exception_fragments_match_real_prefix_spellings() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_case_insensitive_exception_fragments_match_real_prefix_spellings",
        r##"(let* ((fragments
                  (mapcar
                   #'astute-case-insensitize
                   '("bout"
                     "twas"
                     "cause"
                     "n'")))
                 (regexp
                  (concat
                   "\\`\\(?:"
                   (string-join fragments "\\|")
                   "\\)\\'")))
         (list
          fragments
          regexp
          (mapcar
           (lambda (word)
             (cons
              word
              (and
               (string-match-p regexp word)
               t)))
           '("bout"
             "BOUT"
             "Bout"
             "tWaS"
             "CAUSE"
             "n'"
             "N'"
             "about"
             "causeway"
             "n"))))"##,
        expect![[
            r#"OK (("[Bb][Oo][Uu][Tt]" "[Tt][Ww][Aa][Ss]" "[Cc][Aa][Uu][Ss][Ee]" "[Nn]'") "\\`\\(?:[Bb][Oo][Uu][Tt]\\|[Tt][Ww][Aa][Ss]\\|[Cc][Aa][Uu][Ss][Ee]\\|[Nn]'\\)\\'" (("bout" . t) ("BOUT" . t) ("Bout" . t) ("tWaS" . t) ("CAUSE" . t) ("n'" . t) ("N'" . t) ("about") ("causeway") ("n")))"#
        ]],
    )
}

fn astute_default_prefix_exception_keyword_matches_decades_and_configured_elisions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_default_prefix_exception_keyword_matches_decades_and_configured_elisions",
        r##"(let ((regexp
                (car
                 (nth 3
                      (astute-init-font-lock)))))
         (list
          regexp
          (astute-test-match-summary
           regexp
           '("'90s"
             "'90"
             "'bout"
             "'Bout"
             "'CAUSE"
             "'round"
             "'twas"
             "'TIS"
             "'em"
             "'n'"
             "'not-an-exception"
             "plain"))))"##,
        expect![[
            r#"OK ("\\(?1:'\\)[0-9][0-9]s?\\|\\(?1:'\\)[Bb][Oo][Uu][Tt]\\|\\(?1:'\\)[Ee][Mm]\\|\\(?1:'\\)[Nn]'\\|\\(?1:'\\)[Cc][Aa][Uu][Ss][Ee]\\|\\(?1:'\\)[Rr][Oo][Uu][Nn][Dd]\\|\\(?1:'\\)[Tt][Ww][Aa][Ss]\\|\\(?1:'\\)[Tt][Ii][Ss]" (("'90s" "'" 0 1) ("'90" "'" 0 1) ("'bout" "'" 0 1) ("'Bout" "'" 0 1) ("'CAUSE" "'" 0 1) ("'round" "'" 0 1) ("'twas" "'" 0 1) ("'TIS" "'" 0 1) ("'em" "'" 0 1) ("'n'" "'" 0 1) nil nil))"#
        ]],
    )
}

fn astute_custom_prefix_exception_keyword_preserves_order_regexes_and_case_folding()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_custom_prefix_exception_keyword_preserves_order_regexes_and_case_folding",
        r##"(let* ((astute-prefix-single-quote-exceptions
                  '("ello"
                    "x.y"
                    "[ab]"))
                 (regexp
                  (car
                   (nth 3
                        (astute-init-font-lock)))))
         (list
          regexp
          (astute-test-match-summary
           regexp
           '("'ello"
             "'ELLO"
             "'x.y"
             "'Xay"
             "'a"
             "'b"
             "'c"
             "'20s"))))"##,
        expect![[
            r#"OK ("\\(?1:'\\)[0-9][0-9]s?\\|\\(?1:'\\)[Ee][Ll][Ll][Oo]\\|\\(?1:'\\)[Xx].[Yy]\\|\\(?1:'\\)[[Aa][Bb]]" (("'ello" "'" 0 1) ("'ELLO" "'" 0 1) ("'x.y" "'" 0 1) ("'Xay" "'" 0 1) nil nil nil ("'20s" "'" 0 1)))"#
        ]],
    )
}

fn astute_quote_regexes_report_open_close_and_inner_capture_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_quote_regexes_report_open_close_and_inner_capture_boundaries",
        r##"(list
         (astute-test-match-summary
          astute-double-quote-open-regexp
          '("\"word"
            "x\"word"
            "\" word"
            "\""
            "\"!"))
         (astute-test-match-summary
          astute-double-quote-close-regexp
          '("word\""
            "word\"x"
            "word \""
            "\""
            "!\""))
         (astute-test-match-summary
          astute-single-quote-open-regexp
          '("'word"
            "x'word"
            "' word"
            "'"
            "'!"))
         (astute-test-match-summary
          astute-single-quote-close-regexp
          '("word'"
            "word'x"
            "word '"
            "'"
            "!'"))
         (astute-test-match-summary
          astute-single-quote-inner-regexp
          '("don't"
            "rock'n'roll"
            "a'b"
            "a'1"
            "1'a"
            "a-'b"
            "ab")))"##,
        expect![[
            r#"OK ((("\"w" "\"" 0 1) ("\"w" "\"" 1 2) nil nil ("\"!" "\"" 0 1)) (("d\"" "\"" 4 5) ("d\"" "\"" 4 5) nil nil ("!\"" "\"" 1 2)) (("'w" "'" 0 1) ("'w" "'" 1 2) nil nil ("'!" "'" 0 1)) (("d'" "'" 4 5) ("d'" "'" 4 5) nil nil ("!'" "'" 1 2)) (nil nil nil nil nil nil nil))"#
        ]],
    )
}

fn astute_dash_regexes_distinguish_bounded_en_em_and_longer_hyphen_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_dash_regexes_distinguish_bounded_en_em_and_longer_hyphen_runs",
        r##"(list
         (astute-test-match-summary
          astute-en-dash-regexp
          '("a--b"
            "--lead"
            "trail--"
            "a---b"
            "a----b"
            "a -- b"
            "x--y--z"))
         (astute-test-match-summary
          astute-em-dash-regexp
          '("a---b"
            "---lead"
            "trail---"
            "a--b"
            "a----b"
            "a --- b"
            "x---y---z")))"##,
        expect![[
            r#"OK ((("a--b" "--" 1 3) nil nil nil nil (" -- " "--" 2 4) ("x--y" "--" 1 3)) (("a---b" "---" 1 4) nil nil nil nil (" --- " "---" 2 5) ("x---y" "---" 1 4)))"#
        ]],
    )
}

pub(super) fn casefold_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        astute_case_insensitize_expands_mixed_ascii_words_without_changing_nonletters(),
        astute_case_insensitive_exception_fragments_match_real_prefix_spellings(),
        astute_default_prefix_exception_keyword_matches_decades_and_configured_elisions(),
        astute_custom_prefix_exception_keyword_preserves_order_regexes_and_case_folding(),
        astute_quote_regexes_report_open_close_and_inner_capture_boundaries(),
        astute_dash_regexes_distinguish_bounded_en_em_and_longer_hyphen_runs(),
    ]
}
