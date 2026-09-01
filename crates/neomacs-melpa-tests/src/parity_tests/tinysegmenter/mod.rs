use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TINYSEGMENTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TINYSEGMENTER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const TINYSEGMENTER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'tinysegmenter)

(defun tinysegmenter-parity-spans (tokens)
  (let ((offset 0)
        spans)
    (dolist (token tokens (nreverse spans))
      (let ((end (+ offset (length token))))
        (push (list token :start offset :end end) spans)
        (setq offset end)))))

(defun tinysegmenter-parity-record (text)
  (let ((tokens (tseg-segment text)))
    (list
     :input text
     :length (length text)
     :tokens tokens
     :count (length tokens)
     :spans (tinysegmenter-parity-spans tokens)
     :rejoined (mapconcat #'identity tokens ""))))

(defun tinysegmenter-parity-frequency (token-lists)
  (let (frequency)
    (dolist (tokens token-lists)
      (dolist (token tokens)
        (let ((cell (assoc token frequency)))
          (if cell
              (setcdr cell (1+ (cdr cell)))
            (push (cons token 1) frequency)))))
    (sort frequency (lambda (left right) (string< (car left) (car right))))))
"####;

fn tinysegmenter_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TINYSEGMENTER_MELPA_PIN, "tinysegmenter.el")
        .expect("prepare pinned TinySegmenter source below ./tmp")
        .with_prelude(TINYSEGMENTER_TEST_PRELUDE)
        .with_timeout(TINYSEGMENTER_TEST_TIMEOUT)
}

fn newsroom_copy_produces_exact_searchable_tokens_and_offsets() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 #'tinysegmenter-parity-record
 '("東京都知事は記者会見で新しい交通計画を発表した。"
   "利用者は来月から新しい路線を利用できます。"))
"####;
    let expect = expect![[
        r####"OK ((:input "東京都知事は記者会見で新しい交通計画を発表した。" :length 24 :tokens ("東京" "都" "知事" "は" "記者" "会見" "で" "新しい" "交通" "計画" "を" "発表" "し" "た" "。") :count 15 :spans (("東京" :start 0 :end 2) ("都" :start 2 :end 3) ("知事" :start 3 :end 5) ("は" :start 5 :end 6) ("記者" :start 6 :end 8) ("会見" :start 8 :end 10) ("で" :start 10 :end 11) ("新しい" :start 11 :end 14) ("交通" :start 14 :end 16) ("計画" :start 16 :end 18) ("を" :start 18 :end 19) ("発表" :start 19 :end 21) ("し" :start 21 :end 22) ("た" :start 22 :end 23) ("。" :start 23 :end 24)) :rejoined "東京都知事は記者会見で新しい交通計画を発表した。") (:input "利用者は来月から新しい路線を利用できます。" :length 21 :tokens ("利用" "者" "は" "来月" "から" "新しい" "路線" "を" "利用" "でき" "ます" "。") :count 12 :spans (("利用" :start 0 :end 2) ("者" :start 2 :end 3) ("は" :start 3 :end 4) ("来月" :start 4 :end 6) ("から" :start 6 :end 8) ("新しい" :start 8 :end 11) ("路線" :start 11 :end 13) ("を" :start 13 :end 14) ("利用" :start 14 :end 16) ("でき" :start 16 :end 18) ("ます" :start 18 :end 20) ("。" :start 20 :end 21)) :rejoined "利用者は来月から新しい路線を利用できます。"))"####
    ]];
    ParityBatchCase::value(
        "newsroom_copy_produces_exact_searchable_tokens_and_offsets",
        elisp_form,
        expect,
    )
}

fn mixed_script_release_notes_keep_products_versions_dates_and_punctuation() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 #'tinysegmenter-parity-record
 '("Neomacs 2.0は2026年8月3日に公開予定です。"
   "Rust製GUIとTUIを同じ設定で利用できます。"
   "バージョン２．１ではIME・SVG表示を改善しました。"))
