use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline story: a user writing Japanese types the romaji
/// `kanji' at the end of a Japanese sentence and auto-complete offers the
/// readings and the kanji conversions of that word.  This pins the candidate
/// list in order (both phases: the readings mozc offers for the preedit, then
/// the conversions it offers after the conversion key), the prefix and its
/// starting point, the complete key-by-key conversation ac-mozc had with the
/// helper -- one `SendKey' per romaji letter, then the space that asks for the
/// conversion -- and that starting completion left the buffer text alone.
fn typing_romaji_offers_the_kana_reading_and_its_kanji_conversions() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_romaji_offers_the_kana_reading_and_its_kanji_conversions",
        r##"(progn
  (ac-mozc-test-setup)
  (ac-mozc-test-with-buffer
   'ac-source-mozc "今日の天気はkanji"
   (let* ((candidates (ac-mozc-test-complete))
          (prefix ac-prefix)
          (point ac-point)
          (symbol (get-text-property 0 'symbol (car ac-candidates)))
          (action (get-text-property 0 'action (car ac-candidates))))
     (ac-abort)
     (list :candidates candidates
           :prefix prefix
           :prefix-start point
           :prefix-function (ac-mozc-prefix)
           :popup-symbol symbol
           :popup-action action
           :buffer (buffer-string)
           :point (point)
           :traffic (ac-mozc-test-traffic)))))"##,
        expect![[
            r#"OK (:candidates ("かんじ" "カンジ" "漢字" "感じ" "幹事") :prefix "kanji" :prefix-start 7 :prefix-function 7 :popup-symbol "M" :popup-action ac-mozc-action :buffer "今日の天気はkanji" :point 12 :traffic (("start" "--suppress_stderr") ("(0 CreateSession)") ("(1 SendKey 1 107)") ("(2 SendKey 1 97)") ("(3 SendKey 1 110)") ("(4 SendKey 1 106)") ("(5 SendKey 1 105)") ("(6 SendKey 1 space)")))"#
        ]],
    )
}

fn completing_inserts_the_japanese_word_and_removes_the_space_before_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_inserts_the_japanese_word_and_removes_the_space_before_it",
        r##"(progn
  (ac-mozc-test-setup)
  (list
   :remove-space
   (ac-mozc-test-with-buffer
    'ac-source-mozc "hello ohayou"
    (ac-mozc-test-complete)
    (ac-complete)
    (list (buffer-string) (point) ac-mozc-ac-point ac-mozc-remove-space))
   :keep-space
   (ac-mozc-test-with-buffer
    'ac-source-mozc "hello ohayou"
    (let ((ac-mozc-remove-space nil))
      (ac-mozc-test-complete)
      (ac-complete)
      (list (buffer-string) (point) ac-mozc-ac-point ac-mozc-remove-space)))))"##,
        expect![[
            r#"OK (:remove-space ("helloおはよう" 10 nil t) :keep-space ("hello おはよう" 11 nil nil))"#
        ]],
    )
}

fn the_prefix_is_the_romaji_run_that_ends_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_prefix_is_the_romaji_run_that_ends_at_point",
        r##"(progn
  (ac-mozc-test-setup)
  (ac-mozc-test-with-buffer
   'ac-source-mozc ""
   (let ((observed nil))
     (dolist (text '("日本語のnihongo" "kanji" "foo bar-baz" "123" "abc " "" "?!"))
       (erase-buffer)
       (insert text)
       (push (list text (ac-mozc-prefix)) observed))
     (erase-buffer)
     (insert "日本語のnihongo")
     (let* ((candidates (ac-mozc-test-complete))
            (prefix ac-prefix)
            (start ac-point))
       (ac-abort)
       (list :prefixes (nreverse observed)
             :candidates candidates
             :prefix prefix
             :prefix-start start)))))"##,
        expect![[
            r#"OK (:prefixes (("日本語のnihongo" 5) ("kanji" 1) ("foo bar-baz" 5) ("123" nil) ("abc " nil) ("" nil) ("?!" 1)) :candidates ("にほんご" "ニホンゴ" "日本語") :prefix "nihongo" :prefix-start 5)"#
        ]],
    )
}

