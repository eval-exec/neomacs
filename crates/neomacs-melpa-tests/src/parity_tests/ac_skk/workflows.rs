use expect_test::expect;

use super::ParityBatchCase;

/// The workflow the package exists for.  Typing `Kanji' in SKK's kana mode
/// puts the buffer into henkan mode showing `▽かんじ'; ac-skk then offers not
/// just the four conversions of かんじ but the conversions of every reading the
/// personal dictionary completes かんじ to — かんじゃ and かんじょう — each
/// candidate carrying the reading it came from and its index in that reading's
/// candidate list.  Choosing 感じ re-inserts the reading, converts to the
/// second candidate and commits, leaving 感じ in the document.
fn converts_a_typed_reading_into_kanji_through_the_completion_menu() -> ParityBatchCase {
    ParityBatchCase::value(
        "converts_a_typed_reading_into_kanji_through_the_completion_menu",
        r##"
        (progn
          (ac-skk-test-install-jisyo)
          (ac-skk-test-open "memo.txt")
          (execute-kbd-macro "Kanji")
          (list
           :typed (append (ac-skk-test-state)
                          (list :prefix-is-start-point
                                (equal (ac-skk-prefix) skk-henkan-start-point)))
           :offered (progn
                      (auto-complete)
                      (list :ac-prefix ac-prefix
                            :candidates
                            (ac-skk-test-candidate-details ac-candidates)
                            :menu (ac-skk-test-menu)
                            :selected (ac-skk-test-selected)))
           :committed (progn
                        (ac-next)
                        (ac-complete)
                        (append (ac-skk-test-state)
                                (list :buffer (buffer-substring-no-properties
                                               (point-min) (point-max)))))))
    "##,
        expect![[
            r##"OK (:typed (:line "本日の議題▽かんじ" :point 18 :henkan-mode on :j-mode t :prefix-is-start-point t) :offered (:ac-prefix "かんじ" :candidates (("漢字" "かんじ" 0 ac-skk-kakutei) ("感じ" "かんじ" 1 ac-skk-kakutei) ("幹事" "かんじ" 2 ac-skk-kakutei) ("監事" "かんじ" 3 ac-skk-kakutei) ("患者" "かんじゃ" 0 ac-skk-kakutei) ("勘定" "かんじょう" 0 ac-skk-kakutei) ("感情" "かんじょう" 1 ac-skk-kakutei)) :menu ("漢字" "感じ" "幹事" "監事" "患者" "勘定" "感情") :selected "漢字") :committed (:line "本日の議題感じ" :point 16 :henkan-mode nil :j-mode t :buffer "# 会議メモ\n\n本日の議題感じ\n\n参加者\n\n場所\n\n時間\n\n決定事項\n\n次回の予定\n\n以上\n"))"##
        ]],
    )
}

