use expect_test::expect;

use super::ParityBatchCase;

fn auto_dictionary_conditional_insert_selects_exact_dictionary_and_registers_overlay()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_conditional_insert_selects_exact_dictionary_and_registers_overlay",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary
                "de"))
           (insert "Alice ")
           (adict-conditional-insert
            "en" "writes"
            "de" "schreibt"
            t "wrote")
           (let ((overlay
                  (car
                   adict-conditional-overlay-list)))
             (list
              (buffer-string)
              (length
               adict-conditional-overlay-list)
              (adict-test-overlay-state
               overlay)
              (and
               (memq
                #'adict-conditional-update
                adict-change-dictionary-hook)
               t)))))"##,
        expect![[
            r#"OK ("Alice schreibt" 1 (7 15 t adict-conditional-text-face ("en" "writes" "de" "schreibt" t "wrote") (adict-conditional-modification)) t)"#
        ]],
    )
}

fn auto_dictionary_conditional_insert_uses_fallback_or_empty_text_when_unmatched() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_conditional_insert_uses_fallback_or_empty_text_when_unmatched",
        r##"(list
         (with-temp-buffer
           (let ((ispell-local-dictionary
                  "fr"))
             (adict-conditional-insert
              "en" "writes"
              "de" "schreibt"
              t "wrote")
             (list
              (buffer-string)
              (adict-test-overlay-state
               (car
                adict-conditional-overlay-list)))))
         (with-temp-buffer
           (let ((ispell-local-dictionary
                  "fr"))
             (adict-conditional-insert
              "en" "writes"
              "de" "schreibt")
             (list
              (buffer-string)
              (adict-test-overlay-state
               (car
                adict-conditional-overlay-list))))))"##,
        expect![[
            r#"OK (("wrote" (1 6 t adict-conditional-text-face ("en" "writes" "de" "schreibt" t "wrote") #1=(adict-conditional-modification))) ("" (nil nil t adict-conditional-text-face ("en" "writes" "de" "schreibt") #1#)))"#
        ]],
    )
}

fn auto_dictionary_conditional_update_replaces_text_in_place_and_moves_overlay() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_conditional_update_replaces_text_in_place_and_moves_overlay",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en"))
           (insert "Bob ")
           (adict-conditional-insert
            "en" "writes"
            "de" "schreibt"
            "fr" "écrit")
           (insert ":\n")
           (let* ((overlay
                   (car
                    adict-conditional-overlay-list))
                  (before
                   (list
                    (buffer-string)
                    (adict-test-overlay-state
                     overlay))))
             (setq ispell-local-dictionary
                   "de")
             (adict-conditional-update)
             (let ((german
                    (list
                     (buffer-string)
                     (adict-test-overlay-state
                      overlay))))
               (setq ispell-local-dictionary
                     "fr")
               (adict-conditional-update)
               (list
                before
                german
                (list
                 (buffer-string)
                 (adict-test-overlay-state
                  overlay)))))))"##,
        expect![[
            r#"OK (("Bob writes:\n" (5 11 t adict-conditional-text-face #1=("en" "writes" "de" "schreibt" "fr" "écrit") #2=(adict-conditional-modification))) ("Bob schreibt:\n" (5 13 t adict-conditional-text-face #1# #2#)) ("Bob écrit:\n" (5 10 t adict-conditional-text-face #1# #2#)))"#
        ]],
    )
}

fn auto_dictionary_conditional_update_handles_multiple_independent_insertions() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_conditional_update_handles_multiple_independent_insertions",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en"))
           (adict-conditional-insert
            "en" "Hello"
            "de" "Hallo")
           (insert ", team. Alice ")
           (adict-conditional-insert
            "en" "writes"
            "de" "schreibt")
           (insert ".")
           (let ((before
                  (buffer-string))
                 (bounds-before
                  (mapcar
                   (lambda (overlay)
                     (cons
                      (overlay-start overlay)
                      (overlay-end overlay)))
                   adict-conditional-overlay-list)))
             (setq ispell-local-dictionary
                   "de")
             (adict-conditional-update)
             (list
              before
              (buffer-string)
              bounds-before
              (mapcar
               (lambda (overlay)
                 (cons
                  (overlay-start overlay)
                  (overlay-end overlay)))
               adict-conditional-overlay-list)
              (length
               adict-conditional-overlay-list)))))"##,
        expect![[
            r#"OK ("Hello, team. Alice writes." "Hallo, team. Alice schreibt." ((20 . 26) (1 . 6)) ((20 . 28) (1 . 6)) 2)"#
        ]],
    )
}

