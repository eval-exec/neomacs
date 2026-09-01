use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PINYINLIB_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PINYINLIB_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PINYINLIB_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pinyinlib)

(defun pinyinlib-test-match-records
    (query candidates &optional no-punc traditional only-chinese mixed case-fold)
  (let ((regexp
         (pinyinlib-build-regexp-string
          query no-punc traditional only-chinese mixed))
        (case-fold-search case-fold)
        records)
    (dolist (candidate candidates (nreverse records))
      (when (string-match regexp candidate)
        (push
         (list candidate
               :start (match-beginning 0)
               :end (match-end 0)
               :text (match-string 0 candidate))
         records)))))
"##;

fn pinyinlib_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PINYINLIB_MELPA_PIN, "pinyinlib.el")
        .expect("prepare pinned Pinyinlib source below ./tmp")
        .with_prelude(PINYINLIB_TEST_PRELUDE)
        .with_timeout(PINYINLIB_TEST_TIMEOUT)
}

fn contact_search_finds_adjacent_pinyin_initials_inside_real_names() -> ParityBatchCase {
    let elisp_form = r##"
(let ((contacts
       '("张三 <zhang@example.cn>"
         "赵四 <zhao@example.cn>"
         "章森 <sen@example.cn>"
         "张思远 <siyuan@example.cn>"
         "李四 <li@example.cn>"
         "王五 <wang@example.cn>"
         "zhangsan <latin@example.cn>")))
  (list
   :zs (pinyinlib-test-match-records "zs" contacts)
   :ls (pinyinlib-test-match-records "ls" contacts)
   :ww (pinyinlib-test-match-records "ww" contacts)))
"##;
    let expect = expect![[
        r####"OK (:zs (("张三 <zhang@example.cn>" :start 0 :end 2 :text "张三") ("赵四 <zhao@example.cn>" :start 0 :end 2 :text "赵四") ("章森 <sen@example.cn>" :start 0 :end 2 :text "章森") ("张思远 <siyuan@example.cn>" :start 0 :end 2 :text "张思")) :ls (("李四 <li@example.cn>" :start 0 :end 2 :text "李四")) :ww (("王五 <wang@example.cn>" :start 0 :end 2 :text "王五")))"####
    ]];
    ParityBatchCase::value(
        "contact_search_finds_adjacent_pinyin_initials_inside_real_names",
        elisp_form,
        expect,
    )
}

fn locale_modes_distinguish_simplified_traditional_and_mixed_catalog_entries() -> ParityBatchCase {
    let elisp_form = r##"
(let ((catalog
       '("中国制造"
         "中國製造"
         "中港物流"
         "中國國際"
         "美国制造"
         "zhongguo")))
  (list
   :simplified
   (pinyinlib-test-match-records "zg" catalog nil nil nil nil nil)
   :traditional
   (pinyinlib-test-match-records "zg" catalog nil t nil nil nil)
   :mixed
   (pinyinlib-test-match-records "zg" catalog nil nil nil t nil)))
"##;
    let expect = expect![[
        r####"OK (:simplified (("中国制造" :start 0 :end 2 :text "中国") ("中港物流" :start 0 :end 2 :text "中港")) :traditional (("中國製造" :start 0 :end 2 :text "中國") ("中港物流" :start 0 :end 2 :text "中港") ("中國國際" :start 0 :end 2 :text "中國")) :mixed (("中国制造" :start 0 :end 2 :text "中国") ("中國製造" :start 0 :end 2 :text "中國") ("中港物流" :start 0 :end 2 :text "中港") ("中國國際" :start 0 :end 2 :text "中國")))"####
    ]];
    ParityBatchCase::value(
        "locale_modes_distinguish_simplified_traditional_and_mixed_catalog_entries",
        elisp_form,
        expect,
    )
}

