//! Divergence tests: regexp engine deep - backreferences, lookahead, boundaries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_regex_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 nil \"abab\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "\\(ab\\)\\1" "abab")
  (string-match "\\(ab\\)\\1" "abba")
  (match-string 0 "abab"))"#,
        expect,
    );
}

#[test]
fn divergence_regex_word_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "\\<hello\\>" "say hello world")
  (string-match "\\<hello\\>" "say helloworld")
  (string-match "\\<hello\\>" "sayhello world"))"#,
        expect,
    );
}

#[test]
fn divergence_regex_non_greedy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"<a>\" \"<a><b><c>\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "<.*?>" "<a><b><c>")
  (match-string 0 "<a><b><c>")
  (progn
    (string-match "<.*>" "<a><b><c>")
    (match-string 0 "<a><b><c>")))"#,
        expect,
    );
}

#[test]
fn divergence_regex_char_class_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "[aeiou]" "xyz")
  (string-match "[aeiou]" "abc")
  (string-match "[^aeiou]" "aei")
  (string-match "[a-z&&[^aeiou]]" "b"))"#,
        expect,
    );
}

#[test]
fn divergence_regex_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "line1\nline2\nline3"))
  (list
    (string-match "^line2" s)
    (string-match "^line2" s t)
    (string-match "line2$" s)))"#,
        expect,
    );
}

#[test]
fn divergence_regex_shy_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abcd\" \"cd\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "\\(?:ab\\)\\(cd\\)" "abcd")
  (list (match-string 0 "abcd")
        (match-string 1 "abcd")
        (match-string 2 "abcd")))"#,
        expect,
    );
}

#[test]
fn divergence_regex_named_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 3 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (string-match "(?2:ab)(cd)" "abcd")
  (list (match-beginning 0)
        (match-end 0)
        (match-beginning 1)
        (match-end 1)))"#,
        expect,
    );
}

#[test]
fn divergence_regex_replace_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"XXX bbb XXX bbb XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa bbb aaa bbb aaa")
  (goto-char 1)
  (let ((count 0))
    (while (re-search-forward "aaa" nil t)
      (setq count (1+ count))
      (replace-match "XXX"))
    (list count (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_regex_unicode_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"hello\" 5 \"123\" 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "[[:alpha:]]+" "hello123")
  (match-string 0 "hello123")
  (string-match "[[:digit:]]+" "hello123")
  (match-string 0 "hello123")
  (string-match "[[:space:]]" "hello world"))"#,
        expect,
    );
}

#[test]
fn divergence_regex_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((case-fold-search t))
  (list (string-match "hello" "HELLO")
        (string-match "hello" "HELLO")))"#,
        expect,
    );
}

#[test]
fn divergence_regex_syntax_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"hello\" 0 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-match "\\sw+" "hello world")
  (match-string 0 "hello world")
  (string-match "\\s(" "(foo)")
  (string-match "\\s)" "(foo)"))"#,
        expect,
    );
}