fn input_that_mozc_cannot_read_as_kana_offers_nothing_and_is_never_converted() -> ParityBatchCase {
    ParityBatchCase::value(
        "input_that_mozc_cannot_read_as_kana_offers_nothing_and_is_never_converted",
        r##"(progn
  (ac-mozc-test-setup)
  (list
   :not-kana
   (ac-mozc-test-with-buffer
    'ac-source-mozc "xyz"
    (let ((candidates (ac-mozc-test-complete)))
      (ac-abort)
      (list candidates (ac-mozc-test-traffic))))
   :kana
   (ac-mozc-test-with-buffer
    'ac-source-mozc "kanji"
    (let ((candidates (ac-mozc-test-complete)))
      (ac-abort)
      (list candidates (last (ac-mozc-test-traffic) 2))))))"##,
        expect![[
            r#"OK (:not-kana (nil (("start" "--suppress_stderr") ("(0 CreateSession)") ("(1 SendKey 1 120)") ("(2 SendKey 1 121)") ("(3 SendKey 1 122)"))) :kana (("かんじ" "カンジ" "漢字" "感じ" "幹事") (("(9 SendKey 2 105)") ("(10 SendKey 2 space)"))))"#
        ]],
    )
    .fresh_process()
}

fn the_ascii_word_source_completes_words_embedded_in_japanese_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_ascii_word_source_completes_words_embedded_in_japanese_text",
        r##"(progn
  (ac-mozc-test-setup)
  (let ((notes (generate-new-buffer "*notes*"))
        (program (generate-new-buffer "*program*")))
    (unwind-protect
        (progn
          (with-current-buffer notes
            (text-mode)
            (insert "変数名myVariable を使う\nmyFunction(引数)\n日本語only\nplain english words\n"))
          (with-current-buffer program
            (prog-mode)
            (insert "myProgModeWord\n"))
          (ac-mozc-test-with-buffer
           'ac-source-ascii-words-in-same-mode-buffers "私はmy"
           (let ((candidates (ac-mozc-test-complete))
                 (prefix ac-prefix)
                 (start ac-point))
             (ac-abort)
             (erase-buffer)
             (insert "日本語のplain")
             (let ((second (ac-mozc-test-complete)))
               (ac-abort)
               (list :candidates candidates
                     :prefix prefix
                     :prefix-start start
                     :second second
                     :split (ac-mozc-remove-non-ascii-character
                             '("変数名myVariable" "日本語only" "plain" "全角のみ"))
                     :partial (ac-mozc-partial-match
                               "my" '("myVariable" "myFunction" "dummyValue" "plain"))
                     :traffic (ac-mozc-test-traffic))))))
      (kill-buffer notes)
      (kill-buffer program))))"##,
        expect![[
            r#"OK (:candidates ("myVariable" "myFunction") :prefix "my" :prefix-start 3 :second ("plain") :split ("myVariable" "only" "plain") :partial ("myVariable" "myFunction") :traffic nothing-recorded)"#
        ]],
    )
}

fn a_missing_or_dying_mozc_helper_reports_the_failure_and_disables_mozc_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_missing_or_dying_mozc_helper_reports_the_failure_and_disables_mozc_mode",
        r##"(progn
  (ac-mozc-test-setup)
  (list
   :missing
   (let ((mozc-helper-program-name "mozc_emacs_helper_not_installed")
         (mark (ac-mozc-test-message-mark)))
     (setq mozc-helper-process nil mozc-session-id nil)
     (ac-mozc-test-with-buffer
      'ac-source-mozc "kanji"
      (list (condition-case error (ac-mozc-test-complete) (error error))
            mozc-mode
            (buffer-string)
            (ac-mozc-test-messages-since mark))))
   :dying
   (let ((mozc-helper-program-name
          (expand-file-name "mozc_emacs_helper_dying" ac-mozc-test-bin))
         (mark (ac-mozc-test-message-mark)))
     (setq mozc-helper-process nil mozc-session-id nil)
     (ac-mozc-test-with-buffer
      'ac-source-mozc "kanji"
      (list (condition-case error (ac-mozc-test-complete) (error error))
            mozc-mode
            (buffer-string)
            (ac-mozc-test-messages-since mark))))))"##,
        expect![[
            r#"OK (:missing ((mozc-helper-process-error) nil "kanji\n" ("mozc.el: Starting mozc-helper-process..." "mozc.el: Failed to start mozc-helper-process.")) :dying ((error "Mozc session failed.") nil "kanji\n" ("mozc.el: Starting mozc-helper-process...done" "mozc.el: No response from the server")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_romaji_offers_the_kana_reading_and_its_kanji_conversions(),
        completing_inserts_the_japanese_word_and_removes_the_space_before_it(),
        the_prefix_is_the_romaji_run_that_ends_at_point(),
        input_that_mozc_cannot_read_as_kana_offers_nothing_and_is_never_converted(),
        the_ascii_word_source_completes_words_embedded_in_japanese_text(),
        a_missing_or_dying_mozc_helper_reports_the_failure_and_disables_mozc_mode(),
    ]
}