fn chinese_only_search_excludes_latin_initials_without_losing_chinese_results() -> ParityBatchCase {
    let elisp_form = r##"
(let ((places
       '("北京总部"
         "背景资料"
         "北疆仓库"
         "东京分部"
         "bj-office"
         "b京混排"
         "BJ-legacy")))
  (list
   :normal
   (pinyinlib-test-match-records "bj" places nil nil nil nil nil)
   :chinese-only
   (pinyinlib-test-match-records "bj" places nil nil t nil nil)
   :case-folded
   (pinyinlib-test-match-records "bj" places nil nil nil nil t)))
"##;
    let expect = expect![[
        r####"OK (:normal (("北京总部" :start 0 :end 2 :text "北京") ("背景资料" :start 0 :end 2 :text "背景") ("北疆仓库" :start 0 :end 2 :text "北疆") ("bj-office" :start 0 :end 2 :text "bj") ("b京混排" :start 0 :end 2 :text "b京")) :chinese-only (("北京总部" :start 0 :end 2 :text "北京") ("背景资料" :start 0 :end 2 :text "背景") ("北疆仓库" :start 0 :end 2 :text "北疆")) :case-folded (("北京总部" :start 0 :end 2 :text "北京") ("背景资料" :start 0 :end 2 :text "背景") ("北疆仓库" :start 0 :end 2 :text "北疆") ("bj-office" :start 0 :end 2 :text "bj") ("b京混排" :start 0 :end 2 :text "b京") ("BJ-legacy" :start 0 :end 2 :text "BJ")))"####
    ]];
    ParityBatchCase::value(
        "chinese_only_search_excludes_latin_initials_without_losing_chinese_results",
        elisp_form,
        expect,
    )
}

fn punctuation_search_accepts_chinese_and_ascii_forms_or_requires_literal_input() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((tickets
       '("北京?"
         "北京？"
         "bj?"
         "bj？"
         "北京!"
         "（北京）?"
         "状态：北京？")))
  (list
   :mapped-question
   (pinyinlib-test-match-records "bj?" tickets nil nil nil nil nil)
   :ascii-question-only
   (pinyinlib-test-match-records "bj?" tickets t nil nil nil nil)
   :mapped-colon
   (pinyinlib-test-match-records ":bj?" tickets nil nil nil nil nil)))
"##;
    let expect = expect![[
        r####"OK (:mapped-question (("北京?" :start 0 :end 3 :text "北京?") ("北京？" :start 0 :end 3 :text "北京？") ("bj?" :start 0 :end 3 :text "bj?") ("bj？" :start 0 :end 3 :text "bj？") ("状态：北京？" :start 3 :end 6 :text "北京？")) :ascii-question-only (("北京?" :start 0 :end 3 :text "北京?") ("bj?" :start 0 :end 3 :text "bj?")) :mapped-colon (("状态：北京？" :start 2 :end 6 :text "：北京？")))"####
    ]];
    ParityBatchCase::value(
        "punctuation_search_accepts_chinese_and_ascii_forms_or_requires_literal_input",
        elisp_form,
        expect,
    )
}

fn regexp_metacharacters_remain_literal_around_a_pinyin_initial() -> ParityBatchCase {
    let elisp_form = r##"
(let ((labels
       '("[阿]+$"
         "[爱]+￥"
         "[a]+$"
         "[安]+USD"
         "[八]+$"
         "prefix [阿]+$ suffix"
         "[阿阿]+$")))
  (list
   :mapped-currency
   (pinyinlib-test-match-records "[a]+$" labels nil nil nil nil nil)
   :ascii-currency
   (pinyinlib-test-match-records "[a]+$" labels t nil nil nil nil)
   :chinese-only
   (pinyinlib-test-match-records "[a]+$" labels nil nil t nil nil)))
"##;
    let expect = expect![[
        r####"OK (:mapped-currency (("[阿]+$" :start 0 :end 5 :text "[阿]+$") ("[爱]+￥" :start 0 :end 5 :text "[爱]+￥") ("[a]+$" :start 0 :end 5 :text "[a]+$") ("prefix [阿]+$ suffix" :start 7 :end 12 :text "[阿]+$")) :ascii-currency (("[阿]+$" :start 0 :end 5 :text "[阿]+$") ("[a]+$" :start 0 :end 5 :text "[a]+$") ("prefix [阿]+$ suffix" :start 7 :end 12 :text "[阿]+$")) :chinese-only (("[阿]+$" :start 0 :end 5 :text "[阿]+$") ("[爱]+￥" :start 0 :end 5 :text "[爱]+￥") ("prefix [阿]+$ suffix" :start 7 :end 12 :text "[阿]+$")))"####
    ]];
    ParityBatchCase::value(
        "regexp_metacharacters_remain_literal_around_a_pinyin_initial",
        elisp_form,
        expect,
    )
}

