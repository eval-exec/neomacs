use expect_test::expect;

use super::ParityBatchCase;

fn ac_emoji_completes_a_shortcode_while_writing_a_commit_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_completes_a_shortcode_while_writing_a_commit_message",
        r##"(ac-emoji-test-in-buffer
 (ac-emoji-setup)
 (insert "Ship the release :rocke")
 (let ((candidates (ac-emoji-test-candidates))
       (prefix ac-prefix))
   (ac-complete)
   (insert " and celebrate :tada")
   (let ((second (ac-emoji-test-candidates)))
     (ac-complete)
     (list candidates
           prefix
           second
           (buffer-string)
           (point)
           ac-sources))))"##,
        expect![[
            r#"OK ((":rocket:") ":rocke" (":tada:") "Ship the release :rocket: and celebrate :tada:" 47 (ac-source-emoji))"#
        ]],
    )
}

fn ac_emoji_prefix_only_triggers_after_a_colon_followed_by_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_prefix_only_triggers_after_a_colon_followed_by_text",
        r##"(ac-emoji-test-in-buffer
 (ac-emoji-setup)
 (let (observed)
   (dolist (text '("plain word" "half :" "emoji :sm" "mid:word" ":smile"))
     (erase-buffer)
     (insert text)
     (let ((candidates (ac-emoji-test-candidates)))
       (push (list text ac-prefix (length candidates) (car candidates)) observed)))
   (list (nreverse observed)
         (cdr (assq 'prefix ac-source-emoji))
         (cdr (assq 'candidates ac-source-emoji)))))"##,
        expect![[
            r#"OK ((("plain word" nil 0 nil) ("half :" nil 0 nil) ("emoji :sm" ":sm" 12 ":smile:") ("mid:word" ":word" 0 nil) (":smile" ":smile" 4 ":smile:")) ":\\S-+" ac-emoji--candidates)"#
        ]],
    )
}

fn ac_emoji_candidates_carry_the_description_and_the_emoji_character() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_candidates_carry_the_description_and_the_emoji_character",
        r##"(list
 (length ac-emoji--candidates)
 (length ac-emoji--data)
 (mapcar #'ac-emoji-test-item '(":rocket:" ":tada:" ":+1:" ":jp:" ":heart:"))
 (ac-emoji-test-item ":definitely-not-an-emoji:")
 (seq-take (mapcar #'substring-no-properties ac-emoji--candidates) 5)
 (car ac-emoji--data)
 (car (last ac-emoji--data)))"##,
        expect![[
            r#"OK (845 845 ((":rocket:" "rocket" "🚀") (":tada:" "party popper" "🎉") (":+1:" "thumbs up sign" "👍") (":jp:" "regional indicator symbol letter j + regional indicator symbol letter p" "🇯") (":heart:" "heavy black heart" "❤")) nil (":smile:" ":smiley:" ":grinning:" ":blush:" ":relaxed:") (:key ":smile:" :codepoint "😄" :description "smiling face with open mouth and smiling eyes") (:key ":small_blue_diamond:" :codepoint "🔹" :description "small blue diamond"))"#
        ]],
    )
}

fn ac_emoji_setup_is_buffer_local_and_repeatable() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_setup_is_buffer_local_and_repeatable",
        r##"(let ((global-before (default-value 'ac-sources)))
  (ac-emoji-test-in-buffer
   (kill-local-variable 'ac-sources)
   (let ((before (list ac-sources (local-variable-p 'ac-sources))))
     (ac-emoji-setup)
     (ac-emoji-setup)
     (let ((after (list ac-sources (local-variable-p 'ac-sources))))
       (list global-before
             before
             after
             (default-value 'ac-sources)
             (equal global-before (default-value 'ac-sources))
             (commandp 'ac-emoji-setup))))))"##,
        expect![
            "OK (#1=(ac-source-words-in-same-mode-buffers) (#1# nil) ((ac-source-emoji . #1#) t) #1# t t)"
        ],
    )
}

fn ac_emoji_narrows_the_candidate_list_as_more_of_the_name_is_typed() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_narrows_the_candidate_list_as_more_of_the_name_is_typed",
        r##"(ac-emoji-test-in-buffer
 (ac-emoji-setup)
 (let (observed)
   (dolist (typed '(":sm" ":smi" ":smil" ":smile"))
     (erase-buffer)
     (insert "Progress " typed)
     (let ((candidates (ac-emoji-test-candidates)))
       (push (list typed (length candidates) (seq-take candidates 4)) observed)))
   (erase-buffer)
   (insert "Progress :smile")
   (ac-emoji-test-candidates)
   (ac-complete)
   (list (nreverse observed) (buffer-string) (point))))"##,
        expect![[
            r#"OK (((":sm" 12 (":smile:" ":smirk:" ":smiley:" ":smoking:")) (":smi" 7 (":smile:" ":smirk:" ":smiley:" ":smile_cat:")) (":smil" 5 (":smile:" ":smiley:" ":smile_cat:" ":smiley_cat:")) (":smile" 4 (":smile:" ":smiley:" ":smile_cat:" ":smiley_cat:"))) "Progress :smile:" 17)"#
        ]],
    )
}

fn ac_emoji_leaves_unknown_shortcodes_and_unicode_prose_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_emoji_leaves_unknown_shortcodes_and_unicode_prose_untouched",
        r##"(ac-emoji-test-in-buffer
 (ac-emoji-setup)
 (insert "Résumé für die Prüfung :zzzzznotanemoji")
 (let ((unknown (ac-emoji-test-candidates))
       (after-unknown (buffer-string)))
   (erase-buffer)
   (insert "已经完成 :100")
   (let ((candidates (ac-emoji-test-candidates)))
     (ac-complete)
     (list unknown
           after-unknown
           candidates
           (buffer-string)
           (point)
           (buffer-size)))))"##,
        expect![[
            r#"OK (nil "Résumé für die Prüfung :zzzzznotanemoji\n\n\n\n\n\n\n\n\n\n\n" (":100:") "已经完成 :100:" 11 10)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_emoji_completes_a_shortcode_while_writing_a_commit_message(),
        ac_emoji_prefix_only_triggers_after_a_colon_followed_by_text(),
        ac_emoji_candidates_carry_the_description_and_the_emoji_character(),
        ac_emoji_setup_is_buffer_local_and_repeatable(),
        ac_emoji_narrows_the_candidate_list_as_more_of_the_name_is_typed(),
        ac_emoji_leaves_unknown_shortcodes_and_unicode_prose_untouched(),
    ]
}
