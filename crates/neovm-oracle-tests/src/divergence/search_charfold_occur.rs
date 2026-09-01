//! Divergence tests: char-folding, isearch, and occur stubs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable char-fold-symmetric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-fold-to-regexp)
  (boundp 'char-fold-symmetric)
  (booleanp char-fold-symmetric))"#,
        expect,
    );
}

#[test]
fn divergence_char_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((regexp (char-fold-to-regexp "e")))
  (list (stringp regexp)
        (string-match regexp "é")
        (string-match regexp "e")
        (string-match regexp "ë")))"#,
        expect,
    );
}

#[test]
fn divergence_isearch_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'isearch-forward)
  (fboundp 'isearch-backward)
  (fboundp 'isearch-forward-regexp)
  (fboundp 'isearch-backward-regexp))"#,
        expect,
    );
}

#[test]
fn divergence_isearch_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (booleanp case-fold-search)
  (booleanp search-highlight)
  (booleanp search-invisible)
  (booleanp isearch-lazy-highlight))"#,
        expect,
    );
}

#[test]
fn divergence_occur_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'occur)
  (fboundp 'multi-occur)
  (fboundp 'how-many)
  (fboundp 'flush-lines)
  (fboundp 'keep-lines))"#,
        expect,
    );
}

#[test]
fn divergence_keep_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"apple\\nbanana\\ncherry\\napricot\\nblueberry\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "apple\nbanana\ncherry\napricot\nblueberry")
  (keep-lines "ap")
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_flush_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"apple\\nbanana\\ncherry\\napricot\\nblueberry\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "apple\nbanana\ncherry\napricot\nblueberry")
  (flush-lines "an")
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_how_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa bbb aaa ccc aaa")
  (how-many "aaa"))"#,
        expect,
    );
}

#[test]
fn divergence_replace_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"XXX bar XXX baz XXX\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo bar foo baz foo")
  (goto-char 1)
  (replace-string "foo" "XXX")
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'query-replace)
  (fboundp 'query-replace-regexp)
  (fboundp 'map-query-replace-regexp))"#,
        expect,
    );
}