fn buffer_navigation_reports_every_matching_initial_pair_in_document_order() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "001 上海仓 completed\n"
   "002 深圳站 delayed\n"
   "003 苏杭线 active\n"
   "004 山河组 queued\n"
   "005 天津港 active\n"
   "006 sh-lab archived\n")
  (goto-char (point-min))
  (let ((regexp (pinyinlib-build-regexp-string "sh"))
        (case-fold-search nil)
        matches)
    (while (re-search-forward regexp nil t)
      (push
       (list :line (line-number-at-pos)
             :column (- (match-beginning 0) (line-beginning-position))
             :text (match-string-no-properties 0)
             :point (point))
       matches))
    (list :matches (nreverse matches)
          :final-point (point))))
"##;
    let expect = expect![[
        r####"OK (:matches ((:line 1 :column 4 :text "上海" :point 7) (:line 3 :column 4 :text "苏杭" :point 41) (:line 4 :column 4 :text "山河" :point 56) (:line 6 :column 4 :text "sh" :point 86)) :final-point 86)"####
    ]];
    ParityBatchCase::value(
        "buffer_navigation_reports_every_matching_initial_pair_in_document_order",
        elisp_form,
        expect,
    )
}

fn uppercase_and_numeric_user_input_remains_literal_under_explicit_case_policy() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((identifiers
       '("A1-北京"
         "a1-北京"
         "A1-shanghai"
         "XA1-suffix"
         "甲A1记录"
         "Ａ1-fullwidth")))
  (list
   :case-sensitive
   (pinyinlib-test-match-records "A1" identifiers nil nil nil nil nil)
   :case-folded
   (pinyinlib-test-match-records "A1" identifiers nil nil nil nil t)
   :lowercase-pinyin
   (pinyinlib-test-match-records "a1" identifiers nil nil nil nil nil)))
"##;
    let expect = expect![[
        r####"OK (:case-sensitive (("A1-北京" :start 0 :end 2 :text "A1") ("A1-shanghai" :start 0 :end 2 :text "A1") ("XA1-suffix" :start 1 :end 3 :text "A1") ("甲A1记录" :start 1 :end 3 :text "A1")) :case-folded (("A1-北京" :start 0 :end 2 :text "A1") ("a1-北京" :start 0 :end 2 :text "a1") ("A1-shanghai" :start 0 :end 2 :text "A1") ("XA1-suffix" :start 1 :end 3 :text "A1") ("甲A1记录" :start 1 :end 3 :text "A1")) :lowercase-pinyin (("a1-北京" :start 0 :end 2 :text "a1")))"####
    ]];
    ParityBatchCase::value(
        "uppercase_and_numeric_user_input_remains_literal_under_explicit_case_policy",
        elisp_form,
        expect,
    )
}

#[test]
fn pinyinlib_package_batch() {
    let cases = vec![
        contact_search_finds_adjacent_pinyin_initials_inside_real_names(),
        locale_modes_distinguish_simplified_traditional_and_mixed_catalog_entries(),
        chinese_only_search_excludes_latin_initials_without_losing_chinese_results(),
        punctuation_search_accepts_chinese_and_ascii_forms_or_requires_literal_input(),
        regexp_metacharacters_remain_literal_around_a_pinyin_initial(),
        buffer_navigation_reports_every_matching_initial_pair_in_document_order(),
        uppercase_and_numeric_user_input_remains_literal_under_explicit_case_policy(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Pinyinlib parity test");
    assert_oracle_batch_cases(pinyinlib_oracle(), test_name, "pinyinlib_parity", &cases);
}
