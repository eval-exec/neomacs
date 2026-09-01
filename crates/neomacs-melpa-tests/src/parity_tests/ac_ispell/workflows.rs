use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow: writing prose, the user types a word stem
/// and gets English completions from the word list.  The documented setup is
/// `ac-ispell-setup' (which declares the sources from the current custom
/// values) followed by `ac-ispell-ac-setup' in the buffer, here bound to the
/// auto-complete trigger key so the completion is started by real typing.
fn ac_ispell_completes_a_typed_prose_word_from_the_real_word_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_completes_a_typed_prose_word_from_the_real_word_list",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "Please send me the rec"
  (ac-set-trigger-key "TAB")
  (ac-ispell-ac-setup)
  (let ((installed (list :sources ac-sources
                         :auto-complete auto-complete-mode
                         :ispell ac-source-ispell
                         :fuzzy ac-source-ispell-fuzzy)))
    (execute-kbd-macro (kbd "i p TAB"))
    (let ((offered (list (ac-ispell-test-session) (ac-ispell-test-menu))))
      (execute-kbd-macro (kbd "M-n"))
      (let ((moved (ac-ispell-test-session)))
        (execute-kbd-macro (kbd "RET"))
        (list :installed installed
              :offered offered
              :moved moved
              :after (ac-ispell-test-buffer-state)
              :lookups (ac-ispell-test-lookups)
              :speller (ac-ispell-test-speller-log)))))))"##,
        expect![[
            r#"OK (:installed (:sources #1=(ac-source-ispell ac-source-ispell-fuzzy) :auto-complete t :ispell ((candidates . ac-ispell--candidates) (requires . 3) (symbol . "s")) :fuzzy ((candidates . ac-ispell--fuzzy-candidates) (match lambda (prefix candidates) candidates) (requires . 3) (limit . 2) (symbol . "s") (candidate-face . ac-ispell-fuzzy-candidate-face))) :offered ((:prefix "recip" :prefix-start 19 :common "recip" :menu-live t :selected "recipe") (("recipe" "s" nil) ("recipient" "s" nil) ("reciprocal" "s" nil))) :moved (:prefix "recip" :prefix-start 19 :common "recip" :menu-live t :selected "recipient") :after (:text "Please send me the recipient" :point 28 :mode text-mode :auto-complete t :sources #1#) :lookups ("grep|-Ei|^recip.*$|[ORACLE-SANDBOX]/words.txt") :speller ("run|-a|-m|-B" "!" "-" "%" "^recip"))"#
        ]],
    )
}

fn ac_ispell_matches_the_case_of_the_typed_stem_and_reuses_one_lookup() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_matches_the_case_of_the_typed_stem_and_reuses_one_lookup",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "Recip"
  (ac-ispell-ac-setup)
  (auto-complete)
  (let ((capitalized (list (ac-ispell-test-session) (ac-ispell-test-menu))))
    (ac-complete)
    (let ((after-capitalized (ac-ispell-test-buffer-state)))
      (goto-char (point-max))
      (insert "\nRECIP")
      (auto-complete)
      (let ((upcased (list (ac-ispell-test-session) (ac-ispell-test-menu))))
        (ac-next)
        (ac-complete)
        (list :capitalized capitalized
              :after-capitalized after-capitalized
              :upcased upcased
              :after (ac-ispell-test-buffer-state)
              :lookups (ac-ispell-test-lookups)))))))"##,
        expect![[
            r#"OK (:capitalized ((:prefix "Recip" :prefix-start 0 :common "Recip" :menu-live t :selected "Recipe") (("Recipe" "s" nil) ("Recipient" "s" nil) ("Reciprocal" "s" nil))) :after-capitalized (:text "Recipe" :point 6 :mode text-mode :auto-complete t :sources #1=(ac-source-ispell ac-source-ispell-fuzzy)) :upcased ((:prefix "RECIP" :prefix-start 7 :common "RECIP" :menu-live t :selected "RECIPE") (("RECIPE" "s" nil) ("RECIPIENT" "s" nil) ("RECIPROCAL" "s" nil))) :after (:text "Recipe\nRECIPIENT" :point 16 :mode text-mode :auto-complete t :sources #1#) :lookups ("grep|-Ei|^recip.*$|[ORACLE-SANDBOX]/words.txt"))"#
        ]],
    )
}

