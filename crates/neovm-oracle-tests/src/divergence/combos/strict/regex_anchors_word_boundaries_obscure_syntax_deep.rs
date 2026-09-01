//! Strict combo oracle probes, batch 107: obscure regex syntax — \`\\' anchors,
//! \= at-point, \< \> word boundaries, \Cc complement, charset ranges crossing
//! multibyte, 10+ capture groups, compare-buffer-substrings, and
//! string-lessp on mixed unibyte/multibyte.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s1_regex_anchors_and_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 4 t 2 2 2 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (string-match-p "\\`" "text")
      (string-match-p "\\'" "text")
      (and (string-match "\\`text\\'" "text") t)
      (string-match-p "\\<word\\>" "a word here")
      (string-match-p "\\bword\\b" "a word here")
      (string-match-p "\\<" "  word")
      (string-match-p "\\>" "word  "))
"####,
        expect,
    );
}

#[test]
fn div_s1_regex_charset_multibyte_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (string-match-p "[あ-ん]" "ほ")
      (string-match-p "[^あ-ん]" "A")
      (string-match-p "[一-龯]+" "日本語")
      (string-match-p "[\\x41-\\x5a]+" "HELLO")
      (and (string-match "[ぁ-ゖ]+" "ひらがな") (match-string 0)))
"####,
        expect,
    );
}

#[test]
fn div_s1_regex_many_capture_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcdefghi\" \"a\" \"i\" 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((pat (apply #'concat (mapcar (lambda (i) (format "\\(.\\)" i)) (number-sequence 1 9))))
      (s "abcdefghi"))
  (and (string-match pat s)
       (list (match-string 0 s)
             (match-string 1 s)
             (match-string 9 s)
             (length (match-data t)))))
"####,
        expect,
    );
}

#[test]
fn div_s1_compare_buffer_substrings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 -3)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (with-temp-buffer
        (insert "hello")
        (compare-buffer-substrings nil nil nil nil nil nil))
      (let ((b1 (generate-new-buffer " *cmp-a*"))
            (b2 (generate-new-buffer " *cmp-b*")))
        (unwind-protect
            (progn
              (with-current-buffer b1 (insert "abc"))
              (with-current-buffer b2 (insert "abd"))
              (compare-buffer-substrings b1 nil nil b2 nil nil))
          (kill-buffer b1)
          (kill-buffer b2))))
"####,
        expect,
    );
}

#[test]
fn div_s1_string_lessp_mixed_unibyte_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (string-lessp "abc" "abd")
      (string-lessp "abc" "abc")
      (string-lessp "abc" "ab")
      (string-lessp "a" "b")
      (string-lessp "z" "aa")
      (string-version-lessp "file2" "file10")
      (string-version-lessp "1.0.0" "1.0.1"))
"####,
        expect,
    );
}
