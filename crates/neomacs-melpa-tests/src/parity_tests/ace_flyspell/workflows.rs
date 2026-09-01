use expect_test::expect;

use super::ParityBatchCase;

/// `ace-flyspell-jump-word' is the "just take me there" command: avy offers one
/// key per flyspell-flagged word in the window, in buffer order, and the chosen
/// key moves point to the start of that word.  Two jumps in a row pin the
/// candidate ordering (`d' is the third misspelling, `a' the first), the mark
/// avy pushes at the departure point, and that jumping alone never edits the
/// text, never touches flyspell's overlays and leaves ace-flyspell's own
/// highlight overlay deleted.
fn jumping_to_a_flagged_word_moves_point_and_leaves_the_text_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "jumping_to_a_flagged_word_moves_point_and_leaves_the_text_untouched",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer)
  (flyspell-mode 1)
  (flyspell-buffer)
  (global-set-key (kbd "C-c j") 'ace-flyspell-jump-word)
  (goto-char (point-min))
  (let ((observed nil))
    (execute-kbd-macro (kbd "C-c j d"))
    (push (list :point (point) :word (thing-at-point 'word t) :mark (mark))
          observed)
    (execute-kbd-macro (kbd "C-c j a"))
    (push (list :point (point) :word (thing-at-point 'word t) :mark (mark))
          observed)
    (push (list :text (buffer-substring-no-properties (point-min) (point-max))
                :overlays (afly-test-flyspell-overlays)
                :box (afly-test-box-overlay)
                :queries (afly-test-queries))
          observed)
    (nreverse observed)))"##,
        expect![[
            r#"OK ((:point 74 :word "occured" :mark 1) (:point 19 :word "recieve" :mark 74) (:text "The commitee will recieve the report.\nWe must seperate the two lists.\nIt occured twice, and it is definately wrong.\n" :overlays ((19 26 "recieve" flyspell-incorrect) (47 55 "seperate" flyspell-incorrect) (74 81 "occured" flyspell-incorrect) (99 109 "definately" flyspell-incorrect)) :box (nil nil ace-flyspell--background) :queries ("The" "commitee" "will" "recieve" "the" "report" "We" "must" "seperate" "the" "two" "lists" "It" "occured" "twice" "and" "it" "is" "definately" "wrong" "The" "occured" "The" "occured" "recieve")))"#
        ]],
    )
}

fn correcting_a_jumped_to_word_replaces_it_and_returns_point_where_it_was() -> ParityBatchCase {
    ParityBatchCase::value(
        "correcting_a_jumped_to_word_replaces_it_and_returns_point_where_it_was",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer)
  (flyspell-mode 1)
  (flyspell-buffer)
  (ace-flyspell-setup)
  (goto-char (point-max))
  (let ((mark (afly-test-message-mark))
        (origin (point)))
    (execute-kbd-macro (kbd "C-. d . q"))
    (list :origin origin
          :point (point)
          :mark (mark)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :overlays (afly-test-flyspell-overlays)
          :box (afly-test-box-overlay)
          :current-word ace-flyspell--current-word
          :queries (last (afly-test-queries) 3)
          :messages (afly-test-messages-since mark))))"##,
        expect![[
            r#"OK (:origin 117 :point 118 :mark 118 :text "The commitee will recieve the report.\nWe must seperate the two lists.\nIt occurred twice, and it is definately wrong.\n" :overlays ((19 26 "recieve" flyspell-incorrect) (47 55 "seperate" flyspell-incorrect) (100 110 "definately" flyspell-incorrect)) :box (nil nil ace-flyspell--background) :current-word "occured" :queries ("occured" "occurred" "wrong") :messages ("[.]: correct word; [,]: save to personal dictionary [2 times]" "Corrections: occurred occurred occurs occupied occured occurred occurs occupied occured" "[.]: correct word; [,]: save to personal dictionary [2 times]"))"#
        ]],
    )
}

fn declining_the_correction_with_control_g_restores_the_original_spelling() -> ParityBatchCase {
    ParityBatchCase::value(
        "declining_the_correction_with_control_g_restores_the_original_spelling",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer)
  (flyspell-mode 1)
  (flyspell-buffer)
  (ace-flyspell-setup)
  (goto-char (point-min))
  (let ((mark (afly-test-message-mark)))
    (execute-kbd-macro (kbd "C-. a . C-g"))
    (list :point (point)
          :mark (mark)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :overlays (afly-test-flyspell-overlays)
          :box (afly-test-box-overlay)
          :current-word ace-flyspell--current-word
          :queries (last (afly-test-queries) 2)
          :messages (afly-test-messages-since mark))))"##,
        expect![[
            r#"OK (:point 1 :mark 1 :text "The commitee will recieve the report.\nWe must seperate the two lists.\nIt occured twice, and it is definately wrong.\n" :overlays ((47 55 "seperate" flyspell-incorrect) (74 81 "occured" flyspell-incorrect) (99 109 "definately" flyspell-incorrect)) :box (nil nil ace-flyspell--background) :current-word "recieve" :queries ("receive" "The") :messages ("[.]: correct word; [,]: save to personal dictionary [2 times]" "Corrections: receive receive relieve reprieve recieve receive relieve reprieve recieve" "[.]: correct word; [,]: save to personal dictionary [2 times]"))"#
        ]],
    )
    .fresh_process()
}