fn ac_ispell_answers_a_longer_stem_from_its_cache_and_researches_a_new_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_answers_a_longer_stem_from_its_cache_and_researches_a_new_one",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "The recip"
  (ac-ispell-ac-setup)
  (auto-complete)
  (let ((first (list (ac-ispell-test-session) (ac-ispell-test-menu))))
    (ac-abort)
    (insert "i")
    (auto-complete)
    (let ((extended (ac-ispell-test-buffer-state)))
      (goto-char (point-max))
      (insert "\nThe rece")
      (auto-complete)
      (list :first first
            :extended extended
            :other-stem (list (ac-ispell-test-session) (ac-ispell-test-menu))
            :lookups (ac-ispell-test-lookups))))))"##,
        expect![[
            r#"OK (:first ((:prefix "recip" :prefix-start 4 :common "recip" :menu-live t :selected "recipe") (("recipe" "s" nil) ("recipient" "s" nil) ("reciprocal" "s" nil))) :extended (:text "The recipient" :point 13 :mode text-mode :auto-complete t :sources (ac-source-ispell ac-source-ispell-fuzzy)) :other-stem ((:prefix "rece" :prefix-start 18 :common "rece" :menu-live t :selected "recess") (("recess" "s" nil) ("receive" "s" nil) ("receiver" "s" nil) ("reception" "s" nil))) :lookups ("grep|-Ei|^recip.*$|[ORACLE-SANDBOX]/words.txt" "grep|-Ei|^rece.*$|[ORACLE-SANDBOX]/words.txt"))"#
        ]],
    )
}

fn ac_ispell_requires_gates_short_stems_and_a_customized_value_takes_effect() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_requires_gates_short_stems_and_a_customized_value_takes_effect",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (let ((default-run
        (ac-ispell-test-with-live-buffer #'text-mode "We re"
         (ac-ispell-ac-setup)
         (let ((short (list (auto-complete)
                            (ac-ispell-test-session)
                            (ac-ispell-test-menu)
                            (ac-ispell-test-lookups))))
           (insert "c")
           (auto-complete)
           (list :requires ac-ispell-requires
                 :fuzzy-limit ac-ispell-fuzzy-limit
                 :sources ac-sources
                 :short short
                 :long (list (ac-ispell-test-session) (ac-ispell-test-menu)))))))
   (custom-set-variables '(ac-ispell-requires 5) '(ac-ispell-fuzzy-limit 0))
   (ac-ispell-setup)
   (let ((custom-run
          (ac-ispell-test-with-live-buffer #'text-mode "We reci"
           (ac-ispell-ac-setup)
           (let ((short (list (auto-complete) (ac-ispell-test-menu))))
             (insert "p")
             (auto-complete)
             (list :ispell ac-source-ispell
                   :fuzzy ac-source-ispell-fuzzy
                   :sources ac-sources
                   :short short
                   :long (list (ac-ispell-test-session)
                               (ac-ispell-test-menu)))))))
     (list :default default-run
           :customized custom-run
           :lookups (ac-ispell-test-lookups)))))"##,
        expect![[
            r#"OK (:default (:requires 3 :fuzzy-limit 2 :sources (ac-source-ispell ac-source-ispell-fuzzy) :short (nil (:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil) nil nothing-recorded) :long ((:prefix "rec" :prefix-start 3 :common "rec" :menu-live t :selected "recall") (("recall" "s" nil) ("recess" "s" nil) ("recipe" "s" nil) ("recite" "s" nil) ("reckon" "s" nil) ("receive" "s" nil) ("receiver" "s" nil) ("reception" "s" nil) ("recipient" "s" nil) ("recommend" "s" nil) ("reciprocal" "s" nil)))) :customized (:ispell ((candidates . ac-ispell--candidates) (requires . 5) (symbol . "s")) :fuzzy ((candidates . ac-ispell--fuzzy-candidates) (match lambda (prefix candidates) candidates) (requires . 5) (limit . 0) (symbol . "s") (candidate-face . ac-ispell-fuzzy-candidate-face)) :sources (ac-source-ispell) :short (nil nil) :long ((:prefix "recip" :prefix-start 3 :common "recip" :menu-live t :selected "recipe") (("recipe" "s" nil) ("recipient" "s" nil) ("reciprocal" "s" nil)))) :lookups ("grep|-Ei|^rec.*$|[ORACLE-SANDBOX]/words.txt"))"#
        ]],
    )
    .fresh_process()
}

