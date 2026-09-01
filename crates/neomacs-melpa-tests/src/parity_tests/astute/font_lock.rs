use expect_test::expect;

use super::ParityBatchCase;

fn astute_fontifies_real_prose_quotes_and_dashes_without_changing_buffer_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_fontifies_real_prose_quotes_and_dashes_without_changing_buffer_text",
        r##"(astute-test-fontify
         "He said \"Hello\" -- then 'good-bye' --- done."
         '(single-quote
           double-quote
           en-dash
           em-dash))"##,
        expect![[
            r#"OK ("He said \"Hello\" -- then 'good-bye' --- done." nil t 8 ((8 34 "“") (14 34 "”") (16 45 "–") (17 45 "–") (24 39 "‘") (33 39 "’") (35 45 "—") (36 45 "—") (37 45 "—")))"#
        ]],
    )
}

fn astute_fontifies_contractions_elisions_and_decades_with_closing_single_quotes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_fontifies_contractions_elisions_and_decades_with_closing_single_quotes",
        r##"(astute-test-fontify
         "don't 'tis 'Twas '90s 'em 'bout rock 'n' roll"
         '(single-quote))"##,
        expect![[
            r#"OK ("don't 'tis 'Twas '90s 'em 'bout rock 'n' roll" nil t 4 ((3 39 "’") (6 39 "’") (11 39 "’") (17 39 "’") (22 39 "’") (26 39 "’") (37 39 "’") (39 39 "’")))"#
        ]],
    )
}

fn astute_quote_boundary_rules_cover_start_end_spacing_punctuation_and_nesting() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_quote_boundary_rules_cover_start_end_spacing_punctuation_and_nesting",
        r##"(astute-test-fontify
         "\"start\" middle \"two words\" end\"; ('inner'), ['bracket'] \"!\""
         '(single-quote
           double-quote))"##,
        expect![[
            r#"OK ("\"start\" middle \"two words\" end\"; ('inner'), ['bracket'] \"!\"" nil t 6 ((0 34 "“") (6 34 "”") (15 34 "“") (25 34 "”") (30 34 "”") (34 39 "’") (40 39 "’") (45 39 "’") (53 39 "’") (56 34 "“") (58 34 "”")))"#
        ]],
    )
}

fn astute_dash_fontification_distinguishes_two_three_and_longer_hyphen_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_dash_fontification_distinguishes_two_three_and_longer_hyphen_runs",
        r##"(astute-test-fontify
         "--lead a--b a---b a----b a-----b trail-- | x -- y --- z"
         '(en-dash
           em-dash))"##,
        expect![[
            r#"OK ("--lead a--b a---b a----b a-----b trail-- | x -- y --- z" nil t 2 ((8 45 "–") (9 45 "–") (13 45 "—") (14 45 "—") (15 45 "—") (38 45 "–") (39 45 "–") (45 45 "–") (46 45 "–") (50 45 "—") (51 45 "—") (52 45 "—")))"#
        ]],
    )
}

fn astute_selected_transform_subset_changes_only_double_quotes_and_em_dashes() -> ParityBatchCase {
    ParityBatchCase::value(
        "astute_selected_transform_subset_changes_only_double_quotes_and_em_dashes",
        r##"(astute-test-fontify
         "'single' \"double\" a--b a---b don't"
         '(double-quote
           em-dash))"##,
        expect![[
            r#"OK ("'single' \"double\" a--b a---b don't" nil t 3 ((9 34 "“") (16 34 "”") (24 45 "—") (25 45 "—") (26 45 "—")))"#
        ]],
    )
}

fn astute_empty_or_unknown_transform_configuration_leaves_practical_text_unmodified()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_empty_or_unknown_transform_configuration_leaves_practical_text_unmodified",
        r##"(list
         (astute-test-fontify
          "'single' \"double\" a--b a---b"
          nil)
         (astute-test-fontify
          "'single' \"double\" a--b a---b"
          '(unknown)))"##,
        expect![[
            r#"OK (("'single' \"double\" a--b a---b" nil t 0 nil) ("'single' \"double\" a--b a---b" nil t 0 nil))"#
        ]],
    )
}

fn astute_multiline_unicode_prose_tracks_exact_display_positions_across_newlines() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_multiline_unicode_prose_tracks_exact_display_positions_across_newlines",
        r##"(astute-test-fontify
         "Résumé -- \"naïve\"\n'Tokyo' --- café\n'cause déjà-vu"
         '(single-quote
           double-quote
           en-dash
           em-dash))"##,
        expect![[
            r#"OK ("Résumé -- \"naïve\"\n'Tokyo' --- café\n'cause déjà-vu" nil t 8 ((7 45 "–") (8 45 "–") (10 34 "“") (16 34 "”") (18 39 "‘") (24 39 "’") (26 45 "—") (27 45 "—") (28 45 "—") (35 39 "’")))"#
        ]],
    )
}