"####;
    let expect = expect![[
        r####"OK ((:input "Neomacs 2.0は2026年8月3日に公開予定です。" :length 29 :tokens ("Neomacs" " " "2" "." "0" "は" "2" "0" "2" "6" "年" "8月" "3" "日" "に" "公開" "予定" "です" "。") :count 19 :spans (("Neomacs" :start 0 :end 7) (" " :start 7 :end 8) ("2" :start 8 :end 9) ("." :start 9 :end 10) ("0" :start 10 :end 11) ("は" :start 11 :end 12) ("2" :start 12 :end 13) ("0" :start 13 :end 14) ("2" :start 14 :end 15) ("6" :start 15 :end 16) ("年" :start 16 :end 17) ("8月" :start 17 :end 19) ("3" :start 19 :end 20) ("日" :start 20 :end 21) ("に" :start 21 :end 22) ("公開" :start 22 :end 24) ("予定" :start 24 :end 26) ("です" :start 26 :end 28) ("。" :start 28 :end 29)) :rejoined "Neomacs 2.0は2026年8月3日に公開予定です。") (:input "Rust製GUIとTUIを同じ設定で利用できます。" :length 25 :tokens ("Rust" "製GUI" "と" "TUI" "を" "同じ" "設定" "で" "利用" "でき" "ます" "。") :count 12 :spans (("Rust" :start 0 :end 4) ("製GUI" :start 4 :end 8) ("と" :start 8 :end 9) ("TUI" :start 9 :end 12) ("を" :start 12 :end 13) ("同じ" :start 13 :end 15) ("設定" :start 15 :end 17) ("で" :start 17 :end 18) ("利用" :start 18 :end 20) ("でき" :start 20 :end 22) ("ます" :start 22 :end 24) ("。" :start 24 :end 25)) :rejoined "Rust製GUIとTUIを同じ設定で利用できます。") (:input "バージョン２．１ではIME・SVG表示を改善しました。" :length 27 :tokens ("バージョン" "２" "．" "１" "で" "は" "IME" "・SVG" "表示" "を" "改善" "し" "まし" "た" "。") :count 15 :spans (("バージョン" :start 0 :end 5) ("２" :start 5 :end 6) ("．" :start 6 :end 7) ("１" :start 7 :end 8) ("で" :start 8 :end 9) ("は" :start 9 :end 10) ("IME" :start 10 :end 13) ("・SVG" :start 13 :end 17) ("表示" :start 17 :end 19) ("を" :start 19 :end 20) ("改善" :start 20 :end 22) ("し" :start 22 :end 23) ("まし" :start 23 :end 25) ("た" :start 25 :end 26) ("。" :start 26 :end 27)) :rejoined "バージョン２．１ではIME・SVG表示を改善しました。"))"####
    ]];
    ParityBatchCase::value(
        "mixed_script_release_notes_keep_products_versions_dates_and_punctuation",
        elisp_form,
        expect,
    )
}

