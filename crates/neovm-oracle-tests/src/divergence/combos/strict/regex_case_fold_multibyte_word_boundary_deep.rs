//! Strict combo oracle probes, batch 105: regex case-folding with multibyte,
//! word-boundary on CJK, case-fold-search with custom case tables, and
//! char-fold with combining marks.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r9_regex_case_fold_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((case-fold-search t))
  (list (string-match-p "café" "CAFÉ")
        (string-match-p "hello" "HELLO")
        (string-match-p "[a-z]+" "ABCDEF")
        (string-match-p "\\ca+" "ABCdef")
        (string-match-p "naïve" "NAÏVE")))
"####,
        expect,
    );
}

#[test]
fn div_r9_word_boundary_cjk_and_char_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 \"\\\\(?:e\u{301}\\\\|é\\\\)\" 11 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((char-fold-mode t))
  (list (string-match-p "\\bword\\b" "a word here")
        (char-fold-to-regexp "é")
        (length (char-fold-to-regexp "é"))
        (string-match-p (char-fold-to-regexp "a") "å")
        (string-match-p (char-fold-to-regexp "n") "ñ")))
"####,
        expect,
    );
}

#[test]
fn div_r9_regex_backreference_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((case-fold-search t))
  (list (and (string-match "\\(foo\\)\\1" "FOOfoo") (match-string 1))
        (and (string-match "\\(foo\\)\\1" "fooFOO") (match-string 1))
        (and (string-match "\\(.\\)\\1" "aa") (match-string 1))
        (and (string-match "\\(.\\)\\1" "AA") (match-string 1))))
"####,
        expect,
    );
}

#[test]
fn div_r9_regex_alternation_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"ab\" \"a\" \"bc\" \"ab\")""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((s "abc"))
  (list (and (string-match "a\\|b" s) (match-string 0 s))
        (and (string-match "ab\\|c" s) (match-string 0 s))
        (and (string-match "a\\|bc" s) (match-string 0 s))
        (and (string-match "\\(a\\|b\\)c" s) (match-string 0 s))
        (and (string-match "a\\(?:b\\|c\\)" s) (match-string 0 s))))
"####,
        expect,
    );
}