fn dwim_corrects_the_word_under_point_without_starting_an_avy_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "dwim_corrects_the_word_under_point_without_starting_an_avy_selection",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer)
  (flyspell-mode 1)
  (flyspell-buffer)
  (ace-flyspell-setup)
  (goto-char 50)
  (let ((mark (afly-test-message-mark))
        (origin (list (point) (thing-at-point 'word t))))
    (execute-kbd-macro (kbd "C-. X"))
    (list :origin origin
          :point (point)
          :mark (mark)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :overlays (afly-test-flyspell-overlays)
          :box (afly-test-box-overlay)
          :messages (afly-test-messages-since mark))))"##,
        expect![[
            r#"OK (:origin (50 "seperate") :point 51 :mark nil :text "The commitee will recieve the report.\nWe must sepXarate the two lists.\nIt occured twice, and it is definately wrong.\n" :overlays ((19 26 "recieve" flyspell-incorrect) (75 82 "occured" flyspell-incorrect) (100 110 "definately" flyspell-incorrect)) :box (nil nil ace-flyspell--background) :messages ("Corrections: separate separate desperate temperate seperate separate desperate temperate"))"#
        ]],
    )
}

fn saving_a_word_to_the_personal_dictionary_stops_flyspell_flagging_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "saving_a_word_to_the_personal_dictionary_stops_flyspell_flagging_it",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer "The Umlaut is fine.\nBut recieve is not.\n")
  (setq ace-flyspell-new-word-no-query t)
  (flyspell-mode 1)
  (flyspell-buffer)
  (ace-flyspell-setup)
  (goto-char (point-max))
  (let ((mark (afly-test-message-mark))
        (before (afly-test-flyspell-overlays)))
    (execute-kbd-macro (kbd "C-. a , q"))
    (let ((after (afly-test-flyspell-overlays)))
      (flyspell-buffer)
      (list :before before
            :after after
            :rechecked (afly-test-flyspell-overlays)
            :point (point)
            :text (buffer-substring-no-properties (point-min) (point-max))
            :personal (with-temp-buffer
                        (insert-file-contents afly-test-personal)
                        (split-string (buffer-string) "\n" t))
            :session (afly-test-session)
            :messages (afly-test-messages-since mark)))))"##,
        expect![[
            r##"OK (:before ((5 11 "Umlaut" flyspell-incorrect) (25 32 "recieve" flyspell-incorrect)) :after ((25 32 "recieve" flyspell-incorrect)) :rechecked ((25 32 "recieve" flyspell-incorrect)) :point 41 :text "The Umlaut is fine.\nBut recieve is not.\n" :personal ("Umlaut") :session ("run|-a|-m|-B" "!" "-" "!" "-" "*Umlaut" "#" "saved") :messages ("[.]: correct word; [,]: save to personal dictionary [2 times]" "Personal dictionary saved." "[.]: correct word; [,]: save to personal dictionary"))"##
        ]],
    )
    .fresh_process()
}

fn a_correctly_spelled_buffer_offers_zero_candidates_and_changes_nothing() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_correctly_spelled_buffer_offers_zero_candidates_and_changes_nothing",
        r##"(progn
  (afly-test-setup)
  (afly-test-buffer "All of these words are spelled correctly.\n")
  (flyspell-mode 1)
  (flyspell-buffer)
  (ace-flyspell-setup)
  (goto-char 5)
  (let ((mark (afly-test-message-mark)))
    (execute-kbd-macro (kbd "C-."))
    (list :point (point)
          :mark (mark)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :overlays (afly-test-flyspell-overlays)
          :box (afly-test-box-overlay)
          :queries (afly-test-queries)
          :messages (afly-test-messages-since mark))))"##,
        expect![[
            r#"OK (:point 5 :mark nil :text "All of these words are spelled correctly.\n" :overlays nil :box (nil nil ace-flyspell--background) :queries ("All" "of" "these" "words" "are" "spelled" "correctly" "of") :messages ("zero candidates"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        jumping_to_a_flagged_word_moves_point_and_leaves_the_text_untouched(),
        correcting_a_jumped_to_word_replaces_it_and_returns_point_where_it_was(),
        declining_the_correction_with_control_g_restores_the_original_spelling(),
        dwim_corrects_the_word_under_point_without_starting_an_avy_selection(),
        saving_a_word_to_the_personal_dictionary_stops_flyspell_flagging_it(),
        a_correctly_spelled_buffer_offers_zero_candidates_and_changes_nothing(),
    ]
}
