use expect_test::expect;

use super::ParityBatchCase;

fn auto_dictionary_dictionary_name_search_honors_candidate_and_valid_list_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_dictionary_name_search_honors_candidate_and_valid_list_order",
        r##"(list
         (adict-guess-dictionary-name
          '("de" "deutsch" "german")
          '("francais" "deutsch" "english"))
         (adict-guess-dictionary-name
          '("de" "deutsch" "german")
          '("francais" "english"))
         (adict-guess-dictionary-name
          '("german" "deutsch")
          '("deutsch" "german"))
         (let ((adict-test-valid-dictionaries
                '("english" "deutsch")))
           (adict-guess-dictionary-name
            '("en" "english")))
         (adict-guess-dictionary-name nil
                                      '("en")))"##,
        expect![[r#"OK ("deutsch" nil "german" "english" nil)"#]],
    )
}

fn auto_dictionary_dictionary_cons_preserves_language_even_when_dictionary_missing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_dictionary_cons_preserves_language_even_when_dictionary_missing",
        r##"(let ((adict-test-valid-dictionaries
                                '("deutsch" "english")))
         (list
          (adict--guess-dictionary-cons
           '("de" "deutsch"))
          (adict--guess-dictionary-cons
           '("en" "english"))
          (adict--guess-dictionary-cons
           '("fr" "francais" "french"))))"##,
        expect![[r#"OK (("de" . "deutsch") ("en" . "english") ("fr"))"#]],
    )
}

fn auto_dictionary_custom_type_is_derived_from_language_list_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_custom_type_is_derived_from_language_list_in_order",
        r##"(let ((adict-language-list
                                '(nil "de" "en" "eo")))
         (adict--dictionary-alist-type))"##,
        expect![[
            r#"OK (repeat (cons (choice (const "de") (const "en") (const "eo")) (choice (const :tag "Off" nil) (string :tag "Dictionary name"))))"#
        ]],
    )
}

fn auto_dictionary_real_multilingual_buffer_produces_exact_score_vector_and_winner()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_real_multilingual_buffer_produces_exact_score_vector_and_winner",
        r##"(with-temp-buffer
         (insert
          "Hello dear friend, you are welcome and we have some news.\n"
          "Bonjour, vous allez revoir cette nouvelle.\n"
          "Zwei deutsche Wörter sind zunächst dafür.\n")
         (list
          (append
           (adict-evaluate-buffer)
           nil)
          (adict-guess-buffer-language)
          (adict--evaluate-buffer-find-max-index
           nil)
          (adict--evaluate-buffer-find-lang
           nil)
          (adict--evaluate-buffer-find-dictionary
           nil)))"##,
        expect![[r#"OK ((7 8 4 5 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0) "en" 1 "en" "en")"#]],
    )
}

fn auto_dictionary_tied_scores_choose_lowest_positive_language_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_tied_scores_choose_lowest_positive_language_index",
        r##"(list
         (cl-letf
             (((symbol-function
                'adict-evaluate-buffer)
               (lambda (_idle)
                 [12 4 4 4])))
           (adict--evaluate-buffer-find-max-index
            nil))
         (cl-letf
             (((symbol-function
                'adict-evaluate-buffer)
               (lambda (_idle)
                 [0 2 7 7 7])))
           (adict--evaluate-buffer-find-max-index
            t))
         (cl-letf
             (((symbol-function
                'adict-evaluate-buffer)
               (lambda (_idle)
                 [99 0 0])))
           (adict--evaluate-buffer-find-max-index
            nil)))"##,
        expect!["OK (1 2 1)"],
    )
}

fn auto_dictionary_current_and_legacy_dictionary_formats_map_same_language_index() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_current_and_legacy_dictionary_formats_map_same_language_index",
        r##"(list
         (let ((adict-language-list
                '(nil "de" "en" "fr"))
               (adict-dictionary-list
                '(("de" . "de_DE")
                  ("en" . "en_US")
                  ("fr" . "fr_FR"))))
           (cl-letf
               (((symbol-function
                  'adict--evaluate-buffer-find-max-index)
                 (lambda (_idle) 2)))
             (list
              (adict--evaluate-buffer-find-lang
               nil)
              (adict--evaluate-buffer-find-dictionary
               nil))))
         (let ((adict-language-list
                '(nil "de" "en" "fr"))
               (adict-dictionary-list
                '(nil "de_DE" "en_US" "fr_FR")))
           (cl-letf
               (((symbol-function
                  'adict--evaluate-buffer-find-max-index)
                 (lambda (_idle) 2)))
             (list
              (adict--evaluate-buffer-find-lang
               t)
              (adict--evaluate-buffer-find-dictionary
               t)))))"##,
        expect![[r#"OK (("en" "en_US") ("en" "en_US"))"#]],
    )
}

fn auto_dictionary_disabled_language_still_detects_language_but_returns_no_dictionary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_disabled_language_still_detects_language_but_returns_no_dictionary",
        r##"(with-temp-buffer
         (insert
          "kaj ĉu ankaŭ antaŭ morgaŭ ĉar ŝi ĉiam estas ĉi tie")
         (let ((adict-dictionary-list
                '(("en" . "en")
                  ("eo" . nil))))
           (list
            (adict-guess-buffer-language)
            (adict--evaluate-buffer-find-dictionary
             nil)
            (adict-guess-dictionary))))"##,
        expect![[r#"OK ("eo" nil nil)"#]],
    )
}

fn auto_dictionary_foreach_word_handles_punctuation_case_length_and_region_bounds()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_foreach_word_handles_punctuation_case_length_and_region_bounds",
        r##"(with-temp-buffer
         (insert
          "zero HELLO, bonjour extraordinarily drei; goodbye")
         (let (words)
           (adict-foreach-word
            6
            (- (point-max) 8)
            8
            (lambda (word)
              (push word words)))
           (nreverse words)))"##,
        expect![[r#"OK ("HELLO" "bonjour" "drei" "goodbye")"#]],
    )
}

fn auto_dictionary_foreach_word_respects_flyspell_generic_word_predicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_foreach_word_respects_flyspell_generic_word_predicate",
        r##"(with-temp-buffer
         (insert
          "hello skipme bonjour keepme")
         (let ((flyspell-generic-check-word-p
                (lambda ()
                  (not
                   (member
                    (thing-at-point 'word t)
                    '("skipme" "keepme")))))
               words)
           (adict-foreach-word
            (point-min)
            (point-max)
            20
            (lambda (word)
              (push word words)))
           (nreverse words)))"##,
        expect![[r#"OK ("hello" "bonjour")"#]],
    )
}

fn auto_dictionary_foreach_word_excludes_conditional_overlay_text_from_scoring() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_foreach_word_excludes_conditional_overlay_text_from_scoring",
        r##"(with-temp-buffer
         (insert "hello ")
         (let ((overlay
                (make-overlay
                 (point) (point))))
           (insert "bonjour")
           (move-overlay overlay 7 (point))
           (overlay-put
            overlay
            'adict-conditional-list
            '("fr" "bonjour"))
           (goto-char (point-max))
           (insert " drei")
           (let (words)
             (adict-foreach-word
              (point-min)
              (point-max)
              20
              (lambda (word)
                (push word words)))
             (list
              (nreverse words)
              (append
               (adict-evaluate-buffer)
               nil)))))"##,
        expect![[r#"OK (("hello" "drei") (0 1 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0))"#]],
    )
}

fn auto_dictionary_idle_only_scan_stops_when_input_becomes_pending() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_idle_only_scan_stops_when_input_becomes_pending",
        r##"(with-temp-buffer
         (insert
          "hello bonjour zunächst además svenska pozdrav")
         (let ((polls 0)
               words)
           (cl-letf
               (((symbol-function
                  'input-pending-p)
                 (lambda ()
                   (setq polls (1+ polls))
                   (> polls 4))))
             (adict-foreach-word
              (point-min)
              (point-max)
              20
              (lambda (word)
                (push word words))
              t)
             (list
              (nreverse words)
              polls))))"##,
        expect![[r#"OK (("hello" "bonjour" "zunächst" "además") 5)"#]],
    )
}

fn auto_dictionary_third_party_word_and_buffer_apis_cover_case_unknown_and_idle_abort()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_third_party_word_and_buffer_apis_cover_case_unknown_and_idle_abort",
        r##"(list
         (mapcar
          #'adict-guess-word-language
          '("HELLO" "Bonjour" "MORGAŬ"
            "unknown"))
         (with-temp-buffer
           (insert
            "morgaŭ kaj ĉiam ŝi estas ĉi tie")
           (adict-guess-buffer-language))
         (with-temp-buffer
           (insert "hello dear friend")
           (cl-letf
               (((symbol-function
                  'input-pending-p)
                 (lambda () t)))
             (adict-guess-buffer-language t))))"##,
        expect![[r#"OK (("en" "fr" "eo" nil) "eo" nil)"#]],
    )
}

pub(super) fn detection_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dictionary_dictionary_name_search_honors_candidate_and_valid_list_order(),
        auto_dictionary_dictionary_cons_preserves_language_even_when_dictionary_missing(),
        auto_dictionary_custom_type_is_derived_from_language_list_in_order(),
        auto_dictionary_real_multilingual_buffer_produces_exact_score_vector_and_winner(),
        auto_dictionary_tied_scores_choose_lowest_positive_language_index(),
        auto_dictionary_current_and_legacy_dictionary_formats_map_same_language_index(),
        auto_dictionary_disabled_language_still_detects_language_but_returns_no_dictionary(),
        auto_dictionary_foreach_word_handles_punctuation_case_length_and_region_bounds(),
        auto_dictionary_foreach_word_respects_flyspell_generic_word_predicate(),
        auto_dictionary_foreach_word_excludes_conditional_overlay_text_from_scoring(),
        auto_dictionary_idle_only_scan_stops_when_input_becomes_pending(),
        auto_dictionary_third_party_word_and_buffer_apis_cover_case_unknown_and_idle_abort(),
    ]
}