fn astute_custom_prefix_exceptions_apply_case_insensitively_during_real_fontification()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_custom_prefix_exceptions_apply_case_insensitively_during_real_fontification",
        r##"(astute-test-fontify
         "'ello 'ELLO 'x.y 'X.Y 'alpha '20s 'bout"
         '(single-quote)
         '("ello"
           "x.y"))"##,
        expect![[
            r#"OK ("'ello 'ELLO 'x.y 'X.Y 'alpha '20s 'bout" nil t 4 ((0 39 "’") (6 39 "’") (12 39 "’") (17 39 "’") (22 39 "‘") (29 39 "’") (34 39 "‘")))"#
        ]],
    )
}

fn astute_refontification_after_insertions_updates_new_typography_and_keeps_existing_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_refontification_after_insertions_updates_new_typography_and_keeps_existing_text",
        r##"(with-temp-buffer
         (insert
          "\"first\" -- one")
         (text-mode)
         (setq-local
          astute-transform-list
          '(double-quote
            en-dash
            em-dash))
         (astute-mode 1)
         (font-lock-ensure)
         (let ((before
                (astute-test-display-map)))
           (goto-char
            (point-max))
           (insert
            " and \"second\" --- two")
           (font-lock-ensure)
           (list
            before
            (buffer-substring-no-properties
             (point-min)
             (point-max))
            (astute-test-display-map)
            astute-mode
            (length astute--keywords))))"##,
        expect![[
            r#"OK (((0 34 "“") (6 34 "”") (8 45 "–") (9 45 "–")) "\"first\" -- one and \"second\" --- two" ((0 34 "“") (6 34 "”") (8 45 "–") (9 45 "–") (19 34 "“") (26 34 "”") (28 45 "—") (29 45 "—") (30 45 "—")) t 4)"#
        ]],
    )
}

fn astute_fontification_preserves_help_echo_and_custom_properties_while_face_stays_font_lock_managed()
-> ParityBatchCase {
    ParityBatchCase::value(
        "astute_fontification_preserves_help_echo_and_custom_properties_while_face_stays_font_lock_managed",
        r##"(with-temp-buffer
         (insert
          "\"quoted\" -- plain")
         (text-mode)
         (add-text-properties
          1
          9
          '(face bold
            help-echo "quoted help"
            astute-test-property 17))
         (set-buffer-modified-p nil)
         (astute-mode 1)
         (font-lock-ensure)
         (list
          (buffer-substring-no-properties
           (point-min)
           (point-max))
          (buffer-modified-p)
          (astute-test-display-map)
          (get-text-property 2 'face)
          (get-text-property 2 'help-echo)
          (get-text-property
           2
           'astute-test-property)))"##,
        expect![[
            r#"OK ("\"quoted\" -- plain" nil ((0 34 "“") (7 34 "”") (9 45 "–") (10 45 "–")) nil "quoted help" 17)"#
        ]],
    )
}

fn astute_dense_editorial_paragraph_handles_adjacent_real_world_typography_cases() -> ParityBatchCase
{
    ParityBatchCase::value(
        "astute_dense_editorial_paragraph_handles_adjacent_real_world_typography_cases",
        r##"(astute-test-fontify
         "In '84, \"editors\" said: 'Tis useful--sometimes---but don't overdo it. \"Really?\" 'Yes!'"
         '(single-quote
           double-quote
           en-dash
           em-dash))"##,
        expect![[
            r#"OK ("In '84, \"editors\" said: 'Tis useful--sometimes---but don't overdo it. \"Really?\" 'Yes!'" nil t 8 ((3 39 "’") (8 34 "“") (16 34 "”") (24 39 "’") (35 45 "–") (36 45 "–") (46 45 "—") (47 45 "—") (48 45 "—") (56 39 "’") (70 34 "“") (78 34 "”") (80 39 "‘") (85 39 "’")))"#
        ]],
    )
}

pub(super) fn font_lock_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        astute_fontifies_real_prose_quotes_and_dashes_without_changing_buffer_text(),
        astute_fontifies_contractions_elisions_and_decades_with_closing_single_quotes(),
        astute_quote_boundary_rules_cover_start_end_spacing_punctuation_and_nesting(),
        astute_dash_fontification_distinguishes_two_three_and_longer_hyphen_runs(),
        astute_selected_transform_subset_changes_only_double_quotes_and_em_dashes(),
        astute_empty_or_unknown_transform_configuration_leaves_practical_text_unmodified(),
        astute_multiline_unicode_prose_tracks_exact_display_positions_across_newlines(),
        astute_custom_prefix_exceptions_apply_case_insensitively_during_real_fontification(),
        astute_refontification_after_insertions_updates_new_typography_and_keeps_existing_text(),
        astute_fontification_preserves_help_echo_and_custom_properties_while_face_stays_font_lock_managed(),
        astute_dense_editorial_paragraph_handles_adjacent_real_world_typography_cases(),
    ]
}
