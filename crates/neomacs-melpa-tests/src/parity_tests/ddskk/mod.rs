use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DDSKK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DDSKK_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn ddskk_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DDSKK_MELPA_PIN, "skk.el")
        .expect("prepare pinned DDSKK source below ./tmp")
        .with_prelude(
            r##"
(require 'skk-comp)
(require 'skk-num)

(defun neomacs-ddskk-test-run (dictionary action)
  (let* ((root (make-temp-file "neomacs-ddskk-" t))
         (large-jisyo (make-temp-file (expand-file-name "skk-large-jisyo" root)))
         (user-jisyo (make-temp-file (expand-file-name "skk-jisyo" root))))
    (unwind-protect
        (progn
          (with-temp-buffer
            (insert dictionary)
            (write-region (point-min) (point-max) large-jisyo nil 'silent))
          (with-temp-buffer
            (save-window-excursion
              (switch-to-buffer (current-buffer))
              (let ((default-directory root)
                    (temporary-file-directory root)
                    (small-temporary-file-directory root)
                    (skk-initial-search-jisyo nil)
                    (skk-large-jisyo (cons large-jisyo "UTF-8"))
                    (skk-cdb-large-jisyo nil)
                    (skk-aux-large-jisyo nil)
                    (skk-extra-jisyo-file-list nil)
                    (skk-jisyo user-jisyo)
                    (skk-jisyo-code "utf-8")
                    (skk-inhibit-ja-dic-search t)
                    (skk-save-jisyo nil)
                    (skk-kakutei-count 0)
                    ;; Opening the personal dictionary initializes DDSKK's
                    ;; section markers before any conversion updates it.
                    (jisyo-buffer (skk-get-jisyo-buffer user-jisyo 'nomsg)))
                (skk-mode 1)
                (prog1 (funcall action user-jisyo)
                  (when jisyo-buffer
                    (with-current-buffer jisyo-buffer
                      (set-buffer-modified-p nil))
                    (kill-buffer jisyo-buffer))
                  (let ((large-buffer (get-file-buffer large-jisyo)))
                    (when large-buffer
                      (kill-buffer large-buffer))))))))
      (when (file-directory-p root)
        (delete-directory root t)))))
"##,
        )
        .with_timeout(DDSKK_TEST_TIMEOUT)
}

fn japanese_note_switches_hiragana_katakana_and_latin_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "japanese_note_switches_hiragana_katakana_and_latin_input",
        r##"
(neomacs-ddskk-test-run
 ";; okuri-ari entries.
;; okuri-nasi entries.
"
 (lambda (_user-jisyo)
   (execute-kbd-macro (kbd "k a n a q k a n a q l a b c C-j a"))
   (list :text (buffer-substring-no-properties (point-min) (point-max))
         :skk-mode skk-mode
         :j-mode skk-j-mode
         :katakana skk-katakana
         :latin skk-latin-mode
         :henkan skk-henkan-mode)))
"##,
        expect![[
            r##"OK (:text "かなカナabcあ" :skk-mode t :j-mode t :katakana nil :latin nil :henkan nil)"##
        ]],
    )
}

fn candidate_selection_is_learned_and_reused_without_the_large_dictionary() -> ParityBatchCase {
    ParityBatchCase::value(
        "candidate_selection_is_learned_and_reused_without_the_large_dictionary",
        r##"
(neomacs-ddskk-test-run
 ";; okuri-ari entries.
;; okuri-nasi entries.
かんじ /幹事/漢字/換字/
"
 (lambda (user-jisyo)
   (execute-kbd-macro (kbd "K a n j i SPC SPC C-j"))
   (let ((chosen (buffer-substring-no-properties (point-min) (point-max)))
         learned-entry)
     (with-current-buffer (skk-get-jisyo-buffer user-jisyo)
       (goto-char (point-min))
       (re-search-forward "^かんじ .*$")
       (setq learned-entry (match-string-no-properties 0)))
     (erase-buffer)
     (setq skk-large-jisyo nil)
     (execute-kbd-macro (kbd "K a n j i SPC C-j"))
     (list :chosen chosen
           :learned-entry learned-entry
           :reused-without-large-jisyo
           (buffer-substring-no-properties (point-min) (point-max))))))
"##,
        expect![[
            r##"OK (:chosen "漢字" :learned-entry "かんじ /漢字/" :reused-without-large-jisyo "漢字")"##
        ]],
    )
}

fn inflected_words_and_sahen_verbs_are_composed_in_a_sentence() -> ParityBatchCase {
    ParityBatchCase::value(
        "inflected_words_and_sahen_verbs_are_composed_in_a_sentence",
        r##"
(list
 :feeling
 (neomacs-ddskk-test-run
  ";; okuri-ari entries.
かんj /感/
;; okuri-nasi entries.
"
  (lambda (_user-jisyo)
    (execute-kbd-macro (kbd "K a n J i C-j"))
    (buffer-substring-no-properties (point-min) (point-max))))
 :running
 (neomacs-ddskk-test-run
  ";; okuri-ari entries.
はしr /走/
;; okuri-nasi entries.
"
  (lambda (_user-jisyo)
    (execute-kbd-macro (kbd "H a s i R u C-j"))
    (buffer-substring-no-properties (point-min) (point-max))))
 :sahen
 (neomacs-ddskk-test-run
  ";; okuri-ari entries.
;; okuri-nasi entries.
へんかん /変換/
"
  (lambda (_user-jisyo)
    (setq-local skk-search-sagyo-henkaku t)
    (execute-kbd-macro (kbd "H e n k a n S u C-j r u"))
    (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[r##"OK (:feeling "感じ" :running "走る" :sahen "変換する")"##]],
    )
}

fn invoice_entry_converts_numbers_and_respects_punctuation_style() -> ParityBatchCase {
    ParityBatchCase::value(
        "invoice_entry_converts_numbers_and_respects_punctuation_style",
        r##"
(list
 :amount
 (neomacs-ddskk-test-run
  ";; okuri-ari entries.
;; okuri-nasi entries.
#えん /#3円/
"
  (lambda (_user-jisyo)
    (execute-kbd-macro (kbd "Q 1 2 3 e n SPC C-j"))
    (buffer-substring-no-properties (point-min) (point-max))))
 :punctuation
 (neomacs-ddskk-test-run
  ";; okuri-ari entries.
;; okuri-nasi entries.
"
  (lambda (_user-jisyo)
    (setq-local skk-kutouten-type 'jp)
    (execute-kbd-macro [?. ?, return])
    (setq-local skk-kutouten-type 'en)
    (execute-kbd-macro [?. ?,])
    (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[r##"OK (:amount "百二十三円" :punctuation "。、\n．，")"##]],
    )
}

fn learned_abbreviation_completion_replays_a_symbol_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "learned_abbreviation_completion_replays_a_symbol_entry",
        r##"
(neomacs-ddskk-test-run
 ";; okuri-ari entries.
;; okuri-nasi entries.
alpha /α/
"
 (lambda (_user-jisyo)
   (execute-kbd-macro (kbd "/ a l p h a SPC C-j"))
   (let ((first (buffer-substring-no-properties (point-min) (point-max))))
     (execute-kbd-macro (kbd "/ a TAB SPC C-j"))
     (list :first first
           :after-completion
           (buffer-substring-no-properties (point-min) (point-max))))))
"##,
        expect![[r##"OK (:first "α" :after-completion "αα")"##]],
    )
}

#[test]
fn ddskk_package_batch() {
    let cases = vec![
        japanese_note_switches_hiragana_katakana_and_latin_input(),
        candidate_selection_is_learned_and_reused_without_the_large_dictionary(),
        inflected_words_and_sahen_verbs_are_composed_in_a_sentence(),
        invoice_entry_converts_numbers_and_respects_punctuation_style(),
        learned_abbreviation_completion_replays_a_symbol_entry(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed DDSKK parity test");
    assert_oracle_batch_cases(ddskk_oracle(), test_name, "ddskk_parity", &cases);
}
