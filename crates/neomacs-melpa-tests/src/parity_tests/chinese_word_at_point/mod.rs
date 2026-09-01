use std::time::Duration;

use expect_test::expect;

use crate::{CHINESE_WORD_AT_POINT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CHINESE_WORD_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CHINESE_WORD_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-chinese-test-install-segmenter (name)
  "Install a deterministic external segmenter and return (COMMAND . LOG)."
  (let* ((root (file-name-as-directory
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (script (expand-file-name "chinese-segmenter.sh" root))
         (log (expand-file-name (concat name "-segmenter.log") root))
         (coding-system-for-write 'utf-8-unix))
    (when (file-exists-p log)
      (delete-file log))
    (with-temp-file script
      (insert "#!/bin/sh\n"
              "printf '%s\\n' \"$1\" >> \"$2\"\n"
              "case \"$1\" in\n"
              "  中国人使用中文) printf '%s\\n' '中国 人 使用 中文' ;;\n"
              "  我喜欢自然语言处理) printf '%s\\n' '我 喜欢 自然语言 处理' ;;\n"
              "  北京大学生前来应聘) printf '%s\\n' '北京 大学生 前来 应聘' ;;\n"
              "  也关注北京大学生前来应聘) printf '%s\\n' '也 关注 北京 大学生 前来 应聘' ;;\n"
              "  发布说明) printf '%s\\n' '发布 说明' ;;\n"
              "  配置) printf '%s\\n' '配置' ;;\n"
              "  東京) printf '%s\\n' '東京' ;;\n"
              "  *) printf '%s\\n' \"$1\" ;;\n"
              "esac\n"))
    (set-file-modes script #o700)
    (cons (format "%s %%s %s"
                  (shell-quote-argument script)
                  (shell-quote-argument log))
          log)))

(defun neomacs-chinese-test-log-lines (path)
  "Return non-empty segmenter input lines recorded at PATH."
  (if (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents path)
        (split-string (buffer-string) "\n" t))
    nil))
"##;

fn chinese_word_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CHINESE_WORD_AT_POINT_MELPA_PIN, "chinese-word-at-point.el")
        .expect("prepare pinned Chinese Word at Point source below ./tmp")
        .with_prelude(CHINESE_WORD_TEST_PRELUDE)
        .with_timeout(CHINESE_WORD_TEST_TIMEOUT)
}

fn reader_selects_each_segment_across_a_chinese_sentence() -> ParityBatchCase {
    ParityBatchCase::value(
        "reader_selects_each_segment_across_a_chinese_sentence",
        r##"
(let* ((fixture (neomacs-chinese-test-install-segmenter "reader"))
       (chinese-word-split-command (car fixture))
       (log (cdr fixture)))
  (with-temp-buffer
    (insert "中国人使用中文。")
    (let (selections)
      (dolist (position '(1 2 3 4 5 6 7 8))
        (goto-char position)
        (push (list :position position
                    :word (chinese-word-at-point)
                    :bounds (bounds-of-thing-at-point 'chinese-word))
              selections))
      (let ((inputs (neomacs-chinese-test-log-lines log)))
        (list :selections (nreverse selections)
              :segmenter-call-count (length inputs)
              :unique-segmenter-inputs (delete-dups (copy-sequence inputs)))))))
"##,
        expect![[
            r##"OK (:selections ((:position 1 :word "中国" :bounds (1 . 3)) (:position 2 :word "中国" :bounds (1 . 3)) (:position 3 :word "人" :bounds (3 . 4)) (:position 4 :word "使用" :bounds (4 . 6)) (:position 5 :word "使用" :bounds (4 . 6)) (:position 6 :word "中文" :bounds (6 . 8)) (:position 7 :word "中文" :bounds (6 . 8)) (:position 8 :word "中文" :bounds (6 . 8))) :segmenter-call-count 16 :unique-segmenter-inputs ("中国人使用中文"))"##
        ]],
    )
}

fn mixed_release_notes_extract_chinese_english_and_numeric_words() -> ParityBatchCase {
    ParityBatchCase::value(
        "mixed_release_notes_extract_chinese_english_and_numeric_words",
        r##"
(let* ((fixture (neomacs-chinese-test-install-segmenter "release-notes"))
       (chinese-word-split-command (car fixture))
       (log (cdr fixture)))
  (with-temp-buffer
    (insert "发布说明：Release version 42 safely。")
    (goto-char (point-min))
    (let ((chinese (chinese-or-other-word-at-point)))
      (search-forward "说明")
      (backward-char 1)
      (let ((second-chinese (chinese-or-other-word-at-point)))
        (search-forward "Release")
        (backward-char 3)
        (let ((english (chinese-or-other-word-at-point))
              chinese-only-on-english)
          (setq chinese-only-on-english (chinese-word-at-point))
          (search-forward "42")
          (backward-char 1)
          (let ((number (chinese-or-other-word-at-point)))
            (search-forward "。")
            (list :words (list chinese second-chinese english number)
                  :chinese-only-on-english chinese-only-on-english
                  :at-punctuation (chinese-or-other-word-at-point)
                  :segmenter-inputs
                  (neomacs-chinese-test-log-lines log))))))))
"##,
        expect![[
            r##"OK (:words ("发布" "说明" "Release" "42") :chinese-only-on-english nil :at-punctuation nil :segmenter-inputs ("发布说明" "发布说明"))"##
        ]],
    )
}

fn editor_uses_segment_bounds_to_replace_a_domain_term() -> ParityBatchCase {
    ParityBatchCase::value(
        "editor_uses_segment_bounds_to_replace_a_domain_term",
        r##"
(let* ((fixture (neomacs-chinese-test-install-segmenter "editor"))
       (chinese-word-split-command (car fixture))
       (log (cdr fixture)))
  (with-temp-buffer
    (insert "我喜欢自然语言处理，也关注北京大学生前来应聘。")
    (goto-char (point-min))
    (search-forward "自然语言")
    (backward-char 2)
    (let* ((selected (chinese-word-at-point))
           (bounds (bounds-of-thing-at-point 'chinese-word)))
      (delete-region (car bounds) (cdr bounds))
      (goto-char (car bounds))
      (insert "NLP")
      (search-forward "大学生")
      (backward-char 2)
      (list :selected selected
            :candidate (chinese-word-at-point)
            :candidate-bounds (bounds-of-thing-at-point 'chinese-word)
            :edited (buffer-string)
            :segmenter-inputs
            (neomacs-chinese-test-log-lines log)))))
"##,
        expect![[
            r##"OK (:selected "自然语言" :candidate "大学生" :candidate-bounds (15 . 18) :edited "我喜欢NLP处理，也关注北京大学生前来应聘。" :segmenter-inputs ("我喜欢自然语言处理" "我喜欢自然语言处理" "也关注北京大学生前来应聘" "也关注北京大学生前来应聘"))"##
        ]],
    )
}

fn glossary_extraction_handles_cjk_and_ascii_entries_in_one_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "glossary_extraction_handles_cjk_and_ascii_entries_in_one_document",
        r##"
(let* ((fixture (neomacs-chinese-test-install-segmenter "glossary"))
       (chinese-word-split-command (car fixture))
       (log (cdr fixture)))
  (with-temp-buffer
    (insert "配置 config 東京 deploy")
    (let (entries)
      (dolist (needle '("配置" "config" "東京" "deploy"))
        (goto-char (point-min))
        (search-forward needle)
        (backward-char 1)
        (push (list needle
                    (chinese-or-other-word-at-point)
                    (chinese-word-at-point))
              entries))
      (list :entries (nreverse entries)
            :predicates
            (mapcar #'chinese-word-chinese-string-p
                    '("配置" "東京" "config" "配置2" ""))
            :segmenter-inputs
            (neomacs-chinese-test-log-lines log)))))
"##,
        expect![[
            r##"OK (:entries (("配置" "配置" "配置") ("config" "config" nil) ("東京" "東京" "東京") ("deploy" "deploy" nil)) :predicates (t t nil nil t) :segmenter-inputs ("配置" "配置" "東京" "東京"))"##
        ]],
    )
}

#[test]
fn chinese_word_at_point_package_batch() {
    let cases = vec![
        reader_selects_each_segment_across_a_chinese_sentence(),
        mixed_release_notes_extract_chinese_english_and_numeric_words(),
        editor_uses_segment_bounds_to_replace_a_domain_term(),
        glossary_extraction_handles_cjk_and_ascii_entries_in_one_document(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed Chinese Word at Point parity test");
    assert_oracle_batch_cases(
        chinese_word_oracle(),
        test_name,
        "chinese_word_at_point_parity",
        &cases,
    );
}
