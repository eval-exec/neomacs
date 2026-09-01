//! Divergence tests: regex, string, and search semantics.
//!
//! Tests for regex matching edge cases, string operations, and search
//! behavior that may differ between neomacs and GNU Emacs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_string_match_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-match "ā" "xāy")"#, expect);
}

#[test]
fn divergence_string_match_capture_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 5 1 3 3 5 \"abcd\" \"ab\" \"cd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "\\(ab\\)\\(cd\\)" "XabcdY")
  (list (match-beginning 0) (match-end 0)
        (match-beginning 1) (match-end 1)
        (match-beginning 2) (match-end 2)
        (match-string 0 "XabcdY")
        (match-string 1 "XabcdY")
        (match-string 2 "XabcdY")))"#,
        expect,
    );
}

#[test]
fn divergence_string_match_zero_width_assertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-match "\\<hello\\>" "say hello world")"#,
        expect,
    );
}

#[test]
fn divergence_replace_match_backreferences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (search-failed \"\\\\<\\\\(\\\\w+\\\\)\\\\> \\\\1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "foo bar baz")
  (goto-char 1)
  (re-search-forward "\\<\\(\\w+\\)\\> \\1")
  (replace-match "XXX")
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_string_match_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((case-fold-search t))
  (list (string-match "HELLO" "say hello world")
        (let ((case-fold-search nil))
          (string-match "HELLO" "say hello world"))))"#,
        expect,
    );
}

#[test]
fn divergence_regexp_quote_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"a\\\\.b\\\\*c\\\\+d\\\\?e\\\\[f]g{h}i(j)j|k\\\\^l\\\\$m\\\\\\\\n\\\\\\\\o\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(regexp-quote "a.b*c+d?e[f]g{h}i(j)j|k^l$m\\n\\o")"#,
        expect,
    );
}

#[test]
fn divergence_string_replace_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"bXbcXd\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string "a+" "X" "baaabcaaaaad")"#,
        expect,
    );
}

#[test]
fn divergence_string_replace_with_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"world hello bar foo\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(replace-regexp-in-string
 "\\(\\w+\\) \\(\\w+\\)" "\\2 \\1" "hello world foo bar")"#,
        expect,
    );
}

#[test]
fn divergence_re_search_forward_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"line1.*line3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "line1\nline2\nline3")
  (goto-char 1)
  (re-search-forward "line1.*line3")
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_string_match_shy_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 5 3 5 \"cd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "\\(?:ab\\)\\(cd\\)" "XabcdY")
  (list (match-beginning 0) (match-end 0)
        (match-beginning 1) (match-end 1)
        (match-string 1 "XabcdY")))"#,
        expect,
    );
}

#[test]
fn divergence_string_match_nested_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 1 4 2 3 \"abc\" \"abc\" \"b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "\\(a\\(b\\)c\\)" "XabcY")
  (list (match-beginning 0) (match-end 0)
        (match-beginning 1) (match-end 1)
        (match-beginning 2) (match-end 2)
        (match-string 0 "XabcY")
        (match-string 1 "XabcY")
        (match-string 2 "XabcY")))"#,
        expect,
    );
}

#[test]
fn divergence_skip_chars_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abc   def")
  (goto-char 1)
  (skip-chars-forward "a-z")
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_skip_chars_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abc   def")
  (goto-char 9)
  (skip-chars-backward "a-z")
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_string_multibyte_length_vs_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 5 \"ābcd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "ābcd"))
  (list (length s)
        (string-bytes s)
        (string-to-multibyte s)))"#,
        expect,
    );
}

#[test]
fn divergence_string_equal_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s1 "café")
        (s2 "caf\\u00e9"))
  (list (string= s1 s2) (string-equal s1 s2)))"#,
        expect,
    );
}

#[test]
fn divergence_substring_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ābc\" \"c中d\" \"中def\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "ābc中def"))
  (list (substring s 0 3)
        (substring s 2 5)
        (substring s 3)))"#,
        expect,
    );
}