fn ac_ispell_fuzzy_source_offers_limited_speller_near_misses_for_a_misspelling() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ac_ispell_fuzzy_source_offers_limited_speller_near_misses_for_a_misspelling",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "Dear recipiant"
  (ac-ispell-ac-setup)
  (auto-complete)
  (let ((offered (list (ac-ispell-test-session) (ac-ispell-test-menu))))
    (ac-complete)
    (list :limit ac-ispell-fuzzy-limit
          :offered offered
          :after (ac-ispell-test-buffer-state)
          :lookups (ac-ispell-test-lookups)
          :speller (ac-ispell-test-speller-log)))))"##,
        expect![[
            r#"OK (:limit 2 :offered ((:prefix "recipiant" :prefix-start 5 :common nil :menu-live t :selected "recipient") (("recipient" "s" ac-ispell-fuzzy-candidate-face) ("recipients" "s" ac-ispell-fuzzy-candidate-face))) :after (:text "Dear recipient" :point 14 :mode text-mode :auto-complete t :sources (ac-source-ispell ac-source-ispell-fuzzy)) :lookups ("grep|-Ei|^recipiant.*$|[ORACLE-SANDBOX]/words.txt") :speller ("run|-a|-m|-B" "!" "-" "%" "^recipiant"))"#
        ]],
    )
    .fresh_process()
}

fn ac_ispell_offers_nothing_for_a_non_word_stem_or_an_unknown_word() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_offers_nothing_for_a_non_word_stem_or_an_unknown_word",
        r##"(progn
 (ac-ispell-test-setup)
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "encoding utf8"
  (ac-ispell-ac-setup)
  (let ((non-word (list (auto-complete)
                        (ac-ispell-test-menu)
                        (ac-ispell-test-buffer-state)
                        (ac-ispell-test-lookups)
                        (ac-ispell-test-speller-log))))
    (goto-char (point-max))
    (insert " and unmatchable")
    (list :non-word non-word
          :unknown-word (list (auto-complete)
                              (ac-ispell-test-menu)
                              (ac-ispell-test-buffer-state))
          :lookups (ac-ispell-test-lookups)
          :speller (ac-ispell-test-speller-log)))))"##,
        expect![[
            r#"OK (:non-word (t nil (:text "encoding utf8" :point 13 :mode text-mode :auto-complete t :sources #1=(ac-source-ispell ac-source-ispell-fuzzy)) nothing-recorded ("run|-a|-m|-B" "!" "-" "%" "^utf8")) :unknown-word (t nil (:text "encoding utf8 and unmatchable" :point 29 :mode text-mode :auto-complete t :sources #1#)) :lookups ("grep|-Ei|^unmatchable.*$|[ORACLE-SANDBOX]/words.txt") :speller ("run|-a|-m|-B" "!" "-" "%" "^utf8" "!" "-" "%" "^unmatchable"))"#
        ]],
    )
    .fresh_process()
}

fn ac_ispell_reports_a_missing_word_list_out_of_the_public_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_ispell_reports_a_missing_word_list_out_of_the_public_command",
        r##"(progn
 (ac-ispell-test-setup "dictionaries/words.txt")
 (ac-ispell-setup)
 (ac-ispell-test-with-live-buffer #'text-mode "The recip"
  (ac-ispell-ac-setup)
  (list :outcome (condition-case failure (auto-complete) (error failure))
        :menu (ac-ispell-test-menu)
        :after (ac-ispell-test-buffer-state)
        :dictionary ispell-complete-word-dict
        :lookups (ac-ispell-test-lookups)
        :speller (ac-ispell-test-speller-log))))"##,
        expect![[
            r#"OK (:outcome (error "ispell-lookup-words: Unreadable or missing plain word-list [ORACLE-SANDBOX]/dictionaries/words.txt") :menu nil :after (:text "The recip\n" :point 9 :mode text-mode :auto-complete t :sources (ac-source-ispell ac-source-ispell-fuzzy)) :dictionary "[ORACLE-SANDBOX]/dictionaries/words.txt" :lookups nothing-recorded :speller nothing-recorded)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_ispell_completes_a_typed_prose_word_from_the_real_word_list(),
        ac_ispell_matches_the_case_of_the_typed_stem_and_reuses_one_lookup(),
        ac_ispell_answers_a_longer_stem_from_its_cache_and_researches_a_new_one(),
        ac_ispell_requires_gates_short_stems_and_a_customized_value_takes_effect(),
        ac_ispell_fuzzy_source_offers_limited_speller_near_misses_for_a_misspelling(),
        ac_ispell_offers_nothing_for_a_non_word_stem_or_an_unknown_word(),
        ac_ispell_reports_a_missing_word_list_out_of_the_public_command(),
    ]
}