fn auto_dictionary_user_edit_inside_only_conditional_overlay_unregisters_update_hook()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_user_edit_inside_only_conditional_overlay_unregisters_update_hook",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en"))
           (adict-conditional-insert
            "en" "writes"
            "de" "schreibt")
           (let ((overlay
                  (car
                   adict-conditional-overlay-list)))
             (goto-char
              (1+ (overlay-start overlay)))
             (insert "!")
             (list
              (buffer-string)
              (overlay-buffer overlay)
              (local-variable-p
               'adict-conditional-overlay-list)
              adict-conditional-overlay-list
              (memq
               #'adict-conditional-update
               adict-change-dictionary-hook)))))"##,
        expect![[r#"OK ("w!rites" nil nil nil nil)"#]],
    )
}

fn auto_dictionary_editing_one_of_two_conditional_overlays_keeps_remaining_update_live()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_editing_one_of_two_conditional_overlays_keeps_remaining_update_live",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en"))
           (adict-conditional-insert
            "en" "hello"
            "de" "hallo")
           (insert " ")
           (adict-conditional-insert
            "en" "friend"
            "de" "freund")
           (let* ((first
                   (car
                    (last
                     adict-conditional-overlay-list)))
                  (second
                   (car
                    adict-conditional-overlay-list)))
             (goto-char
              (1+ (overlay-start first)))
             (insert "!")
             (setq ispell-local-dictionary
                   "de")
             (adict-conditional-update)
             (list
              (buffer-string)
              (overlay-buffer first)
              (and
               (overlay-buffer second)
               t)
              (length
               adict-conditional-overlay-list)
              (and
               (memq
                #'adict-conditional-update
                adict-change-dictionary-hook)
               t)))))"##,
        expect![[r#"OK ("h!ello freund" nil t 1 t)"#]],
    )
}

fn auto_dictionary_conditional_insert_prefers_first_duplicate_dictionary_pair() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_conditional_insert_prefers_first_duplicate_dictionary_pair",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en"))
           (adict-conditional-insert
            "en" "first"
            "en" "second"
            t "fallback")
           (list
            (buffer-string)
            (overlay-get
             (car
              adict-conditional-overlay-list)
             'adict-conditional-list))))"##,
        expect![[r#"OK ("first" ("en" "first" "en" "second" t "fallback"))"#]],
    )
}

fn auto_dictionary_dictionary_change_hook_updates_practical_localized_signature() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_dictionary_change_hook_updates_practical_localized_signature",
        r##"(with-temp-buffer
         (let ((ispell-local-dictionary "en")
               (changes nil)
               (adict-change-dictionary-hook
                nil)
               (adict-stop-updating-on-dictionary-change
                nil))
           (insert "Regards,\nAlice — ")
           (adict-conditional-insert
            "en" "Engineering"
            "de" "Entwicklung"
            "fr" "Ingénierie"
            t "Team")
           (cl-letf
               (((symbol-function
                  'ispell-change-dictionary)
                 (lambda (lang)
                   (push lang changes)
                   (setq
                    ispell-local-dictionary
                    lang))))
             (let ((english
                    (buffer-string)))
               (adict-change-dictionary
                "de")
               (let ((german
                      (buffer-string)))
                 (adict-change-dictionary
                  "fr")
                 (list
                  english
                  german
                  (buffer-string)
                  (nreverse changes)
                  adict-lighter
                  (length
                   adict-conditional-overlay-list)))))))"##,
        expect![[
            r#"OK ("Regards,\nAlice — Engineering" "Regards,\nAlice — Entwicklung" "Regards,\nAlice — Ingénierie" ("de" "fr") " fr" 1)"#
        ]],
    )
}

pub(super) fn conditional_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dictionary_conditional_insert_selects_exact_dictionary_and_registers_overlay(),
        auto_dictionary_conditional_insert_uses_fallback_or_empty_text_when_unmatched(),
        auto_dictionary_conditional_update_replaces_text_in_place_and_moves_overlay(),
        auto_dictionary_conditional_update_handles_multiple_independent_insertions(),
        auto_dictionary_user_edit_inside_only_conditional_overlay_unregisters_update_hook(),
        auto_dictionary_editing_one_of_two_conditional_overlays_keeps_remaining_update_live(),
        auto_dictionary_conditional_insert_prefers_first_duplicate_dictionary_pair(),
        auto_dictionary_dictionary_change_hook_updates_practical_localized_signature(),
    ]
}