fn learns_the_chosen_conversion_and_offers_it_first_next_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "learns_the_chosen_conversion_and_offers_it_first_next_time",
        r##"
        (progn
          (ac-skk-test-install-jisyo)
          (ac-skk-test-open "memo.txt")
          (list
           :first (progn
                    (execute-kbd-macro "Kanji")
                    (auto-complete)
                    (let ((menu (ac-skk-test-menu)))
                      (ac-next)
                      (ac-complete)
                      (list :menu menu :line (ac-skk-test-line))))
           :jisyo-after-first (ac-skk-test-jisyo-buffer)
           :second (progn
                     (forward-line 2)
                     (end-of-line)
                     (execute-kbd-macro "Kanji")
                     (auto-complete)
                     (let ((menu (ac-skk-test-menu)))
                       (ac-complete)
                       (list :menu menu :line (ac-skk-test-line))))
           :jisyo-after-second (ac-skk-test-jisyo-buffer)
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :on-disk (list :jisyo-bytes-unchanged
                          (equal (ac-skk-test-file-bytes skk-jisyo)
                                 (append (encode-coding-string
                                          ac-skk-test-jisyo-entries 'utf-8)
                                         nil))
                          :home (sort (directory-files (getenv "HOME")) #'string<))))
    "##,
        expect![[
            r##"OK (:first (:menu ("漢字" "感じ" "幹事" "監事" "患者" "勘定" "感情") :line "本日の議題感じ") :jisyo-after-first ";; okuri-ari entries.\n;; okuri-nasi entries.\nかんじ /感じ/漢字/幹事/監事/\nかんじゃ /患者/\nかんじょう /勘定/感情/\nかんきょう /環境/\nにほんご /日本語/\nにほん /日本/二本/\nご /語/五/\n" :second (:menu ("感じ" "漢字" "幹事" "監事" "患者" "勘定" "感情") :line "参加者感じ") :jisyo-after-second ";; okuri-ari entries.\n;; okuri-nasi entries.\nかんじ /感じ/漢字/幹事/監事/\nかんじゃ /患者/\nかんじょう /勘定/感情/\nかんきょう /環境/\nにほんご /日本語/\nにほん /日本/二本/\nご /語/五/\n" :buffer "# 会議メモ\n\n本日の議題感じ\n\n参加者感じ\n\n場所\n\n時間\n\n決定事項\n\n次回の予定\n\n以上\n" :on-disk (:jisyo-bytes-unchanged t :home ("." ".." ".emacs.d" ".skk-record")))"##
        ]],
    )
    .fresh_process()
}

fn reads_a_legacy_encoded_dictionary_and_converts_out_of_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "reads_a_legacy_encoded_dictionary_and_converts_out_of_it",
        r##"
        (let ((coding (skk-find-coding-system skk-jisyo-code)))
          (ac-skk-test-install-jisyo coding)
          (list
           :dictionary (list :configured skk-jisyo-code
                             :resolved coding
                             :first-entry-bytes
                             (seq-subseq (ac-skk-test-file-bytes skk-jisyo) 44 62)
                             :size (nth 7 (file-attributes skk-jisyo)))
           :session (progn
                      (ac-skk-test-open "memo.txt")
                      (execute-kbd-macro "Kanji")
                      (auto-complete)
                      (list :line (ac-skk-test-line)
                            :candidates
                            (ac-skk-test-candidate-details ac-candidates)))
           :committed (progn
                        (ac-next)
                        (ac-next)
                        (ac-complete)
                        (append (ac-skk-test-state)
                                (list :buffer (buffer-substring-no-properties
                                               (point-min) (point-max)))))))
    "##,
        expect![[
            r##"OK (:dictionary (:configured nil :resolved euc-jis-2004 :first-entry-bytes (10 164 171 164 243 164 184 32 47 180 193 187 250 47 180 182 164 184) :size 179) :session (:line "本日の議題▽かんじ" :candidates (("漢字" "かんじ" 0 ac-skk-kakutei) ("感じ" "かんじ" 1 ac-skk-kakutei) ("幹事" "かんじ" 2 ac-skk-kakutei) ("監事" "かんじ" 3 ac-skk-kakutei) ("患者" "かんじゃ" 0 ac-skk-kakutei) ("勘定" "かんじょう" 0 ac-skk-kakutei) ("感情" "かんじょう" 1 ac-skk-kakutei))) :committed (:line "本日の議題幹事" :point 16 :henkan-mode nil :j-mode t :buffer "# 会議メモ\n\n本日の議題幹事\n\n参加者\n\n場所\n\n時間\n\n決定事項\n\n次回の予定\n\n以上\n"))"##
        ]],
    )
    .fresh_process()
}

fn completes_plain_kana_with_the_hiracomp_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_plain_kana_with_the_hiracomp_source",
        r##"
        (progn
          (ac-skk-test-install-jisyo)
          (ac-skk-test-open "memo.txt")
          (list
           :typed (progn
                    (goto-char (point-min))
                    (forward-line 3)
                    (execute-kbd-macro "nihongo")
                    (append (ac-skk-test-state)
                            (list :skk-source-prefix (ac-skk-prefix))))
           :offered (progn
                      (ac-start :force-init t)
                      (ac-update t)
                      (list :ac-prefix ac-prefix
                            :ac-point ac-point
                            :candidates
                            (mapcar (lambda (candidate)
                                      (list (substring-no-properties candidate)
                                            (get-text-property 0 'action candidate)))
                                    ac-candidates)))
           :converted (progn
                        (auto-complete)
                        (ac-complete)
                        (append (ac-skk-test-state)
                                (list :menu-live (ac-menu-live-p))))
           :marked (progn
                     (forward-line 2)
                     (end-of-line)
                     (execute-kbd-macro "nihongo")
                     (auto-complete)
                     (dotimes (_ 4) (ac-next))
                     (let ((chosen (ac-skk-test-selected)))
                       (ac-complete)
                       (append (list :chosen chosen) (ac-skk-test-state))))
           :buffer (buffer-substring-no-properties (point-min) (point-max))))
    "##,
        expect![[
            r##"OK (:typed (:line "にほんご" :point 19 :henkan-mode nil :j-mode t :skk-source-prefix nil) :offered (:ac-prefix "にほんご" :ac-point 15 :candidates (("日本語" nil) ("ニホンゴ" nil) ("▽にほんご" ac-skk-hiracomp-mes) ("に▽ほんご" ac-skk-hiracomp-mes) ("にほ▽んご" ac-skk-hiracomp-mes) ("にほん▽ご" ac-skk-hiracomp-mes))) :converted (:line "日本語" :point 18 :henkan-mode nil :j-mode t :menu-live nil) :marked (:chosen "にほ▽んご" :line "にほ▽んご" :point 28 :henkan-mode on :j-mode t) :buffer "# 会議メモ\n\n本日の議題\n日本語\n参加者\nにほ▽んご\n場所\n\n時間\n\n決定事項\n\n次回の予定\n\n以上\n")"##
        ]],
    )
    .fresh_process()
}

fn enabling_and_leaving_skk_mode_restore_the_original_sources() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_and_leaving_skk_mode_restore_the_original_sources",
        r##"
        (progn
          (ac-skk-test-install-jisyo)
          (list
           :flag (list :initial ac-skk-enable
                       :after-enable (progn (call-interactively 'ac-skk-enable)
                                            ac-skk-enable)
                       :after-disable (progn (call-interactively 'ac-skk-disable)
                                             ac-skk-enable)
                       :after-toggle (progn (call-interactively 'ac-skk-toggle)
                                            ac-skk-enable)
                       :after-toggle-again (progn (call-interactively 'ac-skk-toggle)
                                                  ac-skk-enable))
           :while-disabled
           (with-current-buffer (get-buffer-create "*ac-skk-off*")
             (skk-mode 1)
             (prog1 (ac-skk-test-ac-state)
               (skk-mode -1)))
           :while-enabled
           (progn
             (call-interactively 'ac-skk-enable)
             (with-current-buffer (get-buffer-create "*ac-skk-on*")
               (list :before (ac-skk-test-ac-state)
                     :after-skk-mode (progn (skk-mode 1) (ac-skk-test-ac-state))
                     :after-ascii-input (progn (skk-latin-mode 1)
                                               (ac-skk-test-ac-state))
                     :after-kana-input (progn (skk-j-mode-on)
                                              (ac-skk-test-ac-state))
                     :after-leaving-skk (progn (skk-mode -1)
                                               (ac-skk-test-ac-state)))))
           :messages (ac-skk-test-messages)))
    "##,
        expect![[
            r#"OK (:flag (:initial nil :after-enable t :after-disable nil :after-toggle t :after-toggle-again nil) :while-disabled (:sources #1=(ac-source-words-in-same-mode-buffers) :trigger-head (self-insert-command) :saved-sources nil :trigger-is-local nil) :while-enabled (:before (:sources #1# :trigger-head (self-insert-command) :saved-sources nil :trigger-is-local nil) :after-skk-mode (:sources #2=(ac-source-skk ac-source-skk-hiracomp) :trigger-head (skk-insert skk-previous-candidate) :saved-sources #1# :trigger-is-local t) :after-ascii-input (:sources #1# :trigger-head (self-insert-command) :saved-sources #1# :trigger-is-local t) :after-kana-input (:sources #2# :trigger-head (skk-previous-candidate skk-insert) :saved-sources #1# :trigger-is-local t) :after-leaving-skk (:sources #1# :trigger-head (self-insert-command) :saved-sources nil :trigger-is-local nil)) :messages ("enabled ac-skk." "disabled ac-skk." "enabled ac-skk." "disabled ac-skk." "enabled ac-skk."))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        converts_a_typed_reading_into_kanji_through_the_completion_menu(),
        learns_the_chosen_conversion_and_offers_it_first_next_time(),
        reads_a_legacy_encoded_dictionary_and_converts_out_of_it(),
        completes_plain_kana_with_the_hiracomp_source(),
        enabling_and_leaving_skk_mode_restore_the_original_sources(),
    ]
}
