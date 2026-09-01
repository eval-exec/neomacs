//! Strict combo oracle probes, batch 157: advanced regexp semantics. shy
//! non-capturing \(?:...\) / \(?\) and numbered \(?1:...\) groups, backreference
//! \1 capture skipping shy groups, word-boundary \b/\B, POSIX leftmost-longest
//! matching of \(a\|ab\) vs first-alternative, regexp-opt with/without
//! parentheses, and case-fold multi-byte matching.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_regex_shy_numbered_groups_capture_skipping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-match "\\(?:shy\\)\\(cap\\)" "shycap")
      (match-beginning 1)
      (match-string 1 "shycap")
      (condition-case err (match-string 2 "shycap") (error 'err))
      ;; numbered explicit group ?1: skips nothing but allows backref to it
      (string-match "\\(?1:foo\\)" "foo")
      ;; shy then captured: \1 refers to first CAPTURING group, not shy
      (string-match "\\(?:ab\\)\\(cd\\) \\1" "abcd cd")
      (match-string 1 "abcd cd")
      ;; backref to shy should fail to capture
      (condition-case err
          (progn (string-match "\\(?:x\\)\\1" "xx")
                 'unexpected-match)
        (invalid-regexp 'caught-invalid)))
"##;
    let expect = expect_test::expect![[r#""OK (0 3 \"cap\" nil 0 0 \"cd\" caught-invalid)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_regex_word_boundary_posix_longest_opt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-match "\\bword\\b" "a word here")
      (match-beginning 0)
      (string-match "\\bword\\b" "wordsmith")
      (string-match "\\Bword" "password")
      (match-beginning 0)
      ;; POSIX leftmost-longest: does \(a\|ab\) match "a" or "ab" against "ab"?
      (string-match "\\(a\\|ab\\)" "ab")
      (match-string 0 "ab")
      (string-match "\\(ab\\|a\\)" "ab")
      (match-string 0 "ab")
      ;; nested alternation with backtracking
      (string-match "\\(foo\\|fo\\)\\(bar\\|bar\\)" "foobar")
      (list (match-string 1 "foobar") (match-string 2 "foobar"))
      (regexp-opt '("foo" "bar" "baz"))
      (regexp-opt '("foo" "bar" "baz") t)
      (regexp-opt '("a" "ab" "abc")))
"##;
    let expect = expect_test::expect![[
        r#""OK (2 2 nil 4 4 0 \"a\" 0 \"ab\" 0 (\"foo\" \"bar\") \"\\\\(?:ba[rz]\\\\|foo\\\\)\" \"\\\\(ba[rz]\\\\|foo\\\\)\" \"\\\\(?:a\\\\(?:bc?\\\\)?\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_regex_case_fold_multibyte_charclass() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s "Café NAÏVE 日本語"))
  (list (let ((case-fold-search t)) (string-match "café" s) (match-beginning 0))
        (let ((case-fold-search t)) (string-match "naïve" s) (match-beginning 0))
        (let ((case-fold-search nil)) (string-match "café" s) 'no-match-when-case-sensitive)
        (string-match "日\\(.\\)語" s)
        (match-string 1 s)
        (string-match "[一-鿿]+" s)
        (match-string 0 s)
        (string-match "A\\(.\\)A" s)))
"##;
    let expect = expect_test::expect![[
        r#""OK (0 5 no-match-when-case-sensitive 11 \"本\" 11 \"日本語\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