fn incident_knowledge_base_builds_a_deterministic_token_index() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((documents
        '((incident-42 . "東京リージョンで通信障害が発生した。現在は復旧しています。")
          (runbook . "通信障害を確認したら監視画面と復旧手順を開く。")
          (release . "新しい監視機能を本番環境へ公開した。")))
       (tokenized
        (mapcar
         (lambda (document)
           (list (car document) (tseg-segment (cdr document))))
         documents))
       (token-lists (mapcar #'cadr tokenized)))
  (list
   :documents tokenized
   :frequency (tinysegmenter-parity-frequency token-lists)
   :communication-hits
   (cl-loop for (id tokens) in tokenized
            when (member "通信" tokens) collect id)
   :recovery-hits
   (cl-loop for (id tokens) in tokenized
            when (member "復旧" tokens) collect id)
   :reconstructed
   (mapcar
    (lambda (document)
      (cons (car document) (mapconcat #'identity (cadr document) "")))
    tokenized)))
"####;
    let expect = expect![[
        r####"OK (:documents ((incident-42 ("東京" "リージョン" "で" "通信" "障害" "が" "発生" "し" "た" "。" "現在" "は" "復旧" "し" "て" "い" "ます" "。")) (runbook ("通信" "障害" "を" "確認" "し" "たら" "監視" "画面" "と" "復旧" "手順" "を" "開く" "。")) (release ("新しい" "監視" "機能" "を" "本番" "環境" "へ" "公開" "し" "た" "。"))) :frequency (("。" . 4) ("い" . 1) ("が" . 1) ("し" . 4) ("た" . 2) ("たら" . 1) ("て" . 1) ("で" . 1) ("と" . 1) ("は" . 1) ("へ" . 1) ("ます" . 1) ("を" . 3) ("リージョン" . 1) ("公開" . 1) ("復旧" . 2) ("手順" . 1) ("新しい" . 1) ("本番" . 1) ("東京" . 1) ("機能" . 1) ("現在" . 1) ("環境" . 1) ("画面" . 1) ("発生" . 1) ("監視" . 2) ("確認" . 1) ("通信" . 2) ("開く" . 1) ("障害" . 2)) :communication-hits (incident-42 runbook) :recovery-hits (incident-42 runbook) :reconstructed ((incident-42 . "東京リージョンで通信障害が発生した。現在は復旧しています。") (runbook . "通信障害を確認したら監視画面と復旧手順を開く。") (release . "新しい監視機能を本番環境へ公開した。")))"####
    ]];
    ParityBatchCase::value(
        "incident_knowledge_base_builds_a_deterministic_token_index",
        elisp_form,
        expect,
    )
}

fn editor_buffer_is_rewritten_as_readable_tokenized_lines() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "朝の会議で障害対応を確認する。\n"
   "担当者は修正版を午後に公開する。\n"
   "\n"
   "利用者へ結果を連絡する。")
  (let ((original (buffer-string))
        line-records)
    (goto-char (point-min))
    (while (not (eobp))
      (let* ((line (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position)))
             (tokens (tseg-segment line)))
        (push (list :line (line-number-at-pos)
                    :source line
                    :tokens tokens)
              line-records)
        (delete-region (line-beginning-position) (line-end-position))
        (insert (mapconcat #'identity tokens "/")))
      (forward-line 1))
    (list
     :original original
     :records (nreverse line-records)
     :tokenized-buffer (buffer-string)
     :line-count (line-number-at-pos (point-max)))))
"####;
    let expect = expect![[
        r####"OK (:original "朝の会議で障害対応を確認する。\n担当者は修正版を午後に公開する。\n\n利用者へ結果を連絡する。" :records ((:line 1 :source "朝の会議で障害対応を確認する。" :tokens ("朝" "の" "会議" "で" "障害" "対応" "を" "確認" "する" "。")) (:line 2 :source "担当者は修正版を午後に公開する。" :tokens ("担当者" "は" "修正" "版" "を" "午後" "に" "公開" "する" "。")) (:line 3 :source "" :tokens ("E1")) (:line 4 :source "利用者へ結果を連絡する。" :tokens ("利用" "者" "へ" "結果" "を" "連絡" "する" "。"))) :tokenized-buffer "朝/の/会議/で/障害/対応/を/確認/する/。\n担当者/は/修正/版/を/午後/に/公開/する/。\nE1\n利用/者/へ/結果/を/連絡/する/。" :line-count 4)"####
    ]];
    ParityBatchCase::value(
        "editor_buffer_is_rewritten_as_readable_tokenized_lines",
        elisp_form,
        expect,
    )
}

fn imported_labels_handle_blank_short_halfwidth_and_unknown_script_text() -> ParityBatchCase {
    let elisp_form = r####"
(let ((labels '("" "東京" "AI" "12345" "すごい！" "🎉公開" "   " "ｱﾌﾟﾘを公開")))
  (list
   :records (mapcar #'tinysegmenter-parity-record labels)
   :character-classes
   (mapcar
    (lambda (character)
      (list character (tseg-ctype character)))
    '("一" "東" "あ" "ア" "ｱ" "A" "ｚ" "7" "９" "！" "🎉"))))
"####;
    let expect = expect![[
        r####"OK (:records ((:input "" :length 0 :tokens ("E1") :count 1 :spans (("E1" :start 0 :end 2)) :rejoined "E1") (:input "東京" :length 2 :tokens ("東京") :count 1 :spans (("東京" :start 0 :end 2)) :rejoined "東京") (:input "AI" :length 2 :tokens ("AI") :count 1 :spans (("AI" :start 0 :end 2)) :rejoined "AI") (:input "12345" :length 5 :tokens ("1" "2" "3" "4" "5") :count 5 :spans (("1" :start 0 :end 1) ("2" :start 1 :end 2) ("3" :start 2 :end 3) ("4" :start 3 :end 4) ("5" :start 4 :end 5)) :rejoined "12345") (:input "すごい！" :length 4 :tokens ("すごい" "！") :count 2 :spans (("すごい" :start 0 :end 3) ("！" :start 3 :end 4)) :rejoined "すごい！") (:input "🎉公開" :length 3 :tokens ("🎉" "公開") :count 2 :spans (("🎉" :start 0 :end 1) ("公開" :start 1 :end 3)) :rejoined "🎉公開") (:input "   " :length 3 :tokens (" " "  ") :count 2 :spans ((" " :start 0 :end 1) ("  " :start 1 :end 3)) :rejoined "   ") (:input "ｱﾌﾟﾘを公開" :length 7 :tokens ("ｱﾌ" "ﾟﾘ" "を" "公開") :count 4 :spans (("ｱﾌ" :start 0 :end 2) ("ﾟﾘ" :start 2 :end 4) ("を" :start 4 :end 5) ("公開" :start 5 :end 7)) :rejoined "ｱﾌﾟﾘを公開")) :character-classes (("一" "M") ("東" "H") ("あ" "I") ("ア" "K") ("ｱ" "K") ("A" "A") ("ｚ" "A") ("7" "N") ("９" "N") ("！" "O") ("🎉" "O")))"####
    ]];
    ParityBatchCase::value(
        "imported_labels_handle_blank_short_halfwidth_and_unknown_script_text",
        elisp_form,
        expect,
    )
}

#[test]
fn tinysegmenter_package_batch() {
    let cases = vec![
        newsroom_copy_produces_exact_searchable_tokens_and_offsets(),
        mixed_script_release_notes_keep_products_versions_dates_and_punctuation(),
        incident_knowledge_base_builds_a_deterministic_token_index(),
        editor_buffer_is_rewritten_as_readable_tokenized_lines(),
        imported_labels_handle_blank_short_halfwidth_and_unknown_script_text(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed TinySegmenter parity test");
    assert_oracle_batch_cases(
        tinysegmenter_oracle(),
        test_name,
        "tinysegmenter_parity",
        &cases,
    );
}
